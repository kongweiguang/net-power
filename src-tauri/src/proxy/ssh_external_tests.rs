//! @author kongweiguang
//! 真实 OpenSSH server 外部集成测试。默认跳过，设置 NET_POWER_SSH_TEST=1 后执行。

use super::*;
use std::net::{SocketAddr, TcpListener as StdTcpListener, TcpStream as StdTcpStream};
use std::thread;
use uuid::Uuid;

#[test]
fn external_ssh_socks5_dynamic_proxy_reaches_remote_target() {
    let Some(settings) = ExternalSshSettings::load("SSH SOCKS5 动态代理外部测试") else {
        return;
    };
    let target_host = std::env::var("NET_POWER_SSH_TEST_SOCKS_TARGET_HOST")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let target_port = env_u16("NET_POWER_SSH_TEST_SOCKS_TARGET_PORT", settings.port);
    let chain = vec![settings.password_connection_profile("insecure_skip", None)];

    let listener = StdTcpListener::bind("127.0.0.1:0").expect("SOCKS 本地测试入口应可监听");
    let listen_addr = listener.local_addr().expect("SOCKS 本地测试地址应可读取");
    let bridge_thread = thread::spawn(move || {
        let (server_stream, remote_addr) = listener.accept().expect("SOCKS 测试客户端应连接");
        run_ssh_socks_client_bridge(server_stream, remote_addr, &chain, CancellationToken::new())
    });

    let mut client = StdTcpStream::connect(listen_addr).expect("SOCKS 测试客户端应连接本地入口");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("应可设置读取超时");
    client
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("应可设置写入超时");
    write_socks5_connect(&mut client, &target_host, target_port);
    let mut reply = [0_u8; 10];
    client
        .read_exact(&mut reply)
        .expect("SOCKS CONNECT 成功响应应可读取");
    assert_eq!(reply[0], SOCKS_VERSION);
    assert_eq!(reply[1], SOCKS_REP_SUCCEEDED);

    let mut banner = [0_u8; 128];
    let size = client
        .read(&mut banner)
        .expect("应能通过 SOCKS5 读取远端 TCP banner");
    assert!(
        String::from_utf8_lossy(&banner[..size]).starts_with("SSH-"),
        "SOCKS5 应转发到远端 SSH 服务"
    );
    drop(client);

    let result = bridge_thread
        .join()
        .expect("SOCKS bridge 线程不应 panic")
        .expect("SOCKS bridge 应成功结束");
    assert_eq!(
        result.target_addr,
        format_host_port(&target_host, target_port)
    );
    assert!(
        result.metrics.bytes_out > 0,
        "SOCKS bridge 应记录远端返回字节"
    );
}

#[test]
fn external_ssh_remote_forward_reaches_local_target() {
    let Some(settings) = ExternalSshSettings::load("SSH remote forward 外部测试") else {
        return;
    };
    let remote_port = env_u16("NET_POWER_SSH_TEST_REMOTE_PORT", 29080);
    let remote_bind_host = std::env::var("NET_POWER_SSH_TEST_REMOTE_BIND_HOST")
        .unwrap_or_else(|_| "0.0.0.0".to_string());
    let remote_connect_host = std::env::var("NET_POWER_SSH_TEST_REMOTE_CONNECT_HOST")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let chain = vec![settings.password_connection_profile("insecure_skip", None)];
    let profile = chain[0].profile.clone();

    let target = StdTcpListener::bind("127.0.0.1:0").expect("remote forward 本地目标应可监听");
    let target_addr = target.local_addr().expect("本地目标地址应可读取");
    let target_thread = thread::spawn(move || {
        let (mut socket, _) = target.accept().expect("remote forward 应连接本地目标");
        let mut request = [0_u8; 11];
        socket
            .read_exact(&mut request)
            .expect("本地目标应收到 remote forward 数据");
        assert_eq!(&request, b"remote-ping");
        socket
            .write_all(b"remote-pong")
            .expect("本地目标应可写回响应");
    });

    let ssh_session = connect_authenticated_session_chain(&chain, CancellationToken::new())
        .expect("SSH 连接应成功");
    let session = ssh_session.session.clone();
    let (mut remote_listener, bound_port) = session
        .channel_forward_listen(
            remote_port,
            Some(&remote_bind_host),
            Some(REMOTE_FORWARD_QUEUE_MAX_SIZE),
        )
        .expect("远程监听应创建成功");
    assert_eq!(bound_port, remote_port);
    session.set_blocking(false);
    let accept_session = session.clone();
    let accept_thread = thread::spawn(move || {
        let started = Instant::now();
        loop {
            match remote_listener.accept() {
                Ok(channel) => {
                    return run_ssh_remote_channel_bridge(
                        channel,
                        &accept_session,
                        "127.0.0.1",
                        target_addr.port(),
                        &profile,
                        CancellationToken::new(),
                    );
                }
                Err(err)
                    if ssh_would_block(&err) && started.elapsed() < Duration::from_secs(10) =>
                {
                    thread::sleep(IDLE_SLEEP);
                }
                Err(err) => return Err(AppError::Ssh(err)),
            }
        }
    });

    let mut client = wait_for_tcp_connect(
        SocketAddr::new(
            remote_connect_host
                .parse()
                .expect("NET_POWER_SSH_TEST_REMOTE_CONNECT_HOST 必须是 IP"),
            remote_port,
        ),
        Duration::from_secs(10),
    )
    .expect("应可连接远程转发端口；Docker 需要发布该端口并启用 GatewayPorts");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("应可设置读取超时");
    client
        .write_all(b"remote-ping")
        .expect("应可写入远程转发端口");
    let mut response = [0_u8; 11];
    let response_result = client.read_exact(&mut response);
    drop(client);

    let bridge_result = accept_thread
        .join()
        .expect("remote forward accept 线程不应 panic");
    response_result.unwrap_or_else(|err| {
        panic!("应可读取 remote forward 响应，bridge 结果: {bridge_result:?}, 错误: {err}")
    });
    assert_eq!(&response, b"remote-pong");
    let metrics = bridge_result.expect("remote forward bridge 应成功");
    assert!(metrics.bytes_in > 0, "remote forward 应记录入口字节");
    assert!(metrics.bytes_out > 0, "remote forward 应记录出口字节");
    target_thread.join().expect("本地目标线程不应 panic");
    ssh_session.disconnect("net-power remote external test finished");
}

