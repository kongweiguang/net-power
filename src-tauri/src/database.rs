//! @author kongweiguang
//! SQLite 数据访问层。所有核心配置写入都通过这里完成，前端不直接操作数据库。

use crate::error::{AppError, AppResult};
use crate::models::{
    AppSetting, BodyRewriteRule, BodyRewriteRuleInput, CreateServiceInput, HeaderRule,
    HeaderRuleInput, HttpForwardConfig, HttpReverseConfig, LogFilter, LogRow, RuntimeStatus,
    ServiceDetail, ServiceKind, ServiceSummary, SshAuthType, SshProfile, SshProfileInput,
    SshProfileRuntimeConfig, SshTunnelConfig, SystemProxyProfile, SystemProxyProfileInput,
    SystemProxyTarget, TcpForwardConfig, ToolServiceConfig, ToolServiceContentSource,
    ToolServiceInput, ToolServiceRouteInput, ToolServiceSummary, UdpForwardConfig,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

/// 初始数据库迁移 SQL。
const MIGRATION_0001: &str = include_str!("../migrations/0001_initial.sql");
/// 索引迁移 SQL。
const MIGRATION_0002: &str = include_str!("../migrations/0002_indexes.sql");
/// SSH 跳板 profile 迁移 SQL。
const MIGRATION_0003: &str = include_str!("../migrations/0003_ssh_jump_profiles.sql");
/// 本地工具 HTTP 服务持久化迁移 SQL。
const MIGRATION_0004: &str = include_str!("../migrations/0004_tool_services.sql");
/// 默认日志保留天数。
const DEFAULT_LOG_RETENTION_DAYS: u32 = 7;
/// 默认日志最大总行数。
const DEFAULT_LOG_MAX_ROWS: u32 = 20_000;
/// 防止异常设置让启动清理扫描过大范围。
const MAX_LOG_RETENTION_DAYS: u32 = 3_650;
/// 防止异常设置让本地数据库无限增长。
const MAX_LOG_ROWS: u32 = 1_000_000;
/// SSH 跳板链最大深度，避免循环引用或异常配置拖垮连接建立。
const MAX_SSH_JUMP_DEPTH: usize = 8;

/// SQLite 数据库句柄。
pub struct Database {
    conn: Mutex<Connection>,
}

include!("database/impl.rs");
include!("database/validation.rs");
include!("database/service_rows.rs");
include!("database/mappers.rs");
include!("database/tests.rs");
