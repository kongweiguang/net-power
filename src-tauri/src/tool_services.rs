//! @author kongweiguang
//! 本地工具 HTTP 服务运行器。配置持久化在 SQLite，运行态只负责启动和暂停。

use crate::error::{AppError, AppResult};
use crate::models::{
    RuntimeStatus, ToolServiceContentSource, ToolServiceInput, ToolServiceRouteInput,
    ToolServiceSummary,
};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;

/// 本地工具服务运行态管理器。
#[derive(Default)]
pub struct ToolServiceManager {
    running: HashMap<String, RunningToolService>,
}

/// 已从工具服务运行态表取出的停止任务。
pub(crate) struct StoppingToolService {
    handle: JoinHandle<()>,
}

impl StoppingToolService {
    /// 等待工具服务后台任务退出。
    pub(crate) async fn wait(self) -> AppResult<RuntimeStatus> {
        match timeout(Duration::from_secs(3), self.handle).await {
            Ok(joined) => {
                if let Err(err) = joined {
                    return Ok(RuntimeStatus::Failed {
                        message: format!("工具服务停止异常: {err}"),
                    });
                }
            }
            Err(_) => {
                return Ok(RuntimeStatus::Failed {
                    message: "工具服务停止超时，后台任务可能仍在退出".to_string(),
                });
            }
        }
        Ok(RuntimeStatus::Stopped)
    }
}

impl ToolServiceManager {
    /// 创建空管理器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动一个本地工具 HTTP 服务。id 来自 SQLite 配置主键，暂停后再次启动仍复用该 id。
    pub async fn start(
        &mut self,
        id: String,
        input: ToolServiceInput,
    ) -> AppResult<ToolServiceSummary> {
        if let Some(service) = self.running.get(&id) {
            return Ok(service.summary());
        }
        let config = ToolServiceRuntimeConfig::try_from(input)?;
        let addr = format!("{}:{}", config.host, config.port);
        if self.running.values().any(|service| service.addr == addr) {
            return Err(AppError::Conflict(format!("工具服务端口已被占用: {addr}")));
        }
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|err| AppError::Conflict(format!("HTTP 服务监听失败 {addr}: {err}")))?;
        let actual_addr = listener.local_addr()?;
        let mut config = config;
        config.port = actual_addr.port();

        let started_at = Utc::now().to_rfc3339();
        let cancel = CancellationToken::new();
        let counters = Arc::new(ToolServiceCounters::default());
        let runtime = Arc::new(config.clone());
        let handle = tokio::spawn(run_http_tool_service(
            listener,
            cancel.clone(),
            Arc::clone(&runtime),
            Arc::clone(&counters),
        ));
        let service = RunningToolService {
            id: id.clone(),
            addr: format!("{}:{}", config.host, config.port),
            started_at,
            config,
            cancel,
            handle,
            counters,
        };
        let summary = service.summary();
        self.running.insert(id, service);
        Ok(summary)
    }

    /// 停止一个本地工具服务。
    #[cfg(test)]
    pub async fn stop(&mut self, id: &str) -> AppResult<RuntimeStatus> {
        let Some(stopping) = self.begin_stop(id) else {
            return Ok(RuntimeStatus::Stopped);
        };
        stopping.wait().await
    }

    /// 读取某个正在运行的本地工具服务摘要。
    pub fn running_summary(&self, id: &str) -> Option<ToolServiceSummary> {
        self.running.get(id).map(RunningToolService::summary)
    }

    /// 从运行态表取出工具服务并触发取消；调用方可在不持锁时等待后台任务退出。
    pub(crate) fn begin_stop(&mut self, id: &str) -> Option<StoppingToolService> {
        let service = self.running.remove(id)?;
        service.cancel.cancel();
        Some(StoppingToolService {
            handle: service.handle,
        })
    }
}

#[derive(Debug, Clone)]
struct ToolServiceRuntimeConfig {
    name: String,
    host: String,
    port: u16,
    static_root_dir: Option<PathBuf>,
    static_path_prefix: String,
    routes: Vec<ToolServiceRouteConfig>,
}

