//! @author kongweiguang
//! SQLite 数据访问层。所有核心配置写入都通过这里完成，前端不直接操作数据库。

use crate::error::{AppError, AppResult};
use crate::models::{
    AppSetting, BodyRewriteRule, BodyRewriteRuleInput, CreateServiceInput, HeaderRule,
    HeaderRuleInput, HttpForwardConfig, HttpReverseConfig, LogFilter, LogRow, ServiceDetail,
    ServiceKind, ServiceSummary, SshAuthType, SshProfile, SshProfileInput, SshProfileRuntimeConfig,
    SshTunnelConfig, SystemProxyProfile, SystemProxyProfileInput, SystemProxyTarget,
    TcpForwardConfig, UdpForwardConfig,
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

impl Database {
    /// 打开并迁移数据库文件。
    pub fn init(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::configure_and_migrate(conn)
    }

    /// 创建内存数据库，供单元测试使用。
    #[cfg(test)]
    pub fn in_memory() -> AppResult<Self> {
        Self::configure_and_migrate(Connection::open_in_memory()?)
    }

    fn configure_and_migrate(conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::migrate(&conn)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize_defaults()?;
        Ok(db)
    }

    fn migrate(conn: &Connection) -> AppResult<()> {
        let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(MIGRATION_0001)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            conn.execute_batch(MIGRATION_0002)?;
            conn.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            conn.execute_batch(MIGRATION_0003)?;
            conn.pragma_update(None, "user_version", 3)?;
        }
        Ok(())
    }

    fn conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|err| AppError::Message(format!("数据库连接被占用: {err}")))
    }

    /// 写入首次启动默认设置。
    pub fn initialize_defaults(&self) -> AppResult<()> {
        let conn = self.conn()?;
        let initialized: Option<String> = conn
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = 'app.initialized'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if initialized.is_some() {
            return Ok(());
        }

        let defaults = [
            ("app.initialized", "true"),
            ("logs.retention_days", "7"),
            ("logs.max_rows", "20000"),
            ("services.auto_start_enabled", "false"),
        ];
        for (key, value_json) in defaults {
            conn.execute(
                "INSERT INTO app_settings (key, value_json) VALUES (?1, ?2)",
                params![key, value_json],
            )?;
        }
        Ok(())
    }

    /// 读取全部应用设置。
    pub fn list_settings(&self) -> AppResult<Vec<AppSetting>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT key, value_json FROM app_settings ORDER BY key ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(AppSetting {
                key: row.get(0)?,
                value_json: row.get(1)?,
            })
        })?;
        rows_to_vec(rows)
    }

    /// 更新单个应用设置。
    pub fn update_setting(&self, key: &str, value_json: &str) -> AppResult<()> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("设置 key 不能为空".to_string()));
        }
        serde_json::from_str::<serde_json::Value>(value_json)
            .map_err(|_| AppError::InvalidInput("设置值必须是合法 JSON".to_string()))?;

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO app_settings (key, value_json, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
               value_json = excluded.value_json,
               updated_at = excluded.updated_at",
            params![key, value_json],
        )?;
        Ok(())
    }

    /// 按应用设置清理过期日志，并限制 service_events + connection_events 总行数。
    pub fn prune_logs_from_settings(&self) -> AppResult<()> {
        let settings = self.list_settings()?;
        let retention_days = setting_u32(
            &settings,
            "logs.retention_days",
            DEFAULT_LOG_RETENTION_DAYS,
            1,
            MAX_LOG_RETENTION_DAYS,
        );
        let max_rows = setting_u32(
            &settings,
            "logs.max_rows",
            DEFAULT_LOG_MAX_ROWS,
            1,
            MAX_LOG_ROWS,
        );
        self.prune_logs(retention_days, max_rows)
    }

    /// 清理过期日志，并在两个日志表之间按时间保留最新的 `max_rows` 条。
    pub fn prune_logs(&self, retention_days: u32, max_rows: u32) -> AppResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let retention_modifier = format!("-{} days", retention_days);
        tx.execute(
            "DELETE FROM service_events WHERE created_at < datetime('now', ?1)",
            params![retention_modifier],
        )?;
        tx.execute(
            "DELETE FROM connection_events WHERE created_at < datetime('now', ?1)",
            params![retention_modifier],
        )?;

        let rows_to_delete = {
            let mut stmt = tx.prepare(
                "SELECT source, id
                 FROM (
                   SELECT 'service' AS source, id, created_at FROM service_events
                   UNION ALL
                   SELECT 'connection' AS source, id, created_at FROM connection_events
                 )
                 ORDER BY created_at DESC, source DESC, id DESC
                 LIMIT -1 OFFSET ?1",
            )?;
            let rows = rows_to_vec(stmt.query_map(params![i64::from(max_rows)], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?)?;
            rows
        };
        for (source, id) in rows_to_delete {
            if source == "service" {
                tx.execute("DELETE FROM service_events WHERE id = ?1", params![id])?;
            } else {
                tx.execute("DELETE FROM connection_events WHERE id = ?1", params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 创建服务及其类型专用配置和规则。
    pub fn create_service(&self, input: &CreateServiceInput) -> AppResult<ServiceDetail> {
        validate_service_input(input, None)?;
        let id = Uuid::new_v4().to_string();
        {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            insert_service_tx(&tx, &id, input)?;
            tx.commit()?;
        }
        self.get_service(&id)
    }

    /// 更新服务。类型专用配置和规则采用整组替换，避免残留旧规则。
    pub fn update_service(&self, id: &str, input: &CreateServiceInput) -> AppResult<ServiceDetail> {
        validate_service_input(input, Some(id))?;
        {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM services WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(AppError::NotFound(format!("服务不存在: {id}")));
            }

            tx.execute(
                "UPDATE services
                 SET name = ?1, kind = ?2, enabled = ?3, auto_start = ?4,
                     listen_host = ?5, listen_port = ?6, notes = ?7, updated_at = datetime('now')
                 WHERE id = ?8",
                params![
                    input.name.trim(),
                    input.kind.as_str(),
                    bool_to_i64(input.enabled),
                    bool_to_i64(input.auto_start),
                    input.listen_host.trim(),
                    i64::from(input.listen_port),
                    input.notes.trim(),
                    id
                ],
            )?;
            delete_detail_rows_tx(&tx, id)?;
            insert_detail_rows_tx(&tx, id, input)?;
            tx.commit()?;
        }
        self.get_service(id)
    }

    /// 软删除服务，运行中的服务需要先由 ServiceManager 停止。
    pub fn delete_service(&self, id: &str) -> AppResult<()> {
        let conn = self.conn()?;
        let changed = conn.execute(
            "UPDATE services SET deleted_at = datetime('now'), updated_at = datetime('now')
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("服务不存在: {id}")));
        }
        Ok(())
    }

    /// 复制服务配置，复制后的服务默认不自动启动。
    pub fn duplicate_service(&self, id: &str) -> AppResult<ServiceDetail> {
        let detail = self.get_service(id)?;
        let input = CreateServiceInput {
            name: format!("{} 副本", detail.name),
            kind: detail.kind,
            enabled: detail.enabled,
            auto_start: false,
            listen_host: detail.listen_host,
            listen_port: detail.listen_port.saturating_add(1).max(1),
            notes: detail.notes,
            http_reverse: detail.http_reverse,
            http_forward: detail.http_forward,
            tcp_forward: detail.tcp_forward,
            udp_forward: detail.udp_forward,
            ssh_tunnel: detail.ssh_tunnel,
            header_rules: detail
                .header_rules
                .into_iter()
                .map(|rule| HeaderRuleInput {
                    phase: rule.phase,
                    action: rule.action,
                    name: rule.name,
                    value: rule.value,
                    enabled: rule.enabled,
                    sort_order: rule.sort_order,
                })
                .collect(),
            body_rewrite_rules: detail
                .body_rewrite_rules
                .into_iter()
                .map(|rule| BodyRewriteRuleInput {
                    body_type: rule.body_type,
                    path: rule.path,
                    value_json: rule.value_json,
                    enabled: rule.enabled,
                    sort_order: rule.sort_order,
                })
                .collect(),
        };
        self.create_service(&input)
    }

    /// 读取服务详情。
    pub fn get_service(&self, id: &str) -> AppResult<ServiceDetail> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, enabled, auto_start, listen_host, listen_port,
                    notes, created_at, updated_at
             FROM services
             WHERE id = ?1 AND deleted_at IS NULL",
        )?;
        let base = stmt
            .query_row(params![id], service_detail_from_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("服务不存在: {id}")))?;
        load_service_detail_children(&conn, base)
    }

    /// 读取全部未删除服务。
    pub fn list_services(&self) -> AppResult<Vec<ServiceDetail>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, enabled, auto_start, listen_host, listen_port,
                    notes, created_at, updated_at
             FROM services
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, created_at DESC",
        )?;
        let bases = rows_to_vec(stmt.query_map([], service_detail_from_row)?)?;
        bases
            .into_iter()
            .map(|base| load_service_detail_children(&conn, base))
            .collect()
    }

    /// 读取服务列表摘要，运行态由调用方补齐。
    pub fn list_service_summaries(&self) -> AppResult<Vec<ServiceSummary>> {
        self.list_services().map(|details| {
            details
                .into_iter()
                .map(|detail| {
                    let target_label = target_label(&detail);
                    ServiceSummary {
                        id: detail.id,
                        name: detail.name,
                        kind: detail.kind,
                        enabled: detail.enabled,
                        auto_start: detail.auto_start,
                        listen_host: detail.listen_host,
                        listen_port: detail.listen_port,
                        target_label,
                        runtime_status: Default::default(),
                        active_connections: 0,
                        total_connections: 0,
                    }
                })
                .collect()
        })
    }

    /// 记录服务级事件。
    pub fn insert_service_event(
        &self,
        service_id: Option<&str>,
        level: &str,
        message: &str,
        meta_json: &str,
    ) -> AppResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO service_events (service_id, level, message, meta_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![service_id, level, message, meta_json],
        )?;
        Ok(())
    }

    /// 记录连接事件。
    #[allow(clippy::too_many_arguments)]
    pub fn insert_connection_event(
        &self,
        service_id: &str,
        protocol: &str,
        remote_addr: &str,
        target_addr: &str,
        event_type: &str,
        method: &str,
        host: &str,
        path: &str,
        status_code: Option<u16>,
        bytes_in: u64,
        bytes_out: u64,
        duration_ms: u64,
        error: &str,
    ) -> AppResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO connection_events (
               service_id, protocol, remote_addr, target_addr, event_type, method,
               host, path, status_code, bytes_in, bytes_out, duration_ms, error
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                service_id,
                protocol,
                remote_addr,
                target_addr,
                event_type,
                method,
                host,
                path,
                status_code.map(i64::from),
                u64_to_i64(bytes_in),
                u64_to_i64(bytes_out),
                u64_to_i64(duration_ms),
                error
            ],
        )?;
        Ok(())
    }

    /// 查询服务日志和连接日志。协议筛选只作用于 connection_events。
    pub fn list_logs(&self, filter: &LogFilter) -> AppResult<Vec<LogRow>> {
        let conn = self.conn()?;
        let limit = filter.limit.unwrap_or(300).clamp(1, 2_000);
        let service_id = filter.service_id.as_deref().unwrap_or("");
        let level = filter.level.as_deref().unwrap_or("");
        let protocol = filter.protocol.as_deref().unwrap_or("");
        let keyword = filter.keyword.as_deref().unwrap_or("");
        let created_after = filter.created_after.as_deref().unwrap_or("");
        let created_before = filter.created_before.as_deref().unwrap_or("");
        let like = format!("%{keyword}%");
        let mut logs = Vec::new();

        if protocol.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, service_id, level, message, meta_json, created_at
                 FROM service_events
                 WHERE (?1 = '' OR service_id = ?1)
                   AND (?2 = '' OR level = ?2)
                   AND (?3 = '' OR message LIKE ?4 OR meta_json LIKE ?4)
                   AND (?5 = '' OR created_at >= datetime(?5))
                   AND (?6 = '' OR created_at <= datetime(?6))
                 ORDER BY id DESC
                 LIMIT ?7",
            )?;
            let service_rows = rows_to_vec(stmt.query_map(
                params![
                    service_id,
                    level,
                    keyword,
                    like,
                    created_after,
                    created_before,
                    limit
                ],
                |row| {
                    let id = row.get::<_, i64>(0)?;
                    Ok(LogRow {
                        id: id.saturating_mul(2),
                        service_id: row.get(1)?,
                        level: row.get(2)?,
                        message: row.get(3)?,
                        meta_json: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )?)?;
            logs.extend(service_rows);
        }

        let mut stmt = conn.prepare(
            "SELECT id, service_id, protocol, remote_addr, target_addr, event_type,
                    method, host, path, status_code, bytes_in, bytes_out, duration_ms,
                    error, created_at
             FROM connection_events
             WHERE (?1 = '' OR service_id = ?1)
               AND (?2 = '' OR protocol = ?2)
               AND (
                 ?3 = ''
                 OR (?3 = 'error' AND error <> '')
                 OR (?3 = 'info' AND error = '')
               )
               AND (
                 ?4 = ''
                 OR protocol LIKE ?5
                 OR remote_addr LIKE ?5
                 OR target_addr LIKE ?5
                 OR event_type LIKE ?5
                 OR method LIKE ?5
                 OR host LIKE ?5
                 OR path LIKE ?5
                 OR error LIKE ?5
               )
               AND (?6 = '' OR created_at >= datetime(?6))
               AND (?7 = '' OR created_at <= datetime(?7))
             ORDER BY id DESC
             LIMIT ?8",
        )?;
        let connection_rows = rows_to_vec(stmt.query_map(
            params![
                service_id,
                protocol,
                level,
                keyword,
                like,
                created_after,
                created_before,
                limit
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                ))
            },
        )?)?;
        for (
            id,
            service_id,
            protocol,
            remote_addr,
            target_addr,
            event_type,
            method,
            host,
            path,
            status_code,
            bytes_in,
            bytes_out,
            duration_ms,
            error,
            created_at,
        ) in connection_rows
        {
            logs.push(LogRow {
                id: id.saturating_mul(2).saturating_add(1),
                service_id,
                level: if error.is_empty() {
                    "info".to_string()
                } else {
                    "error".to_string()
                },
                message: connection_log_message(
                    &protocol,
                    &event_type,
                    &method,
                    &host,
                    &path,
                    &target_addr,
                    status_code,
                    bytes_in,
                    bytes_out,
                    duration_ms,
                    &error,
                ),
                meta_json: connection_log_meta_json(
                    &protocol,
                    &remote_addr,
                    &target_addr,
                    &event_type,
                    &method,
                    &host,
                    &path,
                    status_code,
                    bytes_in,
                    bytes_out,
                    duration_ms,
                    &error,
                ),
                created_at,
            });
        }

        logs.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        logs.truncate(limit as usize);
        Ok(logs)
    }

    /// 清理日志。service_id 为空时清空全部服务事件和连接事件。
    pub fn clear_logs(&self, service_id: Option<&str>) -> AppResult<()> {
        let conn = self.conn()?;
        match service_id {
            Some(id) => {
                conn.execute(
                    "DELETE FROM service_events WHERE service_id = ?1",
                    params![id],
                )?;
                conn.execute(
                    "DELETE FROM connection_events WHERE service_id = ?1",
                    params![id],
                )?;
            }
            None => {
                conn.execute("DELETE FROM service_events", [])?;
                conn.execute("DELETE FROM connection_events", [])?;
            }
        }
        Ok(())
    }

    /// 创建 SSH profile。敏感字段由调用方先加密为 secret id。
    pub fn create_ssh_profile(
        &self,
        input: &SshProfileInput,
        password_secret_id: Option<&str>,
        passphrase_secret_id: Option<&str>,
    ) -> AppResult<SshProfile> {
        validate_ssh_profile(input)?;
        if matches!(input.auth_type, SshAuthType::Password) && password_secret_id.is_none() {
            return Err(AppError::InvalidInput(
                "密码认证需要保存密码 secret".to_string(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        {
            let conn = self.conn()?;
            validate_ssh_jump_reference(&conn, None, input.jump_profile_id.as_deref())?;
            conn.execute(
                "INSERT INTO ssh_profiles (
                   id, name, host, port, username, auth_type, password_secret_id,
                   private_key_path, private_key_passphrase_secret_id, known_hosts_mode,
                   known_hosts_path, connect_timeout_ms, keepalive_interval_ms, jump_profile_id
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    id,
                    input.name.trim(),
                    input.host.trim(),
                    i64::from(input.port),
                    input.username.trim(),
                    input.auth_type.as_str(),
                    password_secret_id,
                    input.private_key_path.as_deref(),
                    passphrase_secret_id,
                    input.known_hosts_mode.trim(),
                    input.known_hosts_path.as_deref(),
                    u64_to_i64(input.connect_timeout_ms),
                    u64_to_i64(input.keepalive_interval_ms),
                    normalized_optional(input.jump_profile_id.as_deref()),
                ],
            )?;
        }
        self.get_ssh_profile(&id)
    }

    /// 更新 SSH profile。secret 参数为 None 时保留旧 secret。
    pub fn update_ssh_profile(
        &self,
        id: &str,
        input: &SshProfileInput,
        password_secret_id: Option<&str>,
        passphrase_secret_id: Option<&str>,
    ) -> AppResult<SshProfile> {
        validate_ssh_profile(input)?;
        {
            let conn = self.conn()?;
            let existing: (Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT password_secret_id, private_key_passphrase_secret_id
                     FROM ssh_profiles WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("SSH Profile 不存在: {id}")))?;
            let password_id = password_secret_id
                .map(ToOwned::to_owned)
                .or(existing.0)
                .filter(|_| matches!(input.auth_type, SshAuthType::Password));
            if matches!(input.auth_type, SshAuthType::Password) && password_id.is_none() {
                return Err(AppError::InvalidInput(
                    "密码认证需要保存密码 secret".to_string(),
                ));
            }
            validate_ssh_jump_reference(&conn, Some(id), input.jump_profile_id.as_deref())?;
            let passphrase_id = passphrase_secret_id.map(ToOwned::to_owned).or(existing.1);
            conn.execute(
                "UPDATE ssh_profiles
                 SET name = ?1, host = ?2, port = ?3, username = ?4, auth_type = ?5,
                     password_secret_id = ?6, private_key_path = ?7,
                     private_key_passphrase_secret_id = ?8, known_hosts_mode = ?9,
                     known_hosts_path = ?10, connect_timeout_ms = ?11,
                     keepalive_interval_ms = ?12, jump_profile_id = ?13, updated_at = datetime('now')
                 WHERE id = ?14",
                params![
                    input.name.trim(),
                    input.host.trim(),
                    i64::from(input.port),
                    input.username.trim(),
                    input.auth_type.as_str(),
                    password_id,
                    input.private_key_path.as_deref(),
                    passphrase_id,
                    input.known_hosts_mode.trim(),
                    input.known_hosts_path.as_deref(),
                    u64_to_i64(input.connect_timeout_ms),
                    u64_to_i64(input.keepalive_interval_ms),
                    normalized_optional(input.jump_profile_id.as_deref()),
                    id,
                ],
            )?;
        }
        self.get_ssh_profile(id)
    }

    /// 软删除 SSH profile。
    pub fn delete_ssh_profile(&self, id: &str) -> AppResult<()> {
        let conn = self.conn()?;
        let used: Option<String> = conn
            .query_row(
                "SELECT ref_id FROM (
                   SELECT service_id AS ref_id FROM ssh_tunnel_configs st
                   JOIN services s ON s.id = st.service_id
                   WHERE st.ssh_profile_id = ?1 AND s.deleted_at IS NULL
                   UNION ALL
                   SELECT id AS ref_id FROM ssh_profiles
                   WHERE jump_profile_id = ?1 AND deleted_at IS NULL
                 )
                 LIMIT 1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if used.is_some() {
            return Err(AppError::Conflict(
                "SSH Profile 正在被隧道服务或跳板配置引用，不能删除".to_string(),
            ));
        }
        let changed = conn.execute(
            "UPDATE ssh_profiles SET deleted_at = datetime('now'), updated_at = datetime('now')
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("SSH Profile 不存在: {id}")));
        }
        Ok(())
    }

    /// 读取全部 SSH profile。
    pub fn list_ssh_profiles(&self) -> AppResult<Vec<SshProfile>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_type, password_secret_id,
                    private_key_path, private_key_passphrase_secret_id, known_hosts_mode,
                    known_hosts_path, connect_timeout_ms, keepalive_interval_ms, jump_profile_id
             FROM ssh_profiles
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, created_at DESC",
        )?;
        let rows = stmt.query_map([], ssh_profile_from_row)?;
        rows_to_vec(rows)
    }

    /// 读取单个 SSH profile。
    pub fn get_ssh_profile(&self, id: &str) -> AppResult<SshProfile> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_type, password_secret_id,
                    private_key_path, private_key_passphrase_secret_id, known_hosts_mode,
                    known_hosts_path, connect_timeout_ms, keepalive_interval_ms, jump_profile_id
             FROM ssh_profiles
             WHERE id = ?1 AND deleted_at IS NULL",
        )?;
        stmt.query_row(params![id], ssh_profile_from_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("SSH Profile 不存在: {id}")))
    }

    /// 读取运行 SSH 隧道所需的完整 profile。该结构只在 Rust 内部流转。
    pub fn get_ssh_profile_runtime_config(&self, id: &str) -> AppResult<SshProfileRuntimeConfig> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, username, auth_type, password_secret_id,
                    private_key_path, private_key_passphrase_secret_id, known_hosts_mode,
                    known_hosts_path, connect_timeout_ms, keepalive_interval_ms, jump_profile_id
             FROM ssh_profiles
             WHERE id = ?1 AND deleted_at IS NULL",
        )?;
        stmt.query_row(params![id], ssh_profile_runtime_from_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("SSH Profile 不存在: {id}")))
    }

    /// 解析 SSH profile 的运行时连接链。返回顺序是第一跳到最终目标。
    pub fn resolve_ssh_profile_runtime_chain(
        &self,
        id: &str,
    ) -> AppResult<Vec<SshProfileRuntimeConfig>> {
        let mut chain = Vec::new();
        let mut current_id = Some(id.to_string());
        let mut visited = HashSet::new();

        while let Some(profile_id) = current_id {
            if !visited.insert(profile_id.clone()) {
                return Err(AppError::InvalidInput("SSH 跳板链存在循环引用".to_string()));
            }
            if chain.len() >= MAX_SSH_JUMP_DEPTH {
                return Err(AppError::InvalidInput(format!(
                    "SSH 跳板链最多支持 {MAX_SSH_JUMP_DEPTH} 级"
                )));
            }
            let profile = self.get_ssh_profile_runtime_config(&profile_id)?;
            current_id =
                normalized_optional(profile.jump_profile_id.as_deref()).map(ToOwned::to_owned);
            chain.push(profile);
        }

        chain.reverse();
        Ok(chain)
    }

    /// 保存加密 secret。
    pub fn insert_secret(
        &self,
        name: &str,
        kind: &str,
        ciphertext: &[u8],
        nonce: &[u8],
    ) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO secrets (id, name, kind, ciphertext, nonce)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, kind, ciphertext, nonce],
        )?;
        Ok(id)
    }

    /// 读取加密 secret。
    pub fn get_secret(&self, id: &str) -> AppResult<(Vec<u8>, Vec<u8>)> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT ciphertext, nonce FROM secrets WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Secret 不存在: {id}")))
    }

    /// 解析系统代理目标。传入值可以是系统代理配置 id，也可以是 HTTP forward 服务 id。
    pub fn get_system_proxy_target(&self, profile_id: &str) -> AppResult<SystemProxyTarget> {
        {
            let conn = self.conn()?;
            if let Some(target) = conn
                .query_row(
                    "SELECT proxy_host, proxy_port, bypass
                     FROM system_proxy_profiles
                     WHERE id = ?1",
                    params![profile_id],
                    |row| {
                        Ok(SystemProxyTarget {
                            proxy_host: row.get(0)?,
                            proxy_port: i64_to_u16(row.get(1)?),
                            bypass: row.get(2)?,
                        })
                    },
                )
                .optional()?
            {
                return Ok(target);
            }
        }

        let detail = self.get_service(profile_id)?;
        if detail.kind != ServiceKind::HttpForward {
            return Err(AppError::InvalidInput(
                "系统代理只能指向 HTTP forward 服务或系统代理配置".to_string(),
            ));
        }
        Ok(SystemProxyTarget {
            proxy_host: detail.listen_host,
            proxy_port: detail.listen_port,
            bypass: String::new(),
        })
    }

    /// 读取全部系统代理配置档。
    pub fn list_system_proxy_profiles(&self) -> AppResult<Vec<SystemProxyProfile>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, proxy_host, proxy_port, bypass, active, created_at, updated_at
             FROM system_proxy_profiles
             ORDER BY active DESC, updated_at DESC, created_at DESC",
        )?;
        let rows = stmt.query_map([], system_proxy_profile_from_row)?;
        rows_to_vec(rows)
    }

    /// 创建系统代理配置档。
    pub fn create_system_proxy_profile(
        &self,
        input: &SystemProxyProfileInput,
    ) -> AppResult<SystemProxyProfile> {
        validate_system_proxy_profile_input(input)?;
        let id = Uuid::new_v4().to_string();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO system_proxy_profiles (id, name, proxy_host, proxy_port, bypass, active)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                id,
                input.name.trim(),
                input.proxy_host.trim(),
                i64::from(input.proxy_port),
                input.bypass.trim()
            ],
        )?;
        get_system_proxy_profile_by_id(&conn, &id)
    }

    /// 更新系统代理配置档。
    pub fn update_system_proxy_profile(
        &self,
        id: &str,
        input: &SystemProxyProfileInput,
    ) -> AppResult<SystemProxyProfile> {
        validate_system_proxy_profile_input(input)?;
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE system_proxy_profiles
             SET name = ?1, proxy_host = ?2, proxy_port = ?3, bypass = ?4, updated_at = datetime('now')
             WHERE id = ?5",
            params![
                input.name.trim(),
                input.proxy_host.trim(),
                i64::from(input.proxy_port),
                input.bypass.trim(),
                id
            ],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("系统代理配置档不存在: {id}")));
        }
        get_system_proxy_profile_by_id(&conn, id)
    }

    /// 删除系统代理配置档。
    pub fn delete_system_proxy_profile(&self, id: &str) -> AppResult<()> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "DELETE FROM system_proxy_profiles WHERE id = ?1",
            params![id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("系统代理配置档不存在: {id}")));
        }
        Ok(())
    }

    /// 标记最近一次通过配置档启用的系统代理。传 None 表示清空配置档激活态。
    pub fn set_active_system_proxy_profile(&self, id: Option<&str>) -> AppResult<()> {
        let conn = self.conn()?;
        conn.execute("UPDATE system_proxy_profiles SET active = 0", [])?;
        if let Some(id) = id {
            conn.execute(
                "UPDATE system_proxy_profiles SET active = 1, updated_at = datetime('now') WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }
}

fn validate_service_input(input: &CreateServiceInput, current_id: Option<&str>) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("服务名称不能为空".to_string()));
    }
    if input.listen_host.trim().is_empty() {
        return Err(AppError::InvalidInput("监听 host 不能为空".to_string()));
    }
    if input.listen_port == 0 {
        return Err(AppError::InvalidInput(
            "监听端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    match input.kind {
        ServiceKind::HttpReverse => {
            let cfg = input
                .http_reverse
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP 反向代理缺少目标配置".to_string()))?;
            let url = url::Url::parse(&cfg.target_url)?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(AppError::InvalidInput(
                    "HTTP 反向代理目标 URL 必须是 http 或 https".to_string(),
                ));
            }
        }
        ServiceKind::HttpForward => {
            let cfg = input
                .http_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP Forward 缺少配置".to_string()))?;
            if !cfg.allow_http && !cfg.allow_connect {
                return Err(AppError::InvalidInput(
                    "HTTP Forward 至少需要允许 HTTP 或 CONNECT".to_string(),
                ));
            }
        }
        ServiceKind::TcpForward => {
            let cfg = input
                .tcp_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("TCP 转发缺少目标配置".to_string()))?;
            validate_target(&cfg.target_host, cfg.target_port)?;
        }
        ServiceKind::UdpForward => {
            let cfg = input
                .udp_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("UDP 转发缺少目标配置".to_string()))?;
            validate_target(&cfg.target_host, cfg.target_port)?;
        }
        ServiceKind::SshLocal => {
            let cfg = input
                .ssh_tunnel
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("SSH 隧道缺少配置".to_string()))?;
            if cfg.ssh_profile_id.trim().is_empty() {
                return Err(AppError::InvalidInput("SSH Profile 不能为空".to_string()));
            }
            if cfg.tunnel_type != "local" {
                return Err(AppError::InvalidInput(
                    "SSH 本地隧道类型必须是 local".to_string(),
                ));
            }
            validate_target(
                cfg.target_host.as_deref().unwrap_or(""),
                cfg.target_port.unwrap_or(0),
            )?;
        }
        ServiceKind::SshSocks => {
            let cfg = input
                .ssh_tunnel
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("SSH SOCKS 缺少配置".to_string()))?;
            if cfg.ssh_profile_id.trim().is_empty() {
                return Err(AppError::InvalidInput("SSH Profile 不能为空".to_string()));
            }
            if cfg.tunnel_type != "socks" {
                return Err(AppError::InvalidInput(
                    "SSH SOCKS 隧道类型必须是 socks".to_string(),
                ));
            }
        }
        ServiceKind::SshRemote => {
            let cfg = input
                .ssh_tunnel
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少配置".to_string()))?;
            if cfg.ssh_profile_id.trim().is_empty() {
                return Err(AppError::InvalidInput("SSH Profile 不能为空".to_string()));
            }
            if cfg.tunnel_type != "remote" {
                return Err(AppError::InvalidInput(
                    "SSH remote 隧道类型必须是 remote".to_string(),
                ));
            }
            validate_target(
                cfg.target_host.as_deref().unwrap_or(""),
                cfg.target_port.unwrap_or(0),
            )?;
            validate_remote_bind(
                cfg.remote_bind_host.as_deref().unwrap_or(""),
                cfg.remote_bind_port.unwrap_or(0),
            )?;
        }
    }

    for rule in &input.header_rules {
        if !matches!(rule.phase.as_str(), "request" | "response") {
            return Err(AppError::InvalidInput(
                "Header phase 必须是 request 或 response".to_string(),
            ));
        }
        if !matches!(rule.action.as_str(), "set" | "remove") {
            return Err(AppError::InvalidInput(
                "Header action 必须是 set 或 remove".to_string(),
            ));
        }
        if rule.name.trim().is_empty() {
            return Err(AppError::InvalidInput("Header 名称不能为空".to_string()));
        }
    }
    for rule in &input.body_rewrite_rules {
        if !matches!(rule.body_type.as_str(), "auto" | "json" | "form") {
            return Err(AppError::InvalidInput(
                "Body 类型必须是 auto/json/form".to_string(),
            ));
        }
        if rule.path.trim().is_empty() {
            return Err(AppError::InvalidInput("Body 改写路径不能为空".to_string()));
        }
        serde_json::from_str::<serde_json::Value>(&rule.value_json).map_err(|_| {
            AppError::InvalidInput(format!("Body 改写值不是合法 JSON: {}", rule.path))
        })?;
    }
    let _ = current_id;
    Ok(())
}

