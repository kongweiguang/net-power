//! @author kongweiguang
//! SSH 隧道运行时。local/SOCKS 每个本地客户端使用独立 SSH session；remote forward 使用单个远端监听 session 接收通道。

use crate::crypto;
use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::manager::ServiceCounters;
use crate::models::{
    ConnectionEventPayload, ServiceDetail, ServiceEventPayload, SshAuthType,
    SshProfileRuntimeConfig, TestResult,
};
use ssh2::{
    CheckResult, HostKeyType, KeyboardInteractivePrompt, KnownHostFileKind, KnownHostKeyFormat,
    Prompt, Session,
};
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{
    Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener as StdTcpListener, TcpStream as StdTcpStream,
    ToSocketAddrs,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const BRIDGE_BUFFER_SIZE: usize = 16 * 1024;
const MAX_PENDING_BYTES: usize = 2 * 1024 * 1024;
const IDLE_SLEEP: Duration = Duration::from_millis(5);
const SOCKS_VERSION: u8 = 0x05;
const SOCKS_METHOD_NO_AUTH: u8 = 0x00;
const SOCKS_METHOD_NO_ACCEPTABLE: u8 = 0xff;
const SOCKS_CMD_CONNECT: u8 = 0x01;
const SOCKS_ATYP_IPV4: u8 = 0x01;
const SOCKS_ATYP_DOMAIN: u8 = 0x03;
const SOCKS_ATYP_IPV6: u8 = 0x04;
const SOCKS_REP_SUCCEEDED: u8 = 0x00;
const SOCKS_REP_GENERAL_FAILURE: u8 = 0x01;
const SOCKS_REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
const SOCKS_REP_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;
const REMOTE_FORWARD_QUEUE_MAX_SIZE: u32 = 128;
const SSH_RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
const SSH_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

/// 启动 SSH local forward 服务。
pub async fn run_ssh_local_forward<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let cfg = detail
        .ssh_tunnel
        .clone()
        .ok_or_else(|| AppError::InvalidInput("SSH 隧道缺少配置".to_string()))?;
    if cfg.tunnel_type != "local" {
        return Err(AppError::Unsupported(
            "该运行入口仅支持 SSH local forward".to_string(),
        ));
    }
    let target_host = cfg
        .target_host
        .clone()
        .ok_or_else(|| AppError::InvalidInput("SSH local 缺少目标 host".to_string()))?;
    let target_port = cfg
        .target_port
        .ok_or_else(|| AppError::InvalidInput("SSH local 缺少目标 port".to_string()))?;
    let ssh_chain = load_ssh_connection_chain(&db, &cfg.ssh_profile_id)?;
    let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    log_event(
        &db,
        &app,
        Some(&detail.id),
        "info",
        &format!("SSH local forward 已监听 {listen_addr}"),
    )?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                log_event(&db, &app, Some(&detail.id), "info", "SSH local forward 已停止")?;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (client, remote_addr) = accepted?;
                let std_client = client.into_std()?;
                let child_cancel = cancel.child_token();
                let db_for_task = Arc::clone(&db);
                let app_for_task = app.clone();
                let counters_for_task = Arc::clone(&counters);
                let service_id = detail.id.clone();
                let target_host = target_host.clone();
                let target_addr_label = format!("{target_host}:{target_port}");
                let chain = ssh_chain.clone();
                tokio::spawn(async move {
                    counters_for_task.active_connections.fetch_add(1, Ordering::Relaxed);
                    counters_for_task.total_connections.fetch_add(1, Ordering::Relaxed);
                    let started = Instant::now();
                    let bridge_result = tokio::task::spawn_blocking(move || {
                        run_ssh_client_bridge(
                            std_client,
                            remote_addr,
                            &target_host,
                            target_port,
                            &chain,
                            child_cancel,
                        )
                    })
                    .await
                    .map_err(|err| AppError::Message(format!("SSH 隧道任务异常: {err}")))
                    .and_then(|result| result);
                    counters_for_task.active_connections.fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    let (bytes_in, bytes_out, error) = match bridge_result {
                        Ok(metrics) => (metrics.bytes_in, metrics.bytes_out, String::new()),
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(&db_for_task, &app_for_task, Some(&service_id), "error", &message);
                            (0, 0, message)
                        }
                    };
                    counters_for_task.bytes_in.fetch_add(bytes_in, Ordering::Relaxed);
                    counters_for_task.bytes_out.fetch_add(bytes_out, Ordering::Relaxed);
                    let _ = db_for_task.insert_connection_event(
                        &service_id,
                        "ssh",
                        &remote_addr.to_string(),
                        &target_addr_label,
                        "closed",
                        "",
                        "",
                        "",
                        None,
                        bytes_in,
                        bytes_out,
                        duration_ms,
                        &error,
                    );
                    let _ = app_for_task.emit(
                        "service://connection",
                        ConnectionEventPayload {
                            service_id: service_id.clone(),
                            protocol: "ssh".to_string(),
                            event_type: "closed".to_string(),
                            target_addr: target_addr_label.clone(),
                        },
                    );
                });
            }
        }
    }
}

