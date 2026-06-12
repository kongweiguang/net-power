//! @author kongweiguang
//! HTTP reverse/forward proxy 实现。支持 HTTP/1.1 keep-alive 顺序请求、配置改写、chunked 请求体解码和 CONNECT 隧道。

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::manager::ServiceCounters;
use crate::models::{
    ConnectionEventPayload, HttpReverseConfig, ServiceDetail, ServiceEventPayload, ServiceKind,
};
use crate::proxy::rewrite::{apply_header_rules, rewrite_body, HeaderMap as SimpleHeaders};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method};
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_CHUNK_SIZE_LINE_BYTES: usize = 1024;
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// 启动 HTTP 代理服务。
pub async fn run_http_service<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    log_event(
        &db,
        &app,
        Some(&detail.id),
        "info",
        &format!("HTTP 代理已监听 {listen_addr}"),
    )?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                log_event(&db, &app, Some(&detail.id), "info", "HTTP 代理已停止")?;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (stream, remote_addr) = accepted?;
                let child_cancel = cancel.child_token();
                let db = Arc::clone(&db);
                let app = app.clone();
                let counters = Arc::clone(&counters);
                let detail = detail.clone();
                tokio::spawn(async move {
                    counters.active_connections.fetch_add(1, Ordering::Relaxed);
                    counters.total_connections.fetch_add(1, Ordering::Relaxed);
                    let started = Instant::now();
                    let result = handle_http_connection(stream, &detail, child_cancel).await;
                    counters.active_connections.fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    let events = match result {
                        Ok(events) => events,
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(&db, &app, Some(&detail.id), "error", &message);
                            vec![(
                                "http".to_string(),
                                String::new(),
                                String::new(),
                                String::new(),
                                String::new(),
                                None,
                                0,
                                0,
                                message,
                                None,
                            )]
                        }
                    };
                    for (
                        protocol,
                        target_addr,
                        method,
                        host,
                        path,
                        status,
                        bytes_in,
                        bytes_out,
                        error,
                        warning,
                    ) in events
                    {
                        counters.bytes_in.fetch_add(bytes_in, Ordering::Relaxed);
                        counters.bytes_out.fetch_add(bytes_out, Ordering::Relaxed);
                        if let Some(warning) = warning {
                            let _ = log_event(&db, &app, Some(&detail.id), "warn", &warning);
                        }
                        let _ = db.insert_connection_event(
                            &detail.id,
                            &protocol,
                            &remote_addr.to_string(),
                            &target_addr,
                            "request",
                            &method,
                            &host,
                            &path,
                            status,
                            bytes_in,
                            bytes_out,
                            duration_ms,
                            &error,
                        );
                        let _ = app.emit(
                            "service://connection",
                            ConnectionEventPayload {
                                service_id: detail.id.clone(),
                                protocol,
                                event_type: "request".to_string(),
                                target_addr,
                            },
                        );
                    }
                });
            }
        }
    }
}

type HttpEvent = (
    String,
    String,
    String,
    String,
    String,
    Option<u16>,
    u64,
    u64,
    String,
    Option<String>,
);

async fn handle_http_connection(
    stream: TcpStream,
    detail: &ServiceDetail,
    cancel: CancellationToken,
) -> AppResult<Vec<HttpEvent>> {
    let client = Client::builder()
        .timeout(Duration::from_millis(
            http_request_timeout_ms(detail)?.max(1),
        ))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut connection = ClientConnection::new(stream);
    let mut events = Vec::new();

    while let Some(request) = connection.read_request().await? {
        if request.method.eq_ignore_ascii_case("CONNECT") {
            events.push(handle_connect(connection.into_stream(), request, detail, cancel).await?);
            return Ok(events);
        }
        let (event, keep_alive) =
            handle_plain_http(connection.stream_mut(), request, detail, &client).await?;
        events.push(event);
        if !keep_alive {
            break;
        }
    }
    Ok(events)
}

