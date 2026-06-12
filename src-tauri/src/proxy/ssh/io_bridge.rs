// @author kongweiguang
// SSH 非阻塞双向数据桥接。

fn bridge_nonblocking(
    mut client: StdTcpStream,
    session: &Session,
    channel: &mut ssh2::Channel,
    profile: &SshProfileRuntimeConfig,
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let mut to_channel = VecDeque::<u8>::new();
    let mut to_client = VecDeque::<u8>::new();
    let mut client_eof = false;
    let mut channel_eof = false;
    let mut sent_channel_eof = false;
    let mut metrics = BridgeMetrics::default();
    let keepalive_interval = (profile.keepalive_interval_ms > 0)
        .then(|| Duration::from_millis(profile.keepalive_interval_ms));
    let mut last_keepalive = Instant::now();
    let mut buf = [0_u8; BRIDGE_BUFFER_SIZE];

    while !cancel.is_cancelled() {
        let mut progressed = false;

        if !client_eof && to_channel.len() < MAX_PENDING_BYTES {
            match client.read(&mut buf) {
                Ok(0) => {
                    client_eof = true;
                    progressed = true;
                }
                Ok(size) => {
                    to_channel.extend(&buf[..size]);
                    metrics.bytes_in = metrics.bytes_in.saturating_add(size as u64);
                    progressed = true;
                }
                Err(err) if would_block(&err) => {}
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        while !to_channel.is_empty() {
            let contiguous = to_channel.make_contiguous();
            match channel.write(contiguous) {
                Ok(0) => break,
                Ok(size) => {
                    to_channel.drain(..size);
                    progressed = true;
                }
                Err(err) if would_block(&err) => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        if client_eof && to_channel.is_empty() && !sent_channel_eof {
            match channel.send_eof() {
                Ok(()) => {
                    sent_channel_eof = true;
                    progressed = true;
                }
                Err(err) if ssh_would_block(&err) => {}
                Err(err) => return Err(AppError::Ssh(err)),
            }
        }

        if !channel_eof && to_client.len() < MAX_PENDING_BYTES {
            match channel.read(&mut buf) {
                Ok(0) => {
                    // ssh2 非阻塞 Channel 可能用 Ok(0) 表示本轮暂时没有数据；
                    // 只有明确 EOF 时才关闭桥接方向，避免 remote forward 新连接被提前断开。
                    if channel.eof() {
                        channel_eof = true;
                        progressed = true;
                    }
                }
                Ok(size) => {
                    to_client.extend(&buf[..size]);
                    metrics.bytes_out = metrics.bytes_out.saturating_add(size as u64);
                    progressed = true;
                }
                Err(err) if would_block(&err) => {}
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        while !to_client.is_empty() {
            let contiguous = to_client.make_contiguous();
            match client.write(contiguous) {
                Ok(0) => break,
                Ok(size) => {
                    to_client.drain(..size);
                    progressed = true;
                }
                Err(err) if would_block(&err) => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(AppError::Io(err)),
            }
        }

        if channel.eof() {
            channel_eof = true;
        }
        if channel_eof && to_client.is_empty() && (client_eof || to_channel.is_empty()) {
            break;
        }

        if keepalive_interval.is_some_and(|interval| last_keepalive.elapsed() >= interval) {
            match session.keepalive_send() {
                Ok(_) => last_keepalive = Instant::now(),
                Err(err) if ssh_would_block(&err) => {}
                Err(err) => return Err(AppError::Ssh(err)),
            }
        }

        if !progressed {
            std::thread::sleep(IDLE_SLEEP);
        }
    }
    Ok(metrics)
}
