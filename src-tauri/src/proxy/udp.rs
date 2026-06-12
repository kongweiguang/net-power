//! @author kongweiguang
//! UDP 转发服务实现。按客户端地址维护目标 UDP socket，返回路径独立任务写回原客户端。

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::manager::ServiceCounters;
use crate::models::{ConnectionEventPayload, ServiceDetail, ServiceEventPayload};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant as StdInstant;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// 启动 UDP 转发服务。
pub async fn run_udp_forward<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let cfg = detail
        .udp_forward
        .clone()
        .ok_or_else(|| AppError::InvalidInput("UDP 转发缺少配置".to_string()))?;
    let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
    let target_addr = format!("{}:{}", cfg.target_host, cfg.target_port);
    let socket = Arc::new(UdpSocket::bind(&listen_addr).await?);
    let target: SocketAddr = tokio::net::lookup_host(&target_addr)
        .await?
        .next()
        .ok_or_else(|| AppError::InvalidInput(format!("无法解析 UDP 目标: {target_addr}")))?;
    let events: Arc<dyn UdpEventSink> = Arc::new(TauriUdpEventSink::<R> { db, app });
    log_event(
        events.as_ref(),
        Some(&detail.id),
        "info",
        &format!("UDP 转发已监听 {listen_addr}"),
    )?;
    run_udp_forward_loop(UdpForwardRuntime {
        service_id: detail.id,
        socket,
        target,
        target_addr,
        idle_timeout_ms: cfg.idle_timeout_ms,
        cancel,
        counters,
        events,
        mappings: Arc::new(Mutex::new(HashMap::new())),
    })
    .await
}

struct UdpForwardRuntime {
    service_id: String,
    socket: Arc<UdpSocket>,
    target: SocketAddr,
    target_addr: String,
    idle_timeout_ms: u64,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    events: Arc<dyn UdpEventSink>,
    mappings: Arc<Mutex<HashMap<SocketAddr, UdpClientMapping>>>,
}

async fn run_udp_forward_loop(runtime: UdpForwardRuntime) -> AppResult<()> {
    let mut buf = vec![0_u8; 65_507];
    loop {
        tokio::select! {
            _ = runtime.cancel.cancelled() => {
                cancel_all_mappings(&runtime.mappings).await;
                runtime.counters.active_connections.store(0, Ordering::Relaxed);
                log_event(runtime.events.as_ref(), Some(&runtime.service_id), "info", "UDP 转发已停止")?;
                return Ok(());
            }
            received = runtime.socket.recv_from(&mut buf) => {
                let (len, remote_addr) = received?;
                runtime.counters.total_connections.fetch_add(1, Ordering::Relaxed);
                let target_socket = target_socket_for_client(
                    remote_addr,
                    &runtime,
                )
                .await?;
                let started = StdInstant::now();
                let mut error = String::new();
                match target_socket.send(&buf[..len]).await {
                    Ok(sent) => {
                        runtime.counters.bytes_in.fetch_add(sent as u64, Ordering::Relaxed);
                    }
                    Err(err) => {
                        error = err.to_string();
                        log_event(runtime.events.as_ref(), Some(&runtime.service_id), "error", &error)?;
                    }
                }
                runtime.events.udp_datagram(UdpDatagramEvent {
                    service_id: &runtime.service_id,
                    remote_addr: &remote_addr.to_string(),
                    target_addr: &runtime.target_addr,
                    event_type: "datagram",
                    bytes_in: len as u64,
                    bytes_out: 0,
                    duration_ms: elapsed_ms(started),
                    error: &error,
                });
                let active = cleanup_idle_clients(&runtime.mappings, runtime.idle_timeout_ms).await;
                runtime.counters.active_connections.store(active as u64, Ordering::Relaxed);
            }
        }
    }
}

struct UdpClientMapping {
    target_socket: Arc<UdpSocket>,
    last_seen: Instant,
    cancel: CancellationToken,
    response_handle: JoinHandle<()>,
}

