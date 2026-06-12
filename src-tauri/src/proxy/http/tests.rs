// @author kongweiguang
// HTTP 代理单元测试和外置测试模块声明。

#[cfg(test)]
#[path = "../http_chunked_tests.rs"]
mod chunked_tests;

#[cfg(test)]
#[path = "../http_keep_alive_tests.rs"]
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
