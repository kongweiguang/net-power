//! @author kongweiguang
//! HTTP chunked 请求体回归测试，独立于生产 HTTP 代理实现文件。

use super::*;
use crate::models::{BodyRewriteRule, HeaderRule};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn http_reverse_decodes_chunked_json_body_before_rewrite() {
    let upstream = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试上游应可监听");
    let upstream_addr = upstream.local_addr().expect("应可读取上游地址");
    let upstream_task = tokio::spawn(async move {
        let (mut socket, _) = upstream.accept().await.expect("应收到上游请求");
        let (headers, body) = read_http_message(&mut socket).await;
        assert!(headers.starts_with("POST /api/chunked HTTP/1.1"));
        assert!(headers.contains("content-length: 27"));
        assert!(!headers.to_ascii_lowercase().contains("transfer-encoding"));
        let json: Value = serde_json::from_slice(&body).expect("上游收到的 body 应是 JSON");
        assert_eq!(json["user"]["name"], "chunked");
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
            .await
            .expect("应可写回响应");
    });

    let proxy = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试代理入口应可监听");
    let proxy_addr = proxy.local_addr().expect("应可读取代理入口地址");
    let detail = http_reverse_detail(&format!("http://{upstream_addr}"));
    let proxy_task = tokio::spawn(async move {
        let (stream, _) = proxy.accept().await.expect("应收到客户端请求");
        handle_http_connection(stream, &detail, CancellationToken::new())
            .await
            .expect("HTTP reverse chunked 请求应成功")
    });

    let mut client = TcpStream::connect(proxy_addr)
        .await
        .expect("应可连接测试代理入口");
    client
        .write_all(
            b"POST /api/chunked HTTP/1.1\r\n\
              host: local.test\r\n\
              content-type: application/json\r\n\
              transfer-encoding: chunked\r\n\
              connection: close\r\n\
              \r\n\
              8\r\n{\"user\":\r\n\
              f\r\n{\"name\":\"old\"}}\r\n\
              0\r\n\r\n",
        )
        .await
        .expect("应可发送 chunked 请求");
    let response = read_to_string(&mut client).await;

    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("ok"));
    let events = proxy_task.await.expect("代理任务应完成");
    let event = events.first().expect("应记录 chunked HTTP 事件");
    assert_eq!(event.2, "POST");
    assert_eq!(event.5, Some(200));
    upstream_task.await.expect("上游任务应完成");
}

fn http_reverse_detail(target_url: &str) -> ServiceDetail {
    ServiceDetail {
        id: "svc-http-reverse-chunked".to_string(),
        name: "HTTP Reverse Chunked".to_string(),
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
        header_rules: Vec::<HeaderRule>::new(),
        body_rewrite_rules: vec![BodyRewriteRule {
            id: "chunked-json-name".to_string(),
            body_type: "json".to_string(),
            path: "user.name".to_string(),
            value_json: "\"chunked\"".to_string(),
            enabled: true,
            sort_order: 0,
        }],
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

async fn read_until_headers(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut one = [0_u8; 1];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        stream
            .read_exact(&mut one)
            .await
            .expect("读取 HTTP header 不应提前结束");
        bytes.push(one[0]);
    }
    String::from_utf8(bytes).expect("HTTP header 应是 UTF-8")
}
