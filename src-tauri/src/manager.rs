//! @author kongweiguang
//! 服务运行时管理器。它是生命周期 Facade，负责启动、停止、状态查询和端口冲突保护。

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{RuntimeStatus, ServiceDetail, ServiceKind, ServiceRuntimeSummary};
use crate::proxy;
use chrono::Utc;
use std::collections::HashMap;
use std::net::{TcpListener as StdTcpListener, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

/// 每个运行服务维护的轻量计数器。
#[derive(Debug, Default)]
pub struct ServiceCounters {
    /// 当前活跃连接数。
    pub active_connections: AtomicU64,
    /// 累计连接数。
    pub total_connections: AtomicU64,
    /// 累计入口字节数，预留给后续更细的流量统计。
    pub bytes_in: AtomicU64,
    /// 累计出口字节数，预留给后续更细的流量统计。
    pub bytes_out: AtomicU64,
}

/// 已启动服务的运行时句柄。
pub struct RunningService {
    /// 服务主键。
    pub service_id: String,
    /// 服务类型。
    pub kind: ServiceKind,
    /// 监听地址。
    pub listen_addr: String,
    /// 冲突检测键。SSH remote 监听发生在远端主机，因此不能和本地端口混用同一冲突域。
    conflict_key: String,
    /// 启动时间。
    pub started_at: String,
    /// 取消令牌。
    pub cancel: CancellationToken,
    /// 后台任务句柄。
    pub handle: JoinHandle<()>,
    /// 运行计数器。
    pub counters: Arc<ServiceCounters>,
}

/// 代理服务运行时管理器。
#[derive(Default)]
pub struct ServiceManager {
    running: HashMap<String, RunningService>,
    failed: HashMap<String, String>,
}

impl ServiceManager {
    /// 创建空的服务管理器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动服务。
    pub async fn start_service<R: Runtime>(
        &mut self,
        detail: ServiceDetail,
        db: Arc<Database>,
        app: AppHandle<R>,
    ) -> AppResult<RuntimeStatus> {
        self.reap_finished(&detail.id);
        if self.running.contains_key(&detail.id) {
            return Ok(RuntimeStatus::Running);
        }
        if !detail.enabled {
            return Err(AppError::InvalidInput("服务已禁用，不能启动".to_string()));
        }
        let binding = runtime_binding(&detail)?;
        if self
            .running
            .values()
            .any(|service| service.conflict_key == binding.conflict_key)
        {
            return Err(AppError::Conflict(format!(
                "监听地址已被其他服务占用: {}",
                binding.listen_addr
            )));
        }
        if binding.check_local_port {
            ensure_port_available(detail.kind, &binding.listen_addr)?;
        }

        let cancel = CancellationToken::new();
        let counters = Arc::new(ServiceCounters::default());
        let service_id = detail.id.clone();
        let service_id_for_task = service_id.clone();
        let kind = detail.kind;
        let app_for_task = app.clone();
        let db_for_task = Arc::clone(&db);
        let cancel_for_task = cancel.clone();
        let counters_for_task = Arc::clone(&counters);
        let handle = tokio::spawn(async move {
            let result = proxy::run_service(
                detail,
                cancel_for_task,
                counters_for_task,
                Arc::clone(&db_for_task),
                app_for_task.clone(),
            )
            .await;
            if let Err(err) = result {
                let message = err.to_string();
                let _ = db_for_task.insert_service_event(
                    Some(&service_id_for_task),
                    "error",
                    &message,
                    "{}",
                );
                let _ = app_for_task.emit("service://status-changed", &service_id_for_task);
            }
        });

        self.failed.remove(&service_id);
        self.running.insert(
            service_id.clone(),
            RunningService {
                service_id: service_id.clone(),
                kind,
                listen_addr: binding.listen_addr,
                conflict_key: binding.conflict_key,
                started_at: Utc::now().to_rfc3339(),
                cancel,
                handle,
                counters,
            },
        );
        db.insert_service_event(Some(&service_id), "info", "服务已启动", "{}")?;
        let _ = app.emit("service://status-changed", &service_id);
        Ok(RuntimeStatus::Running)
    }

    /// 停止服务。
    pub async fn stop_service<R: Runtime>(
        &mut self,
        service_id: &str,
        db: Arc<Database>,
        app: AppHandle<R>,
    ) -> AppResult<RuntimeStatus> {
        let Some(running) = self.running.remove(service_id) else {
            return Ok(RuntimeStatus::Stopped);
        };
        running.cancel.cancel();
        match timeout(Duration::from_secs(5), running.handle).await {
            Ok(joined) => {
                if let Err(err) = joined {
                    let message = format!("服务停止时任务异常: {err}");
                    self.failed.insert(service_id.to_string(), message.clone());
                    db.insert_service_event(Some(service_id), "error", &message, "{}")?;
                    return Ok(RuntimeStatus::Failed { message });
                }
            }
            Err(_) => {
                let message = "服务停止超时，后台任务可能仍在退出".to_string();
                self.failed.insert(service_id.to_string(), message.clone());
                db.insert_service_event(Some(service_id), "error", &message, "{}")?;
                return Ok(RuntimeStatus::Failed { message });
            }
        }
        db.insert_service_event(Some(service_id), "info", "服务已停止", "{}")?;
        let _ = app.emit("service://status-changed", service_id);
        Ok(RuntimeStatus::Stopped)
    }

    /// 查询某个服务运行态。
    pub fn runtime_status(&self, service_id: &str) -> RuntimeStatus {
        if let Some(service) = self.running.get(service_id) {
            if service.handle.is_finished() {
                RuntimeStatus::Failed {
                    message: "服务任务已退出".to_string(),
                }
            } else {
                RuntimeStatus::Running
            }
        } else if let Some(message) = self.failed.get(service_id) {
            RuntimeStatus::Failed {
                message: message.clone(),
            }
        } else {
            RuntimeStatus::Stopped
        }
    }

    /// 查询所有运行服务摘要。
    pub fn list_runtime_status(&self) -> Vec<ServiceRuntimeSummary> {
        let mut statuses: Vec<ServiceRuntimeSummary> = self
            .running
            .values()
            .map(|service| ServiceRuntimeSummary {
                service_id: service.service_id.clone(),
                kind: service.kind,
                listen_addr: service.listen_addr.clone(),
                runtime_status: if service.handle.is_finished() {
                    RuntimeStatus::Failed {
                        message: "服务任务已退出".to_string(),
                    }
                } else {
                    RuntimeStatus::Running
                },
                started_at: Some(service.started_at.clone()),
                active_connections: service.counters.active_connections.load(Ordering::Relaxed),
                total_connections: service.counters.total_connections.load(Ordering::Relaxed),
                bytes_in: service.counters.bytes_in.load(Ordering::Relaxed),
                bytes_out: service.counters.bytes_out.load(Ordering::Relaxed),
            })
            .collect();
        statuses.extend(
            self.failed
                .iter()
                .map(|(service_id, message)| ServiceRuntimeSummary {
                    service_id: service_id.clone(),
                    kind: ServiceKind::HttpForward,
                    listen_addr: String::new(),
                    runtime_status: RuntimeStatus::Failed {
                        message: message.clone(),
                    },
                    started_at: None,
                    active_connections: 0,
                    total_connections: 0,
                    bytes_in: 0,
                    bytes_out: 0,
                }),
        );
        statuses
    }

    /// 读取运行服务计数器。
    pub fn counters(&self, service_id: &str) -> (u64, u64) {
        self.running
            .get(service_id)
            .map(|service| {
                (
                    service.counters.active_connections.load(Ordering::Relaxed),
                    service.counters.total_connections.load(Ordering::Relaxed),
                )
            })
            .unwrap_or((0, 0))
    }

    fn reap_finished(&mut self, service_id: &str) {
        if self
            .running
            .get(service_id)
            .is_some_and(|service| service.handle.is_finished())
        {
            self.running.remove(service_id);
            self.failed
                .insert(service_id.to_string(), "服务任务已退出".to_string());
        }
    }

    #[cfg(test)]
    async fn start_service_for_test<F>(
        &mut self,
        detail: ServiceDetail,
        db: Arc<Database>,
        runner: F,
    ) -> AppResult<RuntimeStatus>
    where
        F: FnOnce(ServiceDetail, CancellationToken, Arc<ServiceCounters>) -> JoinHandle<()>,
    {
        self.reap_finished(&detail.id);
        if self.running.contains_key(&detail.id) {
            return Ok(RuntimeStatus::Running);
        }
        if !detail.enabled {
            return Err(AppError::InvalidInput("服务已禁用，不能启动".to_string()));
        }
        let binding = runtime_binding(&detail)?;
        if self
            .running
            .values()
            .any(|service| service.conflict_key == binding.conflict_key)
        {
            return Err(AppError::Conflict(format!(
                "监听地址已被其他服务占用: {}",
                binding.listen_addr
            )));
        }
        if binding.check_local_port {
            ensure_port_available(detail.kind, &binding.listen_addr)?;
        }

        let cancel = CancellationToken::new();
        let counters = Arc::new(ServiceCounters::default());
        let service_id = detail.id.clone();
        let kind = detail.kind;
        let handle = runner(detail, cancel.clone(), Arc::clone(&counters));
        self.failed.remove(&service_id);
        self.running.insert(
            service_id.clone(),
            RunningService {
                service_id: service_id.clone(),
                kind,
                listen_addr: binding.listen_addr,
                conflict_key: binding.conflict_key,
                started_at: Utc::now().to_rfc3339(),
                cancel,
                handle,
                counters,
            },
        );
        db.insert_service_event(Some(&service_id), "info", "服务已启动", "{}")?;
        Ok(RuntimeStatus::Running)
    }

    #[cfg(test)]
    async fn stop_service_for_test(
        &mut self,
        service_id: &str,
        db: Arc<Database>,
    ) -> AppResult<RuntimeStatus> {
        let Some(running) = self.running.remove(service_id) else {
            return Ok(RuntimeStatus::Stopped);
        };
        running.cancel.cancel();
        match timeout(Duration::from_secs(5), running.handle).await {
            Ok(joined) => {
                if let Err(err) = joined {
                    let message = format!("服务停止时任务异常: {err}");
                    self.failed.insert(service_id.to_string(), message.clone());
                    db.insert_service_event(Some(service_id), "error", &message, "{}")?;
                    return Ok(RuntimeStatus::Failed { message });
                }
            }
            Err(_) => {
                let message = "服务停止超时，后台任务可能仍在退出".to_string();
                self.failed.insert(service_id.to_string(), message.clone());
                db.insert_service_event(Some(service_id), "error", &message, "{}")?;
                return Ok(RuntimeStatus::Failed { message });
            }
        }
        db.insert_service_event(Some(service_id), "info", "服务已停止", "{}")?;
        Ok(RuntimeStatus::Stopped)
    }
}

#[derive(Debug)]
struct RuntimeBinding {
    listen_addr: String,
    conflict_key: String,
    check_local_port: bool,
}

fn runtime_binding(detail: &ServiceDetail) -> AppResult<RuntimeBinding> {
    if detail.kind != ServiceKind::SshRemote {
        let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
        return Ok(RuntimeBinding {
            conflict_key: format!("local:{listen_addr}"),
            listen_addr,
            check_local_port: true,
        });
    }

    let cfg = detail
        .ssh_tunnel
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少隧道配置".to_string()))?;
    let host = cfg
        .remote_bind_host
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少远程绑定 host".to_string()))?;
    let port = cfg
        .remote_bind_port
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少远程绑定端口".to_string()))?;
    let listen_addr = format!("{}:{}", host.trim(), port);
    Ok(RuntimeBinding {
        conflict_key: format!("ssh_remote:{}:{listen_addr}", cfg.ssh_profile_id),
        listen_addr,
        check_local_port: false,
    })
}

fn ensure_port_available(kind: ServiceKind, listen_addr: &str) -> AppResult<()> {
    match kind {
        ServiceKind::UdpForward => {
            let socket = StdUdpSocket::bind(listen_addr).map_err(|err| {
                AppError::Conflict(format!("UDP 端口不可用 {listen_addr}: {err}"))
            })?;
            drop(socket);
        }
        _ => {
            let listener = StdTcpListener::bind(listen_addr).map_err(|err| {
                AppError::Conflict(format!("TCP 端口不可用 {listen_addr}: {err}"))
            })?;
            drop(listener);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "manager_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateServiceInput, ServiceDetail, TcpForwardConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn manager_start_stop_tcp_runner_releases_listen_port() {
        let listen_port = reserve_tcp_port();
        let listen_addr = format!("127.0.0.1:{listen_port}");
        let db = Arc::new(Database::in_memory().expect("内存数据库应初始化"));
        let mut manager = ServiceManager::new();
        let detail = db
            .create_service(&tcp_forward_input("svc-one", listen_port))
            .expect("测试服务应可写入数据库");

        let started = manager
            .start_service_for_test(detail.clone(), Arc::clone(&db), test_tcp_runner)
            .await
            .expect("服务应可启动");
        assert_eq!(started, RuntimeStatus::Running);
        wait_until_tcp_port_bound(&listen_addr).await;

        let conflict = manager
            .start_service_for_test(
                tcp_forward_detail("svc-two", listen_port),
                Arc::clone(&db),
                test_tcp_runner,
            )
            .await;
        assert!(matches!(conflict, Err(AppError::Conflict(_))));

        let mut client = TcpStream::connect(&listen_addr)
            .await
            .expect("应可连接 manager 启动的 TCP Forward");
        client.write_all(b"ping").await.expect("应可写入代理入口");
        let mut response = [0_u8; 4];
        client
            .read_exact(&mut response)
            .await
            .expect("应可读取测试 runner 响应");
        assert_eq!(&response, b"pong");
        drop(client);

        let stopped = manager
            .stop_service_for_test(&detail.id, Arc::clone(&db))
            .await
            .expect("服务应可停止");
        assert_eq!(stopped, RuntimeStatus::Stopped);
        wait_until_tcp_port_available(&listen_addr).await;
        assert_eq!(manager.runtime_status(&detail.id), RuntimeStatus::Stopped);
    }

    #[tokio::test]
    async fn finished_task_is_reported_failed_and_can_start_again() {
        let listen_port = reserve_tcp_port();
        let db = Arc::new(Database::in_memory().expect("内存数据库应初始化"));
        let mut manager = ServiceManager::new();
        let detail = db
            .create_service(&tcp_forward_input("svc-finished", listen_port))
            .expect("测试服务应可写入数据库");

        let started = manager
            .start_service_for_test(detail.clone(), Arc::clone(&db), finished_runner)
            .await
            .expect("服务应可启动");
        assert_eq!(started, RuntimeStatus::Running);
        wait_until_runtime_failed(&manager, &detail.id).await;

        let restarted = manager
            .start_service_for_test(detail, Arc::clone(&db), finished_runner)
            .await
            .expect("已退出服务应允许重新启动");
        assert_eq!(restarted, RuntimeStatus::Running);
    }

    #[test]
    fn tcp_port_check_rejects_occupied_listener() {
        let listener = StdTcpListener::bind("127.0.0.1:0").expect("测试 TCP 端口应可监听");
        let addr = listener.local_addr().expect("应可读取 TCP 测试端口");

        let result = ensure_port_available(ServiceKind::HttpForward, &addr.to_string());

        assert!(matches!(result, Err(AppError::Conflict(_))));
        drop(listener);
        ensure_port_available(ServiceKind::HttpForward, &addr.to_string())
            .expect("释放后的 TCP 端口应可再次监听");
    }

    #[test]
    fn udp_port_check_rejects_occupied_socket() {
        let socket = StdUdpSocket::bind("127.0.0.1:0").expect("测试 UDP 端口应可监听");
        let addr = socket.local_addr().expect("应可读取 UDP 测试端口");

        let result = ensure_port_available(ServiceKind::UdpForward, &addr.to_string());

        assert!(matches!(result, Err(AppError::Conflict(_))));
        drop(socket);
        ensure_port_available(ServiceKind::UdpForward, &addr.to_string())
            .expect("释放后的 UDP 端口应可再次监听");
    }

    fn tcp_forward_detail(id: &str, listen_port: u16) -> ServiceDetail {
        ServiceDetail {
            id: id.to_string(),
            name: id.to_string(),
            kind: ServiceKind::TcpForward,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port,
            notes: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            http_reverse: None,
            http_forward: None,
            tcp_forward: Some(TcpForwardConfig {
                target_host: "127.0.0.1".to_string(),
                target_port: 9,
                connect_timeout_ms: 2_000,
                idle_timeout_ms: 0,
            }),
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: Vec::new(),
            body_rewrite_rules: Vec::new(),
        }
    }

    fn tcp_forward_input(name: &str, listen_port: u16) -> CreateServiceInput {
        CreateServiceInput {
            name: name.to_string(),
            kind: ServiceKind::TcpForward,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port,
            notes: String::new(),
            http_reverse: None,
            http_forward: None,
            tcp_forward: Some(TcpForwardConfig {
                target_host: "127.0.0.1".to_string(),
                target_port: 9,
                connect_timeout_ms: 2_000,
                idle_timeout_ms: 0,
            }),
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: Vec::new(),
            body_rewrite_rules: Vec::new(),
        }
    }

    fn test_tcp_runner(
        detail: ServiceDetail,
        cancel: CancellationToken,
        counters: Arc<ServiceCounters>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
            let listener = TcpListener::bind(&listen_addr)
                .await
                .expect("测试 runner 应可监听 manager 已检查的端口");
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    accepted = listener.accept() => {
                        let (mut socket, _) = accepted.expect("测试 runner accept 应成功");
                        counters.active_connections.fetch_add(1, Ordering::Relaxed);
                        counters.total_connections.fetch_add(1, Ordering::Relaxed);
                        let mut buffer = [0_u8; 4];
                        socket
                            .read_exact(&mut buffer)
                            .await
                            .expect("测试 runner 应收到客户端数据");
                        assert_eq!(&buffer, b"ping");
                        counters.bytes_in.fetch_add(4, Ordering::Relaxed);
                        socket.write_all(b"pong").await.expect("测试 runner 应可响应");
                        counters.bytes_out.fetch_add(4, Ordering::Relaxed);
                        counters.active_connections.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }
        })
    }

    fn finished_runner(
        _detail: ServiceDetail,
        _cancel: CancellationToken,
        _counters: Arc<ServiceCounters>,
    ) -> JoinHandle<()> {
        tokio::spawn(async {})
    }

    fn reserve_tcp_port() -> u16 {
        let listener = StdTcpListener::bind("127.0.0.1:0").expect("应可临时监听 TCP 端口");
        let port = listener.local_addr().expect("应可读取临时端口").port();
        drop(listener);
        port
    }

    async fn wait_until_tcp_port_bound(addr: &str) {
        for _ in 0..100 {
            if StdTcpListener::bind(addr).is_err() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("TCP 端口未按预期进入监听状态: {addr}");
    }

    async fn wait_until_runtime_failed(manager: &ServiceManager, service_id: &str) {
        for _ in 0..100 {
            if matches!(
                manager.runtime_status(service_id),
                RuntimeStatus::Failed { .. }
            ) {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("服务任务未按预期进入失败状态: {service_id}");
    }

    async fn wait_until_tcp_port_available(addr: &str) {
        for _ in 0..100 {
            if let Ok(listener) = StdTcpListener::bind(addr) {
                drop(listener);
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("TCP 端口未按预期释放: {addr}");
    }
}
