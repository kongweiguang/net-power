// @author kongweiguang
// 服务配置明细表写入和读取逻辑。

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

fn insert_tool_service_tx(
    tx: &Transaction<'_>,
    id: &str,
    input: &ToolServiceInput,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO tool_services (
           id, name, host, port, static_root_dir, static_path_prefix
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            input.name.trim(),
            input.host.trim(),
            i64::from(input.port),
            normalized_optional(input.static_root_dir.as_deref()),
            normalize_db_http_path(&input.static_path_prefix),
        ],
    )?;
    for (index, route) in input.routes.iter().enumerate() {
        let method = route.method.trim().to_ascii_uppercase();
        let content_type = route.content_type.trim();
        let content_type = if content_type.is_empty() {
            "application/json; charset=utf-8"
        } else {
            content_type
        };
        tx.execute(
            "INSERT INTO tool_service_routes (
               id, service_id, method, path, response_status, content_type,
               content_source, body, file_path, sort_order
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                id,
                method,
                normalize_db_http_path(&route.path),
                i64::from(route.response_status),
                content_type,
                route.content_source.as_str(),
                route.body.as_deref(),
                normalized_optional(route.file_path.as_deref()),
                index as i64,
            ],
        )?;
    }
    Ok(())
}

fn load_tool_service_routes(
    conn: &Connection,
    mut config: ToolServiceConfig,
) -> AppResult<ToolServiceConfig> {
    let mut stmt = conn.prepare(
        "SELECT method, path, response_status, content_type, content_source, body, file_path
         FROM tool_service_routes
         WHERE service_id = ?1
         ORDER BY sort_order ASC, created_at ASC",
    )?;
    config.routes = rows_to_vec(stmt.query_map(params![config.id], |row| {
        let content_source_raw: String = row.get(4)?;
        let content_source =
            ToolServiceContentSource::try_from(content_source_raw.as_str()).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
                )
            })?;
        Ok(ToolServiceRouteInput {
            method: row.get(0)?,
            path: row.get(1)?,
            response_status: i64_to_u16(row.get(2)?),
            content_type: row.get(3)?,
            content_source,
            body: row.get(5)?,
            file_path: row.get(6)?,
        })
    })?)?;
    Ok(config)
}
