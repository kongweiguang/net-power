//! @author kongweiguang
//! SSH remote forward 轻量 helper 测试，独立于生产 SSH runtime 文件。

use super::*;
use std::time::Duration;

#[test]
fn host_port_labels_wrap_ipv6_addresses() {
    assert_eq!(format_host_port("127.0.0.1", 8080), "127.0.0.1:8080");
    assert_eq!(format_host_port("2001:db8::1", 8080), "[2001:db8::1]:8080");
    assert_eq!(
        RemoteForwardSpec {
            service_id: "svc".to_string(),
            remote_bind_host: "0.0.0.0".to_string(),
            remote_bind_port: 18080,
            target_host: "127.0.0.1".to_string(),
            target_port: 8080,
        }
        .log_target_addr(),
        "0.0.0.0:18080 -> 127.0.0.1:8080"
    );
}

#[test]
fn ssh_reconnect_delay_exponentially_backs_off_and_caps() {
    assert_eq!(ssh_reconnect_delay(0), Duration::from_secs(1));
    assert_eq!(ssh_reconnect_delay(1), Duration::from_secs(2));
    assert_eq!(ssh_reconnect_delay(5), Duration::from_secs(30));
    assert_eq!(ssh_reconnect_delay(12), Duration::from_secs(30));
}
