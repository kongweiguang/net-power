// @author kongweiguang
// SSH session 建立、跳板链和认证逻辑。

fn connect_authenticated_session(
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<Session> {
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    let addr = resolve_ssh_addr(&profile.host, profile.port)?;
    let tcp = StdTcpStream::connect_timeout(&addr, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    connect_authenticated_session_over_tcp(tcp, profile, auth)
}

fn connect_authenticated_session_over_tcp(
    tcp: StdTcpStream,
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<Session> {
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.set_timeout(profile.connect_timeout_ms.min(u64::from(u32::MAX)) as u32);
    session.handshake()?;
    verify_known_host(&session, profile)?;
    authenticate(&session, profile, auth)?;
    if !session.authenticated() {
        return Err(AppError::InvalidInput("SSH 认证失败".to_string()));
    }
    let keepalive_secs = (profile.keepalive_interval_ms / 1000).clamp(0, u64::from(u32::MAX));
    if keepalive_secs > 0 {
        session.set_keepalive(true, keepalive_secs as u32);
    }
    Ok(session)
}

fn load_ssh_connection_chain(
    db: &Database,
    profile_id: &str,
) -> AppResult<Vec<SshConnectionProfile>> {
    db.resolve_ssh_profile_runtime_chain(profile_id)?
        .into_iter()
        .map(|profile| {
            let auth = SshAuthMaterial::load(db, &profile)?;
            Ok(SshConnectionProfile { profile, auth })
        })
        .collect()
}

fn connect_authenticated_session_chain(
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<SshSessionHandle> {
    let Some(first) = chain.first() else {
        return Err(AppError::InvalidInput("SSH 连接链不能为空".to_string()));
    };
    let first_session = connect_authenticated_session(&first.profile, &first.auth)?;
    let mut handle = SshSessionHandle::direct(first_session, first.profile.clone());
    for next in &chain[1..] {
        if cancel.is_cancelled() {
            handle.disconnect("net-power ssh chain cancelled");
            return Err(AppError::Message("SSH 连接已取消".to_string()));
        }
        handle = connect_next_hop(handle, next, cancel.child_token())?;
    }
    Ok(handle)
}

fn connect_next_hop(
    mut upstream: SshSessionHandle,
    next: &SshConnectionProfile,
    bridge_cancel: CancellationToken,
) -> AppResult<SshSessionHandle> {
    let mut channel = upstream.session.channel_direct_tcpip(
        &next.profile.host,
        next.profile.port,
        Some(("127.0.0.1", 0)),
    )?;
    upstream.session.set_blocking(false);

    let timeout = Duration::from_millis(next.profile.connect_timeout_ms.max(1));
    let (client_stream, bridge_stream) = loopback_tcp_pair(timeout)?;
    let upstream_session_for_bridge = upstream.session.clone();
    let upstream_profile_for_bridge = upstream.profile.clone();
    let bridge_cancel_for_thread = bridge_cancel.clone();
    let bridge_handle = std::thread::spawn(move || {
        bridge_nonblocking(
            bridge_stream,
            &upstream_session_for_bridge,
            &mut channel,
            &upstream_profile_for_bridge,
            bridge_cancel_for_thread,
        )
    });

    let next_session =
        match connect_authenticated_session_over_tcp(client_stream, &next.profile, &next.auth) {
            Ok(session) => session,
            Err(err) => {
                bridge_cancel.cancel();
                let _ = bridge_handle.join();
                upstream.disconnect("net-power ssh jump failed");
                return Err(err);
            }
        };

    upstream.upstream_sessions.push(upstream.session.clone());
    upstream.jump_bridges.push(JumpBridge {
        cancel: bridge_cancel,
        handle: bridge_handle,
    });
    Ok(SshSessionHandle {
        session: next_session,
        profile: next.profile.clone(),
        jump_bridges: upstream.jump_bridges,
        upstream_sessions: upstream.upstream_sessions,
    })
}

fn loopback_tcp_pair(timeout: Duration) -> AppResult<(StdTcpStream, StdTcpStream)> {
    let listener = StdTcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let client = StdTcpStream::connect_timeout(&addr, timeout)?;
    let (server, _) = listener.accept()?;
    client.set_nodelay(true)?;
    client.set_read_timeout(Some(timeout))?;
    client.set_write_timeout(Some(timeout))?;
    server.set_nodelay(true)?;
    server.set_nonblocking(true)?;
    Ok((client, server))
}

fn authenticate(
    session: &Session,
    profile: &SshProfileRuntimeConfig,
    auth: &SshAuthMaterial,
) -> AppResult<()> {
    match profile.auth_type {
        SshAuthType::Password => {
            let password = auth
                .password
                .as_deref()
                .ok_or_else(|| AppError::InvalidInput("密码认证缺少密码".to_string()))?;
            match session.userauth_password(&profile.username, password) {
                Ok(()) => Ok(()),
                Err(first_err) => {
                    let mut prompter = StaticPasswordPrompt {
                        password: password.to_string(),
                    };
                    session
                        .userauth_keyboard_interactive(&profile.username, &mut prompter)
                        .map_err(|second_err| {
                            AppError::InvalidInput(format!(
                                "SSH 密码认证失败: {first_err}; keyboard-interactive 失败: {second_err}"
                            ))
                        })
                }
            }
        }
        SshAuthType::PrivateKey => {
            let key_path = profile
                .private_key_path
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AppError::InvalidInput("私钥认证缺少私钥路径".to_string()))?;
            session.userauth_pubkey_file(
                &profile.username,
                None,
                Path::new(key_path),
                auth.private_key_passphrase.as_deref(),
            )?;
            Ok(())
        }
        SshAuthType::Agent => {
            session.userauth_agent(&profile.username)?;
            Ok(())
        }
    }
}