#[test]
fn external_ssh_jump_chain_reaches_target_through_openssh() {
    let Some(settings) = ExternalSshSettings::load("SSH 跳板链外部测试") else {
        return;
    };
    let inner_host = std::env::var("NET_POWER_SSH_TEST_JUMP_FINAL_HOST")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let inner_port = env_u16("NET_POWER_SSH_TEST_JUMP_FINAL_PORT", settings.target_port());
    let target_host =
        std::env::var("NET_POWER_SSH_TEST_JUMP_TARGET_HOST").unwrap_or_else(|_| inner_host.clone());
    let target_port = env_u16("NET_POWER_SSH_TEST_JUMP_TARGET_PORT", inner_port);
    let chain = vec![
        settings.password_connection_profile_with_target(
            "external-ssh-jump-1",
            "外部 SSH 一级跳板",
            &settings.host,
            settings.port,
            "insecure_skip",
            None,
        ),
        settings.password_connection_profile_with_target(
            "external-ssh-jump-2",
            "外部 SSH 二级跳板",
            &inner_host,
            inner_port,
            "insecure_skip",
            None,
        ),
        settings.password_connection_profile_with_target(
            "external-ssh-final",
            "外部 SSH 最终目标",
            &inner_host,
            inner_port,
            "insecure_skip",
            None,
        ),
    ];

    let ssh_session = connect_authenticated_session_chain(&chain, CancellationToken::new())
        .expect("SSH 跳板链应完成每一跳认证");
    let mut channel = ssh_session
        .session
        .channel_direct_tcpip(&target_host, target_port, Some(("127.0.0.1", 0)))
        .expect("应可通过最终 SSH session 建立 direct-tcpip");
    let mut banner = [0_u8; 128];
    let size = channel
        .read(&mut banner)
        .expect("应能通过跳板链读取目标 TCP banner");
    assert!(
        String::from_utf8_lossy(&banner[..size]).starts_with("SSH-"),
        "跳板链应能转发到目标 SSH 服务"
    );
    let _ = channel.close();
    ssh_session.disconnect("net-power jump external test finished");
}

#[test]
fn external_ssh_strict_known_hosts_rejects_changed_host_key() {
    let Some(settings) = ExternalSshSettings::load("SSH known_hosts strict 外部测试") else {
        return;
    };
    let known_hosts_path =
        std::env::temp_dir().join(format!("net-power-known-hosts-mismatch-{}", Uuid::new_v4()));
    write_mismatched_known_hosts(&settings, &known_hosts_path);
    let chain = vec![settings.password_connection_profile(
        "strict",
        Some(known_hosts_path.to_string_lossy().to_string()),
    )];

    let error = match connect_authenticated_session_chain(&chain, CancellationToken::new()) {
        Ok(session) => {
            session.disconnect("net-power strict known_hosts should have failed");
            panic!("strict 模式必须拒绝已变化的 host key");
        }
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("SSH host key 与 known_hosts 不匹配"),
        "错误信息应指向 host key mismatch，实际: {error}"
    );
    let _ = std::fs::remove_file(known_hosts_path);
}

