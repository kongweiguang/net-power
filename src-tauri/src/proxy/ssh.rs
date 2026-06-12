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

include!("ssh/remote.rs");
include!("ssh/bridges.rs");
include!("ssh/session.rs");
include!("ssh/known_hosts.rs");
include!("ssh/io_bridge.rs");
include!("ssh/utils_tests.rs");
