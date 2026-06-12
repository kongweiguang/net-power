//! @author kongweiguang
//! 系统代理平台命令构造与解析测试，避免真实修改当前机器代理设置。

use super::*;

fn target() -> SystemProxyTarget {
    SystemProxyTarget {
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: 7890,
        bypass: "localhost;127.*,.local".to_string(),
    }
}

#[test]
fn bypass_entries_accept_semicolon_comma_and_newline() {
    assert_eq!(
        bypass_entries(" localhost;127.*,.local\n10.* "),
        vec!["localhost", "127.*", ".local", "10.*"]
    );
}

#[test]
fn macos_services_skip_disabled_and_banner_lines() {
    let raw = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*Bluetooth PAN\nUSB 10/100/1000 LAN\n";

    assert_eq!(
        parse_macos_services(raw),
        vec!["Wi-Fi".to_string(), "USB 10/100/1000 LAN".to_string()]
    );
}

#[test]
fn macos_set_commands_cover_http_https_and_bypass() {
    let services = vec!["Wi-Fi".to_string()];
    let commands = macos_set_proxy_commands(&target(), &services);

    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands[0],
        CommandSpec::new(
            "networksetup",
            ["-setwebproxy", "Wi-Fi", "127.0.0.1", "7890"]
        )
    );
    assert_eq!(
        commands[1],
        CommandSpec::new(
            "networksetup",
            ["-setsecurewebproxy", "Wi-Fi", "127.0.0.1", "7890"]
        )
    );
    assert_eq!(
        commands[2],
        CommandSpec::new(
            "networksetup",
            [
                "-setproxybypassdomains",
                "Wi-Fi",
                "localhost",
                "127.*",
                ".local"
            ]
        )
    );
}

#[test]
fn macos_clear_commands_turn_off_http_https_and_clear_bypass() {
    let services = vec!["Wi-Fi".to_string()];
    let commands = macos_clear_proxy_commands(&services);

    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands[0],
        CommandSpec::new("networksetup", ["-setwebproxystate", "Wi-Fi", "off"])
    );
    assert_eq!(
        commands[1],
        CommandSpec::new("networksetup", ["-setsecurewebproxystate", "Wi-Fi", "off"])
    );
    assert_eq!(
        commands[2],
        CommandSpec::new("networksetup", ["-setproxybypassdomains", "Wi-Fi", "Empty"])
    );
}

#[test]
fn macos_status_parsers_read_proxy_and_bypass() {
    let (enabled, host, port) = parse_macos_proxy_output(
        "Enabled: Yes\nServer: 127.0.0.1\nPort: 7890\nAuthenticated Proxy Enabled: 0\n",
    );

    assert!(enabled);
    assert_eq!(host, "127.0.0.1");
    assert_eq!(port, Some(7890));
    assert_eq!(
        parse_macos_bypass_output("localhost\n127.*\n.local\n"),
        "localhost;127.*;.local"
    );
}

#[test]
fn linux_gsettings_commands_set_manual_http_https_and_ignore_hosts() {
    let commands = linux_set_proxy_commands(&target());

    assert_eq!(commands.len(), 6);
    assert_eq!(
        commands[0],
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "mode", "manual"]
        )
    );
    assert_eq!(
        commands[1],
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"]
        )
    );
    assert_eq!(
        commands[5],
        CommandSpec::new(
            "gsettings",
            [
                "set",
                "org.gnome.system.proxy",
                "ignore-hosts",
                "['localhost', '127.*', '.local']"
            ]
        )
    );
}

#[test]
fn linux_gsettings_clear_commands_disable_manual_mode() {
    let commands = linux_clear_proxy_commands();

    assert_eq!(commands.len(), 6);
    assert_eq!(
        commands[0],
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "mode", "none"]
        )
    );
    assert_eq!(
        commands[5],
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "ignore-hosts", "[]"]
        )
    );
}

#[test]
fn linux_env_file_content_exports_upper_and_lowercase_proxy_vars() {
    let content = linux_env_file_content(&target());

    assert!(content.contains("HTTP_PROXY=http://127.0.0.1:7890\n"));
    assert!(content.contains("https_proxy=http://127.0.0.1:7890\n"));
    assert!(content.contains("NO_PROXY=localhost,127.*,.local\n"));
    assert!(content.contains("no_proxy=localhost,127.*,.local\n"));
}

