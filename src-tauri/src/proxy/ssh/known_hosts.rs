// @author kongweiguang
// SSH known_hosts 校验决策。

fn verify_known_host(session: &Session, profile: &SshProfileRuntimeConfig) -> AppResult<()> {
    if profile.known_hosts_mode == "insecure_skip" {
        return Ok(());
    }
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| AppError::InvalidInput("SSH 服务端没有返回 host key".to_string()))?;
    let known_hosts_path = known_hosts_path(profile)?;
    let mut known_hosts = session.known_hosts()?;
    if should_read_known_hosts_file(&profile.known_hosts_mode, &known_hosts_path)? {
        known_hosts.read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)?;
    }
    let check = known_hosts.check_port(&profile.host, profile.port, key);
    match decide_known_host(
        &profile.known_hosts_mode,
        check,
        &profile.host,
        profile.port,
    )? {
        KnownHostDecision::Trusted => Ok(()),
        KnownHostDecision::AddNew => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            known_hosts.add(
                &known_host_name(&profile.host, profile.port),
                key,
                "net-power accept_new",
                host_key_format(key_type),
            )?;
            known_hosts.write_file(&known_hosts_path, KnownHostFileKind::OpenSSH)?;
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum KnownHostDecision {
    Trusted,
    AddNew,
}

fn should_read_known_hosts_file(mode: &str, path: &Path) -> AppResult<bool> {
    if path.exists() {
        return Ok(true);
    }
    if mode == "strict" {
        return Err(AppError::InvalidInput(format!(
            "known_hosts 文件不存在: {}",
            path.display()
        )));
    }
    Ok(false)
}

fn decide_known_host(
    mode: &str,
    check: CheckResult,
    host: &str,
    port: u16,
) -> AppResult<KnownHostDecision> {
    match (mode, check) {
        (_, CheckResult::Match) => Ok(KnownHostDecision::Trusted),
        ("strict", CheckResult::NotFound) => Err(AppError::InvalidInput(format!(
            "known_hosts 中不存在 {host}:{port}"
        ))),
        (_, CheckResult::NotFound) => Ok(KnownHostDecision::AddNew),
        (_, CheckResult::Mismatch) => Err(AppError::InvalidInput(format!(
            "SSH host key 与 known_hosts 不匹配: {host}:{port}"
        ))),
        (_, CheckResult::Failure) => Err(AppError::InvalidInput(format!(
            "known_hosts 校验失败: {host}:{port}"
        ))),
    }
}
