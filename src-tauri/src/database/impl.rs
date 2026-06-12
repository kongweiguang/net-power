// @author kongweiguang
// Database 对外数据访问方法实现。

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
        if version < 4 {
            conn.execute_batch(MIGRATION_0004)?;
            conn.pragma_update(None, "user_version", 4)?;
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

    /// 创建本地工具 HTTP 服务配置。创建只写 SQLite，启动由命令层显式触发。
    pub fn create_tool_service(&self, input: &ToolServiceInput) -> AppResult<ToolServiceConfig> {
        validate_tool_service_input(input)?;
        let id = Uuid::new_v4().to_string();
        {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            insert_tool_service_tx(&tx, &id, input)?;
            tx.commit()?;
        }
        self.get_tool_service(&id)
    }

    /// 读取本地工具 HTTP 服务配置。
    pub fn get_tool_service(&self, id: &str) -> AppResult<ToolServiceConfig> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, static_root_dir, static_path_prefix, created_at, updated_at
             FROM tool_services
             WHERE id = ?1 AND deleted_at IS NULL",
        )?;
        let base = stmt
            .query_row(params![id], tool_service_config_from_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("工具服务不存在: {id}")))?;
        load_tool_service_routes(&conn, base)
    }

    /// 读取全部未删除的本地工具 HTTP 服务配置。
    pub fn list_tool_services(&self) -> AppResult<Vec<ToolServiceConfig>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, host, port, static_root_dir, static_path_prefix, created_at, updated_at
             FROM tool_services
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, created_at DESC",
        )?;
        let bases = rows_to_vec(stmt.query_map([], tool_service_config_from_row)?)?;
        bases
            .into_iter()
            .map(|base| load_tool_service_routes(&conn, base))
            .collect()
    }

    /// 读取本地工具 HTTP 服务列表摘要，运行态由调用方补齐。
    pub fn list_tool_service_summaries(&self) -> AppResult<Vec<ToolServiceSummary>> {
        self.list_tool_services().map(|configs| {
            configs
                .into_iter()
                .map(tool_service_summary_from_config)
                .collect()
        })
    }

    /// 软删除本地工具 HTTP 服务配置，运行中的服务需要先由调用方暂停。
    pub fn delete_tool_service(&self, id: &str) -> AppResult<()> {
        let conn = self.conn()?;
        let changed = conn.execute(
            "UPDATE tool_services SET deleted_at = datetime('now'), updated_at = datetime('now')
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("工具服务不存在: {id}")));
        }
        Ok(())
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