#[test]
fn linux_env_file_parser_reads_proxy_target() {
    let status =
        parse_linux_env_proxy("HTTP_PROXY=http://10.0.0.8:8080\nNO_PROXY=localhost,127.*\n")
            .expect("环境文件应可解析为系统代理状态");

    assert!(status.enabled);
    assert_eq!(status.proxy_host, "10.0.0.8");
    assert_eq!(status.proxy_port, Some(8080));
    assert_eq!(status.bypass, "localhost;127.*");
}

#[test]
fn linux_gsettings_parsers_read_strings_arrays_and_mode() {
    assert!(parse_gsettings_bool("'manual'", "manual"));
    assert_eq!(parse_gsettings_string("'proxy.local'"), "proxy.local");
    assert_eq!(
        parse_gsettings_string_array("['localhost', '127.*', '.local']"),
        vec![
            "localhost".to_string(),
            "127.*".to_string(),
            ".local".to_string()
        ]
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_system_proxy_registry_roundtrip_restores_original_values() {
    if std::env::var("NET_POWER_SYSTEM_PROXY_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skip Windows system proxy registry test: set NET_POWER_SYSTEM_PROXY_TEST=1 to enable"
        );
        return;
    }

    let _guard = WindowsProxyRegistryGuard::capture();
    let target = SystemProxyTarget {
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: 28991,
        bypass: "localhost;127.0.0.1;<local>".to_string(),
    };

    let set_status = set_system_proxy(&target).expect("Windows 系统代理应可写入注册表");
    assert!(set_status.enabled);
    assert_eq!(set_status.proxy_host, "127.0.0.1");
    assert_eq!(set_status.proxy_port, Some(28991));

    let current = get_system_proxy_status().expect("Windows 系统代理状态应可读取");
    assert!(current.enabled);
    assert_eq!(current.proxy_host, "127.0.0.1");
    assert_eq!(current.proxy_port, Some(28991));
    assert_eq!(current.bypass, "localhost;127.0.0.1;<local>");

    let cleared = clear_system_proxy().expect("Windows 系统代理应可清理");
    assert!(!cleared.enabled);
    let current = get_system_proxy_status().expect("Windows 清理后状态应可读取");
    assert!(!current.enabled);
    assert_eq!(current.proxy_host, "");
    assert_eq!(current.proxy_port, None);
    assert_eq!(current.bypass, "");
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct WindowsProxyRegistrySnapshot {
    proxy_enable: Option<String>,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
}

#[cfg(target_os = "windows")]
struct WindowsProxyRegistryGuard {
    snapshot: WindowsProxyRegistrySnapshot,
}

#[cfg(target_os = "windows")]
impl WindowsProxyRegistryGuard {
    fn capture() -> Self {
        Self {
            snapshot: WindowsProxyRegistrySnapshot {
                proxy_enable: query_windows_proxy_value("ProxyEnable"),
                proxy_server: query_windows_proxy_value("ProxyServer"),
                proxy_override: query_windows_proxy_value("ProxyOverride"),
            },
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsProxyRegistryGuard {
    fn drop(&mut self) {
        restore_windows_proxy_value(
            "ProxyEnable",
            "REG_DWORD",
            self.snapshot.proxy_enable.as_deref(),
        );
        restore_windows_proxy_value(
            "ProxyServer",
            "REG_SZ",
            self.snapshot.proxy_server.as_deref(),
        );
        restore_windows_proxy_value(
            "ProxyOverride",
            "REG_SZ",
            self.snapshot.proxy_override.as_deref(),
        );
    }
}

#[cfg(target_os = "windows")]
fn query_windows_proxy_value(name: &str) -> Option<String> {
    let output = run_command(&CommandSpec::new(
        "reg.exe",
        [
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            name,
        ],
    ))
    .ok()?;
    output.split_whitespace().last().map(ToOwned::to_owned)
}

#[cfg(target_os = "windows")]
fn restore_windows_proxy_value(name: &str, value_type: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            let _ = run_command(&CommandSpec::new(
                "reg.exe",
                [
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    name,
                    "/t",
                    value_type,
                    "/d",
                    value,
                    "/f",
                ],
            ));
        }
        None => {
            let _ = run_command(&CommandSpec::new(
                "reg.exe",
                [
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    name,
                    "/f",
                ],
            ));
        }
    }
}