impl ToolServiceRuntimeConfig {
    fn first_access_path(&self) -> &str {
        self.static_root_dir
            .as_ref()
            .map(|_| self.static_path_prefix.as_str())
            .or_else(|| self.routes.first().map(|route| route.path.as_str()))
            .unwrap_or("/")
    }
}

#[derive(Debug, Clone)]
struct ToolServiceRouteConfig {
    method: String,
    path: String,
    response_status: u16,
    content_type: String,
    content: ToolServiceRouteContent,
}

#[derive(Debug, Clone)]
enum ToolServiceRouteContent {
    Inline(Vec<u8>),
    File(PathBuf),
}

impl TryFrom<ToolServiceInput> for ToolServiceRuntimeConfig {
    type Error = AppError;

    fn try_from(input: ToolServiceInput) -> Result<Self, Self::Error> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput("服务名称不能为空".to_string()));
        }
        let host = input.host.trim();
        if host.is_empty() {
            return Err(AppError::InvalidInput("监听主机不能为空".to_string()));
        }
        if input.port == 0 {
            return Err(AppError::InvalidInput("端口必须是 1-65535".to_string()));
        }
        let static_root_dir = input
            .static_root_dir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(validate_static_root_dir)
            .transpose()?;
        let routes = input
            .routes
            .into_iter()
            .enumerate()
            .map(|(index, route)| validate_route(index, route))
            .collect::<Result<Vec<_>, _>>()?;
        if static_root_dir.is_none() && routes.is_empty() {
            return Err(AppError::InvalidInput(
                "请至少配置静态目录或一个接口".to_string(),
            ));
        }

        Ok(Self {
            name: name.to_string(),
            host: host.to_string(),
            port: input.port,
            static_root_dir,
            static_path_prefix: normalize_service_path(&input.static_path_prefix),
            routes,
        })
    }
}

fn validate_static_root_dir(value: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(value);
    let metadata = std::fs::metadata(&path)
        .map_err(|err| AppError::InvalidInput(format!("静态目录不可访问: {err}")))?;
    if !metadata.is_dir() {
        return Err(AppError::InvalidInput("静态目录必须是文件夹".to_string()));
    }
    Ok(std::fs::canonicalize(path)?)
}

fn validate_route(index: usize, input: ToolServiceRouteInput) -> AppResult<ToolServiceRouteConfig> {
    let label = format!("接口 #{}", index + 1);
    let method = normalize_method(&input.method)
        .ok_or_else(|| AppError::InvalidInput(format!("{label} 的请求方法无效")))?;
    let path = normalize_service_path(&input.path);
    let response_status = input.response_status;
    if !(100..=599).contains(&response_status) {
        return Err(AppError::InvalidInput(format!(
            "{label} 的响应状态码必须是 100-599"
        )));
    }
    let content_type = input.content_type.trim();
    let content_type = if content_type.is_empty() {
        "application/json; charset=utf-8"
    } else {
        content_type
    }
    .to_string();
    let content = match input.content_source {
        ToolServiceContentSource::Inline => {
            ToolServiceRouteContent::Inline(input.body.unwrap_or_default().into_bytes())
        }
        ToolServiceContentSource::File => {
            let raw_path = input
                .file_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::InvalidInput(format!("{label} 需要选择响应文件")))?;
            let path = PathBuf::from(raw_path);
            let metadata = std::fs::metadata(&path).map_err(|err| {
                AppError::InvalidInput(format!("{label} 响应文件不可访问: {err}"))
            })?;
            if !metadata.is_file() {
                return Err(AppError::InvalidInput(format!(
                    "{label} 响应文件必须是文件"
                )));
            }
            ToolServiceRouteContent::File(std::fs::canonicalize(path)?)
        }
    };
    Ok(ToolServiceRouteConfig {
        method,
        path,
        response_status,
        content_type,
        content,
    })
}

struct RunningToolService {
    id: String,
    addr: String,
    started_at: String,
    config: ToolServiceRuntimeConfig,
    cancel: CancellationToken,
    handle: JoinHandle<()>,
    counters: Arc<ToolServiceCounters>,
}

