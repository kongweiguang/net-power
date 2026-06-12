//! @author kongweiguang
//! TCP 转发服务实现。每个客户端连接独立任务处理，停止服务时由取消令牌统一打断。

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::manager::ServiceCounters;
use crate::models::{ConnectionEventPayload, ServiceDetail};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

/// 启动 TCP 转发服务。
pub async fn run_tcp_forward<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let cfg = detail
        .tcp_forward
        .clone()
        .ok_or_else(|| AppError::InvalidInput("TCP 转发缺少配置".to_string()))?;
    let listen_addr = format!("{}:{}", detail.listen_host, detail.listen_port);
    let target_addr = format!("{}:{}", cfg.target_host, cfg.target_port);
    let listener = TcpListener::bind(&listen_addr).await?;
    log_event(
        &db,
        &app,
        Some(&detail.id),
        "info",
        &format!("TCP 转发已监听 {listen_addr}"),
    )?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                log_event(&db, &app, Some(&detail.id), "info", "TCP 转发已停止")?;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (client, remote_addr) = accepted?;
                let child_cancel = cancel.child_token();
                let db = Arc::clone(&db);
                let app = app.clone();
                let counters = Arc::clone(&counters);
                let service_id = detail.id.clone();
                let target_addr = target_addr.clone();
                let connect_timeout = Duration::from_millis(cfg.connect_timeout_ms.max(1));
                tokio::spawn(async move {
                    counters.active_connections.fetch_add(1, Ordering::Relaxed);
                    counters.total_connections.fetch_add(1, Ordering::Relaxed);
                    let started = Instant::now();
                    let result = handle_tcp_client(
                        client,
                        &target_addr,
                        connect_timeout,
                        child_cancel,
                    )
                    .await;
                    counters.active_connections.fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    let (bytes_in, bytes_out, error) = match result {
                        Ok((bytes_in, bytes_out)) => (bytes_in, bytes_out, String::new()),
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(&db, &app, Some(&service_id), "error", &message);
                            (0, 0, message)
                        }
                    };
                    counters.bytes_in.fetch_add(bytes_in, Ordering::Relaxed);
                    counters.bytes_out.fetch_add(bytes_out, Ordering::Relaxed);
                    let _ = db.insert_connection_event(
                        &service_id,
                        "tcp",
                        &remote_addr.to_string(),
                        &target_addr,
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
                    let _ = app.emit(
                        "service://connection",
                        ConnectionEventPayload {
                            service_id: service_id.clone(),
                            protocol: "tcp".to_string(),
                            event_type: "closed".to_string(),
                            target_addr: target_addr.clone(),
                        },
                    );
                });
            }
        }
    }
}

async fn handle_tcp_client(
    mut client: TcpStream,
    target_addr: &str,
    connect_timeout: Duration,
    cancel: CancellationToken,
) -> AppResult<(u64, u64)> {
    let mut target = timeout(connect_timeout, TcpStream::connect(target_addr))
        .await
        .map_err(|_| AppError::Message(format!("连接目标 {target_addr} 超时")))??;
    let result = tokio::select! {
        _ = cancel.cancelled() => Ok((0, 0)),
        copied = copy_bidirectional(&mut client, &mut target) => copied.map_err(AppError::from),
    }?;
    Ok(result)
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
        crate::models::ServiceEventPayload {
            service_id: service_id.map(ToOwned::to_owned),
            level: level.to_string(),
            message: message.to_string(),
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn tcp_client_copies_bytes_in_both_directions() {
        let target = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试 TCP 目标应可监听");
        let target_addr = target.local_addr().expect("应可读取目标地址");
        let target_task = tokio::spawn(async move {
            let (mut socket, _) = target.accept().await.expect("应收到目标连接");
            let mut buffer = [0_u8; 4];
            socket
                .read_exact(&mut buffer)
                .await
                .expect("目标应收到客户端数据");
            assert_eq!(&buffer, b"ping");
            socket.write_all(b"pong").await.expect("目标应可写回响应");
        });

        let proxy_entry = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试 TCP 入口应可监听");
        let proxy_addr = proxy_entry.local_addr().expect("应可读取入口地址");
        let proxy_task = tokio::spawn(async move {
            let (client, _) = proxy_entry.accept().await.expect("应收到客户端连接");
            handle_tcp_client(
                client,
                &target_addr.to_string(),
                Duration::from_secs(2),
                CancellationToken::new(),
            )
            .await
            .expect("TCP 双向复制应成功")
        });

        let mut client = TcpStream::connect(proxy_addr)
            .await
            .expect("应可连接测试 TCP 入口");
        client
            .write_all(b"ping")
            .await
            .expect("客户端应可写入 TCP 入口");
        let mut response = [0_u8; 4];
        client
            .read_exact(&mut response)
            .await
            .expect("客户端应可读取目标响应");
        assert_eq!(&response, b"pong");
        drop(client);

        let (bytes_in, bytes_out) = timeout(Duration::from_secs(2), proxy_task)
            .await
            .expect("TCP 复制任务应在客户端关闭后结束")
            .expect("TCP 复制任务应完成");
        assert!(bytes_in >= 4);
        assert!(bytes_out >= 4);
        target_task.await.expect("目标任务应完成");
    }
}