async fn handle_connect(
    mut stream: TcpStream,
    request: ClientRequest,
    detail: &ServiceDetail,
    cancel: CancellationToken,
) -> AppResult<HttpEvent> {
    let cfg = detail
        .http_forward
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("CONNECT 仅 HTTP forward 服务可用".to_string()))?;
    if !cfg.allow_connect {
        write_simple_response(&mut stream, 403, "CONNECT disabled").await?;
        return Ok(connect_event(&request, 403, 0, 0, "CONNECT disabled"));
    }
    let target_addr = request.target.clone();
    let mut upstream = match tokio::time::timeout(
        Duration::from_millis(cfg.connect_timeout_ms.max(1)),
        TcpStream::connect(&target_addr),
    )
    .await
    {
        Ok(Ok(upstream)) => upstream,
        Ok(Err(err)) => {
            write_simple_response(&mut stream, 502, "CONNECT failed").await?;
            return Ok(connect_event(&request, 502, 0, 0, &err.to_string()));
        }
        Err(_) => {
            write_simple_response(&mut stream, 504, "CONNECT timeout").await?;
            return Ok(connect_event(&request, 504, 0, 0, "CONNECT timeout"));
        }
    };
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\nConnection: close\r\n\r\n")
        .await?;
    let (bytes_in, bytes_out) = tokio::select! {
        _ = cancel.cancelled() => Ok::<(u64, u64), std::io::Error>((0, 0)),
        copied = copy_bidirectional(&mut stream, &mut upstream) => copied,
    }?;
    Ok(connect_event(&request, 200, bytes_in, bytes_out, ""))
}

fn connect_event(
    request: &ClientRequest,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    error: &str,
) -> HttpEvent {
    (
        "https".to_string(),
        request.target.clone(),
        request.method.clone(),
        request.target.clone(),
        String::new(),
        Some(status),
        bytes_in,
        bytes_out,
        error.to_string(),
        None,
    )
}

fn http_request_timeout_ms(detail: &ServiceDetail) -> AppResult<u64> {
    match detail.kind {
        ServiceKind::HttpReverse => detail
            .http_reverse
            .as_ref()
            .map(|cfg| cfg.request_timeout_ms)
            .ok_or_else(|| AppError::InvalidInput("HTTP 反向代理缺少配置".to_string())),
        ServiceKind::HttpForward => detail
            .http_forward
            .as_ref()
            .map(|cfg| cfg.connect_timeout_ms)
            .ok_or_else(|| AppError::InvalidInput("HTTP Forward 缺少配置".to_string())),
        _ => Err(AppError::InvalidInput(
            "当前服务类型不是 HTTP 代理".to_string(),
        )),
    }
}

async fn handle_plain_http(
    stream: &mut TcpStream,
    mut request: ClientRequest,
    detail: &ServiceDetail,
    client: &Client,
) -> AppResult<(HttpEvent, bool)> {
    let mut rewrite_warning = None;
    let keep_alive = request.keep_alive;
    let (target_url, reverse_cfg) = match detail.kind {
        ServiceKind::HttpReverse => {
            let cfg = detail
                .http_reverse
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP 反向代理缺少配置".to_string()))?;
            (build_reverse_url(cfg, &request.target)?, Some(cfg))
        }
        ServiceKind::HttpForward => {
            let cfg = detail
                .http_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP Forward 缺少配置".to_string()))?;
            if !cfg.allow_http {
                write_simple_response(stream, 403, "HTTP disabled").await?;
                return Ok((plain_event(&request, "", 403, 0, 0, "HTTP disabled"), false));
            }
            (build_forward_url(&request)?, None)
        }
        _ => {
            return Err(AppError::InvalidInput(
                "当前服务类型不是 HTTP 代理".to_string(),
            ));
        }
    };

    apply_header_rules(&mut request.headers, &detail.header_rules, "request");
    if let Some(cfg) = reverse_cfg {
        if !cfg.preserve_host {
            request.headers.remove("host");
        }
        let outcome = rewrite_body(
            &request.body,
            request.headers.get("content-type").map(String::as_str),
            request.headers.get("content-encoding").map(String::as_str),
            cfg.max_rewrite_body_bytes,
            cfg.skip_compressed_body,
            &detail.body_rewrite_rules,
        )?;
        let changed_body = outcome.changed.then_some(outcome.body);
        rewrite_warning = outcome.warning;
        if let Some(body) = changed_body {
            request.body = body;
            request
                .headers
                .insert("content-length".to_string(), request.body.len().to_string());
        }
    }

    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| AppError::InvalidInput(format!("HTTP 方法无效: {}", request.method)))?;
    let mut builder = client.request(method, target_url.clone());
    for (name, value) in request.headers.iter() {
        if should_skip_request_header(name) {
            continue;
        }
        if let (Ok(header_name), Ok(header_value)) =
            (HeaderName::from_str(name), HeaderValue::from_str(value))
        {
            builder = builder.header(header_name, header_value);
        }
    }
    let upstream = builder.body(request.body.clone()).send().await;
    let response = match upstream {
        Ok(response) => response,
        Err(err) if err.is_timeout() => {
            write_simple_response(stream, 504, "Gateway Timeout").await?;
            return Ok((
                plain_event(
                    &request,
                    target_url.as_str(),
                    504,
                    request.body.len() as u64,
                    0,
                    "upstream timeout",
                ),
                false,
            ));
        }
        Err(err) => {
            write_simple_response(stream, 502, "Bad Gateway").await?;
            return Ok((
                plain_event(
                    &request,
                    target_url.as_str(),
                    502,
                    request.body.len() as u64,
                    0,
                    &err.to_string(),
                ),
                false,
            ));
        }
    };

    let status = response.status().as_u16();
    let mut response_headers = SimpleHeaders::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            response_headers.insert(name.as_str().to_ascii_lowercase(), value.to_string());
        }
    }
    apply_header_rules(&mut response_headers, &detail.header_rules, "response");
    let body = response.bytes().await?;
    write_upstream_response(stream, status, &response_headers, &body, keep_alive).await?;
    Ok((
        plain_event_with_warning(
            &request,
            target_url.as_str(),
            status,
            request.body.len() as u64,
            body.len() as u64,
            "",
            rewrite_warning,
        ),
        keep_alive,
    ))
}

