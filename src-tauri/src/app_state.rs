//! @author kongweiguang
//! Tauri 全局状态。Rust 侧集中持有数据库和服务运行时，前端只通过 Command 访问。

use crate::database::Database;
use crate::manager::ServiceManager;
use crate::tool_services::ToolServiceManager;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 应用级共享状态。
pub struct AppState {
    /// SQLite 数据库访问门面。
    pub db: Arc<Database>,
    /// 服务运行时管理器。
    pub manager: RwLock<ServiceManager>,
    /// 本地工具服务运行时管理器。
    pub tool_services: RwLock<ToolServiceManager>,
}

impl AppState {
    /// 构造应用状态。
    pub fn new(db: Database) -> Self {
        Self {
            db: Arc::new(db),
            manager: RwLock::new(ServiceManager::new()),
            tool_services: RwLock::new(ToolServiceManager::new()),
        }
    }
}