fn validate_target(host: &str, port: u16) -> AppResult<()> {
    if host.trim().is_empty() {
        return Err(AppError::InvalidInput("目标 host 不能为空".to_string()));
    }
    if port == 0 {
        return Err(AppError::InvalidInput(
            "目标端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(())
}

fn validate_remote_bind(host: &str, port: u16) -> AppResult<()> {
    if host.trim().is_empty() {
        return Err(AppError::InvalidInput("远程绑定 host 不能为空".to_string()));
    }
    if port == 0 {
        return Err(AppError::InvalidInput(
            "远程绑定端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(())
}

fn validate_ssh_profile(input: &SshProfileInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "SSH Profile 名称不能为空".to_string(),
        ));
    }
    validate_target(&input.host, input.port)?;
    if input.username.trim().is_empty() {
        return Err(AppError::InvalidInput("SSH 用户名不能为空".to_string()));
    }
    if !matches!(
        input.known_hosts_mode.as_str(),
        "strict" | "accept_new" | "insecure_skip"
    ) {
        return Err(AppError::InvalidInput(
            "known_hosts 模式必须是 strict/accept_new/insecure_skip".to_string(),
        ));
    }
    if matches!(input.auth_type, SshAuthType::PrivateKey)
        && input
            .private_key_path
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(AppError::InvalidInput(
            "私钥认证需要填写私钥路径".to_string(),
        ));
    }
    Ok(())
}

fn validate_ssh_jump_reference(
    conn: &Connection,
    current_profile_id: Option<&str>,
    jump_profile_id: Option<&str>,
) -> AppResult<()> {
    let Some(jump_profile_id) = normalized_optional(jump_profile_id) else {
        return Ok(());
    };
    if current_profile_id.is_some_and(|current| current == jump_profile_id) {
        return Err(AppError::InvalidInput(
            "SSH Profile 不能选择自己作为跳板".to_string(),
        ));
    }

    let mut next_id = Some(jump_profile_id.to_string());
    let mut visited = HashSet::new();
    while let Some(profile_id) = next_id {
        if !visited.insert(profile_id.clone()) {
            return Err(AppError::InvalidInput("SSH 跳板链存在循环引用".to_string()));
        }
        if visited.len() > MAX_SSH_JUMP_DEPTH {
            return Err(AppError::InvalidInput(format!(
                "SSH 跳板链最多支持 {MAX_SSH_JUMP_DEPTH} 级"
            )));
        }
        if current_profile_id.is_some_and(|current| current == profile_id) {
            return Err(AppError::InvalidInput(
                "SSH 跳板链不能形成循环引用".to_string(),
            ));
        }
        let profile_jump: Option<Option<String>> = conn
            .query_row(
                "SELECT jump_profile_id FROM ssh_profiles
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![profile_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(profile_jump) = profile_jump else {
            return Err(AppError::InvalidInput(format!(
                "跳板 SSH Profile 不存在: {jump_profile_id}"
            )));
        };
        next_id = normalized_optional(profile_jump.as_deref()).map(ToOwned::to_owned);
    }
    Ok(())
}

fn validate_system_proxy_profile_input(input: &SystemProxyProfileInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "系统代理配置档名称不能为空".to_string(),
        ));
    }
    if input.proxy_host.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "系统代理配置档主机不能为空".to_string(),
        ));
    }
    if input.proxy_port == 0 {
        return Err(AppError::InvalidInput(
            "系统代理配置档端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(())
}

fn insert_service_tx(tx: &Transaction<'_>, id: &str, input: &CreateServiceInput) -> AppResult<()> {
    tx.execute(
        "INSERT INTO services (
           id, name, kind, enabled, auto_start, listen_host, listen_port, notes
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            input.name.trim(),
            input.kind.as_str(),
            bool_to_i64(input.enabled),
            bool_to_i64(input.auto_start),
            input.listen_host.trim(),
            i64::from(input.listen_port),
            input.notes.trim(),
        ],
    )?;
    insert_detail_rows_tx(tx, id, input)
}

fn delete_detail_rows_tx(tx: &Transaction<'_>, id: &str) -> AppResult<()> {
    for table in [
        "http_reverse_configs",
        "http_forward_configs",
        "tcp_forward_configs",
        "udp_forward_configs",
        "ssh_tunnel_configs",
        "http_header_rules",
        "body_rewrite_rules",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE service_id = ?1"),
            params![id],
        )?;
    }
    Ok(())
}