impl RunningToolService {
    fn summary(&self) -> ToolServiceSummary {
        let url = format!(
            "http://{}:{}{}",
            self.config.host,
            self.config.port,
            self.config.first_access_path()
        );
        ToolServiceSummary {
            id: self.id.clone(),
            name: self.config.name.clone(),
            host: self.config.host.clone(),
            port: self.config.port,
            url,
            static_root_dir: self
                .config
                .static_root_dir
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            static_path_prefix: self.config.static_path_prefix.clone(),
            route_count: self.config.routes.len(),
            started_at: Some(self.started_at.clone()),
            total_requests: self.counters.total_requests.load(Ordering::Relaxed),
            runtime_status: RuntimeStatus::Running,
        }
    }
}

#[derive(Default)]
struct ToolServiceCounters {
    total_requests: AtomicU64,
}

async fn run_http_tool_service(
    listener: TcpListener,
    cancel: CancellationToken,
    config: Arc<ToolServiceRuntimeConfig>,
    counters: Arc<ToolServiceCounters>,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    continue;
                };
                let config = Arc::clone(&config);
                let counters = Arc::clone(&counters);
                tokio::spawn(async move {
                    let _ = handle_connection(stream, config, counters).await;
                });
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    config: Arc<ToolServiceRuntimeConfig>,
    counters: Arc<ToolServiceCounters>,
) -> AppResult<()> {
    let request = match read_http_request(&mut stream).await {
        Ok(request) => request,
        Err(err) => {
            write_response(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                err.to_string().into_bytes(),
                false,
            )
            .await?;
            return Ok(());
        }
    };
    counters.total_requests.fetch_add(1, Ordering::Relaxed);
    let is_head = request.method.eq_ignore_ascii_case("HEAD");
    let response = build_tool_response(&config, &request).await;
    write_response(
        &mut stream,
        response.status,
        &response.content_type,
        response.body,
        is_head,
    )
    .await
}

struct HttpRequest {
    method: String,
    path: String,
}

struct ToolHttpResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

