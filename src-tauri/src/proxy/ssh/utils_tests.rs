// @author kongweiguang
// SSH 工具函数、事件日志和测试模块声明。

struct StaticPasswordPrompt {
    password: String,
}

impl KeyboardInteractivePrompt for StaticPasswordPrompt {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        _instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        prompts.iter().map(|_| self.password.clone()).collect()
    }
}

fn resolve_ssh_addr(host: &str, port: u16) -> AppResult<SocketAddr> {
    resolve_tcp_addr(host, port)
        .map_err(|_| AppError::InvalidInput(format!("无法解析 SSH 主机: {host}:{port}")))
}

fn resolve_tcp_addr(host: &str, port: u16) -> AppResult<SocketAddr> {
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| AppError::InvalidInput(format!("无法解析目标主机: {host}:{port}")))
}

fn format_host_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn known_hosts_path(profile: &SshProfileRuntimeConfig) -> AppResult<PathBuf> {
    if let Some(path) = profile
        .known_hosts_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| AppError::InvalidInput("无法定位用户主目录用于 known_hosts".to_string()))?;
    Ok(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

fn known_host_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn host_key_format(key_type: HostKeyType) -> KnownHostKeyFormat {
    match key_type {
        HostKeyType::Unknown => KnownHostKeyFormat::Unknown,
        HostKeyType::Rsa => KnownHostKeyFormat::SshRsa,
        HostKeyType::Dss => KnownHostKeyFormat::SshDss,
        HostKeyType::Ecdsa256 => KnownHostKeyFormat::Ecdsa256,
        HostKeyType::Ecdsa384 => KnownHostKeyFormat::Ecdsa384,
        HostKeyType::Ecdsa521 => KnownHostKeyFormat::Ecdsa521,
        HostKeyType::Ed25519 => KnownHostKeyFormat::Ed25519,
    }
}

fn would_block(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut
}

fn ssh_would_block(err: &ssh2::Error) -> bool {
    let message = err.message().to_ascii_lowercase();
    message.contains("would block") || message.contains("timed out")
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
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
#[path = "../ssh_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "../ssh_known_hosts_tests.rs"]
mod known_hosts_tests;

#[cfg(test)]
#[path = "../ssh_external_tests.rs"]
mod external_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SshProfileInput;
    use std::io::{Read, Write};
    use std::net::TcpListener as StdTcpListener;
    use std::thread;

    #[test]
    fn known_host_name_uses_openssh_port_format() {
        assert_eq!(known_host_name("example.com", 22), "example.com");
        assert_eq!(known_host_name("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn socks5_parser_accepts_domain_connect_request() {
        let listener = StdTcpListener::bind("127.0.0.1:0").expect("SOCKS 测试监听应成功");
        let addr = listener.local_addr().expect("SOCKS 测试地址应可读取");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("SOCKS 测试客户端应连接");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("应可设置读取超时");
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .expect("应可设置写入超时");
            read_socks5_connect_request(&mut stream)
        });

        let mut client = StdTcpStream::connect(addr).expect("SOCKS 测试客户端应连接");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("应可设置读取超时");
        client
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("应可设置写入超时");
        client
            .write_all(&[SOCKS_VERSION, 1, SOCKS_METHOD_NO_AUTH])
            .expect("SOCKS greeting 应写入成功");
        let mut method_reply = [0_u8; 2];
        client
            .read_exact(&mut method_reply)
            .expect("SOCKS method reply 应可读取");
        assert_eq!(method_reply, [SOCKS_VERSION, SOCKS_METHOD_NO_AUTH]);
        let domain = b"example.com";
        let mut request = vec![
            SOCKS_VERSION,
            SOCKS_CMD_CONNECT,
            0,
            SOCKS_ATYP_DOMAIN,
            domain.len() as u8,
        ];
        request.extend_from_slice(domain);
        request.extend_from_slice(&443_u16.to_be_bytes());
        client
            .write_all(&request)
            .expect("SOCKS CONNECT 请求应写入成功");

        let parsed = server
            .join()
            .expect("SOCKS parser 测试线程不应 panic")
            .expect("SOCKS CONNECT 请求应解析成功");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.target_addr(), "example.com:443");
    }

    #[test]
    fn socks5_parser_rejects_unsupported_command_with_reply() {
        let listener = StdTcpListener::bind("127.0.0.1:0").expect("SOCKS 测试监听应成功");
        let addr = listener.local_addr().expect("SOCKS 测试地址应可读取");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("SOCKS 测试客户端应连接");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("应可设置读取超时");
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .expect("应可设置写入超时");
            read_socks5_connect_request(&mut stream)
        });

        let mut client = StdTcpStream::connect(addr).expect("SOCKS 测试客户端应连接");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("应可设置读取超时");
        client
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("应可设置写入超时");
        client
            .write_all(&[SOCKS_VERSION, 1, SOCKS_METHOD_NO_AUTH])
            .expect("SOCKS greeting 应写入成功");
        let mut method_reply = [0_u8; 2];
        client
            .read_exact(&mut method_reply)
            .expect("SOCKS method reply 应可读取");
        assert_eq!(method_reply, [SOCKS_VERSION, SOCKS_METHOD_NO_AUTH]);
        client
            .write_all(&[SOCKS_VERSION, 2, 0, SOCKS_ATYP_IPV4, 127, 0, 0, 1, 0, 80])
            .expect("SOCKS BIND 请求应写入成功");
        let mut reply = [0_u8; 10];
        client
            .read_exact(&mut reply)
            .expect("SOCKS failure reply 应可读取");

        let parsed = server.join().expect("SOCKS parser 测试线程不应 panic");
        assert!(matches!(parsed, Err(AppError::Unsupported(_))));
        assert_eq!(reply[0], SOCKS_VERSION);
        assert_eq!(reply[1], SOCKS_REP_COMMAND_NOT_SUPPORTED);
    }

    #[test]
    fn external_ssh_password_auth_and_direct_tcpip_bridge() {
        if std::env::var("NET_POWER_SSH_TEST").as_deref() != Ok("1") {
            eprintln!("skip external SSH test: set NET_POWER_SSH_TEST=1 to enable");
            return;
        }

        let host =
            std::env::var("NET_POWER_SSH_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env_u16("NET_POWER_SSH_TEST_PORT", 2222);
        let username =
            std::env::var("NET_POWER_SSH_TEST_USER").unwrap_or_else(|_| "netpower".to_string());
        let password = std::env::var("NET_POWER_SSH_TEST_PASSWORD")
            .unwrap_or_else(|_| "netpower-pass".to_string());
        let target_host = std::env::var("NET_POWER_SSH_TEST_TARGET_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        let target_port = env_u16("NET_POWER_SSH_TEST_TARGET_PORT", port);

        let db = Arc::new(Database::in_memory().expect("内存数据库应初始化成功"));
        let password_secret =
            crypto::store_secret(&db, "external ssh password", "ssh_password", &password)
                .expect("密码 secret 应加密写入");
        let profile = db
            .create_ssh_profile(
                &SshProfileInput {
                    name: "外部 SSH 集成测试".to_string(),
                    host,
                    port,
                    username,
                    auth_type: SshAuthType::Password,
                    password: None,
                    private_key_path: None,
                    private_key_passphrase: None,
                    known_hosts_mode: "insecure_skip".to_string(),
                    known_hosts_path: None,
                    connect_timeout_ms: 10_000,
                    keepalive_interval_ms: 1_000,
                    jump_profile_id: None,
                },
                Some(&password_secret),
                None,
            )
            .expect("SSH Profile 应创建成功");

        let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime 应创建成功");
        let test_result = runtime
            .block_on(test_ssh_profile_connection(
                Arc::clone(&db),
                profile.id.clone(),
            ))
            .expect("SSH 测试命令应返回结果");
        assert!(test_result.ok, "SSH 认证应成功: {}", test_result.message);

        let profile_runtime = db
            .get_ssh_profile_runtime_config(&profile.id)
            .expect("运行时 SSH Profile 应可读取");
        let auth = SshAuthMaterial::load(&db, &profile_runtime).expect("认证材料应可读取");
        let listener = StdTcpListener::bind("127.0.0.1:0").expect("本地桥接监听应成功");
        let listen_addr = listener.local_addr().expect("本地桥接地址应可读取");
        let bridge_chain = vec![SshConnectionProfile {
            profile: profile_runtime.clone(),
            auth: auth.clone(),
        }];
        let bridge_thread = thread::spawn(move || {
            let (server_stream, remote_addr) = listener.accept().expect("测试客户端应连接");
            run_ssh_client_bridge(
                server_stream,
                remote_addr,
                &target_host,
                target_port,
                &bridge_chain,
                CancellationToken::new(),
            )
        });

        let mut client = StdTcpStream::connect(listen_addr).expect("测试客户端应连接本地桥接端口");
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("应可设置读取超时");
        let mut buffer = [0_u8; 128];
        let size = client
            .read(&mut buffer)
            .expect("应能通过 SSH direct-tcpip 读取远端 banner");
        let banner = String::from_utf8_lossy(&buffer[..size]);
        assert!(
            banner.starts_with("SSH-"),
            "应读取到 SSH banner，实际: {banner}"
        );
        drop(client);
        let metrics = bridge_thread
            .join()
            .expect("桥接线程不应 panic")
            .expect("SSH direct-tcpip 桥接应成功");
        assert!(metrics.bytes_out > 0, "桥接应记录远端到本地字节数");
    }

    #[test]
    fn external_ssh_private_key_passphrase_auth() {
        if std::env::var("NET_POWER_SSH_TEST").as_deref() != Ok("1") {
            eprintln!("skip external SSH private-key test: set NET_POWER_SSH_TEST=1 to enable");
            return;
        }

        let key_path = match std::env::var("NET_POWER_SSH_TEST_PRIVATE_KEY_PATH") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                eprintln!(
                    "skip external SSH private-key test: set NET_POWER_SSH_TEST_PRIVATE_KEY_PATH"
                );
                return;
            }
        };
        let passphrase = match std::env::var("NET_POWER_SSH_TEST_PRIVATE_KEY_PASSPHRASE") {
            Ok(value) => value,
            _ => {
                eprintln!(
                    "skip external SSH private-key test: set NET_POWER_SSH_TEST_PRIVATE_KEY_PASSPHRASE"
                );
                return;
            }
        };

        let host =
            std::env::var("NET_POWER_SSH_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env_u16("NET_POWER_SSH_TEST_PORT", 2222);
        let username =
            std::env::var("NET_POWER_SSH_TEST_USER").unwrap_or_else(|_| "netpower".to_string());

        let db = Arc::new(Database::in_memory().expect("内存数据库应初始化成功"));
        let passphrase_secret = crypto::store_secret(
            &db,
            "external ssh key passphrase",
            "ssh_key_passphrase",
            &passphrase,
        )
        .expect("私钥 passphrase secret 应加密写入");
        let profile = db
            .create_ssh_profile(
                &SshProfileInput {
                    name: "外部 SSH 私钥集成测试".to_string(),
                    host,
                    port,
                    username,
                    auth_type: SshAuthType::PrivateKey,
                    password: None,
                    private_key_path: Some(key_path),
                    private_key_passphrase: None,
                    known_hosts_mode: "insecure_skip".to_string(),
                    known_hosts_path: None,
                    connect_timeout_ms: 10_000,
                    keepalive_interval_ms: 1_000,
                    jump_profile_id: None,
                },
                None,
                Some(&passphrase_secret),
            )
            .expect("SSH 私钥 Profile 应创建成功");

        let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime 应创建成功");
        let test_result = runtime
            .block_on(test_ssh_profile_connection(
                Arc::clone(&db),
                profile.id.clone(),
            ))
            .expect("SSH 私钥测试命令应返回结果");
        assert!(
            test_result.ok,
            "SSH passphrase 私钥认证应成功: {}",
            test_result.message
        );
    }

    fn env_u16(key: &str, default_value: u16) -> u16 {
        std::env::var(key)
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default_value)
    }
}