struct UdpDatagramEvent<'a> {
    service_id: &'a str,
    remote_addr: &'a str,
    target_addr: &'a str,
    event_type: &'a str,
    bytes_in: u64,
    bytes_out: u64,
    duration_ms: u64,
    error: &'a str,
}

trait UdpEventSink: Send + Sync {
    fn service_event(&self, service_id: Option<&str>, level: &str, message: &str) -> AppResult<()>;
    fn udp_datagram(&self, event: UdpDatagramEvent<'_>);
}

struct TauriUdpEventSink<R: Runtime> {
    db: Arc<Database>,
    app: AppHandle<R>,
}

impl<R: Runtime> UdpEventSink for TauriUdpEventSink<R> {
    fn service_event(&self, service_id: Option<&str>, level: &str, message: &str) -> AppResult<()> {
        self.db
            .insert_service_event(service_id, level, message, "{}")?;
        let _ = self.app.emit(
            "service://log",
            ServiceEventPayload {
                service_id: service_id.map(ToOwned::to_owned),
                level: level.to_string(),
                message: message.to_string(),
            },
        );
        Ok(())
    }

    fn udp_datagram(&self, event: UdpDatagramEvent<'_>) {
        let _ = self.db.insert_connection_event(
            event.service_id,
            "udp",
            event.remote_addr,
            event.target_addr,
            event.event_type,
            "",
            "",
            "",
            None,
            event.bytes_in,
            event.bytes_out,
            event.duration_ms,
            event.error,
        );
        let _ = self.app.emit(
            "service://connection",
            ConnectionEventPayload {
                service_id: event.service_id.to_string(),
                protocol: "udp".to_string(),
                event_type: event.event_type.to_string(),
                target_addr: event.target_addr.to_string(),
            },
        );
    }
}

async fn target_socket_for_client(
    remote_addr: SocketAddr,
    runtime: &UdpForwardRuntime,
) -> AppResult<Arc<UdpSocket>> {
    if let Some(socket) = refresh_existing_mapping(&runtime.mappings, remote_addr).await {
        return Ok(socket);
    }

    let bind_addr = if runtime.target.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let target_socket = Arc::new(UdpSocket::bind(bind_addr).await?);
    target_socket.connect(runtime.target).await?;
    let client_cancel = runtime.cancel.child_token();
    let response_handle = spawn_udp_response_path(UdpResponsePath {
        listener_socket: Arc::clone(&runtime.socket),
        target_socket: Arc::clone(&target_socket),
        remote_addr,
        cancel: client_cancel.clone(),
        counters: Arc::clone(&runtime.counters),
        service_id: runtime.service_id.clone(),
        target_addr: runtime.target_addr.clone(),
        events: Arc::clone(&runtime.events),
    });

    let mut guard = runtime.mappings.lock().await;
    let entry = guard
        .entry(remote_addr)
        .or_insert_with(|| UdpClientMapping {
            target_socket: Arc::clone(&target_socket),
            last_seen: Instant::now(),
            cancel: client_cancel,
            response_handle,
        });
    Ok(Arc::clone(&entry.target_socket))
}

async fn refresh_existing_mapping(
    mappings: &Arc<Mutex<HashMap<SocketAddr, UdpClientMapping>>>,
    remote_addr: SocketAddr,
) -> Option<Arc<UdpSocket>> {
    let mut guard = mappings.lock().await;
    guard.get_mut(&remote_addr).map(|mapping| {
        mapping.last_seen = Instant::now();
        Arc::clone(&mapping.target_socket)
    })
}

struct UdpResponsePath {
    listener_socket: Arc<UdpSocket>,
    target_socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    service_id: String,
    target_addr: String,
    events: Arc<dyn UdpEventSink>,
}

fn spawn_udp_response_path(path: UdpResponsePath) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut response = vec![0_u8; 65_507];
        loop {
            tokio::select! {
                _ = path.cancel.cancelled() => return,
                received = path.target_socket.recv(&mut response) => {
                    let started = StdInstant::now();
                    let mut bytes_out = 0_u64;
                    let mut error = String::new();
                    match received {
                        Ok(size) => {
                            bytes_out = size as u64;
                            if let Err(err) = path.listener_socket.send_to(&response[..size], path.remote_addr).await {
                                error = err.to_string();
                            }
                        }
                        Err(err) => {
                            error = err.to_string();
                        }
                    }
                    path.counters.bytes_out.fetch_add(bytes_out, Ordering::Relaxed);
                    path.events.udp_datagram(UdpDatagramEvent {
                        service_id: &path.service_id,
                        remote_addr: &path.remote_addr.to_string(),
                        target_addr: &path.target_addr,
                        event_type: "response",
                        bytes_in: 0,
                        bytes_out,
                        duration_ms: elapsed_ms(started),
                        error: &error,
                    });
                    if !error.is_empty() {
                        let _ = log_event(path.events.as_ref(), Some(&path.service_id), "error", &error);
                        return;
                    }
                }
            }
        }
    })
}