async fn read_http_request(stream: &mut TcpStream) -> AppResult<HttpRequest> {
    let mut buffer = Vec::with_capacity(4096);
    let header_end = loop {
        if let Some(index) = find_header_end(&buffer) {
            break index + 4;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Err(AppError::InvalidInput("HTTP 请求头过大".to_string()));
        }
        let mut chunk = [0_u8; 4096];
        let read = timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .map_err(|_| AppError::InvalidInput("读取 HTTP 请求超时".to_string()))??;
        if read == 0 {
            return Err(AppError::InvalidInput("HTTP 请求为空".to_string()));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let parsed_head = parse_http_head(&buffer[..header_end])?;
    if parsed_head.content_length > MAX_REQUEST_BODY_BYTES {
        return Err(AppError::InvalidInput(format!(
            "请求体超过 {} 字节",
            MAX_REQUEST_BODY_BYTES
        )));
    }
    while buffer.len() < header_end + parsed_head.content_length {
        let mut chunk = [0_u8; 4096];
        let read = timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .map_err(|_| AppError::InvalidInput("读取 HTTP 请求体超时".to_string()))??;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(HttpRequest {
        method: parsed_head.method,
        path: parsed_head.path,
    })
}

struct ParsedHttpHead {
    method: String,
    path: String,
    content_length: usize,
}

fn parse_http_head(head: &[u8]) -> AppResult<ParsedHttpHead> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    let status = request
        .parse(head)
        .map_err(|err| AppError::InvalidInput(format!("HTTP 请求解析失败: {err}")))?;
    if status.is_partial() {
        return Err(AppError::InvalidInput("HTTP 请求头不完整".to_string()));
    }
    let method = request
        .method
        .ok_or_else(|| AppError::InvalidInput("HTTP 请求缺少方法".to_string()))?
        .to_string();
    let path = request
        .path
        .ok_or_else(|| AppError::InvalidInput("HTTP 请求缺少路径".to_string()))?
        .to_string();
    let content_length = request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-length"))
        .and_then(|header| String::from_utf8_lossy(header.value).trim().parse().ok())
        .unwrap_or(0);
    Ok(ParsedHttpHead {
        method,
        path,
        content_length,
    })
}

async fn build_tool_response(
    config: &ToolServiceRuntimeConfig,
    request: &HttpRequest,
) -> ToolHttpResponse {
    if let Some(route) = config
        .routes
        .iter()
        .find(|route| route_matches(route, request))
    {
        return route_response(route).await;
    }
    if config.static_root_dir.is_some()
        && path_matches(
            &config.static_path_prefix,
            request_path_without_query(&request.path),
        )
    {
        return static_response(config, request).await;
    }
    plain_response(404, "请求路径未匹配")
}

async fn route_response(route: &ToolServiceRouteConfig) -> ToolHttpResponse {
    let body = match &route.content {
        ToolServiceRouteContent::Inline(body) => body.clone(),
        ToolServiceRouteContent::File(path) => match tokio::fs::read(path).await {
            Ok(body) => body,
            Err(_) => return plain_response(500, "响应文件读取失败"),
        },
    };
    ToolHttpResponse {
        status: route.response_status,
        content_type: route.content_type.clone(),
        body,
    }
}

async fn static_response(
    config: &ToolServiceRuntimeConfig,
    request: &HttpRequest,
) -> ToolHttpResponse {
    if !request.method.eq_ignore_ascii_case("GET") && !request.method.eq_ignore_ascii_case("HEAD") {
        return plain_response(405, "静态目录只支持 GET 和 HEAD");
    }
    let Some(root_dir) = &config.static_root_dir else {
        return plain_response(404, "静态目录未配置");
    };
    let Some(target) =
        resolve_static_target(root_dir, &config.static_path_prefix, &request.path).await
    else {
        return plain_response(404, "文件不存在");
    };
    let content_type = content_type_for_path(&target).to_string();
    match tokio::fs::read(&target).await {
        Ok(body) => ToolHttpResponse {
            status: 200,
            content_type,
            body,
        },
        Err(_) => plain_response(404, "文件不存在"),
    }
}

async fn resolve_static_target(
    root_dir: &Path,
    prefix: &str,
    request_path: &str,
) -> Option<PathBuf> {
    let path = request_path_without_query(request_path);
    let relative = relative_request_path(prefix, path)?;
    let mut target = root_dir.to_path_buf();
    if relative.is_empty() {
        target.push("index.html");
    } else {
        for segment in relative.split('/') {
            if segment.is_empty() {
                continue;
            }
            if segment == "." || segment == ".." || segment.contains('\\') {
                return None;
            }
            target.push(segment);
        }
    }
    let metadata = tokio::fs::metadata(&target).await.ok()?;
    if metadata.is_dir() {
        target.push("index.html");
    }
    let canonical = tokio::fs::canonicalize(&target).await.ok()?;
    if canonical.starts_with(root_dir) {
        Some(canonical)
    } else {
        None
    }
}

fn route_matches(route: &ToolServiceRouteConfig, request: &HttpRequest) -> bool {
    method_matches(&route.method, &request.method)
        && request_path_without_query(&request.path) == route.path
}

fn method_matches(route_method: &str, request_method: &str) -> bool {
    route_method == "ANY"
        || route_method.eq_ignore_ascii_case(request_method)
        || (route_method == "GET" && request_method.eq_ignore_ascii_case("HEAD"))
}

fn normalize_method(value: &str) -> Option<String> {
    let method = value.trim().to_ascii_uppercase();
    match method.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "ANY" => Some(method),
        _ => None,
    }
}

fn write_status_line(status: u16) -> String {
    format!("HTTP/1.1 {status} {}\r\n", status_reason(status))
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    head_only: bool,
) -> AppResult<()> {
    let mut response = Vec::new();
    response.extend_from_slice(write_status_line(status).as_bytes());
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    response.extend_from_slice(b"Connection: close\r\n\r\n");
    if !head_only {
        response.extend_from_slice(&body);
    }
    stream.write_all(&response).await?;
    stream.shutdown().await?;
    Ok(())
}

fn plain_response(status: u16, message: &str) -> ToolHttpResponse {
    ToolHttpResponse {
        status,
        content_type: "text/plain; charset=utf-8".to_string(),
        body: message.as_bytes().to_vec(),
    }
}

fn normalize_service_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let with_leading = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    with_leading.trim_end_matches('/').to_string()
}

fn path_matches(prefix: &str, path: &str) -> bool {
    prefix == "/" || path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn relative_request_path<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    if !path_matches(prefix, path) {
        return None;
    }
    if prefix == "/" {
        return Some(path.trim_start_matches('/'));
    }
    Some(path.strip_prefix(prefix)?.trim_start_matches('/'))
}