#[derive(Debug, Clone)]
struct ExternalSshSettings {
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl ExternalSshSettings {
    fn load(test_name: &str) -> Option<Self> {
        if std::env::var("NET_POWER_SSH_TEST").as_deref() != Ok("1") {
            eprintln!("skip {test_name}: set NET_POWER_SSH_TEST=1 to enable");
            return None;
        }
        Some(Self {
            host: std::env::var("NET_POWER_SSH_TEST_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env_u16("NET_POWER_SSH_TEST_PORT", 2222),
            username: std::env::var("NET_POWER_SSH_TEST_USER")
                .unwrap_or_else(|_| "netpower".to_string()),
            password: std::env::var("NET_POWER_SSH_TEST_PASSWORD")
                .unwrap_or_else(|_| "netpower-pass".to_string()),
        })
    }

    fn password_connection_profile(
        &self,
        known_hosts_mode: &str,
        known_hosts_path: Option<String>,
    ) -> SshConnectionProfile {
        self.password_connection_profile_with_target(
            "external-ssh",
            "外部 SSH 集成测试",
            &self.host,
            self.port,
            known_hosts_mode,
            known_hosts_path,
        )
    }

    fn password_connection_profile_with_target(
        &self,
        id: &str,
        name: &str,
        host: &str,
        port: u16,
        known_hosts_mode: &str,
        known_hosts_path: Option<String>,
    ) -> SshConnectionProfile {
        SshConnectionProfile {
            profile: SshProfileRuntimeConfig {
                id: id.to_string(),
                name: name.to_string(),
                host: host.to_string(),
                port,
                username: self.username.clone(),
                auth_type: SshAuthType::Password,
                password_secret_id: None,
                private_key_path: None,
                private_key_passphrase_secret_id: None,
                known_hosts_mode: known_hosts_mode.to_string(),
                known_hosts_path,
                connect_timeout_ms: 10_000,
                keepalive_interval_ms: 1_000,
                jump_profile_id: None,
            },
            auth: SshAuthMaterial {
                password: Some(self.password.clone()),
                private_key_passphrase: None,
            },
        }
    }

    fn target_port(&self) -> u16 {
        env_u16("NET_POWER_SSH_TEST_TARGET_PORT", 2222)
    }
}

fn write_socks5_connect(client: &mut StdTcpStream, host: &str, port: u16) {
    client
        .write_all(&[SOCKS_VERSION, 1, SOCKS_METHOD_NO_AUTH])
        .expect("SOCKS greeting 应写入成功");
    let mut method_reply = [0_u8; 2];
    client
        .read_exact(&mut method_reply)
        .expect("SOCKS method reply 应可读取");
    assert_eq!(method_reply, [SOCKS_VERSION, SOCKS_METHOD_NO_AUTH]);

    let host_bytes = host.as_bytes();
    assert!(
        host_bytes.len() <= u8::MAX as usize,
        "SOCKS domain host 长度不能超过 255"
    );
    let mut request = vec![
        SOCKS_VERSION,
        SOCKS_CMD_CONNECT,
        0,
        SOCKS_ATYP_DOMAIN,
        host_bytes.len() as u8,
    ];
    request.extend_from_slice(host_bytes);
    request.extend_from_slice(&port.to_be_bytes());
    client
        .write_all(&request)
        .expect("SOCKS CONNECT 请求应写入成功");
}

fn write_mismatched_known_hosts(settings: &ExternalSshSettings, path: &Path) {
    let timeout = Duration::from_secs(10);
    let addr = resolve_ssh_addr(&settings.host, settings.port).expect("SSH 地址应可解析");
    let tcp = StdTcpStream::connect_timeout(&addr, timeout).expect("SSH TCP 应可连接");
    tcp.set_read_timeout(Some(timeout))
        .expect("应可设置读取超时");
    tcp.set_write_timeout(Some(timeout))
        .expect("应可设置写入超时");
    let mut session = Session::new().expect("SSH session 应可创建");
    session.set_tcp_stream(tcp);
    session.handshake().expect("SSH handshake 应成功");
    let (key, key_type) = session.host_key().expect("SSH 服务端应返回 host key");
    let mut mismatched_key = key.to_vec();
    let last = mismatched_key.last_mut().expect("SSH host key 不应为空");
    *last ^= 0x01;
    let mut known_hosts = session.known_hosts().expect("known_hosts 句柄应可创建");
    known_hosts
        .add(
            &known_host_name(&settings.host, settings.port),
            &mismatched_key,
            "net-power mismatch test",
            host_key_format(key_type),
        )
        .expect("mismatch known_hosts 记录应可写入内存");
    known_hosts
        .write_file(path, KnownHostFileKind::OpenSSH)
        .expect("mismatch known_hosts 文件应可写入");
    let _ = session.disconnect(None, "net-power mismatch known_hosts prepared", None);
}

fn wait_for_tcp_connect(addr: SocketAddr, timeout: Duration) -> std::io::Result<StdTcpStream> {
    let started = Instant::now();
    loop {
        match StdTcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
            Ok(stream) => return Ok(stream),
            Err(err) if started.elapsed() < timeout => {
                let last_error = err;
                thread::sleep(Duration::from_millis(100));
                if started.elapsed() >= timeout {
                    return Err(last_error);
                }
            }
            Err(err) => return Err(err),
        }
    }
}

fn env_u16(key: &str, default_value: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default_value)
}