/// 启动 SSH SOCKS5 动态代理服务。
pub async fn run_ssh_socks_forward<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let cfg = detail
        .ssh_tunnel
        .clone()
        .ok_or_else(|| AppError::InvalidInput("SSH SOCKS 缺少配置".to_string()))?;
    if cfg.tunnel_type != "socks" {
        return Err(AppError::InvalidInput(
            "SSH SOCKS 隧道类型必须是 socks".to_string(),
        ));
    }
    let ssh_chain = load_ssh_connection_chain(&db, &cfg.ssh_profile_id)?;
    let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    log_event(
        &db,
        &app,
        Some(&detail.id),
        "info",
        &format!("SSH SOCKS5 动态代理已监听 {listen_addr}"),
    )?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                log_event(&db, &app, Some(&detail.id), "info", "SSH SOCKS5 动态代理已停止")?;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (client, remote_addr) = accepted?;
                let std_client = client.into_std()?;
                let child_cancel = cancel.child_token();
                let db_for_task = Arc::clone(&db);
                let app_for_task = app.clone();
                let counters_for_task = Arc::clone(&counters);
                let service_id = detail.id.clone();
                let chain = ssh_chain.clone();
                tokio::spawn(async move {
                    counters_for_task.active_connections.fetch_add(1, Ordering::Relaxed);
                    counters_for_task.total_connections.fetch_add(1, Ordering::Relaxed);
                    let started = Instant::now();
                    let bridge_result = tokio::task::spawn_blocking(move || {
                        run_ssh_socks_client_bridge(
                            std_client,
                            remote_addr,
                            &chain,
                            child_cancel,
                        )
                    })
                    .await
                    .map_err(|err| AppError::Message(format!("SSH SOCKS 任务异常: {err}")))
                    .and_then(|result| result);
                    counters_for_task.active_connections.fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    let (target_addr, bytes_in, bytes_out, error) = match bridge_result {
                        Ok(result) => (
                            result.target_addr,
                            result.metrics.bytes_in,
                            result.metrics.bytes_out,
                            String::new(),
                        ),
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(&db_for_task, &app_for_task, Some(&service_id), "error", &message);
                            (String::new(), 0, 0, message)
                        }
                    };
                    counters_for_task.bytes_in.fetch_add(bytes_in, Ordering::Relaxed);
                    counters_for_task.bytes_out.fetch_add(bytes_out, Ordering::Relaxed);
                    let _ = db_for_task.insert_connection_event(
                        &service_id,
                        "ssh",
                        &remote_addr.to_string(),
                        &target_addr,
                        "closed",
                        "SOCKS5 CONNECT",
                        "",
                        "",
                        None,
                        bytes_in,
                        bytes_out,
                        duration_ms,
                        &error,
                    );
                    let _ = app_for_task.emit(
                        "service://connection",
                        ConnectionEventPayload {
                            service_id: service_id.clone(),
                            protocol: "ssh".to_string(),
                            event_type: "closed".to_string(),
                            target_addr,
                        },
                    );
                });
            }
        }
    }
}

/// 启动 SSH remote forward 服务。
pub async fn run_ssh_remote_forward<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let cfg = detail
        .ssh_tunnel
        .clone()
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少配置".to_string()))?;
    if cfg.tunnel_type != "remote" {
        return Err(AppError::InvalidInput(
            "SSH remote 隧道类型必须是 remote".to_string(),
        ));
    }
    let spec = RemoteForwardSpec {
        service_id: detail.id.clone(),
        remote_bind_host: cfg
            .remote_bind_host
            .clone()
            .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少远程绑定 host".to_string()))?,
        remote_bind_port: cfg
            .remote_bind_port
            .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少远程绑定端口".to_string()))?,
        target_host: cfg
            .target_host
            .clone()
            .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少目标 host".to_string()))?,
        target_port: cfg
            .target_port
            .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少目标 port".to_string()))?,
    };
    let ssh_chain = load_ssh_connection_chain(&db, &cfg.ssh_profile_id)?;
    tokio::task::spawn_blocking(move || {
        run_ssh_remote_reconnect_loop(spec, ssh_chain, cancel, counters, db, app)
    })
    .await
    .map_err(|err| AppError::Message(format!("SSH remote 任务异常: {err}")))?
}