fn plain_event(
    request: &ClientRequest,
    target_addr: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    error: &str,
) -> HttpEvent {
    plain_event_with_warning(
        request,
        target_addr,
        status,
        bytes_in,
        bytes_out,
        error,
        None,
    )
}

fn plain_event_with_warning(
    request: &ClientRequest,
    target_addr: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    error: &str,
    warning: Option<String>,
) -> HttpEvent {
    let host = request.headers.get("host").cloned().unwrap_or_default();
    (
        "http".to_string(),
        target_addr.to_string(),
        request.method.clone(),
        host,
        request.target.clone(),
        Some(status),
        bytes_in,
        bytes_out,
        error.to_string(),
        warning,
    )
}

fn build_reverse_url(cfg: &HttpReverseConfig, request_target: &str) -> AppResult<Url> {
    let mut target = Url::parse(&cfg.target_url)?;
    let (incoming_path, incoming_query) = split_path_query(request_target);
    let base_path = target.path().trim_end_matches('/');
    let path = if incoming_path.starts_with('/') {
        format!("{base_path}{incoming_path}")
    } else {
        format!("{base_path}/{incoming_path}")
    };
    target.set_path(if path.is_empty() { "/" } else { &path });
    target.set_query(incoming_query);
    Ok(target)
}

fn build_forward_url(request: &ClientRequest) -> AppResult<Url> {
    if request.target.starts_with("http://") || request.target.starts_with("https://") {
        return Ok(Url::parse(&request.target)?);
    }
    Err(AppError::InvalidInput(
        "HTTP forward 请求行必须包含完整 URL".to_string(),
    ))
}

fn split_path_query(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

fn should_skip_request_header(name: &str) -> bool {
    HOP_BY_HOP_HEADERS.contains(&name) || matches!(name, "content-length" | "host")
}

async fn write_upstream_response(
    stream: &mut TcpStream,
    status: u16,
    headers: &SimpleHeaders,
    body: &[u8],
    keep_alive: bool,
) -> AppResult<()> {
    let reason = reason_phrase(status);
    stream
        .write_all(format!("HTTP/1.1 {status} {reason}\r\n").as_bytes())
        .await?;
    for (name, value) in headers {
        if HOP_BY_HOP_HEADERS.contains(&name.as_str()) || name == "content-length" {
            continue;
        }
        stream
            .write_all(format!("{name}: {value}\r\n").as_bytes())
            .await?;
    }
    stream
        .write_all(
            format!(
                "content-length: {}\r\nconnection: {}\r\n\r\n",
                body.len(),
                if keep_alive { "keep-alive" } else { "close" }
            )
            .as_bytes(),
        )
        .await?;
    stream.write_all(body).await?;
    Ok(())
}

async fn write_simple_response(stream: &mut TcpStream, status: u16, body: &str) -> AppResult<()> {
    let reason = reason_phrase(status);
    stream
        .write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        502 => "Bad Gateway",
        504 => "Gateway Timeout",
        _ => "Proxy Response",
    }
}

