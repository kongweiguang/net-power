// @author kongweiguang
// SSH remote forward 重连和远端监听循环。

fn run_ssh_remote_reconnect_loop<R: Runtime>(
    spec: RemoteForwardSpec,
    chain: Vec<SshConnectionProfile>,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let mut attempt = 0_u32;
    while !cancel.is_cancelled() {
        let result = run_ssh_remote_accept_loop(
            spec.clone(),
            chain.clone(),
            cancel.child_token(),
            Arc::clone(&counters),
            Arc::clone(&db),
            app.clone(),
        );
        match result {
            Ok(()) => return Ok(()),
            Err(_) if cancel.is_cancelled() => return Ok(()),
            Err(err) => {
                let delay = ssh_reconnect_delay(attempt);
                attempt = attempt.saturating_add(1);
                log_event(
                    &db,
                    &app,
                    Some(&spec.service_id),
                    "warn",
                    &format!(
                        "SSH remote forward 连接中断: {err}，将在 {} 秒后重连",
                        delay.as_secs()
                    ),
                )?;
                if sleep_until_cancelled(&cancel, delay) {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn run_ssh_remote_accept_loop<R: Runtime>(
    spec: RemoteForwardSpec,
    chain: Vec<SshConnectionProfile>,
    cancel: CancellationToken,
    counters: Arc<ServiceCounters>,
    db: Arc<Database>,
    app: AppHandle<R>,
) -> AppResult<()> {
    let final_profile = chain
        .last()
        .ok_or_else(|| AppError::InvalidInput("SSH remote 缺少连接链".to_string()))?
        .profile
        .clone();
    let ssh_session = connect_authenticated_session_chain(&chain, cancel.child_token())?;
    let session = ssh_session.session.clone();
    let (mut listener, bound_port) = session.channel_forward_listen(
        spec.remote_bind_port,
        Some(&spec.remote_bind_host),
        Some(REMOTE_FORWARD_QUEUE_MAX_SIZE),
    )?;
    let mut spec = spec;
    spec.remote_bind_port = bound_port;
    session.set_blocking(false);
    log_event(
        &db,
        &app,
        Some(&spec.service_id),
        "info",
        &format!(
            "SSH remote forward 已在远端监听 {}，转发到 {}",
            spec.remote_bind_addr(),
            spec.target_addr()
        ),
    )?;

    let keepalive_interval = (final_profile.keepalive_interval_ms > 0)
        .then(|| Duration::from_millis(final_profile.keepalive_interval_ms));
    let mut last_keepalive = Instant::now();
    while !cancel.is_cancelled() {
        match listener.accept() {
            Ok(channel) => {
                counters.active_connections.fetch_add(1, Ordering::Relaxed);
                counters.total_connections.fetch_add(1, Ordering::Relaxed);
                let child_cancel = cancel.child_token();
                let session_for_channel = session.clone();
                let db_for_thread = Arc::clone(&db);
                let app_for_thread = app.clone();
                let counters_for_thread = Arc::clone(&counters);
                let profile_for_thread = final_profile.clone();
                let spec_for_thread = spec.clone();
                std::thread::spawn(move || {
                    let started = Instant::now();
                    let bridge_result = run_ssh_remote_channel_bridge(
                        channel,
                        &session_for_channel,
                        &spec_for_thread.target_host,
                        spec_for_thread.target_port,
                        &profile_for_thread,
                        child_cancel,
                    );
                    counters_for_thread
                        .active_connections
                        .fetch_sub(1, Ordering::Relaxed);
                    let duration_ms = elapsed_ms(started);
                    let (bytes_in, bytes_out, error) = match bridge_result {
                        Ok(metrics) => (metrics.bytes_in, metrics.bytes_out, String::new()),
                        Err(err) => {
                            let message = err.to_string();
                            let _ = log_event(
                                &db_for_thread,
                                &app_for_thread,
                                Some(&spec_for_thread.service_id),
                                "error",
                                &message,
                            );
                            (0, 0, message)
                        }
                    };
                    counters_for_thread
                        .bytes_in
                        .fetch_add(bytes_in, Ordering::Relaxed);
                    counters_for_thread
                        .bytes_out
                        .fetch_add(bytes_out, Ordering::Relaxed);
                    let remote_bind_addr = spec_for_thread.remote_bind_addr();
                    let target_addr = spec_for_thread.log_target_addr();
                    let _ = db_for_thread.insert_connection_event(
                        &spec_for_thread.service_id,
                        "ssh",
                        &remote_bind_addr,
                        &target_addr,
                        "closed",
                        "REMOTE",
                        "",
                        "",
                        None,
                        bytes_in,
                        bytes_out,
                        duration_ms,
                        &error,
                    );
                    let _ = app_for_thread.emit(
                        "service://connection",
                        ConnectionEventPayload {
                            service_id: spec_for_thread.service_id.clone(),
                            protocol: "ssh".to_string(),
                            event_type: "closed".to_string(),
                            target_addr,
                        },
                    );
                });
            }
            Err(err) if ssh_would_block(&err) => {
                if keepalive_interval.is_some_and(|interval| last_keepalive.elapsed() >= interval) {
                    match session.keepalive_send() {
                        Ok(_) => last_keepalive = Instant::now(),
                        Err(err) if ssh_would_block(&err) => {}
                        Err(err) => {
                            ssh_session.disconnect("net-power remote forward failed");
                            return Err(AppError::Ssh(err));
                        }
                    }
                }
                std::thread::sleep(IDLE_SLEEP);
            }
            Err(err) => {
                ssh_session.disconnect("net-power remote forward failed");
                return Err(AppError::Ssh(err));
            }
        }
    }

    drop(listener);
    ssh_session.disconnect("net-power remote forward stopped");
    log_event(
        &db,
        &app,
        Some(&spec.service_id),
        "info",
        "SSH remote forward 已停止",
    )?;
    Ok(())
}

fn ssh_reconnect_delay(attempt: u32) -> Duration {
    let factor = 1_u32.checked_shl(attempt.min(5)).unwrap_or(32);
    SSH_RECONNECT_INITIAL_DELAY
        .saturating_mul(factor)
        .min(SSH_RECONNECT_MAX_DELAY)
}

fn sleep_until_cancelled(cancel: &CancellationToken, delay: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < delay {
        if cancel.is_cancelled() {
            return true;
        }
        let remaining = delay.saturating_sub(started.elapsed());
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    cancel.is_cancelled()
}