/// 对 SSH profile 做真实握手和认证测试。
pub async fn test_ssh_profile_connection(
    db: Arc<Database>,
    profile_id: String,
) -> AppResult<TestResult> {
    let started = Instant::now();
    let chain = load_ssh_connection_chain(&db, &profile_id)?;
    let result = tokio::task::spawn_blocking(move || {
        connect_authenticated_session_chain(&chain, CancellationToken::new()).map(|session| {
            let hop_count = chain.len();
            session.disconnect("net-power test finished");
            if hop_count > 1 {
                format!("SSH 握手和认证成功，已通过 {} 级跳板", hop_count - 1)
            } else {
                "SSH 握手和认证成功".to_string()
            }
        })
    })
    .await
    .map_err(|err| AppError::Message(format!("SSH 测试任务异常: {err}")))?;
    Ok(match result {
        Ok(message) => TestResult {
            ok: true,
            message,
            duration_ms: elapsed_ms(started),
        },
        Err(err) => TestResult {
            ok: false,
            message: err.to_string(),
            duration_ms: elapsed_ms(started),
        },
    })
}

#[derive(Debug, Clone)]
struct SshAuthMaterial {
    password: Option<String>,
    private_key_passphrase: Option<String>,
}

impl SshAuthMaterial {
    fn load(db: &Database, profile: &SshProfileRuntimeConfig) -> AppResult<Self> {
        let password = match (&profile.auth_type, &profile.password_secret_id) {
            (SshAuthType::Password, Some(secret_id)) => Some(crypto::load_secret(db, secret_id)?),
            (SshAuthType::Password, None) => {
                return Err(AppError::InvalidInput(format!(
                    "SSH Profile {}({}) 缺少密码 secret",
                    profile.name, profile.id
                )));
            }
            _ => None,
        };
        let private_key_passphrase = match &profile.private_key_passphrase_secret_id {
            Some(secret_id) => Some(crypto::load_secret(db, secret_id)?),
            None => None,
        };
        Ok(Self {
            password,
            private_key_passphrase,
        })
    }
}

#[derive(Debug, Clone)]
struct SshConnectionProfile {
    profile: SshProfileRuntimeConfig,
    auth: SshAuthMaterial,
}

struct JumpBridge {
    cancel: CancellationToken,
    handle: std::thread::JoinHandle<AppResult<BridgeMetrics>>,
}

struct SshSessionHandle {
    session: Session,
    profile: SshProfileRuntimeConfig,
    jump_bridges: Vec<JumpBridge>,
    upstream_sessions: Vec<Session>,
}

impl SshSessionHandle {
    fn direct(session: Session, profile: SshProfileRuntimeConfig) -> Self {
        Self {
            session,
            profile,
            jump_bridges: Vec::new(),
            upstream_sessions: Vec::new(),
        }
    }

    fn disconnect(self, reason: &str) {
        let _ = self.session.disconnect(None, reason, None);
        for session in self.upstream_sessions.iter().rev() {
            let _ = session.disconnect(None, reason, None);
        }
        for bridge in self.jump_bridges {
            bridge.cancel.cancel();
            let _ = bridge.handle.join();
        }
    }
}

#[derive(Debug, Default)]
struct BridgeMetrics {
    bytes_in: u64,
    bytes_out: u64,
}

#[derive(Debug)]
struct SocksConnectRequest {
    host: String,
    port: u16,
}

impl SocksConnectRequest {
    fn target_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug)]
struct SocksBridgeResult {
    target_addr: String,
    metrics: BridgeMetrics,
}

#[derive(Debug, Clone)]
struct RemoteForwardSpec {
    service_id: String,
    remote_bind_host: String,
    remote_bind_port: u16,
    target_host: String,
    target_port: u16,
}

impl RemoteForwardSpec {
    fn remote_bind_addr(&self) -> String {
        format_host_port(&self.remote_bind_host, self.remote_bind_port)
    }

    fn target_addr(&self) -> String {
        format_host_port(&self.target_host, self.target_port)
    }

    fn log_target_addr(&self) -> String {
        format!("{} -> {}", self.remote_bind_addr(), self.target_addr())
    }
}