#[derive(Debug, Clone)]
struct ClientRequest {
    method: String,
    target: String,
    headers: SimpleHeaders,
    body: Vec<u8>,
    keep_alive: bool,
}

struct ClientConnection {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl ClientConnection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffer: Vec::with_capacity(4096),
        }
    }

    fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    fn into_stream(self) -> TcpStream {
        self.stream
    }

    async fn read_request(&mut self) -> AppResult<Option<ClientRequest>> {
        let header_end = loop {
            if let Some(pos) = find_header_end(&self.buffer) {
                break pos;
            }
            let mut chunk = [0_u8; 2048];
            let read = self.stream.read(&mut chunk).await?;
            if read == 0 {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                return Err(AppError::InvalidInput(
                    "HTTP header 未结束，客户端连接已关闭".to_string(),
                ));
            }
            self.buffer.extend_from_slice(&chunk[..read]);
            if self.buffer.len() > MAX_HEADER_BYTES {
                return Err(AppError::InvalidInput("HTTP header 过大".to_string()));
            }
        };

        let body_start = header_end + 4;
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut parsed = httparse::Request::new(&mut headers);
        parsed
            .parse(&self.buffer[..body_start])
            .map_err(|err| AppError::InvalidInput(format!("HTTP 请求解析失败: {err}")))?;
        let version = parsed.version.unwrap_or(1);
        let method = parsed
            .method
            .ok_or_else(|| AppError::InvalidInput("HTTP 方法缺失".to_string()))?
            .to_string();
        let target = parsed
            .path
            .ok_or_else(|| AppError::InvalidInput("HTTP 请求目标缺失".to_string()))?
            .to_string();
        let mut simple_headers = SimpleHeaders::new();
        for header in parsed.headers {
            if header.name.is_empty() {
                continue;
            }
            if let Ok(value) = std::str::from_utf8(header.value) {
                simple_headers.insert(header.name.to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let keep_alive = request_keep_alive(version, &simple_headers);
        let body = if is_chunked_request(&simple_headers) {
            self.buffer.drain(..body_start);
            let body = read_chunked_body(&mut self.stream, &mut self.buffer).await?;
            simple_headers.remove("transfer-encoding");
            simple_headers.insert("content-length".to_string(), body.len().to_string());
            body
        } else {
            let content_length = simple_headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let request_end = body_start + content_length;
            while self.buffer.len() < request_end {
                let mut chunk = [0_u8; 2048];
                let read = self.stream.read(&mut chunk).await?;
                if read == 0 {
                    break;
                }
                self.buffer.extend_from_slice(&chunk[..read]);
            }
            let available_end = request_end.min(self.buffer.len());
            let body = self.buffer[body_start..available_end].to_vec();
            self.buffer.drain(..available_end);
            body
        };
        Ok(Some(ClientRequest {
            method,
            target,
            headers: simple_headers,
            body,
            keep_alive,
        }))
    }
}

fn request_keep_alive(version: u8, headers: &SimpleHeaders) -> bool {
    let connection = headers
        .get("connection")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if version >= 1 {
        !connection
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("close"))
    } else {
        connection
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("keep-alive"))
    }
}

async fn read_chunked_body(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> AppResult<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let line = read_chunk_size_line(stream, buffer).await?;
        let size_text = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| AppError::InvalidInput("chunked 请求体 chunk size 无效".to_string()))?;
        if size == 0 {
            read_chunked_trailers(stream, buffer).await?;
            return Ok(decoded);
        }
        read_at_least_buffered(stream, buffer, size + 2).await?;
        decoded.extend_from_slice(&buffer[..size]);
        if &buffer[size..size + 2] != b"\r\n" {
            return Err(AppError::InvalidInput(
                "chunked 请求体 chunk 缺少结尾 CRLF".to_string(),
            ));
        }
        buffer.drain(..size + 2);
    }
}

fn is_chunked_request(headers: &SimpleHeaders) -> bool {
    headers
        .get("transfer-encoding")
        .map(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
        })
        .unwrap_or(false)
}