async fn cleanup_idle_clients(
    mappings: &Arc<Mutex<HashMap<SocketAddr, UdpClientMapping>>>,
    idle_timeout_ms: u64,
) -> usize {
    if idle_timeout_ms == 0 {
        return mappings.lock().await.len();
    }
    let cutoff = Duration::from_millis(idle_timeout_ms);
    let mut guard = mappings.lock().await;
    let stale_clients: Vec<SocketAddr> = guard
        .iter()
        .filter_map(|(client, mapping)| (mapping.last_seen.elapsed() > cutoff).then_some(*client))
        .collect();
    for client in stale_clients {
        if let Some(mapping) = guard.remove(&client) {
            mapping.cancel.cancel();
            mapping.response_handle.abort();
        }
    }
    guard.len()
}

async fn cancel_all_mappings(mappings: &Arc<Mutex<HashMap<SocketAddr, UdpClientMapping>>>) {
    let mut guard = mappings.lock().await;
    for (_, mapping) in guard.drain() {
        mapping.cancel.cancel();
        mapping.response_handle.abort();
    }
}

fn log_event(
    events: &dyn UdpEventSink,
    service_id: Option<&str>,
    level: &str,
    message: &str,
) -> AppResult<()> {
    events.service_event(service_id, level, message)
}