fn run_ssh_remote_reconnect_loop<R: Runtime>(
    spec: RemoteForwardSpec,
    chain: Vec<SshConnectionProfile>,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let mut attempt = 0_u32;
    while !cancel.is_cancelled() {
        let result = run_ssh_remote_accept_loop(
            spec.clone(),
            chain.clone(),
            cancel.child_token(),
            Arc::clone(&counters),
            Arc::clone(&db),
            app.clone(),
        );
        match result {
            Ok(()) => return Ok(()),
            Err(_) if cancel.is_cancelled() => return Ok(()),
            Err(err) => {
                let delay = ssh_reconnect_delay(attempt);
                attempt = attempt.saturating_add(1);
                log_event(
                    &db,
                    &app,
                    Some(&spec.service_id),
                    "warn",
                    &format!(
                        "SSH remote forward 连接中断: {err}，将在 {} 秒后重连",
                        delay.as_secs()
                    ),
                )?;
                if sleep_until_cancelled(&cancel, delay) {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn run_ssh_remote_accept_loop<R: Runtime>(
    spec: RemoteForwardSpec,
    chain: Vec<SshConnectionProfile>,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let final_profile = chain
        .last()
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少连接链".to_string()))?
        .profile
        .clone();
    let ssh_session = connect_authenticated_session_chain(&chain, cancel.child_token())?;
    let session = ssh_session.session.clone();
    let (mut listener, bound_port) = session.channel_forward_listen(
        spec.remote_bind_port,
        Some(&spec.remote_bind_host),
        Some(REMOTE_FORWARD_QUEUE_MAX_SIZE),
    )?;
    let mut spec = spec;
    spec.remote_bind_port = bound_port;
    session.set_blocking(false);
    log_event(
        &db,
        &app,
        Some(&spec.service_id),
        "info",
        &format!(
            "SSH remote forward 已在远端监听 {}，转发到 {}",
            spec.remote_bind_addr(),
            spec.target_addr()
        ),
    )?;

    let keepalive_interval = (final_profile.keepalive_interval_ms > 0)
        .then(|| Duration::from_millis(final_profile.keepalive_interval_ms));
    let mut last_keepalive = Instant::now();
    while !cancel.is_cancelled() {
        match listener.accept() {
            Ok(channel) => {
                counters.active_connections.fetch_add(1, Ordering::Relaxed);
                counters.total_connections.fetch_add(1, Ordering::Relaxed);
                let child_cancel = cancel.child_token();
                let session_for_channel = session.clone();
                let db_for_thread = Arc::clone(&db);
                let app_for_thread = app.clone();
                let counters_for_thread = Arc::clone(&counters);
                let profile_for_thread = final_profile.clone();
                let spec_for_thread = spec.clone();
                std::thread::spawn(move || {
                    let started = Instant::now();
                    let bridge_result = run_ssh_remote_channel_bridge(
                        channel,
                        &session_for_channel,
                        &spec_for_thread.target_host,
                        spec_for_thread.target_port,
                        &profile_for_thread,
                        child_cancel,
                    );
                    counters_for_thread
                        .active_connections
                        .fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = elapsed_ms(started);
                    let (bytes_in, bytes_out, error) = match bridge_result {
                        Ok(metrics) => (metrics.bytes_in, metrics.bytes_out, String::new()),
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(
                                &db_for_thread,
                                &app_for_thread,
                                Some(&spec_for_thread.service_id),
                                "error",
                                &message,
                            );
                            (0, 0, message)
                        }
                    };
                    counters_for_thread
                        .bytes_in
                        .fetch_add(bytes_in, Ordering::Relaxed);
                    counters_for_thread
                        .bytes_out
                        .fetch_add(bytes_out, Ordering::Relaxed);
                    let remote_bind_addr = spec_for_thread.remote_bind_addr();
                    let target_addr = spec_for_thread.log_target_addr();
                    let _ = db_for_thread.insert_connection_event(
                        &spec_for_thread.service_id,
                        "ssh",
                        &remote_bind_addr,
                        &target_addr,
                        "closed",
                        "REMOTE",
                        "",
                        "",
                        None,
                        bytes_in,
                        bytes_out,
                        duration_ms,
                        &error,
                    );
                    let _ = app_for_thread.emit(
                        "service://connection",
                        ConnectionEventPayload {
                            service_id: spec_for_thread.service_id.clone(),
                            protocol: "ssh".to_string(),
                            event_type: "closed".to_string(),
                            target_addr,
                        },
                    );
                });
            }
            Err(err) if ssh_would_block(&err) => {
                if keepalive_interval.is_some_and(|interval| last_keepalive.elapsed() >= interval) {
                    match session.keepalive_send() {
                        Ok(_) => last_keepalive = Instant::now(),
                        Err(err) if ssh_would_block(&err) => {}
                        Err(err) => {
                            ssh_session.disconnect("net-power remote forward failed");
                            return Err(AppError::Ssh(err));
                        }
                    }
                }
                std::thread::sleep(IDLE_SLEEP);
            }
            Err(err) => {
                ssh_session.disconnect("net-power remote forward failed");
                return Err(AppError::Ssh(err));
            }
        }
    }

    drop(listener);
    ssh_session.disconnect("net-power remote forward stopped");
    log_event(
        &db,
        &app,
        Some(&spec.service_id),
        "info",
        "SSH remote forward 已停止",
    )?;
    Ok(())
}

fn ssh_reconnect_delay(attempt: u32) -> Duration {
    let factor = 1_u32.checked_shl(attempt.min(5)).unwrap_or(32);
    SSH_RECONNECT_INITIAL_DELAY
        .saturating_mul(factor)
        .min(SSH_RECONNECT_MAX_DELAY)
}

fn sleep_until_cancelled(cancel: &CancellationToken, delay: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < delay {
        if cancel.is_cancelled() {
            return true;
        }
        let remaining = delay.saturating_sub(started.elapsed());
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    cancel.is_cancelled()
}

fn run_ssh_client_bridge(
    client: StdTcpStream,
    remote_addr: SocketAddr,
    target_host: &str,
    target_port: u16,
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let ssh_session = connect_authenticated_session_chain(chain, cancel.child_token())?;
    let session = ssh_session.session.clone();
    let mut channel = session.channel_direct_tcpip(
        target_host,
        target_port,
        Some((remote_addr.ip().to_string().as_str(), remote_addr.port())),
    )?;
    session.set_blocking(false);
    client.set_nonblocking(true)?;
    let mut metrics =
        bridge_nonblocking(client, &session, &mut channel, &ssh_session.profile, cancel)?;
    let _ = channel.close();
    ssh_session.disconnect("net-power tunnel closed");
    // direct-tcpip 中 local -> SSH 是入口流量，SSH -> local 是出口流量。
    if metrics.bytes_in == 0 && metrics.bytes_out == 0 && channel.eof() {
        metrics.bytes_out = 0;
    }
    Ok(metrics)
}

fn run_ssh_remote_channel_bridge(
    mut channel: ssh2::Channel,
    session: &Session,
    target_host: &str,
    target_port: u16,
    profile: &SshProfileRuntimeConfig,
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    let target_addr = resolve_tcp_addr(target_host, target_port)?;
    let local_target = StdTcpStream::connect_timeout(&target_addr, timeout)?;
    local_target.set_read_timeout(Some(timeout))?;
    local_target.set_write_timeout(Some(timeout))?;
    local_target.set_nonblocking(true)?;
    let mut metrics = bridge_nonblocking(local_target, session, &mut channel, profile, cancel)?;
    let _ = channel.close();
    // remote forward 的入口流量方向是 SSH channel -> 本地目标，和 bridge_nonblocking 的默认方向相反。
    std::mem::swap(&mut metrics.bytes_in, &mut metrics.bytes_out);
    Ok(metrics)
}

fn run_ssh_socks_client_bridge(
    mut client: StdTcpStream,
    remote_addr: SocketAddr,
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<SocksBridgeResult> {
    let profile = chain
        .last()
        .ok_or_else(|| AppError::InvalidInput("SSH SOCKS 缺少连接链".to_string()))?
        .profile
        .clone();
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    client.set_nonblocking(false)?;
    client.set_read_timeout(Some(timeout))?;
    client.set_write_timeout(Some(timeout))?;

    let request = read_socks5_connect_request(&mut client)?;
    let target_addr = request.target_addr();
    if cancel.is_cancelled() {
        let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::Message("SOCKS5 连接已取消".to_string()));
    }

    let ssh_session = match connect_authenticated_session_chain(chain, cancel.child_token()) {
        Ok(session) => session,
        Err(err) => {
            let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
            return Err(err);
        }
    };
    let session = ssh_session.session.clone();
    let mut channel = match session.channel_direct_tcpip(
        &request.host,
        request.port,
        Some((remote_addr.ip().to_string().as_str(), remote_addr.port())),
    ) {
        Ok(channel) => channel,
        Err(err) => {
            let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
            ssh_session.disconnect("net-power socks target failed");
            return Err(AppError::Ssh(err));
        }
    };
    send_socks5_reply(&mut client, SOCKS_REP_SUCCEEDED)?;

    session.set_blocking(false);
    client.set_nonblocking(true)?;
    let metrics = bridge_nonblocking(client, &session, &mut channel, &profile, cancel)?;
    let _ = channel.close();
    ssh_session.disconnect("net-power socks tunnel closed");
    Ok(SocksBridgeResult {
        target_addr,
        metrics,
    })
}

fn read_socks5_connect_request(client: &mut StdTcpStream) -> AppResult<SocksConnectRequest> {
    let mut greeting = [0_u8; 2];
    client.read_exact(&mut greeting)?;
    if greeting[0] != SOCKS_VERSION {
        return Err(AppError::InvalidInput(format!(
            "SOCKS5 版本无效: {}",
            greeting[0]
        )));
    }

    let mut methods = vec![0_u8; greeting[1] as usize];
    client.read_exact(&mut methods)?;
    if !methods.contains(&SOCKS_METHOD_NO_AUTH) {
        client.write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_ACCEPTABLE])?;
        return Err(AppError::InvalidInput(
            "SOCKS5 客户端不支持 no-auth".to_string(),
        ));
    }
    client.write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_AUTH])?;

    let mut header = [0_u8; 4];
    client.read_exact(&mut header)?;
    if header[0] != SOCKS_VERSION {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput(format!(
            "SOCKS5 请求版本无效: {}",
            header[0]
        )));
    }
    if header[1] != SOCKS_CMD_CONNECT {
        let _ = send_socks5_reply(client, SOCKS_REP_COMMAND_NOT_SUPPORTED);
        return Err(AppError::Unsupported(
            "SOCKS5 当前仅支持 CONNECT 命令".to_string(),
        ));
    }
    if header[2] != 0 {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput("SOCKS5 RSV 字段无效".to_string()));
    }

    let host = match header[3] {
        SOCKS_ATYP_IPV4 => {
            let mut addr = [0_u8; 4];
            client.read_exact(&mut addr)?;
            Ipv4Addr::from(addr).to_string()
        }
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            client.read_exact(&mut len)?;
            if len[0] == 0 {
                let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
                return Err(AppError::InvalidInput("SOCKS5 域名不能为空".to_string()));
            }
            let mut domain = vec![0_u8; len[0] as usize];
            client.read_exact(&mut domain)?;
            String::from_utf8(domain)
                .map_err(|_| AppError::InvalidInput("SOCKS5 域名不是 UTF-8".to_string()))?
        }
        SOCKS_ATYP_IPV6 => {
            let mut addr = [0_u8; 16];
            client.read_exact(&mut addr)?;
            Ipv6Addr::from(addr).to_string()
        }
        _ => {
            let _ = send_socks5_reply(client, SOCKS_REP_ADDRESS_TYPE_NOT_SUPPORTED);
            return Err(AppError::Unsupported(
                "SOCKS5 地址类型仅支持 IPv4、域名和 IPv6".to_string(),
            ));
        }
    };
    let mut port_bytes = [0_u8; 2];
    client.read_exact(&mut port_bytes)?;
    let port = u16::from_be_bytes(port_bytes);
    if port == 0 {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput(
            "SOCKS5 目标端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(SocksConnectRequest { host, port })
}

fn send_socks5_reply(client: &mut StdTcpStream, reply: u8) -> std::io::Result<()> {
    client.write_all(&[
        SOCKS_VERSION,
        reply,
        0x00,
        SOCKS_ATYP_IPV4,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}

fn connect_authenticated_session(
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<Session> {
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    let addr = resolve_ssh_addr(&profile.host, profile.port)?;
    let tcp = StdTcpStream::connect_timeout(&addr, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    connect_authenticated_session_over_tcp(tcp, profile, auth)
}

fn connect_authenticated_session_over_tcp(
    tcp: StdTcpStream,
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<Session> {
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.set_timeout(profile.connect_timeout_ms.min(u64::from(u32::MAX)) as u32);
    session.handshake()?;
    verify_known_host(&session, profile)?;
    authenticate(&session, profile, auth)?;
    if !session.authenticated() {
        return Err(AppError::InvalidInput("SSH 认证失败".to_string()));
    }
    let keepalive_secs = (profile.keepalive_interval_ms / 1000).clamp(0, u64::from(u32::MAX));
    if keepalive_secs > 0 {
        session.set_keepalive(true, keepalive_secs as u32);
    }
    Ok(session)
}

fn load_ssh_connection_chain(
    db: &Database,
    profile_id: &str,
) -> AppResult<Vec<SshConnectionProfile>> {
    db.resolve_ssh_profile_runtime_chain(profile_id)?
        .into_iter()
        .map(|profile| {
            let auth = SshAuthMaterial::load(db, &profile)?;
            Ok(SshConnectionProfile { profile, auth })
        })
        .collect()
}

fn connect_authenticated_session_chain(
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<SshSessionHandle> {
    let Some(first) = chain.first() else {
        return Err(AppError::InvalidInput("SSH 连接链不能为空".to_string()));
    };
    let first_session = connect_authenticated_session(&first.profile, &first.auth)?;
    let mut handle = SshSessionHandle::direct(first_session, first.profile.clone());
    for next in &chain[1..] {
        if cancel.is_cancelled() {
            handle.disconnect("net-power ssh chain cancelled");
            return Err(AppError::Message("SSH 连接已取消".to_string()));
        }
        handle = connect_next_hop(handle, next, cancel.child_token())?;
    }
    Ok(handle)
}

fn connect_next_hop(
    mut upstream: SshSessionHandle,
    next: &SshConnectionProfile,
    bridge_cancel: CancellationToken,
) -> AppResult<SshSessionHandle> {
    let mut channel = upstream.session.channel_direct_tcpip(
        &next.profile.host,
        next.profile.port,
        Some(("127.0.0.1", 0)),
    )?;
    upstream.session.set_blocking(false);

    let timeout = Duration::from_millis(next.profile.connect_timeout_ms.max(1));
    let (client_stream, bridge_stream) = loopback_tcp_pair(timeout)?;
    let upstream_session_for_bridge = upstream.session.clone();
    let upstream_profile_for_bridge = upstream.profile.clone();
    let bridge_cancel_for_thread = bridge_cancel.clone();
    let bridge_handle = std::thread::spawn(move || {
        bridge_nonblocking(
            bridge_stream,
            &upstream_session_for_bridge,
            &mut channel,
            &upstream_profile_for_bridge,
            bridge_cancel_for_thread,
        )
    });

    let next_session =
        match connect_authenticated_session_over_tcp(client_stream, &next.profile, &next.auth) {
            Ok(session) => session,
            Err(err) => {
                bridge_cancel.cancel();
                let _ = bridge_handle.join();
                upstream.disconnect("net-power ssh jump failed");
                return Err(err);
            }
        };

    upstream.upstream_sessions.push(upstream.session.clone());
    upstream.jump_bridges.push(JumpBridge {
        cancel: bridge_cancel,
        handle: bridge_handle,
    });
    Ok(SshSessionHandle {
        session: next_session,
        profile: next.profile.clone(),
        jump_bridges: upstream.jump_bridges,
        upstream_sessions: upstream.upstream_sessions,
    })
}

fn loopback_tcp_pair(timeout: Duration) -> AppResult<(StdTcpStream, StdTcpStream)> {
    let listener = StdTcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let client = StdTcpStream::connect_timeout(&addr, timeout)?;
    let (server, _) = listener.accept()?;
    client.set_nodelay(true)?;
    client.set_read_timeout(Some(timeout))?;
    client.set_write_timeout(Some(timeout))?;
    server.set_nodelay(true)?;
    server.set_nonblocking(true)?;
    Ok((client, server))
}

fn authenticate(
    session: &Session,
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<()> {
    match profile.auth_type {
        SshAuthType::Password => {
            let password = auth
                .password
                .as_deref()
                .ok_or_else(|| AppError::InvalidInput("密码认证缺少密码".to_string()))?;
            match session.userauth_password(&profile.username, password) {
                Ok(()) => Ok(()),
                Err(first_err) => {
                    let mut prompter = StaticPasswordPrompt {
                        password: password.to_string(),
                    };
                    session
                        .userauth_keyboard_interactive(&profile.username, &mut prompter)
                        .map_err(|second_err| {
                            AppError::InvalidInput(format!(
                                "SSH 密码认证失败: {first_err}; keyboard-interactive 失败: {second_err}"
                            ))
                        })
                }
            }
        }
        SshAuthType::PrivateKey => {
            let key_path = profile
                .private_key_path
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AppError::InvalidInput("私钥认证缺少私钥路径".to_string()))?;
            session.userauth_pubkey_file(
                &profile.username,
                None,
                Path::new(key_path),
                auth.private_key_passphrase.as_deref(),
            )?;
            Ok(())
        }
        SshAuthType::Agent => {
            session.userauth_agent(&profile.username)?;
            Ok(())
        }
    }
}

fn verify_known_host(session: &Session, profile: &SshProfileRuntimeConfig) -> AppResult<()> {
    if profile.known_hosts_mode == "insecure_skip" {
        return Ok(());
    }
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| AppError::InvalidInput("SSH 服务端没有返回 host key".to_string()))?;
    let known_hosts_path = known_hosts_path(profile)?;
    let mut known_hosts = session.known_hosts()?;
    if should_read_known_hosts_file(&profile.known_hosts_mode, &known_hosts_path)? {
        known_hosts.read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)?;
    }
    let check = known_hosts.check_port(&profile.host, profile.port, key);
    match decide_known_host(
        &profile.known_hosts_mode,
        check,
        &profile.host,
        profile.port,
    )? {
        KnownHostDecision::Trusted => Ok(()),
        KnownHostDecision::AddNew => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            known_hosts.add(
                &known_host_name(&profile.host, profile.port),
                key,
                "net-power accept_new",
                host_key_format(key_type),
            )?;
            known_hosts.write_file(&known_hosts_path, KnownHostFileKind::OpenSSH)?;
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum KnownHostDecision {
    Trusted,
    AddNew,
}

fn should_read_known_hosts_file(mode: &str, path: &Path) -> AppResult<bool> {
    if path.exists() {
        return Ok(true);
    }
    if mode == "strict" {
        return Err(AppError::InvalidInput(format!(
            "known_hosts 文件不存在: {}",
            path.display()
        )));
    }
    Ok(false)
}

fn decide_known_host(
    mode: &str,
    check: CheckResult,
    host: &str,
    port: u16,
) -> AppResult<KnownHostDecision> {
    match (mode, check) {
        (_, CheckResult::Match) => Ok(KnownHostDecision::Trusted),
        ("strict", CheckResult::NotFound) => Err(AppError::InvalidInput(format!(
            "known_hosts 中不存在 {host}:{port}"
        ))),
        (_, CheckResult::NotFound) => Ok(KnownHostDecision::AddNew),
        (_, CheckResult::Mismatch) => Err(AppError::InvalidInput(format!(
            "SSH host key 与 known_hosts 不匹配: {host}:{port}"
        ))),
        (_, CheckResult::Failure) => Err(AppError::InvalidInput(format!(
            "known_hosts 校验失败: {host}:{port}"
        ))),
    }
}

fn bridge_nonblocking(
    mut client: StdTcpStream,
    session: &Session,
    channel: &mut ssh2::Channel,
    profile: &SshProfileRuntimeConfig,
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let mut to_channel = VecDeque::<u8>::new();
    let mut to_client = VecDeque::<u8>::new();
    let mut client_eof = false;
    let mut channel_eof = false;
    let mut sent_channel_eof = false;
    let mut metrics = BridgeMetrics::default();
    let keepalive_interval = (profile.keepalive_interval_ms > 0)
        .then(|| Duration::from_millis(profile.keepalive_interval_ms));
    let mut last_keepalive = Instant::now();
    let mut buf = [0_u8; BRIDGE_BUFFER_SIZE];

    while !cancel.is_cancelled() {
        let mut progressed = false;

        if !client_eof && to_channel.len() < MAX_PENDING_BYTES {
            match client.read(&mut buf) {
                Ok(0) => {
                    client_eof = true;
                    progressed = true;
                }
                Ok(size) => {
                    to_channel.extend(&buf[..size]);
                    metrics.bytes_in = metrics.bytes_in.saturating_add(size as u64);
                    progressed = true;
                }
                Err(err) if would_block(&err) => {}
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        while !to_channel.is_empty() {
            let contiguous = to_channel.make_contiguous();
            match channel.write(contiguous) {
                Ok(0) => break,
                Ok(size) => {
                    to_channel.drain(..size);
                    progressed = true;
                }
                Err(err) if would_block(&err) => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        if client_eof && to_channel.is_empty() && !sent_channel_eof {
            match channel.send_eof() {
                Ok(()) => {
                    sent_channel_eof = true;
                    progressed = true;
                }
                Err(err) if ssh_would_block(&err) => {}
                Err(err) => return Err(AppError::Ssh(err)),
            }
        }

        if !channel_eof && to_client.len() < MAX_PENDING_BYTES {
            match channel.read(&mut buf) {
                Ok(0) => {
                    // ssh2 非阻塞 Channel 可能用 Ok(0) 表示本轮暂时没有数据；
                    // 只有明确 EOF 时才关闭桥接方向，避免 remote forward 新连接被提前断开。
                    if channel.eof() {
                        channel_eof = true;
                        progressed = true;
                    }
                }
                Ok(size) => {
                    to_client.extend(&buf[..size]);
                    metrics.bytes_out = metrics.bytes_out.saturating_add(size as u64);
                    progressed = true;
                }
                Err(err) if would_block(&err) => {}
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        while !to_client.is_empty() {
            let contiguous = to_client.make_contiguous();
            match client.write(contiguous) {
                Ok(0) => break,
                Ok(size) => {
                    to_client.drain(..size);
                    progressed = true;
                }
                Err(err) if would_block(&err) => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        if channel.eof() {
            channel_eof = true;
        }
        if channel_eof && to_client.is_empty() && (client_eof || to_channel.is_empty()) {
            break;
        }

        if keepalive_interval.is_some_and(|interval| last_keepalive.elapsed() >= interval) {
            match session.keepalive_send() {
                Ok(_) => last_keepalive = Instant::now(),
                Err(err) if ssh_would_block(&err) => {}
                Err(err) => return Err(AppError::Ssh(err)),
            }
        }

        if !progressed {
            std::thread::sleep(IDLE_SLEEP);
        }
    }
    Ok(metrics)
}

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
#[path = "ssh_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "ssh_known_hosts_tests.rs"]
mod known_hosts_tests;

#[cfg(test)]
#[path = "ssh_external_tests.rs"]
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
