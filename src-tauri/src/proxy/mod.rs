//! @author kongweiguang
//! 代理服务运行入口。各协议服务共享取消令牌、计数器和日志写入能力。

pub mod http;
pub mod rewrite;
pub mod ssh;
pub mod tcp;
pub mod udp;

use crate::database::Database;
use crate::error::AppResult;
use crate::manager::ServiceCounters;
use crate::models::{ServiceDetail, ServiceKind};
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

/// 按服务类型进入对应代理实现。
pub async fn run_service<R: Runtime>(
    detail: ServiceDetail,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    match detail.kind {
        ServiceKind::HttpReverse | ServiceKind::HttpForward => {
            http::run_http_service(detail, cancel, counters, db, app).await
        }
        ServiceKind::TcpForward => tcp::run_tcp_forward(detail, cancel, counters, db, app).await,
        ServiceKind::UdpForward => udp::run_udp_forward(detail, cancel, counters, db, app).await,
        ServiceKind::SshLocal => {
            ssh::run_ssh_local_forward(detail, cancel, counters, db, app).await
        }
        ServiceKind::SshSocks => {
            ssh::run_ssh_socks_forward(detail, cancel, counters, db, app).await
        }
        ServiceKind::SshRemote => {
            ssh::run_ssh_remote_forward(detail, cancel, counters, db, app).await
        }
    }
}
