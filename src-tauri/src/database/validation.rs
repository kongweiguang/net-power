// @author kongweiguang
// Database 输入校验逻辑。

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

fn validate_tool_service_input(input: &ToolServiceInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("服务名称不能为空".to_string()));
    }
    if input.host.trim().is_empty() {
        return Err(AppError::InvalidInput("监听主机不能为空".to_string()));
    }
    if input.port == 0 {
        return Err(AppError::InvalidInput(
            "监听端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    let has_static_root = input
        .static_root_dir
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if !has_static_root && input.routes.is_empty() {
        return Err(AppError::InvalidInput(
            "请至少配置静态目录或一个接口".to_string(),
        ));
    }
    for (index, route) in input.routes.iter().enumerate() {
        let label = format!("接口 #{}", index + 1);
        if !matches!(
            route.method.trim().to_ascii_uppercase().as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "ANY"
        ) {
            return Err(AppError::InvalidInput(format!("{label} 的请求方法无效")));
        }
        if route.path.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "{label} 的请求路径不能为空"
            )));
        }
        if !(100..=599).contains(&route.response_status) {
            return Err(AppError::InvalidInput(format!(
                "{label} 的响应状态码必须是 100-599"
            )));
        }
        if matches!(route.content_source, ToolServiceContentSource::File)
            && route
                .file_path
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(AppError::InvalidInput(format!("{label} 需要选择响应文件")));
        }
    }
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