async fn read_chunk_size_line(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> AppResult<String> {
    loop {
        if let Some(pos) = buffer.windows(2).position(|window| window == b"\r\n") {
            let line = buffer.drain(..pos).collect::<Vec<_>>();
            buffer.drain(..2);
            return String::from_utf8(line).map_err(|_| {
                AppError::InvalidInput("chunked 请求体 size 行不是 UTF-8".to_string())
            });
        }
        if buffer.len() > MAX_CHUNK_SIZE_LINE_BYTES {
            return Err(AppError::InvalidInput(
                "chunked 请求体 size 行过长".to_string(),
            ));
        }
        read_more_or_closed(stream, buffer).await?;
    }
}

async fn read_chunked_trailers(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> AppResult<()> {
    loop {
        if let Some(pos) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            buffer.drain(..pos + 4);
            return Ok(());
        }
        if buffer.starts_with(b"\r\n") {
            buffer.drain(..2);
            return Ok(());
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Err(AppError::InvalidInput(
                "chunked 请求体 trailer 过大".to_string(),
            ));
        }
        read_more_or_closed(stream, buffer).await?;
    }
}

async fn read_at_least_buffered(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
    needed: usize,
) -> AppResult<()> {
    while buffer.len() < needed {
        read_more_or_closed(stream, buffer).await?;
    }
    Ok(())
}

async fn read_more_or_closed(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> AppResult<()> {
    let mut chunk = [0_u8; 2048];
    let read = stream.read(&mut chunk).await?;
    if read == 0 {
        return Err(AppError::InvalidInput(
            "chunked 请求体尚未结束，客户端连接已关闭".to_string(),
        ));
    }
    buffer.extend_from_slice(&chunk[..read]);
    Ok(())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn log_event<R: Runtime>(
    db: &Database,
    app: &AppHandle<R>,
    service_id: Option<&str>,
    level: &str,
    message: &str,
) -> AppResult<()> {
    db.insert_service_event(service_id, level, message, "{}")?;
    let _ = app.emit(
        "service://log",
        ServiceEventPayload {
            service_id: service_id.map(ToOwned::to_owned),
            level: level.to_string(),
            message: message.to_string(),
        },
    );
    Ok(())
}

#[cfg(test)]
#[path = "http_chunked_tests.rs"]
mod chunked_tests;

#[cfg(test)]
#[path = "http_keep_alive_tests.rs"]
mod keep_alive_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BodyRewriteRule, HeaderRule, HttpForwardConfig, ServiceDetail};
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};

    #[test]
    fn reverse_url_preserves_path_query() {
        let url = build_reverse_url(
            &HttpReverseConfig {
                target_url: "https://example.com/api".to_string(),
                preserve_host: false,
                request_timeout_ms: 30_000,
                max_rewrite_body_bytes: 1024,
                skip_compressed_body: true,
            },
            "/v1/chat?x=1",
        )
        .expect("URL 应可构造");
        assert_eq!(url.as_str(), "https://example.com/api/v1/chat?x=1");
    }

    #[tokio::test]
    async fn http_forward_proxies_full_url_request() {
        let upstream = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试上游应可监听");
        let upstream_addr = upstream.local_addr().expect("应可读取上游地址");
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream.accept().await.expect("应收到上游请求");
            let request = read_until_headers(&mut socket).await;
            assert!(request.starts_with("GET /v1/proxy?x=1 HTTP/1.1"));
            assert!(request.contains("x-from-test: yes"));
            socket
                .write_all(
                    b"HTTP/1.1 201 Created\r\ncontent-type: text/plain\r\ncontent-length: 7\r\n\r\nforward",
                )
                .await
                .expect("应可写回响应");
        });

        let proxy = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试代理入口应可监听");
        let proxy_addr = proxy.local_addr().expect("应可读取代理入口地址");
        let detail = http_forward_detail();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.expect("应收到客户端请求");
            handle_http_connection(stream, &detail, CancellationToken::new())
                .await
                .expect("HTTP forward 应成功")
        });

        let mut client = TcpStream::connect(proxy_addr)
            .await
            .expect("应可连接测试代理入口");
        let request = format!(
            "GET http://{upstream_addr}/v1/proxy?x=1 HTTP/1.1\r\nhost: {upstream_addr}\r\nx-from-test: yes\r\nconnection: close\r\n\r\n"
        );
        client
            .write_all(request.as_bytes())
            .await
            .expect("应可发送代理请求");
        let response = read_to_string(&mut client).await;

        assert!(response.starts_with("HTTP/1.1 201 Proxy Response"));
        assert!(response.contains("forward"));
        let events = proxy_task.await.expect("代理任务应完成");
        let event = events.first().expect("应记录 HTTP forward 事件");
        assert_eq!(event.0, "http");
        assert_eq!(event.2, "GET");
        assert_eq!(event.5, Some(201));
        upstream_task.await.expect("上游任务应完成");
    }

    #[tokio::test]
    async fn http_reverse_rewrites_headers_body_and_response_headers() {
        let upstream = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试上游应可监听");
        let upstream_addr = upstream.local_addr().expect("应可读取上游地址");
        let upstream_task = tokio::spawn(async move {
            let (mut socket, _) = upstream.accept().await.expect("应收到上游请求");
            let (request, body) = read_http_message(&mut socket).await;
            assert!(request.starts_with("POST /base/api/users?debug=1 HTTP/1.1"));
            assert!(request.contains("x-added: ok"));
            assert!(!request.contains("x-remove:"));
            let json: Value = serde_json::from_slice(&body).expect("改写后 body 应是 JSON");
            assert_eq!(json["user"]["name"], "new");
            socket
                .write_all(
                    b"HTTP/1.1 202 Accepted\r\nx-upstream-remove: secret\r\ncontent-length: 7\r\n\r\nreverse",
                )
                .await
                .expect("应可写回响应");
        });

        let proxy = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试代理入口应可监听");
        let proxy_addr = proxy.local_addr().expect("应可读取代理入口地址");
        let detail = http_reverse_detail(&format!("http://{upstream_addr}/base"));
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.expect("应收到客户端请求");
            handle_http_connection(stream, &detail, CancellationToken::new())
                .await
                .expect("HTTP reverse 应成功")
        });

        let mut client = TcpStream::connect(proxy_addr)
            .await
            .expect("应可连接测试代理入口");
        let body = br#"{"user":{"name":"old"}}"#;
        let request = format!(
            "POST /api/users?debug=1 HTTP/1.1\r\nhost: local.test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-remove: should-not-arrive\r\nconnection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body),
        );
        client
            .write_all(request.as_bytes())
            .await
            .expect("应可发送反向代理请求");
        let response = read_to_string(&mut client).await;

        assert!(response.starts_with("HTTP/1.1 202 Proxy Response"));
        assert!(response.contains("x-response-added: yes"));
        assert!(!response.contains("x-upstream-remove"));
        assert!(response.contains("reverse"));
        let events = proxy_task.await.expect("代理任务应完成");
        let event = events.first().expect("应记录 HTTP reverse 事件");
        assert_eq!(event.0, "http");
        assert_eq!(event.2, "POST");
        assert_eq!(event.5, Some(202));
        upstream_task.await.expect("上游任务应完成");
    }

    #[tokio::test]
    async fn connect_tunnels_bytes_between_client_and_target() {
        let target = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试 CONNECT 目标应可监听");
        let target_addr = target.local_addr().expect("应可读取 CONNECT 目标地址");
        let target_task = tokio::spawn(async move {
            let (mut socket, _) = target.accept().await.expect("应收到 CONNECT 连接");
            let mut buffer = [0_u8; 4];
            socket
                .read_exact(&mut buffer)
                .await
                .expect("应收到隧道数据");
            assert_eq!(&buffer, b"ping");
            socket.write_all(b"pong").await.expect("应可写回隧道数据");
        });

        let proxy = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试代理入口应可监听");
        let proxy_addr = proxy.local_addr().expect("应可读取代理入口地址");
        let detail = http_forward_detail();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.expect("应收到客户端 CONNECT");
            handle_http_connection(stream, &detail, CancellationToken::new())
                .await
                .expect("CONNECT 应成功")
        });

        let mut client = TcpStream::connect(proxy_addr)
            .await
            .expect("应可连接测试代理入口");
        let request = format!("CONNECT {target_addr} HTTP/1.1\r\nhost: {target_addr}\r\n\r\n");
        client
            .write_all(request.as_bytes())
            .await
            .expect("应可发送 CONNECT 请求");
        let mut handshake = Vec::new();
        read_until_marker(&mut client, &mut handshake, b"\r\n\r\n").await;
        assert!(
            String::from_utf8_lossy(&handshake).starts_with("HTTP/1.1 200 Connection Established")
        );

        client.write_all(b"ping").await.expect("应可写入隧道");
        let mut response = [0_u8; 4];
        client
            .read_exact(&mut response)
            .await
            .expect("应可读取隧道响应");
        assert_eq!(&response, b"pong");
        drop(client);

        let events = timeout(Duration::from_secs(2), proxy_task)
            .await
            .expect("CONNECT 代理任务应在客户端关闭后结束")
            .expect("代理任务应完成");
        let event = events.first().expect("应记录 CONNECT 事件");
        assert_eq!(event.0, "https");
        assert_eq!(event.2, "CONNECT");
        assert_eq!(event.5, Some(200));
        target_task.await.expect("目标任务应完成");
    }

    fn http_forward_detail() -> ServiceDetail {
        ServiceDetail {
            id: "svc-http-forward".to_string(),
            name: "HTTP Forward".to_string(),
            kind: ServiceKind::HttpForward,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port: 0,
            notes: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            http_reverse: None,
            http_forward: Some(HttpForwardConfig {
                allow_http: true,
                allow_connect: true,
                connect_timeout_ms: 5_000,
                idle_timeout_ms: 60_000,
            }),
            tcp_forward: None,
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: Vec::new(),
            body_rewrite_rules: Vec::new(),
        }
    }

    fn http_reverse_detail(target_url: &str) -> ServiceDetail {
        ServiceDetail {
            id: "svc-http-reverse".to_string(),
            name: "HTTP Reverse".to_string(),
            kind: ServiceKind::HttpReverse,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port: 0,
            notes: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            http_reverse: Some(HttpReverseConfig {
                target_url: target_url.to_string(),
                preserve_host: false,
                request_timeout_ms: 5_000,
                max_rewrite_body_bytes: 1024,
                skip_compressed_body: true,
            }),
            http_forward: None,
            tcp_forward: None,
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: vec![
                header_rule("request", "set", "x-added", Some("ok")),
                header_rule("request", "remove", "x-remove", None),
                header_rule("response", "set", "x-response-added", Some("yes")),
                header_rule("response", "remove", "x-upstream-remove", None),
            ],
            body_rewrite_rules: vec![body_rule("user.name", "\"new\"", "json")],
        }
    }

    fn header_rule(phase: &str, action: &str, name: &str, value: Option<&str>) -> HeaderRule {
        HeaderRule {
            id: format!("{phase}-{action}-{name}"),
            phase: phase.to_string(),
            action: action.to_string(),
            name: name.to_string(),
            value: value.map(ToOwned::to_owned),
            enabled: true,
            sort_order: 0,
        }
    }

    fn body_rule(path: &str, value_json: &str, body_type: &str) -> BodyRewriteRule {
        BodyRewriteRule {
            id: format!("{body_type}-{path}"),
            body_type: body_type.to_string(),
            path: path.to_string(),
            value_json: value_json.to_string(),
            enabled: true,
            sort_order: 0,
        }
    }

    async fn read_to_string(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .await
            .expect("应可读取完整响应");
        String::from_utf8(bytes).expect("HTTP 响应应是 UTF-8")
    }

    async fn read_until_headers(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        read_until_marker(stream, &mut bytes, b"\r\n\r\n").await;
        String::from_utf8(bytes).expect("HTTP 请求应是 UTF-8")
    }

    async fn read_http_message(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let headers = read_until_headers(stream).await;
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        let mut body = vec![0_u8; content_length];
        if content_length > 0 {
            stream
                .read_exact(&mut body)
                .await
                .expect("应可读取 HTTP body");
        }
        (headers, body)
    }

    async fn read_until_marker(stream: &mut TcpStream, bytes: &mut Vec<u8>, marker: &[u8]) {
        let mut one = [0_u8; 1];
        while !bytes.windows(marker.len()).any(|window| window == marker) {
            stream
                .read_exact(&mut one)
                .await
                .expect("读取 HTTP 数据不应提前结束");
            bytes.push(one[0]);
        }
    }
}
