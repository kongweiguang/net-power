//! @author kongweiguang
//! SSH remote forward 数据持久化测试，独立于生产数据访问实现文件。

use super::*;

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
fn ssh_remote_service_roundtrip_keeps_bind_and_target() {
    let db = Database::in_memory().expect("内存数据库应初始化成功");
    let profile = db
        .create_ssh_profile(&sample_ssh_profile(), None, None)
        .expect("SSH Profile 应创建成功");
    let service = db
        .create_service(&CreateServiceInput {
            name: "测试 SSH remote".to_string(),
            kind: ServiceKind::SshRemote,
            enabled: true,
            auto_start: false,
            listen_host: "0.0.0.0".to_string(),
            listen_port: 18080,
            notes: String::new(),
            http_reverse: None,
            http_forward: None,
            tcp_forward: None,
            udp_forward: None,
            ssh_tunnel: Some(SshTunnelConfig {
                ssh_profile_id: profile.id.clone(),
                tunnel_type: "remote".to_string(),
                target_host: Some("127.0.0.1".to_string()),
                target_port: Some(8080),
                remote_bind_host: Some("0.0.0.0".to_string()),
                remote_bind_port: Some(18080),
            }),
            header_rules: Vec::new(),
            body_rewrite_rules: Vec::new(),
        })
        .expect("SSH remote 服务应创建成功");

    assert_eq!(
        target_label(&service),
        "SSH remote 0.0.0.0:18080 -> 127.0.0.1:8080"
    );
    let tunnel = service.ssh_tunnel.expect("SSH remote 配置应存在");
    assert_eq!(tunnel.ssh_profile_id, profile.id);
    assert_eq!(tunnel.tunnel_type, "remote");
    assert_eq!(tunnel.target_host.as_deref(), Some("127.0.0.1"));
    assert_eq!(tunnel.target_port, Some(8080));
    assert_eq!(tunnel.remote_bind_host.as_deref(), Some("0.0.0.0"));
    assert_eq!(tunnel.remote_bind_port, Some(18080));
}
