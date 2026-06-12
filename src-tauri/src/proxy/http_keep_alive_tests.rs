//! @author kongweiguang
//! HTTP/1.1 keep-alive 长连接复用回归测试。

use super::*;
use crate::models::ServiceDetail;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn http_reverse_reuses_client_connection_for_sequential_keep_alive_requests() {
    let upstream = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试上游应可监听");
    let upstream_addr = upstream.local_addr().expect("应可读取上游地址");
    let upstream_task = tokio::spawn(async move {
        for index in 1..=2 {
            let (mut socket, _) = upstream.accept().await.expect("应收到上游请求");
            let request = read_until_headers(&mut socket).await;
            assert!(request.starts_with(&format!("GET /api/keep/{index} HTTP/1.1")));
            let body = format!("ok-{index}");
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .expect("应可写回响应");
        }
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
            .expect("HTTP keep-alive 请求应成功")
    });

    let mut client = TcpStream::connect(proxy_addr)
        .await
        .expect("应可连接测试代理入口");
    client
        .write_all(
            b"GET /api/keep/1 HTTP/1.1\r\n\
              host: local.test\r\n\
              connection: keep-alive\r\n\
              \r\n",
        )
        .await
        .expect("应可发送第一次 keep-alive 请求");
    let first = read_http_response(&mut client).await;
    assert!(first.starts_with("HTTP/1.1 200 OK"));
    assert!(first
        .to_ascii_lowercase()
        .contains("connection: keep-alive"));
    assert!(first.ends_with("ok-1"));

    client
        .write_all(
            b"GET /api/keep/2 HTTP/1.1\r\n\
              host: local.test\r\n\
              connection: close\r\n\
              \r\n",
        )
        .await
        .expect("应可在同一连接发送第二次请求");
    let second = read_http_response(&mut client).await;
    assert!(second.starts_with("HTTP/1.1 200 OK"));
    assert!(second.to_ascii_lowercase().contains("connection: close"));
    assert!(second.ends_with("ok-2"));

    let events = proxy_task.await.expect("代理任务应完成");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].2, "GET");
    assert_eq!(events[0].4, "/api/keep/1");
    assert_eq!(events[0].5, Some(200));
    assert_eq!(events[1].2, "GET");
    assert_eq!(events[1].4, "/api/keep/2");
    assert_eq!(events[1].5, Some(200));
    upstream_task.await.expect("上游任务应完成");
}

fn http_reverse_detail(target_url: &str) -> ServiceDetail {
    ServiceDetail {
        id: "svc-http-reverse-keep-alive".to_string(),
        name: "HTTP Reverse Keep Alive".to_string(),
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
        header_rules: Vec::new(),
        body_rewrite_rules: Vec::new(),
    }
}

async fn read_http_response(stream: &mut TcpStream) -> String {
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
            .expect("应可读取响应 body");
    }
    format!(
        "{}{}",
        headers,
        String::from_utf8(body).expect("响应 body 应是 UTF-8")
    )
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
