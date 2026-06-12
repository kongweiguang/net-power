// @author kongweiguang
// 数据库行到领域模型的映射和通用转换。

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

fn tool_service_config_from_row(row: &Row<'_>) -> rusqlite::Result<ToolServiceConfig> {
    Ok(ToolServiceConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: i64_to_u16(row.get(3)?),
        static_root_dir: row.get(4)?,
        static_path_prefix: row.get(5)?,
        routes: Vec::new(),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn tool_service_summary_from_config(config: ToolServiceConfig) -> ToolServiceSummary {
    let url = format!(
        "http://{}:{}{}",
        config.host,
        config.port,
        config.first_access_path()
    );
    ToolServiceSummary {
        id: config.id,
        name: config.name,
        host: config.host,
        port: config.port,
        url,
        static_root_dir: config.static_root_dir,
        static_path_prefix: config.static_path_prefix,
        route_count: config.routes.len(),
        started_at: None,
        total_requests: 0,
        runtime_status: RuntimeStatus::Stopped,
    }
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

fn normalize_db_http_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let with_leading = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    with_leading.trim_end_matches('/').to_string()
}
