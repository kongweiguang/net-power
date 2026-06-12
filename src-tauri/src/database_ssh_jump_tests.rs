//! @author kongweiguang
//! SSH 跳板 profile 持久化与链路解析测试，独立于生产数据访问实现文件。

use super::*;

#[test]
fn ssh_profile_jump_chain_roundtrip_resolves_first_hop_order() {
    let db = Database::in_memory().expect("内存数据库应初始化成功");
    let bastion = db
        .create_ssh_profile(&sample_ssh_profile("一级跳板", None), None, None)
        .expect("一级跳板 Profile 应创建成功");
    let middle = db
        .create_ssh_profile(
            &sample_ssh_profile("二级跳板", Some(&bastion.id)),
            None,
            None,
        )
        .expect("二级跳板 Profile 应创建成功");
    let target = db
        .create_ssh_profile(
            &sample_ssh_profile("目标主机", Some(&middle.id)),
            None,
            None,
        )
        .expect("目标 Profile 应创建成功");

    let stored = db
        .get_ssh_profile(&target.id)
        .expect("目标 Profile 应可读取");
    assert_eq!(stored.jump_profile_id.as_deref(), Some(middle.id.as_str()));

    let chain = db
        .resolve_ssh_profile_runtime_chain(&target.id)
        .expect("跳板链应可解析");
    let ids = chain
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![bastion.id.as_str(), middle.id.as_str(), target.id.as_str()]
    );
}

#[test]
fn ssh_profile_jump_reference_rejects_self_cycle_and_delete() {
    let db = Database::in_memory().expect("内存数据库应初始化成功");
    let first = db
        .create_ssh_profile(&sample_ssh_profile("first", None), None, None)
        .expect("first Profile 应创建成功");
    let second = db
        .create_ssh_profile(&sample_ssh_profile("second", Some(&first.id)), None, None)
        .expect("second Profile 应创建成功");
    let third = db
        .create_ssh_profile(&sample_ssh_profile("third", Some(&second.id)), None, None)
        .expect("third Profile 应创建成功");

    let self_reference = db.update_ssh_profile(
        &first.id,
        &sample_ssh_profile("first", Some(&first.id)),
        None,
        None,
    );
    assert!(matches!(self_reference, Err(AppError::InvalidInput(_))));

    let cycle = db.update_ssh_profile(
        &first.id,
        &sample_ssh_profile("first", Some(&third.id)),
        None,
        None,
    );
    assert!(matches!(cycle, Err(AppError::InvalidInput(_))));

    let referenced_delete = db.delete_ssh_profile(&first.id);
    assert!(matches!(referenced_delete, Err(AppError::Conflict(_))));
}

fn sample_ssh_profile(name: &str, jump_profile_id: Option<&str>) -> SshProfileInput {
    SshProfileInput {
        name: name.to_string(),
        host: format!("{name}.example.test"),
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
        jump_profile_id: jump_profile_id.map(ToOwned::to_owned),
    }
}