fn insert_detail_rows_tx(
    tx: &Transaction<'_>,
    id: &str,
    input: &CreateServiceInput,
) -> AppResult<()> {
    match input.kind {
        ServiceKind::HttpReverse => {
            let cfg = input
                .http_reverse
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP 反向代理缺少目标配置".to_string()))?;
            tx.execute(
                "INSERT INTO http_reverse_configs (
                   service_id, target_url, preserve_host, request_timeout_ms,
                   max_rewrite_body_bytes, skip_compressed_body
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    cfg.target_url.trim(),
                    bool_to_i64(cfg.preserve_host),
                    u64_to_i64(cfg.request_timeout_ms),
                    u64_to_i64(cfg.max_rewrite_body_bytes),
                    bool_to_i64(cfg.skip_compressed_body),
                ],
            )?;
        }
        ServiceKind::HttpForward => {
            let cfg = input
                .http_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("HTTP Forward 缺少配置".to_string()))?;
            tx.execute(
                "INSERT INTO http_forward_configs (
                   service_id, allow_http, allow_connect, connect_timeout_ms, idle_timeout_ms
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    bool_to_i64(cfg.allow_http),
                    bool_to_i64(cfg.allow_connect),
                    u64_to_i64(cfg.connect_timeout_ms),
                    u64_to_i64(cfg.idle_timeout_ms),
                ],
            )?;
        }
        ServiceKind::TcpForward => {
            let cfg = input
                .tcp_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("TCP 转发缺少目标配置".to_string()))?;
            tx.execute(
                "INSERT INTO tcp_forward_configs (
                   service_id, target_host, target_port, connect_timeout_ms, idle_timeout_ms
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    cfg.target_host.trim(),
                    i64::from(cfg.target_port),
                    u64_to_i64(cfg.connect_timeout_ms),
                    u64_to_i64(cfg.idle_timeout_ms),
                ],
            )?;
        }
        ServiceKind::UdpForward => {
            let cfg = input
                .udp_forward
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("UDP 转发缺少目标配置".to_string()))?;
            tx.execute(
                "INSERT INTO udp_forward_configs (
                   service_id, target_host, target_port, idle_timeout_ms
                 )
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    cfg.target_host.trim(),
                    i64::from(cfg.target_port),
                    u64_to_i64(cfg.idle_timeout_ms),
                ],
            )?;
        }
        ServiceKind::SshLocal | ServiceKind::SshRemote | ServiceKind::SshSocks => {
            let cfg = input
                .ssh_tunnel
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput("SSH 隧道缺少配置".to_string()))?;
            tx.execute(
                "INSERT INTO ssh_tunnel_configs (
                   service_id, ssh_profile_id, tunnel_type, target_host, target_port,
                   remote_bind_host, remote_bind_port
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    cfg.ssh_profile_id.trim(),
                    cfg.tunnel_type.trim(),
                    cfg.target_host.as_deref(),
                    cfg.target_port.map(i64::from),
                    cfg.remote_bind_host.as_deref(),
                    cfg.remote_bind_port.map(i64::from),
                ],
            )?;
        }
    }

    for rule in &input.header_rules {
        tx.execute(
            "INSERT INTO http_header_rules (
               id, service_id, phase, action, name, value, enabled, sort_order
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Uuid::new_v4().to_string(),
                id,
                rule.phase.trim(),
                rule.action.trim(),
                rule.name.trim(),
                rule.value.as_deref(),
                bool_to_i64(rule.enabled),
                rule.sort_order,
            ],
        )?;
    }
    for rule in &input.body_rewrite_rules {
        tx.execute(
            "INSERT INTO body_rewrite_rules (
               id, service_id, body_type, path, value_json, enabled, sort_order
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                id,
                rule.body_type.trim(),
                rule.path.trim(),
                rule.value_json.trim(),
                bool_to_i64(rule.enabled),
                rule.sort_order,
            ],
        )?;
    }
    Ok(())
}