fn request_path_without_query(path: &str) -> &str {
    path.split_once('?').map_or(path, |(head, _)| head)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn status_reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use uuid::Uuid;

    #[tokio::test]
    async fn http_service_returns_inline_route_body() {
        let port = reserve_tcp_port();
        let mut manager = ToolServiceManager::new();
        let summary = manager
            .start(
                "tool-http".to_string(),
                ToolServiceInput {
                    name: "HTTP".to_string(),
                    host: "127.0.0.1".to_string(),
                    port,
                    static_root_dir: None,
                    static_path_prefix: "/".to_string(),
                    routes: vec![ToolServiceRouteInput {
                        method: "GET".to_string(),
                        path: "/api/hello".to_string(),
                        response_status: 201,
                        content_type: "application/json; charset=utf-8".to_string(),
                        content_source: ToolServiceContentSource::Inline,
                        body: Some("{\"ok\":true}".to_string()),
                        file_path: None,
                    }],
                },
            )
            .await
            .expect("HTTP 服务应可启动");

        let response = request(
            summary.port,
            "GET /api/hello HTTP/1.1\r\nHost: local\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 201 Created"));
        assert!(response.contains("{\"ok\":true}"));
        assert_eq!(
            manager.stop(&summary.id).await.expect("应可停止"),
            RuntimeStatus::Stopped
        );
    }

    #[tokio::test]
    async fn http_service_returns_file_route_body() {
        let port = reserve_tcp_port();
        let file_path = std::env::temp_dir().join(format!(
            "net-power-tool-service-response-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&file_path, "{\"fromFile\":true}").expect("应可写入测试响应文件");
        let mut manager = ToolServiceManager::new();
        let summary = manager
            .start(
                "tool-file".to_string(),
                ToolServiceInput {
                    name: "HTTP".to_string(),
                    host: "127.0.0.1".to_string(),
                    port,
                    static_root_dir: None,
                    static_path_prefix: "/".to_string(),
                    routes: vec![ToolServiceRouteInput {
                        method: "POST".to_string(),
                        path: "/api/file".to_string(),
                        response_status: 200,
                        content_type: "application/json; charset=utf-8".to_string(),
                        content_source: ToolServiceContentSource::File,
                        body: None,
                        file_path: Some(file_path.to_string_lossy().into_owned()),
                    }],
                },
            )
            .await
            .expect("HTTP 服务应可启动");

        let response = request(
            summary.port,
            "POST /api/file HTTP/1.1\r\nHost: local\r\nContent-Length: 2\r\n\r\n{}",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("{\"fromFile\":true}"));
        assert_eq!(
            manager.stop(&summary.id).await.expect("应可停止"),
            RuntimeStatus::Stopped
        );
        let _ = std::fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn http_service_serves_static_file_after_route_miss() {
        let port = reserve_tcp_port();
        let root = std::env::temp_dir().join(format!("net-power-tool-service-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("assets")).expect("应可创建测试目录");
        std::fs::write(root.join("assets").join("hello.txt"), "hello").expect("应可写入测试文件");
        let mut manager = ToolServiceManager::new();
        let summary = manager
            .start(
                "tool-static".to_string(),
                ToolServiceInput {
                    name: "Static".to_string(),
                    host: "127.0.0.1".to_string(),
                    port,
                    static_root_dir: Some(root.to_string_lossy().into_owned()),
                    static_path_prefix: "/public".to_string(),
                    routes: vec![],
                },
            )
            .await
            .expect("静态服务应可启动");

        let response = request(
            summary.port,
            "GET /public/assets/hello.txt HTTP/1.1\r\nHost: local\r\n\r\n",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("hello"));
        assert_eq!(
            manager.stop(&summary.id).await.expect("应可停止"),
            RuntimeStatus::Stopped
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn reserve_tcp_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("应可绑定临时端口");
        let port = listener.local_addr().expect("应可读取临时端口").port();
        drop(listener);
        port
    }

    async fn request(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应可连接测试服务");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("应可写入测试请求");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("应可读取测试响应");
        String::from_utf8_lossy(&response).into_owned()
    }
}
