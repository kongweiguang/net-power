//! @author kongweiguang
//! SSH known_hosts 严格校验的纯逻辑回归测试。

use super::*;
use uuid::Uuid;

#[test]
fn strict_known_hosts_rejects_missing_file() {
    let path = missing_known_hosts_path();
    let error =
        should_read_known_hosts_file("strict", &path).expect_err("strict 模式必须拒绝缺失文件");
    assert!(error.to_string().contains("known_hosts 文件不存在"));
}

#[test]
fn accept_new_known_hosts_allows_missing_file() {
    let path = missing_known_hosts_path();
    let should_read =
        should_read_known_hosts_file("accept_new", &path).expect("accept_new 可等待首次写入");
    assert!(!should_read);
}

#[test]
fn strict_known_hosts_rejects_unknown_host_and_mismatch() {
    let missing = decide_known_host("strict", CheckResult::NotFound, "example.com", 2222)
        .expect_err("strict 模式必须拒绝 known_hosts 中不存在的主机");
    assert!(missing
        .to_string()
        .contains("known_hosts 中不存在 example.com:2222"));

    let mismatch = decide_known_host("strict", CheckResult::Mismatch, "example.com", 2222)
        .expect_err("strict 模式必须拒绝 host key mismatch");
    assert!(mismatch
        .to_string()
        .contains("SSH host key 与 known_hosts 不匹配: example.com:2222"));
}

#[test]
fn accept_new_known_hosts_adds_unknown_host_but_rejects_mismatch() {
    let decision = decide_known_host("accept_new", CheckResult::NotFound, "example.com", 2222)
        .expect("accept_new 应允许首次写入 known_hosts");
    assert_eq!(decision, KnownHostDecision::AddNew);

    let mismatch = decide_known_host("accept_new", CheckResult::Mismatch, "example.com", 2222)
        .expect_err("accept_new 后续也必须拒绝 mismatch");
    assert!(mismatch
        .to_string()
        .contains("SSH host key 与 known_hosts 不匹配: example.com:2222"));
}

#[test]
fn matching_known_hosts_entry_is_trusted() {
    let decision = decide_known_host("strict", CheckResult::Match, "example.com", 22)
        .expect("匹配的 known_hosts 记录应通过");
    assert_eq!(decision, KnownHostDecision::Trusted);
}

fn missing_known_hosts_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("net-power-missing-known-hosts-{}", Uuid::new_v4()))
}