fn load_service_detail_children(
    conn: &Connection,
    mut detail: ServiceDetail,
) -> AppResult<ServiceDetail> {
    detail.http_reverse = conn
        .query_row(
            "SELECT target_url, preserve_host, request_timeout_ms, max_rewrite_body_bytes,
                    skip_compressed_body
             FROM http_reverse_configs WHERE service_id = ?1",
            params![detail.id],
            |row| {
                Ok(HttpReverseConfig {
                    target_url: row.get(0)?,
                    preserve_host: i64_to_bool(row.get(1)?),
                    request_timeout_ms: i64_to_u64(row.get(2)?),
                    max_rewrite_body_bytes: i64_to_u64(row.get(3)?),
                    skip_compressed_body: i64_to_bool(row.get(4)?),
                })
            },
        )
        .optional()?;
    detail.http_forward = conn
        .query_row(
            "SELECT allow_http, allow_connect, connect_timeout_ms, idle_timeout_ms
             FROM http_forward_configs WHERE service_id = ?1",
            params![detail.id],
            |row| {
                Ok(HttpForwardConfig {
                    allow_http: i64_to_bool(row.get(0)?),
                    allow_connect: i64_to_bool(row.get(1)?),
                    connect_timeout_ms: i64_to_u64(row.get(2)?),
                    idle_timeout_ms: i64_to_u64(row.get(3)?),
                })
            },
        )
        .optional()?;
    detail.tcp_forward = conn
        .query_row(
            "SELECT target_host, target_port, connect_timeout_ms, idle_timeout_ms
             FROM tcp_forward_configs WHERE service_id = ?1",
            params![detail.id],
            |row| {
                Ok(TcpForwardConfig {
                    target_host: row.get(0)?,
                    target_port: i64_to_u16(row.get(1)?),
                    connect_timeout_ms: i64_to_u64(row.get(2)?),
                    idle_timeout_ms: i64_to_u64(row.get(3)?),
                })
            },
        )
        .optional()?;
    detail.udp_forward = conn
        .query_row(
            "SELECT target_host, target_port, idle_timeout_ms
             FROM udp_forward_configs WHERE service_id = ?1",
            params![detail.id],
            |row| {
                Ok(UdpForwardConfig {
                    target_host: row.get(0)?,
                    target_port: i64_to_u16(row.get(1)?),
                    idle_timeout_ms: i64_to_u64(row.get(2)?),
                })
            },
        )
        .optional()?;
    detail.ssh_tunnel = conn
        .query_row(
            "SELECT ssh_profile_id, tunnel_type, target_host, target_port,
                    remote_bind_host, remote_bind_port
             FROM ssh_tunnel_configs WHERE service_id = ?1",
            params![detail.id],
            |row| {
                Ok(SshTunnelConfig {
                    ssh_profile_id: row.get(0)?,
                    tunnel_type: row.get(1)?,
                    target_host: row.get(2)?,
                    target_port: row.get::<_, Option<i64>>(3)?.map(i64_to_u16),
                    remote_bind_host: row.get(4)?,
                    remote_bind_port: row.get::<_, Option<i64>>(5)?.map(i64_to_u16),
                })
            },
        )
        .optional()?;

    let mut header_stmt = conn.prepare(
        "SELECT id, phase, action, name, value, enabled, sort_order
         FROM http_header_rules
         WHERE service_id = ?1
         ORDER BY sort_order ASC, created_at ASC",
    )?;
    detail.header_rules = rows_to_vec(header_stmt.query_map(params![detail.id], |row| {
        Ok(HeaderRule {
            id: row.get(0)?,
            phase: row.get(1)?,
            action: row.get(2)?,
            name: row.get(3)?,
            value: row.get(4)?,
            enabled: i64_to_bool(row.get(5)?),
            sort_order: row.get(6)?,
        })
    })?)?;

    let mut body_stmt = conn.prepare(
        "SELECT id, body_type, path, value_json, enabled, sort_order
         FROM body_rewrite_rules
         WHERE service_id = ?1
         ORDER BY sort_order ASC, created_at ASC",
    )?;
    detail.body_rewrite_rules = rows_to_vec(body_stmt.query_map(params![detail.id], |row| {
        Ok(BodyRewriteRule {
            id: row.get(0)?,
            body_type: row.get(1)?,
            path: row.get(2)?,
            value_json: row.get(3)?,
            enabled: i64_to_bool(row.get(4)?),
            sort_order: row.get(5)?,
        })
    })?)?;

    Ok(detail)
}

