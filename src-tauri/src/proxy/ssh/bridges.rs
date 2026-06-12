// @author kongweiguang
// SSH local、remote channel 和 SOCKS 客户端桥接。

fn run_ssh_client_bridge(
    client: StdTcpStream,
    remote_addr: SocketAddr,
    target_host: &str,
    target_port: u16,
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let ssh_session = connect_authenticated_session_chain(chain, cancel.child_token())?;
    let session = ssh_session.session.clone();
    let mut channel = session.channel_direct_tcpip(
        target_host,
        target_port,
        Some((remote_addr.ip().to_string().as_str(), remote_addr.port())),
    )?;
    session.set_blocking(false);
    client.set_nonblocking(true)?;
    let mut metrics =
        bridge_nonblocking(client, &session, &mut channel, &ssh_session.profile, cancel)?;
    let _ = channel.close();
    ssh_session.disconnect("net-power tunnel closed");
    // direct-tcpip 中 local -> SSH 是入口流量，SSH -> local 是出口流量。
    if metrics.bytes_in == 0 && metrics.bytes_out == 0 && channel.eof() {
        metrics.bytes_out = 0;
    }
    Ok(metrics)
}

fn run_ssh_remote_channel_bridge(
    mut channel: ssh2::Channel,
    session: &Session,
    target_host: &str,
    target_port: u16,
    profile: &SshProfileRuntimeConfig,
    cancel: CancellationToken,
) -> AppResult<BridgeMetrics> {
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    let target_addr = resolve_tcp_addr(target_host, target_port)?;
    let local_target = StdTcpStream::connect_timeout(&target_addr, timeout)?;
    local_target.set_read_timeout(Some(timeout))?;
    local_target.set_write_timeout(Some(timeout))?;
    local_target.set_nonblocking(true)?;
    let mut metrics = bridge_nonblocking(local_target, session, &mut channel, profile, cancel)?;
    let _ = channel.close();
    // remote forward 的入口流量方向是 SSH channel -> 本地目标，和 bridge_nonblocking 的默认方向相反。
    std::mem::swap(&mut metrics.bytes_in, &mut metrics.bytes_out);
    Ok(metrics)
}

fn run_ssh_socks_client_bridge(
    mut client: StdTcpStream,
    remote_addr: SocketAddr,
    chain: &[SshConnectionProfile],
    cancel: CancellationToken,
) -> AppResult<SocksBridgeResult> {
    let profile = chain
        .last()
        .ok_or_else(|| AppError::InvalidInput("SSH SOCKS 缺少连接链".to_string()))?
        .profile
        .clone();
    let timeout = Duration::from_millis(profile.connect_timeout_ms.max(1));
    client.set_nonblocking(false)?;
    client.set_read_timeout(Some(timeout))?;
    client.set_write_timeout(Some(timeout))?;

    let request = read_socks5_connect_request(&mut client)?;
    let target_addr = request.target_addr();
    if cancel.is_cancelled() {
        let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::Message("SOCKS5 连接已取消".to_string()));
    }

    let ssh_session = match connect_authenticated_session_chain(chain, cancel.child_token()) {
        Ok(session) => session,
        Err(err) => {
            let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
            return Err(err);
        }
    };
    let session = ssh_session.session.clone();
    let mut channel = match session.channel_direct_tcpip(
        &request.host,
        request.port,
        Some((remote_addr.ip().to_string().as_str(), remote_addr.port())),
    ) {
        Ok(channel) => channel,
        Err(err) => {
            let _ = send_socks5_reply(&mut client, SOCKS_REP_GENERAL_FAILURE);
            ssh_session.disconnect("net-power socks target failed");
            return Err(AppError::Ssh(err));
        }
    };
    send_socks5_reply(&mut client, SOCKS_REP_SUCCEEDED)?;

    session.set_blocking(false);
    client.set_nonblocking(true)?;
    let metrics = bridge_nonblocking(client, &session, &mut channel, &profile, cancel)?;
    let _ = channel.close();
    ssh_session.disconnect("net-power socks tunnel closed");
    Ok(SocksBridgeResult {
        target_addr,
        metrics,
    })
}

fn read_socks5_connect_request(client: &mut StdTcpStream) -> AppResult<SocksConnectRequest> {
    let mut greeting = [0_u8; 2];
    client.read_exact(&mut greeting)?;
    if greeting[0] != SOCKS_VERSION {
        return Err(AppError::InvalidInput(format!(
            "SOCKS5 版本无效: {}",
            greeting[0]
        )));
    }

    let mut methods = vec![0_u8; greeting[1] as usize];
    client.read_exact(&mut methods)?;
    if !methods.contains(&SOCKS_METHOD_NO_AUTH) {
        client.write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_ACCEPTABLE])?;
        return Err(AppError::InvalidInput(
            "SOCKS5 客户端不支持 no-auth".to_string(),
        ));
    }
    client.write_all(&[SOCKS_VERSION, SOCKS_METHOD_NO_AUTH])?;

    let mut header = [0_u8; 4];
    client.read_exact(&mut header)?;
    if header[0] != SOCKS_VERSION {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput(format!(
            "SOCKS5 请求版本无效: {}",
            header[0]
        )));
    }
    if header[1] != SOCKS_CMD_CONNECT {
        let _ = send_socks5_reply(client, SOCKS_REP_COMMAND_NOT_SUPPORTED);
        return Err(AppError::Unsupported(
            "SOCKS5 当前仅支持 CONNECT 命令".to_string(),
        ));
    }
    if header[2] != 0 {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput("SOCKS5 RSV 字段无效".to_string()));
    }

    let host = match header[3] {
        SOCKS_ATYP_IPV4 => {
            let mut addr = [0_u8; 4];
            client.read_exact(&mut addr)?;
            Ipv4Addr::from(addr).to_string()
        }
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            client.read_exact(&mut len)?;
            if len[0] == 0 {
                let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
                return Err(AppError::InvalidInput("SOCKS5 域名不能为空".to_string()));
            }
            let mut domain = vec![0_u8; len[0] as usize];
            client.read_exact(&mut domain)?;
            String::from_utf8(domain)
                .map_err(|_| AppError::InvalidInput("SOCKS5 域名不是 UTF-8".to_string()))?
        }
        SOCKS_ATYP_IPV6 => {
            let mut addr = [0_u8; 16];
            client.read_exact(&mut addr)?;
            Ipv6Addr::from(addr).to_string()
        }
        _ => {
            let _ = send_socks5_reply(client, SOCKS_REP_ADDRESS_TYPE_NOT_SUPPORTED);
            return Err(AppError::Unsupported(
                "SOCKS5 地址类型仅支持 IPv4、域名和 IPv6".to_string(),
            ));
        }
    };
    let mut port_bytes = [0_u8; 2];
    client.read_exact(&mut port_bytes)?;
    let port = u16::from_be_bytes(port_bytes);
    if port == 0 {
        let _ = send_socks5_reply(client, SOCKS_REP_GENERAL_FAILURE);
        return Err(AppError::InvalidInput(
            "SOCKS5 目标端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(SocksConnectRequest { host, port })
}

fn send_socks5_reply(client: &mut StdTcpStream, reply: u8) -> std::io::Result<()> {
    client.write_all(&[
        SOCKS_VERSION,
        reply,
        0x00,
        SOCKS_ATYP_IPV4,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ])
}
