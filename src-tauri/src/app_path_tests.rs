//! @author kongweiguang
//! 应用数据目录解析测试，覆盖 release smoke 使用的环境变量覆盖入口。

use super::{app_data_dir_override, service_auto_start_enabled, services_selected_for_auto_start};
use crate::models::{AppSetting, ServiceDetail, ServiceKind};
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn app_data_dir_override_ignores_missing_or_empty_value() {
    assert_eq!(app_data_dir_override(None), None);
    assert_eq!(app_data_dir_override(Some(OsString::new())), None);
}

#[test]
fn app_data_dir_override_accepts_explicit_path() {
    let path = PathBuf::from(r"C:\tmp\net-power-smoke");

    assert_eq!(
        app_data_dir_override(Some(path.clone().into_os_string())),
        Some(path)
    );
}

#[test]
fn service_auto_start_requires_global_setting_true() {
    assert!(!service_auto_start_enabled(&[]));
    assert!(!service_auto_start_enabled(&[setting(
        "services.auto_start_enabled",
        "false",
    )]));
    assert!(service_auto_start_enabled(&[setting(
        "services.auto_start_enabled",
        "true",
    )]));
}

#[test]
fn services_selected_for_auto_start_filters_disabled_and_manual_services() {
    let selected = services_selected_for_auto_start(vec![
        service("manual", true, false),
        service("disabled", false, true),
        service("ready", true, true),
    ]);

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "ready");
}

fn setting(key: &str, value_json: &str) -> AppSetting {
    AppSetting {
        key: key.to_string(),
        value_json: value_json.to_string(),
    }
}

fn service(id: &str, enabled: bool, auto_start: bool) -> ServiceDetail {
    ServiceDetail {
        id: id.to_string(),
        name: id.to_string(),
        kind: ServiceKind::HttpForward,
        enabled,
        auto_start,
        listen_host: "127.0.0.1".to_string(),
        listen_port: 0,
        notes: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
        http_reverse: None,
        http_forward: None,
        tcp_forward: None,
        udp_forward: None,
        ssh_tunnel: None,
        header_rules: Vec::new(),
        body_rewrite_rules: Vec::new(),
    }
}