fn service_detail_from_row(row: &Row<'_>) -> rusqlite::Result<ServiceDetail> {
    let kind_raw: String = row.get(2)?;
    let kind = ServiceKind::try_from(kind_raw.as_str()).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
        )
    })?;
    Ok(ServiceDetail {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        enabled: i64_to_bool(row.get(3)?),
        auto_start: i64_to_bool(row.get(4)?),
        listen_host: row.get(5)?,
        listen_port: i64_to_u16(row.get(6)?),
        notes: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        http_reverse: None,
        http_forward: None,
        tcp_forward: None,
        udp_forward: None,
        ssh_tunnel: None,
        header_rules: Vec::new(),
        body_rewrite_rules: Vec::new(),
    })
}

fn ssh_profile_from_row(row: &Row<'_>) -> rusqlite::Result<SshProfile> {
    let auth_raw: String = row.get(5)?;
    let auth_type = SshAuthType::try_from(auth_raw.as_str()).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
        )
    })?;
    let password_secret_id: Option<String> = row.get(6)?;
    let passphrase_secret_id: Option<String> = row.get(8)?;
    Ok(SshProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: i64_to_u16(row.get(3)?),
        username: row.get(4)?,
        auth_type,
        has_password: password_secret_id.is_some(),
        private_key_path: row.get(7)?,
        has_passphrase: passphrase_secret_id.is_some(),
        known_hosts_mode: row.get(9)?,
        known_hosts_path: row.get(10)?,
        connect_timeout_ms: i64_to_u64(row.get(11)?),
        keepalive_interval_ms: i64_to_u64(row.get(12)?),
        jump_profile_id: row.get(13)?,
    })
}

