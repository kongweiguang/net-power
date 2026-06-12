//! @author kongweiguang
//! 系统代理设置。按平台封装当前用户代理配置，前端只接触统一的目标和状态模型。

use crate::error::{AppError, AppResult};
use crate::models::{SystemProxyStatus, SystemProxyTarget};
use std::process::Command;

/// 设置系统代理。
pub fn set_system_proxy(target: &SystemProxyTarget) -> AppResult<SystemProxyStatus> {
    validate_target(target)?;
    platform::set_system_proxy(target)
}

fn validate_target(target: &SystemProxyTarget) -> AppResult<()> {
    if target.proxy_host.trim().is_empty() {
        return Err(AppError::InvalidInput("系统代理主机不能为空".to_string()));
    }
    if target.proxy_port == 0 {
        return Err(AppError::InvalidInput(
            "系统代理端口必须在 1 到 65535 之间".to_string(),
        ));
    }
    Ok(())
}

/// 清理系统代理。
pub fn clear_system_proxy() -> AppResult<SystemProxyStatus> {
    platform::clear_system_proxy()
}

/// 获取当前系统代理状态。
pub fn get_system_proxy_status() -> AppResult<SystemProxyStatus> {
    platform::get_system_proxy_status()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

impl CommandSpec {
    fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

fn run_command(spec: &CommandSpec) -> AppResult<String> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(AppError::Message(format!(
            "{} 执行失败: {}",
            spec.program,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn bypass_entries(bypass: &str) -> Vec<String> {
    bypass
        .split([';', ',', '\n', '\r'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn quoted_gsettings_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(any(target_os = "linux", test))]
fn gsettings_string_array(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| quoted_gsettings_string(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{items}]")
}

#[cfg(any(target_os = "linux", test))]
fn parse_gsettings_string(raw: &str) -> String {
    raw.trim()
        .trim_matches('\'')
        .replace("\\'", "'")
        .replace("\\\\", "\\")
}

#[cfg(any(target_os = "linux", test))]
fn parse_gsettings_string_array(raw: &str) -> Vec<String> {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(parse_gsettings_string)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn parse_gsettings_bool(raw: &str, expected: &str) -> bool {
    parse_gsettings_string(raw) == expected
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_services(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("An asterisk"))
        .filter(|line| !line.starts_with('*'))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_proxy_output(raw: &str) -> (bool, String, Option<u16>) {
    let enabled = raw.lines().any(|line| {
        line.trim()
            .strip_prefix("Enabled:")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("Yes"))
    });
    let host = macos_proxy_value(raw, "Server:").unwrap_or_default();
    let port = macos_proxy_value(raw, "Port:").and_then(|value| value.parse::<u16>().ok());
    (enabled, host, port)
}

#[cfg(any(target_os = "macos", test))]
fn macos_proxy_value(raw: &str, prefix: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(prefix).map(str::trim))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_bypass_output(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("There aren't any"))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(any(target_os = "macos", test))]
fn macos_set_proxy_commands(target: &SystemProxyTarget, services: &[String]) -> Vec<CommandSpec> {
    let bypass = bypass_entries(&target.bypass);
    services
        .iter()
        .flat_map(|service| {
            let mut commands = vec![
                CommandSpec::new(
                    "networksetup",
                    [
                        "-setwebproxy",
                        service.as_str(),
                        target.proxy_host.as_str(),
                        &target.proxy_port.to_string(),
                    ],
                ),
                CommandSpec::new(
                    "networksetup",
                    [
                        "-setsecurewebproxy",
                        service.as_str(),
                        target.proxy_host.as_str(),
                        &target.proxy_port.to_string(),
                    ],
                ),
            ];
            let mut bypass_args = vec!["-setproxybypassdomains".to_string(), service.clone()];
            if bypass.is_empty() {
                bypass_args.push("Empty".to_string());
            } else {
                bypass_args.extend(bypass.iter().cloned());
            }
            commands.push(CommandSpec::new("networksetup", bypass_args));
            commands
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn macos_clear_proxy_commands(services: &[String]) -> Vec<CommandSpec> {
    services
        .iter()
        .flat_map(|service| {
            vec![
                CommandSpec::new(
                    "networksetup",
                    ["-setwebproxystate", service.as_str(), "off"],
                ),
                CommandSpec::new(
                    "networksetup",
                    ["-setsecurewebproxystate", service.as_str(), "off"],
                ),
                CommandSpec::new(
                    "networksetup",
                    ["-setproxybypassdomains", service.as_str(), "Empty"],
                ),
            ]
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn linux_set_proxy_commands(target: &SystemProxyTarget) -> Vec<CommandSpec> {
    let bypass = gsettings_string_array(&bypass_entries(&target.bypass));
    vec![
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "mode", "manual"],
        ),
        CommandSpec::new(
            "gsettings",
            [
                "set",
                "org.gnome.system.proxy.http",
                "host",
                target.proxy_host.as_str(),
            ],
        ),
        CommandSpec::new(
            "gsettings",
            [
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &target.proxy_port.to_string(),
            ],
        ),
        CommandSpec::new(
            "gsettings",
            [
                "set",
                "org.gnome.system.proxy.https",
                "host",
                target.proxy_host.as_str(),
            ],
        ),
        CommandSpec::new(
            "gsettings",
            [
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &target.proxy_port.to_string(),
            ],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "ignore-hosts", &bypass],
        ),
    ]
}

#[cfg(any(target_os = "linux", test))]
fn linux_clear_proxy_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "mode", "none"],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy.http", "host", ""],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy.http", "port", "0"],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy.https", "host", ""],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy.https", "port", "0"],
        ),
        CommandSpec::new(
            "gsettings",
            ["set", "org.gnome.system.proxy", "ignore-hosts", "[]"],
        ),
    ]
}

#[cfg(any(target_os = "linux", test))]
fn linux_env_file_content(target: &SystemProxyTarget) -> String {
    let proxy_url = format!("http://{}:{}", target.proxy_host.trim(), target.proxy_port);
    let bypass = bypass_entries(&target.bypass).join(",");
    format!(
        "HTTP_PROXY={proxy_url}\nHTTPS_PROXY={proxy_url}\nhttp_proxy={proxy_url}\nhttps_proxy={proxy_url}\nNO_PROXY={bypass}\nno_proxy={bypass}\n"
    )
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_env_proxy(raw: &str) -> Option<SystemProxyStatus> {
    let proxy_url = raw.lines().find_map(|line| {
        line.strip_prefix("http_proxy=")
            .or_else(|| line.strip_prefix("HTTP_PROXY="))
    })?;
    let endpoint = proxy_url.strip_prefix("http://").unwrap_or(proxy_url);
    let (host, port) = endpoint.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    let bypass = raw
        .lines()
        .find_map(|line| {
            line.strip_prefix("no_proxy=")
                .or_else(|| line.strip_prefix("NO_PROXY="))
        })
        .unwrap_or_default()
        .replace(',', ";");
    Some(SystemProxyStatus {
        enabled: true,
        proxy_host: host.to_string(),
        proxy_port: Some(port),
        bypass,
        message: "Linux shell 代理环境文件已配置，新登录会话生效".to_string(),
    })
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    const REG_PATH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    pub fn set_system_proxy(target: &SystemProxyTarget) -> AppResult<SystemProxyStatus> {
        run_reg(&[
            "add",
            REG_PATH,
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "1",
            "/f",
        ])?;
        run_reg(&[
            "add",
            REG_PATH,
            "/v",
            "ProxyServer",
            "/t",
            "REG_SZ",
            "/d",
            &format!("{}:{}", target.proxy_host, target.proxy_port),
            "/f",
        ])?;
        if !target.bypass.trim().is_empty() {
            run_reg(&[
                "add",
                REG_PATH,
                "/v",
                "ProxyOverride",
                "/t",
                "REG_SZ",
                "/d",
                &target.bypass,
                "/f",
            ])?;
        }
        Ok(SystemProxyStatus {
            enabled: true,
            proxy_host: target.proxy_host.clone(),
            proxy_port: Some(target.proxy_port),
            bypass: target.bypass.clone(),
            message: "Windows 系统代理已设置".to_string(),
        })
    }

    pub fn clear_system_proxy() -> AppResult<SystemProxyStatus> {
        run_reg(&[
            "add",
            REG_PATH,
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            "0",
            "/f",
        ])?;
        let _ = run_reg(&["delete", REG_PATH, "/v", "ProxyServer", "/f"]);
        Ok(SystemProxyStatus {
            enabled: false,
            proxy_host: String::new(),
            proxy_port: None,
            bypass: String::new(),
            message: "Windows 系统代理已清理".to_string(),
        })
    }

    pub fn get_system_proxy_status() -> AppResult<SystemProxyStatus> {
        let enabled_raw = query_reg("ProxyEnable").unwrap_or_default();
        let server_raw = query_reg("ProxyServer").unwrap_or_default();
        let bypass = query_reg("ProxyOverride")
            .ok()
            .and_then(|raw| parse_reg_value(&raw))
            .unwrap_or_default();
        let enabled =
            enabled_raw.contains("0x1") || enabled_raw.split_whitespace().last() == Some("1");
        let (host, port) = parse_proxy_server(&server_raw);
        Ok(SystemProxyStatus {
            enabled,
            proxy_host: host,
            proxy_port: port,
            bypass,
            message: if enabled {
                "Windows 系统代理已开启".to_string()
            } else {
                "Windows 系统代理未开启".to_string()
            },
        })
    }

    fn run_reg(args: &[&str]) -> AppResult<String> {
        run_command(&CommandSpec::new("reg.exe", args.iter().copied()))
    }

    fn query_reg(name: &str) -> AppResult<String> {
        run_reg(&["query", REG_PATH, "/v", name])
    }

    fn parse_proxy_server(raw: &str) -> (String, Option<u16>) {
        let value = parse_reg_value(raw).unwrap_or_default();
        let plain = value
            .strip_prefix("http=")
            .or_else(|| value.strip_prefix("https="))
            .unwrap_or(value.as_str());
        match plain.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), port.parse::<u16>().ok()),
            None => (plain.to_string(), None),
        }
    }

    fn parse_reg_value(raw: &str) -> Option<String> {
        raw.lines()
            .map(str::trim)
            .find_map(|line| {
                let (_, value) = line.split_once("REG_")?;
                value
                    .split_once(char::is_whitespace)
                    .map(|(_, data)| data.trim().to_string())
            })
            .filter(|value| !value.is_empty())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn set_system_proxy(target: &SystemProxyTarget) -> AppResult<SystemProxyStatus> {
        let services = active_services()?;
        for command in macos_set_proxy_commands(target, &services) {
            run_command(&command)?;
        }
        Ok(SystemProxyStatus {
            enabled: true,
            proxy_host: target.proxy_host.trim().to_string(),
            proxy_port: Some(target.proxy_port),
            bypass: target.bypass.clone(),
            message: "macOS HTTP/HTTPS 系统代理已设置".to_string(),
        })
    }

    pub fn clear_system_proxy() -> AppResult<SystemProxyStatus> {
        let services = active_services()?;
        for command in macos_clear_proxy_commands(&services) {
            run_command(&command)?;
        }
        Ok(SystemProxyStatus {
            enabled: false,
            proxy_host: String::new(),
            proxy_port: None,
            bypass: String::new(),
            message: "macOS HTTP/HTTPS 系统代理已清理".to_string(),
        })
    }

    pub fn get_system_proxy_status() -> AppResult<SystemProxyStatus> {
        let services = active_services()?;
        for service in services {
            let output = run_command(&CommandSpec::new(
                "networksetup",
                ["-getwebproxy", service.as_str()],
            ));
            let Ok(output) = output else {
                continue;
            };
            let (enabled, host, port) = parse_macos_proxy_output(&output);
            if enabled {
                let bypass = run_command(&CommandSpec::new(
                    "networksetup",
                    ["-getproxybypassdomains", service.as_str()],
                ))
                .map(|value| parse_macos_bypass_output(&value))
                .unwrap_or_default();
                return Ok(SystemProxyStatus {
                    enabled: true,
                    proxy_host: host,
                    proxy_port: port,
                    bypass,
                    message: format!("macOS 系统代理已开启: {service}"),
                });
            }
        }
        Ok(SystemProxyStatus {
            enabled: false,
            proxy_host: String::new(),
            proxy_port: None,
            bypass: String::new(),
            message: "macOS 系统代理未开启".to_string(),
        })
    }

    fn active_services() -> AppResult<Vec<String>> {
        let output = run_command(&CommandSpec::new(
            "networksetup",
            ["-listallnetworkservices"],
        ))?;
        let services = parse_macos_services(&output);
        if services.is_empty() {
            return Err(AppError::Message("macOS 未找到可用网络服务".to_string()));
        }
        Ok(services)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::path::PathBuf;

    pub fn set_system_proxy(target: &SystemProxyTarget) -> AppResult<SystemProxyStatus> {
        let desktop_applied = run_linux_desktop_commands(linux_set_proxy_commands(target))?;
        write_linux_env_proxy(target)?;
        Ok(SystemProxyStatus {
            enabled: true,
            proxy_host: target.proxy_host.trim().to_string(),
            proxy_port: Some(target.proxy_port),
            bypass: target.bypass.clone(),
            message: if desktop_applied {
                "Linux GNOME 系统代理和 shell 代理环境文件已设置".to_string()
            } else {
                "Linux shell 代理环境文件已设置；未找到 gsettings，桌面代理未修改".to_string()
            },
        })
    }

    pub fn clear_system_proxy() -> AppResult<SystemProxyStatus> {
        let desktop_applied = run_linux_desktop_commands(linux_clear_proxy_commands())?;
        remove_linux_env_proxy()?;
        Ok(SystemProxyStatus {
            enabled: false,
            proxy_host: String::new(),
            proxy_port: None,
            bypass: String::new(),
            message: if desktop_applied {
                "Linux GNOME 系统代理和 shell 代理环境文件已清理".to_string()
            } else {
                "Linux shell 代理环境文件已清理；未找到 gsettings，桌面代理未修改".to_string()
            },
        })
    }

    pub fn get_system_proxy_status() -> AppResult<SystemProxyStatus> {
        let mode = run_command(&CommandSpec::new(
            "gsettings",
            ["get", "org.gnome.system.proxy", "mode"],
        ))
        .ok();
        let Some(mode) = mode else {
            return linux_env_proxy_status();
        };
        let enabled = parse_gsettings_bool(&mode, "manual");
        if !enabled {
            if let Ok(status) = linux_env_proxy_status() {
                if status.enabled {
                    return Ok(status);
                }
            }
            return Ok(SystemProxyStatus {
                enabled: false,
                proxy_host: String::new(),
                proxy_port: None,
                bypass: String::new(),
                message: "Linux GNOME 系统代理未开启".to_string(),
            });
        }
        let host = run_command(&CommandSpec::new(
            "gsettings",
            ["get", "org.gnome.system.proxy.http", "host"],
        ))
        .map(|value| parse_gsettings_string(&value))?;
        let port = run_command(&CommandSpec::new(
            "gsettings",
            ["get", "org.gnome.system.proxy.http", "port"],
        ))
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok());
        let bypass = run_command(&CommandSpec::new(
            "gsettings",
            ["get", "org.gnome.system.proxy", "ignore-hosts"],
        ))
        .map(|value| parse_gsettings_string_array(&value).join(";"))
        .unwrap_or_default();
        Ok(SystemProxyStatus {
            enabled: true,
            proxy_host: host,
            proxy_port: port,
            bypass,
            message: "Linux GNOME 系统代理已开启".to_string(),
        })
    }

    fn run_linux_desktop_commands(commands: Vec<CommandSpec>) -> AppResult<bool> {
        for command in commands {
            match run_command(&command) {
                Ok(_) => {}
                Err(AppError::Io(io_err)) if io_err.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(false);
                }
                Err(err) => return Err(err),
            }
        }
        Ok(true)
    }

    fn linux_env_proxy_status() -> AppResult<SystemProxyStatus> {
        let path = linux_env_proxy_path()?;
        let raw = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SystemProxyStatus {
                    enabled: false,
                    proxy_host: String::new(),
                    proxy_port: None,
                    bypass: String::new(),
                    message: "Linux 系统代理未开启".to_string(),
                });
            }
            Err(err) => return Err(AppError::Io(err)),
        };
        Ok(
            parse_linux_env_proxy(&raw).unwrap_or_else(|| SystemProxyStatus {
                enabled: false,
                proxy_host: String::new(),
                proxy_port: None,
                bypass: String::new(),
                message: "Linux shell 代理环境文件格式无法识别".to_string(),
            }),
        )
    }

    fn write_linux_env_proxy(target: &SystemProxyTarget) -> AppResult<()> {
        let path = linux_env_proxy_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, linux_env_file_content(target))?;
        Ok(())
    }

    fn remove_linux_env_proxy() -> AppResult<()> {
        let path = linux_env_proxy_path()?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::Io(err)),
        }
    }

    fn linux_env_proxy_path() -> AppResult<PathBuf> {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| AppError::InvalidInput("无法定位 Linux 用户配置目录".to_string()))?;
        Ok(config_dir
            .join("environment.d")
            .join("net-power-proxy.conf"))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn set_system_proxy(_target: &SystemProxyTarget) -> AppResult<SystemProxyStatus> {
        Err(AppError::Unsupported(
            "当前平台暂未接入系统代理设置".to_string(),
        ))
    }

    pub fn clear_system_proxy() -> AppResult<SystemProxyStatus> {
        Err(AppError::Unsupported(
            "当前平台暂未接入系统代理清理".to_string(),
        ))
    }

    pub fn get_system_proxy_status() -> AppResult<SystemProxyStatus> {
        Ok(SystemProxyStatus {
            enabled: false,
            proxy_host: String::new(),
            proxy_port: None,
            bypass: String::new(),
            message: "当前平台暂未接入系统代理状态检测".to_string(),
        })
    }
}

#[cfg(test)]
#[path = "system_proxy_tests.rs"]
mod tests;