fn elapsed_ms(started: StdInstant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future;
    use tokio::time::timeout;

    struct NoopUdpEventSink;

    impl UdpEventSink for NoopUdpEventSink {
        fn service_event(
            &self,
            _service_id: Option<&str>,
            _level: &str,
            _message: &str,
        ) -> AppResult<()> {
            Ok(())
        }

        fn udp_datagram(&self, _event: UdpDatagramEvent<'_>) {}
    }

    #[tokio::test]
    async fn udp_forward_routes_responses_to_the_origin_client_and_reuses_mapping() {
        let target = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("测试 UDP 目标应可监听");
        let target_addr = target.local_addr().expect("应可读取 UDP 目标地址");
        let target_task = tokio::spawn(async move {
            let mut seen_sources = HashMap::<String, SocketAddr>::new();
            let mut buffer = [0_u8; 128];
            for _ in 0..3 {
                let (len, proxy_source) = target
                    .recv_from(&mut buffer)
                    .await
                    .expect("目标应收到 UDP 数据");
                let payload =
                    String::from_utf8(buffer[..len].to_vec()).expect("测试数据应是 UTF-8");
                seen_sources.insert(payload.clone(), proxy_source);
                target
                    .send_to(format!("reply-{payload}").as_bytes(), proxy_source)
                    .await
                    .expect("目标应可写回 UDP 响应");
            }
            seen_sources
        });

        let proxy_socket = Arc::new(
            UdpSocket::bind("127.0.0.1:0")
                .await
                .expect("测试 UDP 代理入口应可监听"),
        );
        let proxy_addr = proxy_socket
            .local_addr()
            .expect("应可读取 UDP 代理入口地址");
        let cancel = CancellationToken::new();
        let counters = Arc::new(ServiceCounters::default());
        let proxy_task = tokio::spawn(run_udp_forward_loop(UdpForwardRuntime {
            service_id: "svc-udp".to_string(),
            socket: proxy_socket,
            target: target_addr,
            target_addr: target_addr.to_string(),
            idle_timeout_ms: 60_000,
            cancel: cancel.clone(),
            counters: Arc::clone(&counters),
            events: Arc::new(NoopUdpEventSink),
            mappings: Arc::new(Mutex::new(HashMap::new())),
        }));

        let client_one = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("client one 应可绑定");
        client_one
            .connect(proxy_addr)
            .await
            .expect("client one 应可连接代理");
        let client_two = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("client two 应可绑定");
        client_two
            .connect(proxy_addr)
            .await
            .expect("client two 应可连接代理");

        assert_eq!(send_and_recv(&client_one, "a1").await, "reply-a1");
        assert_eq!(send_and_recv(&client_two, "b1").await, "reply-b1");
        assert_eq!(send_and_recv(&client_one, "a2").await, "reply-a2");
        assert_eq!(counters.total_connections.load(Ordering::Relaxed), 3);
        assert_eq!(counters.active_connections.load(Ordering::Relaxed), 2);

        let seen_sources = target_task.await.expect("目标任务应完成");
        assert_eq!(seen_sources.get("a1"), seen_sources.get("a2"));
        assert_ne!(seen_sources.get("a1"), seen_sources.get("b1"));

        cancel.cancel();
        timeout(Duration::from_secs(2), proxy_task)
            .await
            .expect("UDP 代理任务应在 cancel 后退出")
            .expect("UDP 代理任务 join 应成功")
            .expect("UDP 代理循环应正常退出");
    }

    #[tokio::test]
    async fn cleanup_idle_clients_removes_only_expired_mappings() {
        let mappings = Arc::new(Mutex::new(HashMap::<SocketAddr, UdpClientMapping>::new()));
        let stale_cancel = CancellationToken::new();
        let fresh_cancel = CancellationToken::new();
        let stale_addr: SocketAddr = "127.0.0.1:10001".parse().expect("地址应合法");
        let fresh_addr: SocketAddr = "127.0.0.1:10002".parse().expect("地址应合法");

        mappings.lock().await.insert(
            stale_addr,
            UdpClientMapping {
                target_socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.expect("应可绑定")),
                last_seen: Instant::now() - Duration::from_millis(100),
                cancel: stale_cancel.clone(),
                response_handle: tokio::spawn(future::pending()),
            },
        );
        mappings.lock().await.insert(
            fresh_addr,
            UdpClientMapping {
                target_socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.expect("应可绑定")),
                last_seen: Instant::now(),
                cancel: fresh_cancel.clone(),
                response_handle: tokio::spawn(future::pending()),
            },
        );

        let active = cleanup_idle_clients(&mappings, 10).await;

        assert_eq!(active, 1);
        assert!(stale_cancel.is_cancelled());
        assert!(!fresh_cancel.is_cancelled());
        assert!(mappings.lock().await.contains_key(&fresh_addr));
        cancel_all_mappings(&mappings).await;
    }

    async fn send_and_recv(socket: &UdpSocket, payload: &str) -> String {
        socket
            .send(payload.as_bytes())
            .await
            .expect("客户端应可发送 UDP 数据");
        let mut buffer = [0_u8; 128];
        let len = timeout(Duration::from_secs(2), socket.recv(&mut buffer))
            .await
            .expect("客户端读取 UDP 响应不应超时")
            .expect("客户端应可读取 UDP 响应");
        String::from_utf8(buffer[..len].to_vec()).expect("UDP 响应应是 UTF-8")
    }
}