fn ssh_profile_runtime_from_row(row: &Row<'_>) -> rusqlite::Result<SshProfileRuntimeConfig> {
    let auth_raw: String = row.get(5)?;
    let auth_type = SshAuthType::try_from(auth_raw.as_str()).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
        )
    })?;
    Ok(SshProfileRuntimeConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: i64_to_u16(row.get(3)?),
        username: row.get(4)?,
        auth_type,
        password_secret_id: row.get(6)?,
        private_key_path: row.get(7)?,
        private_key_passphrase_secret_id: row.get(8)?,
        known_hosts_mode: row.get(9)?,
        known_hosts_path: row.get(10)?,
        connect_timeout_ms: i64_to_u64(row.get(11)?),
        keepalive_interval_ms: i64_to_u64(row.get(12)?),
        jump_profile_id: row.get(13)?,
    })
}

fn system_proxy_profile_from_row(row: &Row<'_>) -> rusqlite::Result<SystemProxyProfile> {
    Ok(SystemProxyProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        proxy_host: row.get(2)?,
        proxy_port: i64_to_u16(row.get(3)?),
        bypass: row.get(4)?,
        active: i64_to_bool(row.get(5)?),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn get_system_proxy_profile_by_id(conn: &Connection, id: &str) -> AppResult<SystemProxyProfile> {
    conn.query_row(
        "SELECT id, name, proxy_host, proxy_port, bypass, active, created_at, updated_at
         FROM system_proxy_profiles
         WHERE id = ?1",
        params![id],
        system_proxy_profile_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("系统代理配置档不存在: {id}")))
}

#[allow(clippy::too_many_arguments)]
fn connection_log_message(
    protocol: &str,
    event_type: &str,
    method: &str,
    host: &str,
    path: &str,
    target_addr: &str,
    status_code: Option<i64>,
    bytes_in: i64,
    bytes_out: i64,
    duration_ms: i64,
    error: &str,
) -> String {
    let target = if !host.is_empty() {
        format!("{host}{path}")
    } else if !target_addr.is_empty() {
        target_addr.to_string()
    } else {
        "-".to_string()
    };
    let action = if method.is_empty() {
        event_type
    } else {
        method
    };
    if error.is_empty() {
        let status = status_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        format!("{protocol} {action} {target} -> {status} ({bytes_in}B in, {bytes_out}B out, {duration_ms}ms)")
    } else {
        format!("{protocol} {action} {target} failed: {error}")
    }
}

#[allow(clippy::too_many_arguments)]
fn connection_log_meta_json(
    protocol: &str,
    remote_addr: &str,
    target_addr: &str,
    event_type: &str,
    method: &str,
    host: &str,
    path: &str,
    status_code: Option<i64>,
    bytes_in: i64,
    bytes_out: i64,
    duration_ms: i64,
    error: &str,
) -> String {
    serde_json::json!({
        "source": "connection_events",
        "protocol": protocol,
        "remoteAddr": remote_addr,
        "targetAddr": target_addr,
        "eventType": event_type,
        "method": method,
        "host": host,
        "path": path,
        "statusCode": status_code,
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "durationMs": duration_ms,
        "error": error,
    })
    .to_string()
}

fn setting_u32(
    settings: &[AppSetting],
    key: &str,
    default_value: u32,
    min_value: u32,
    max_value: u32,
) -> u32 {
    settings
        .iter()
        .find(|setting| setting.key == key)
        .and_then(|setting| serde_json::from_str::<u32>(&setting.value_json).ok())
        .unwrap_or(default_value)
        .clamp(min_value, max_value)
}

fn target_label(detail: &ServiceDetail) -> String {
    if let Some(cfg) = &detail.http_reverse {
        return cfg.target_url.clone();
    }
    if detail.http_forward.is_some() {
        return "动态 HTTP/CONNECT".to_string();
    }
    if let Some(cfg) = &detail.tcp_forward {
        return format!("{}:{}", cfg.target_host, cfg.target_port);
    }
    if let Some(cfg) = &detail.udp_forward {
        return format!("{}:{}", cfg.target_host, cfg.target_port);
    }
    if let Some(cfg) = &detail.ssh_tunnel {
        return match cfg.tunnel_type.as_str() {
            "socks" => "SOCKS5 动态代理".to_string(),
            "remote" => format!(
                "SSH remote {}:{} -> {}:{}",
                cfg.remote_bind_host.as_deref().unwrap_or(""),
                cfg.remote_bind_port.unwrap_or_default(),
                cfg.target_host.as_deref().unwrap_or(""),
                cfg.target_port.unwrap_or_default()
            ),
            _ => format!(
                "SSH -> {}:{}",
                cfg.target_host.as_deref().unwrap_or(""),
                cfg.target_port.unwrap_or_default()
            ),
        };
    }
    String::new()
}

fn rows_to_vec<T, F>(rows: rusqlite::MappedRows<'_, F>) -> AppResult<Vec<T>>
where
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
{
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

fn i64_to_u16(value: i64) -> u16 {
    value.clamp(0, i64::from(u16::MAX)) as u16
}

fn i64_to_u64(value: i64) -> u64 {
    value.max(0) as u64
}

fn u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "database_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "database_ssh_jump_tests.rs"]
mod ssh_jump_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_http_service() -> CreateServiceInput {
        CreateServiceInput {
            name: "测试反向代理".to_string(),
            kind: ServiceKind::HttpReverse,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port: 18080,
            notes: String::new(),
            http_reverse: Some(HttpReverseConfig {
                target_url: "http://127.0.0.1:19090".to_string(),
                preserve_host: false,
                request_timeout_ms: 30_000,
                max_rewrite_body_bytes: 10_485_760,
                skip_compressed_body: true,
            }),
            http_forward: None,
            tcp_forward: None,
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: vec![HeaderRuleInput {
                phase: "request".to_string(),
                action: "set".to_string(),
                name: "x-test".to_string(),
                value: Some("ok".to_string()),
                enabled: true,
                sort_order: 0,
            }],
            body_rewrite_rules: vec![BodyRewriteRuleInput {
                body_type: "json".to_string(),
                path: "user.name".to_string(),
                value_json: "\"kong\"".to_string(),
                enabled: true,
                sort_order: 0,
            }],
        }
    }

    fn sample_ssh_profile() -> SshProfileInput {
        SshProfileInput {
            name: "测试 SSH".to_string(),
            host: "127.0.0.1".to_string(),
            port: 22,
            username: "tester".to_string(),
            auth_type: SshAuthType::Agent,
            password: None,
            private_key_path: None,
            private_key_passphrase: None,
            known_hosts_mode: "insecure_skip".to_string(),
            known_hosts_path: None,
            connect_timeout_ms: 10_000,
            keepalive_interval_ms: 30_000,
            jump_profile_id: None,
        }
    }

    #[test]
    fn migration_creates_default_settings() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let settings = db.list_settings().expect("默认设置应可读取");
        assert!(settings.iter().any(|row| row.key == "app.initialized"));
    }

    #[test]
    fn service_crud_roundtrip_keeps_rules() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        assert_eq!(created.header_rules.len(), 1);
        assert_eq!(created.body_rewrite_rules.len(), 1);

        let listed = db.list_service_summaries().expect("服务列表应可读取");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].target_label, "http://127.0.0.1:19090");

        db.delete_service(&created.id).expect("服务应可软删除");
        assert!(db.list_services().expect("服务列表应可读取").is_empty());
    }

    #[test]
    fn ssh_local_service_roundtrip_keeps_profile_reference() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let profile = db
            .create_ssh_profile(&sample_ssh_profile(), None, None)
            .expect("SSH Profile 应创建成功");
        let service = db
            .create_service(&CreateServiceInput {
                name: "测试 SSH 隧道".to_string(),
                kind: ServiceKind::SshLocal,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19022,
                notes: String::new(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: profile.id.clone(),
                    tunnel_type: "local".to_string(),
                    target_host: Some("127.0.0.1".to_string()),
                    target_port: Some(5432),
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            })
            .expect("SSH local 服务应创建成功");

        let tunnel = service.ssh_tunnel.expect("SSH 隧道配置应存在");
        assert_eq!(tunnel.ssh_profile_id, profile.id);
        assert_eq!(tunnel.target_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(tunnel.target_port, Some(5432));
    }

    #[test]
    fn ssh_socks_service_roundtrip_only_requires_profile() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let profile = db
            .create_ssh_profile(&sample_ssh_profile(), None, None)
            .expect("SSH Profile 应创建成功");
        let service = db
            .create_service(&CreateServiceInput {
                name: "测试 SSH SOCKS".to_string(),
                kind: ServiceKind::SshSocks,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19080,
                notes: String::new(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: profile.id.clone(),
                    tunnel_type: "socks".to_string(),
                    target_host: None,
                    target_port: None,
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            })
            .expect("SSH SOCKS 服务应创建成功");

        assert_eq!(service.kind, ServiceKind::SshSocks);
        assert_eq!(target_label(&service), "SOCKS5 动态代理");
        let tunnel = service.ssh_tunnel.expect("SSH SOCKS 配置应存在");
        assert_eq!(tunnel.ssh_profile_id, profile.id);
        assert_eq!(tunnel.tunnel_type, "socks");
        assert!(tunnel.target_host.is_none());
        assert!(tunnel.target_port.is_none());
    }

    #[test]
    fn password_ssh_profile_requires_saved_secret() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let mut input = sample_ssh_profile();
        input.auth_type = SshAuthType::Password;
        input.password = Some("secret".to_string());

        let failed = db.create_ssh_profile(&input, None, None);
        assert!(matches!(failed, Err(AppError::InvalidInput(_))));

        let created = db
            .create_ssh_profile(&input, Some("password-secret-id"), None)
            .expect("带 secret id 的密码配置应可创建");
        assert!(created.has_password);
    }

    #[test]
    fn system_proxy_profile_crud_and_target_resolution() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_system_proxy_profile(&SystemProxyProfileInput {
                name: " 办公网代理 ".to_string(),
                proxy_host: " 127.0.0.1 ".to_string(),
                proxy_port: 7890,
                bypass: " localhost;127.* ".to_string(),
            })
            .expect("系统代理配置档应可创建");
        assert_eq!(created.name, "办公网代理");
        assert_eq!(created.proxy_host, "127.0.0.1");
        assert_eq!(created.bypass, "localhost;127.*");
        assert!(!created.active);

        let updated = db
            .update_system_proxy_profile(
                &created.id,
                &SystemProxyProfileInput {
                    name: "家庭代理".to_string(),
                    proxy_host: "192.168.1.10".to_string(),
                    proxy_port: 1080,
                    bypass: "localhost;10.*".to_string(),
                },
            )
            .expect("系统代理配置档应可更新");
        assert_eq!(updated.proxy_host, "192.168.1.10");
        assert_eq!(updated.proxy_port, 1080);

        let target = db
            .get_system_proxy_target(&created.id)
            .expect("配置档应可解析为系统代理目标");
        assert_eq!(target.proxy_host, "192.168.1.10");
        assert_eq!(target.proxy_port, 1080);
        assert_eq!(target.bypass, "localhost;10.*");

        db.set_active_system_proxy_profile(Some(&created.id))
            .expect("配置档应可标记 active");
        let profiles = db
            .list_system_proxy_profiles()
            .expect("系统代理配置档列表应可读取");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].active);

        db.delete_system_proxy_profile(&created.id)
            .expect("系统代理配置档应可删除");
        assert!(db
            .list_system_proxy_profiles()
            .expect("删除后配置档列表应可读取")
            .is_empty());
    }

    #[test]
    fn service_update_rolls_back_when_detail_insert_fails() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");

        let failed = db.update_service(
            &created.id,
            &CreateServiceInput {
                name: "无效 SSH 隧道".to_string(),
                kind: ServiceKind::SshLocal,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19022,
                notes: "should rollback".to_string(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: "missing-profile".to_string(),
                    tunnel_type: "local".to_string(),
                    target_host: Some("127.0.0.1".to_string()),
                    target_port: Some(5432),
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            },
        );

        assert!(failed.is_err());
        let current = db.get_service(&created.id).expect("原服务应仍可读取");
        assert_eq!(current.name, "测试反向代理");
        assert_eq!(current.kind, ServiceKind::HttpReverse);
        assert!(current.http_reverse.is_some());
        assert_eq!(current.header_rules.len(), 1);
        assert_eq!(current.body_rewrite_rules.len(), 1);
    }

    #[test]
    fn list_logs_merges_service_and_connection_events_with_protocol_filter() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        db.insert_service_event(Some(&service.id), "info", "服务已启动", "{}")
            .expect("服务事件应写入成功");
        db.insert_connection_event(
            &service.id,
            "http",
            "127.0.0.1:50000",
            "127.0.0.1:19090",
            "request",
            "POST",
            "example.test",
            "/api",
            Some(200),
            12,
            34,
            56,
            "",
        )
        .expect("连接事件应写入成功");

        let all = db
            .list_logs(&LogFilter {
                service_id: Some(service.id.clone()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("日志应可查询");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|row| row.message == "服务已启动"));
        assert!(all
            .iter()
            .any(|row| row.message.contains("http POST example.test/api -> 200")));

        let protocol_logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id.clone()),
                protocol: Some("http".to_string()),
                keyword: Some("example.test".to_string()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("协议日志应可查询");
        assert_eq!(protocol_logs.len(), 1);
        assert_eq!(protocol_logs[0].level, "info");
        assert!(protocol_logs[0].meta_json.contains("\"protocol\":\"http\""));

        db.clear_logs(Some(&service.id))
            .expect("日志应可按服务清理");
        assert!(db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("清理后日志应可查询")
            .is_empty());
    }

    #[test]
    fn list_logs_time_range_filters_service_and_connection_events() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        insert_service_event_at(&db, &service.id, "窗口前服务日志", "2026-01-01 09:59:59");
        insert_service_event_at(&db, &service.id, "窗口内服务日志", "2026-01-01 10:10:00");
        insert_connection_event_at(
            &db,
            &service.id,
            "inside.example",
            "/inside",
            "2026-01-01 10:20:00",
        );
        insert_connection_event_at(
            &db,
            &service.id,
            "after.example",
            "/after",
            "2026-01-01 11:00:01",
        );

        let logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                created_after: Some("2026-01-01 10:00:00".to_string()),
                created_before: Some("2026-01-01 10:59:59".to_string()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("时间范围日志应可查询");

        assert_eq!(logs.len(), 2);
        assert!(logs.iter().any(|row| row.message == "窗口内服务日志"));
        assert!(logs
            .iter()
            .any(|row| row.message.contains("inside.example")));
        assert!(!logs.iter().any(|row| row.message.contains("窗口前")));
        assert!(!logs.iter().any(|row| row.message.contains("after.example")));
    }

    #[test]
    fn prune_logs_applies_retention_and_total_row_limit() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        insert_service_event_at(&db, &service.id, "过期服务日志", "2000-01-01 00:00:00");
        insert_connection_event_at(
            &db,
            &service.id,
            "old.example",
            "/old",
            "2000-01-01 00:00:01",
        );
        insert_service_event_at(&db, &service.id, "新服务日志 1", "2999-01-01 00:00:01");
        insert_connection_event_at(
            &db,
            &service.id,
            "new-a.example",
            "/a",
            "2999-01-01 00:00:02",
        );
        insert_service_event_at(&db, &service.id, "新服务日志 2", "2999-01-01 00:00:03");
        insert_connection_event_at(
            &db,
            &service.id,
            "new-b.example",
            "/b",
            "2999-01-01 00:00:04",
        );

        db.prune_logs(7, 3).expect("日志保留清理应成功");

        let logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("日志应可查询");
        assert_eq!(logs.len(), 3);
        assert!(logs.iter().any(|row| row.message.contains("new-b.example")));
        assert!(logs.iter().any(|row| row.message == "新服务日志 2"));
        assert!(logs.iter().any(|row| row.message.contains("new-a.example")));
        assert!(!logs.iter().any(|row| row.message.contains("过期")));
        assert!(!logs.iter().any(|row| row.message.contains("old.example")));
    }

    fn insert_service_event_at(db: &Database, service_id: &str, message: &str, created_at: &str) {
        let conn = db.conn().expect("数据库连接应可用");
        conn.execute(
            "INSERT INTO service_events (service_id, level, message, meta_json, created_at)
             VALUES (?1, 'info', ?2, '{}', ?3)",
            params![service_id, message, created_at],
        )
        .expect("服务日志应写入成功");
    }

    fn insert_connection_event_at(
        db: &Database,
        service_id: &str,
        host: &str,
        path: &str,
        created_at: &str,
    ) {
        let conn = db.conn().expect("数据库连接应可用");
        conn.execute(
            "INSERT INTO connection_events (
               service_id, protocol, remote_addr, target_addr, event_type,
               method, host, path, status_code, bytes_in, bytes_out,
               duration_ms, error, created_at
             )
             VALUES (?1, 'http', '127.0.0.1:50000', '127.0.0.1:19090',
                     'request', 'GET', ?2, ?3, 200, 10, 20, 30, '', ?4)",
            params![service_id, host, path, created_at],
        )
        .expect("连接日志应写入成功");
    }
}
