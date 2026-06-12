//! @author kongweiguang
//! SSH remote forward 运行态管理测试，保持测试逻辑独立于生产 manager 文件。

use super::*;
use crate::models::{CreateServiceInput, SshAuthType, SshProfileInput, SshTunnelConfig};
use std::sync::Arc;
use tokio::task::JoinHandle;

#[tokio::test]
async fn ssh_remote_start_skips_local_port_probe() {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("测试 TCP 端口应可监听");
    let occupied_port = listener.local_addr().expect("应可读取 TCP 测试端口").port();
    let db = Arc::new(Database::in_memory().expect("内存数据库应初始化"));
    let profile = db
        .create_ssh_profile(&sample_ssh_profile(), None, None)
        .expect("SSH Profile 应创建成功");
    let mut manager = ServiceManager::new();
    let detail = create_ssh_remote_service(&db, "remote-one", &profile.id, occupied_port);

    let started = manager
        .start_service_for_test(detail.clone(), Arc::clone(&db), idle_runner)
        .await
        .expect("SSH remote 监听在远端，不应探测本地端口");

    assert_eq!(started, RuntimeStatus::Running);
    assert_eq!(manager.runtime_status(&detail.id), RuntimeStatus::Running);
    let stopped = manager
        .stop_service_for_test(&detail.id, Arc::clone(&db))
        .await
        .expect("SSH remote 测试服务应可停止");
    assert_eq!(stopped, RuntimeStatus::Stopped);
    drop(listener);
}

#[tokio::test]
async fn ssh_remote_conflict_uses_profile_and_remote_bind() {
    let remote_port = reserve_tcp_port();
    let db = Arc::new(Database::in_memory().expect("内存数据库应初始化"));
    let profile = db
        .create_ssh_profile(&sample_ssh_profile(), None, None)
        .expect("SSH Profile 应创建成功");
    let second_profile = db
        .create_ssh_profile(
            &SshProfileInput {
                name: "第二个 SSH Profile".to_string(),
                ..sample_ssh_profile()
            },
            None,
            None,
        )
        .expect("第二个 SSH Profile 应创建成功");
    let mut manager = ServiceManager::new();

    manager
        .start_service_for_test(
            create_ssh_remote_service(&db, "remote-one", &profile.id, remote_port),
            Arc::clone(&db),
            idle_runner,
        )
        .await
        .expect("第一个 SSH remote 应可启动");
    let conflict = manager
        .start_service_for_test(
            create_ssh_remote_service(&db, "remote-two", &profile.id, remote_port),
            Arc::clone(&db),
            idle_runner,
        )
        .await;
    assert!(matches!(conflict, Err(AppError::Conflict(_))));

    let different_profile = manager
        .start_service_for_test(
            create_ssh_remote_service(&db, "remote-three", &second_profile.id, remote_port),
            Arc::clone(&db),
            idle_runner,
        )
        .await
        .expect("不同 SSH profile 的同名远程绑定不在同一冲突域");
    assert_eq!(different_profile, RuntimeStatus::Running);
}

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

fn create_ssh_remote_service(
    db: &Database,
    name: &str,
    profile_id: &str,
    remote_port: u16,
) -> ServiceDetail {
    db.create_service(&CreateServiceInput {
        name: name.to_string(),
        kind: ServiceKind::SshRemote,
        enabled: true,
        auto_start: false,
        listen_host: "127.0.0.1".to_string(),
        listen_port: remote_port,
        notes: String::new(),
        http_reverse: None,
        http_forward: None,
        tcp_forward: None,
        udp_forward: None,
        ssh_tunnel: Some(SshTunnelConfig {
            ssh_profile_id: profile_id.to_string(),
            tunnel_type: "remote".to_string(),
            target_host: Some("127.0.0.1".to_string()),
            target_port: Some(8080),
            remote_bind_host: Some("127.0.0.1".to_string()),
            remote_bind_port: Some(remote_port),
        }),
        header_rules: Vec::new(),
        body_rewrite_rules: Vec::new(),
    })
    .expect("SSH remote 服务应创建成功")
}

fn idle_runner(
    _detail: ServiceDetail,
    cancel: CancellationToken,
    _counters: Arc<ServiceCounters>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        cancel.cancelled().await;
    })
}

fn reserve_tcp_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("应可临时监听 TCP 端口");
    let port = listener.local_addr().expect("应可读取临时端口").port();
    drop(listener);
    port
}
