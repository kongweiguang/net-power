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

include!("http/tests.rs");
