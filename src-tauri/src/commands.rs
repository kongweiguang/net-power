//! @author kongweiguang
//! Tauri Command 入口。命令层只做参数接收、服务调用和错误转换。

use crate::app_state::AppState;
use crate::crypto;
use crate::error::{AppError, AppResult};
use crate::models::{
    AppSetting, AutostartStatus, CreateServiceInput, LogFilter, LogRow, RuntimeStatus,
    ServiceDetail, ServiceKind, ServiceRuntimeSummary, ServiceSummary, SshProfile, SshProfileInput,
    SystemInfo, SystemProxyProfile, SystemProxyProfileInput, SystemProxyStatus, SystemProxyTarget,
    TestResult, ToolServiceConfig, ToolServiceInput, ToolServiceSummary,
};
use crate::proxy;
use crate::system_proxy;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const LAN_ROUTE_PROBE_TARGETS: [Ipv4Addr; 5] = [
    Ipv4Addr::new(8, 8, 8, 8),
    Ipv4Addr::new(1, 1, 1, 1),
    Ipv4Addr::new(192, 168, 255, 255),
    Ipv4Addr::new(10, 255, 255, 255),
    Ipv4Addr::new(172, 31, 255, 255),
];

/// 读取全部应用设置。
#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<Vec<AppSetting>, String> {
    state.db.list_settings().map_err(String::from)
}

/// 更新应用设置。value_json 必须是合法 JSON 字符串。
#[tauri::command]
pub fn update_app_setting(
    state: State<'_, AppState>,
    key: String,
    value_json: String,
) -> Result<(), String> {
    state
        .db
        .update_setting(&key, &value_json)
        .map_err(String::from)
}

/// 读取系统开机启动状态。
#[tauri::command]
pub fn get_autostart_status(app: AppHandle) -> Result<AutostartStatus, String> {
    let enabled = app
        .autolaunch()
        .is_enabled()
        .map_err(|err| format!("读取开机启动状态失败: {err}"))?;
    Ok(build_autostart_status(enabled))
}

/// 设置系统开机启动。该能力由 Tauri autostart 插件写入当前平台启动项。
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<AutostartStatus, String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|err| format!("开启开机启动失败: {err}"))?;
    } else {
        autolaunch
            .disable()
            .map_err(|err| format!("关闭开机启动失败: {err}"))?;
    }
    let actual = autolaunch
        .is_enabled()
        .map_err(|err| format!("读取开机启动状态失败: {err}"))?;
    Ok(build_autostart_status(actual))
}

fn build_autostart_status(enabled: bool) -> AutostartStatus {
    AutostartStatus {
        enabled,
        supported: true,
        message: if enabled {
            "已注册系统登录启动项。".to_string()
        } else {
            "未启用系统登录启动。".to_string()
        },
    }
}

/// 获取服务列表，自动合并内存运行态。
#[tauri::command]
pub async fn list_services(state: State<'_, AppState>) -> Result<Vec<ServiceSummary>, String> {
    let mut services = state.db.list_service_summaries().map_err(String::from)?;
    let manager = state.manager.read().await;
    for service in &mut services {
        service.runtime_status = manager.runtime_status(&service.id);
        let (active, total) = manager.counters(&service.id);
        service.active_connections = active;
        service.total_connections = total;
    }
    Ok(services)
}

/// 读取服务详情。
#[tauri::command]
pub fn get_service(state: State<'_, AppState>, id: String) -> Result<ServiceDetail, String> {
    state.db.get_service(&id).map_err(String::from)
}

/// 创建服务配置。
#[tauri::command]
pub fn create_service(
    state: State<'_, AppState>,
    input: CreateServiceInput,
) -> Result<ServiceDetail, String> {
    state.db.create_service(&input).map_err(String::from)
}

/// 更新服务配置。
#[tauri::command]
pub fn update_service(
    state: State<'_, AppState>,
    id: String,
    input: CreateServiceInput,
) -> Result<ServiceDetail, String> {
    state.db.update_service(&id, &input).map_err(String::from)
}

/// 删除服务配置。若服务正在运行，会先停止再软删除。
#[tauri::command]
pub async fn delete_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    stop_service_for_command(app, &state, &id).await?;
    state.db.delete_service(&id).map_err(String::from)
}

/// 复制服务配置。
#[tauri::command]
pub fn duplicate_service(state: State<'_, AppState>, id: String) -> Result<ServiceDetail, String> {
    state.db.duplicate_service(&id).map_err(String::from)
}

/// 启动服务。
#[tauri::command]
pub async fn start_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<RuntimeStatus, String> {
    let detail = state.db.get_service(&id).map_err(String::from)?;
    let mut manager = state.manager.write().await;
    manager
        .start_service(detail, Arc::clone(&state.db), app)
        .await
        .map_err(String::from)
}

/// 停止服务。
#[tauri::command]
pub async fn stop_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<RuntimeStatus, String> {
    stop_service_for_command(app, &state, &id).await
}

/// 重启服务。
#[tauri::command]
pub async fn restart_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<RuntimeStatus, String> {
    stop_service_for_command(app.clone(), &state, &id).await?;
    start_service(app, state, id).await
}

/// 获取所有运行态摘要。
#[tauri::command]
pub async fn list_runtime_status(
    state: State<'_, AppState>,
) -> Result<Vec<ServiceRuntimeSummary>, String> {
    Ok(state.manager.read().await.list_runtime_status())
}

/// 创建本地工具 HTTP 服务配置并立即启动。即使后续暂停，配置仍保留在 SQLite。
#[tauri::command]
pub async fn create_tool_service(
    state: State<'_, AppState>,
    input: ToolServiceInput,
) -> Result<ToolServiceSummary, String> {
    let config = state.db.create_tool_service(&input).map_err(String::from)?;
    state
        .tool_services
        .write()
        .await
        .start(config.id.clone(), config.to_input())
        .await
        .map_err(String::from)
}

/// 读取本地工具 HTTP 服务完整配置，用于前端编辑回填。
#[tauri::command]
pub fn get_tool_service(
    state: State<'_, AppState>,
    id: String,
) -> Result<ToolServiceConfig, String> {
    state.db.get_tool_service(&id).map_err(String::from)
}

/// 更新本地工具 HTTP 服务配置；若原服务运行中，会用新配置重启。
#[tauri::command]
pub async fn update_tool_service(
    state: State<'_, AppState>,
    id: String,
    input: ToolServiceInput,
) -> Result<ToolServiceSummary, String> {
    let stopping = {
        let mut tool_services = state.tool_services.write().await;
        tool_services.begin_stop(&id)
    };
    let should_restart = stopping.is_some();
    if let Some(stopping) = stopping {
        stopping.wait().await.map_err(String::from)?;
    }
    let config = state
        .db
        .update_tool_service(&id, &input)
        .map_err(String::from)?;
    if should_restart {
        return state
            .tool_services
            .write()
            .await
            .start(config.id.clone(), config.to_input())
            .await
            .map_err(String::from);
    }
    state.db.get_tool_service_summary(&id).map_err(String::from)
}

/// 启动已保存的本地工具 HTTP 服务。
#[tauri::command]
pub async fn start_tool_service(
    state: State<'_, AppState>,
    id: String,
) -> Result<ToolServiceSummary, String> {
    let config = state.db.get_tool_service(&id).map_err(String::from)?;
    state
        .tool_services
        .write()
        .await
        .start(config.id.clone(), config.to_input())
        .await
        .map_err(String::from)
}

/// 停止本地工具服务。
#[tauri::command]
pub async fn stop_tool_service(
    state: State<'_, AppState>,
    id: String,
) -> Result<RuntimeStatus, String> {
    let stopping = {
        let mut tool_services = state.tool_services.write().await;
        tool_services.begin_stop(&id)
    };
    let Some(stopping) = stopping else {
        return Ok(RuntimeStatus::Stopped);
    };
    stopping.wait().await.map_err(String::from)
}

/// 删除本地工具服务配置。若服务正在运行，会先暂停再软删除。
#[tauri::command]
pub async fn delete_tool_service(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let stopping = {
        let mut tool_services = state.tool_services.write().await;
        tool_services.begin_stop(&id)
    };
    if let Some(stopping) = stopping {
        stopping.wait().await.map_err(String::from)?;
    }
    state.db.delete_tool_service(&id).map_err(String::from)
}

/// 查询本地工具服务配置列表，并合并当前运行态。
#[tauri::command]
pub async fn list_tool_services(
    state: State<'_, AppState>,
) -> Result<Vec<ToolServiceSummary>, String> {
    let mut summaries = state
        .db
        .list_tool_service_summaries()
        .map_err(String::from)?;
    let tool_services = state.tool_services.read().await;
    for summary in &mut summaries {
        if let Some(running) = tool_services.running_summary(&summary.id) {
            *summary = running;
        }
    }
    Ok(summaries)
}

/// 测试服务配置的基础可达性。
#[tauri::command]
pub async fn test_service(state: State<'_, AppState>, id: String) -> Result<TestResult, String> {
    let detail = state.db.get_service(&id).map_err(String::from)?;
    run_test(|| async move { test_service_detail(&detail).await })
        .await
        .map_err(String::from)
}

/// 查询 SSH Profile 列表。
#[tauri::command]
pub fn list_ssh_profiles(state: State<'_, AppState>) -> Result<Vec<SshProfile>, String> {
    state.db.list_ssh_profiles().map_err(String::from)
}

/// 获取单个 SSH Profile，不返回明文 secret。
#[tauri::command]
pub fn get_ssh_profile(state: State<'_, AppState>, id: String) -> Result<SshProfile, String> {
    state.db.get_ssh_profile(&id).map_err(String::from)
}

/// 创建 SSH Profile。明文敏感字段只在 Rust 侧短暂存在。
#[tauri::command]
pub fn create_ssh_profile(
    state: State<'_, AppState>,
    input: SshProfileInput,
) -> Result<SshProfile, String> {
    let password_secret = match input.password.as_deref().filter(|value| !value.is_empty()) {
        Some(password) => Some(
            crypto::store_secret(
                &state.db,
                &format!("{} password", input.name),
                "ssh_password",
                password,
            )
            .map_err(String::from)?,
        ),
        None => None,
    };
    let passphrase_secret = match input
        .private_key_passphrase
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        Some(passphrase) => Some(
            crypto::store_secret(
                &state.db,
                &format!("{} key passphrase", input.name),
                "ssh_key_passphrase",
                passphrase,
            )
            .map_err(String::from)?,
        ),
        None => None,
    };
    state
        .db
        .create_ssh_profile(
            &input,
            password_secret.as_deref(),
            passphrase_secret.as_deref(),
        )
        .map_err(String::from)
}

/// 更新 SSH Profile。secret 字段为空时保留旧值。
#[tauri::command]
pub fn update_ssh_profile(
    state: State<'_, AppState>,
    id: String,
    input: SshProfileInput,
) -> Result<SshProfile, String> {
    let password_secret = match input.password.as_deref().filter(|value| !value.is_empty()) {
        Some(password) => Some(
            crypto::store_secret(
                &state.db,
                &format!("{} password", input.name),
                "ssh_password",
                password,
            )
            .map_err(String::from)?,
        ),
        None => None,
    };
    let passphrase_secret = match input
        .private_key_passphrase
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        Some(passphrase) => Some(
            crypto::store_secret(
                &state.db,
                &format!("{} key passphrase", input.name),
                "ssh_key_passphrase",
                passphrase,
            )
            .map_err(String::from)?,
        ),
        None => None,
    };
    state
        .db
        .update_ssh_profile(
            &id,
            &input,
            password_secret.as_deref(),
            passphrase_secret.as_deref(),
        )
        .map_err(String::from)
}

/// 删除 SSH Profile。
#[tauri::command]
pub fn delete_ssh_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.db.delete_ssh_profile(&id).map_err(String::from)
}

/// 测试 SSH Profile 的真实 SSH 握手和认证。
#[tauri::command]
pub async fn test_ssh_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<TestResult, String> {
    proxy::ssh::test_ssh_profile_connection(Arc::clone(&state.db), id)
        .await
        .map_err(String::from)
}

/// 查询日志。
#[tauri::command]
pub fn list_logs(state: State<'_, AppState>, filter: LogFilter) -> Result<Vec<LogRow>, String> {
    state.db.list_logs(&filter).map_err(String::from)
}

/// 清理日志。
#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>, service_id: Option<String>) -> Result<(), String> {
    state
        .db
        .clear_logs(service_id.as_deref())
        .map_err(String::from)
}

/// 读取全部系统代理配置档。
#[tauri::command]
pub fn list_system_proxy_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SystemProxyProfile>, String> {
    state.db.list_system_proxy_profiles().map_err(String::from)
}

/// 创建系统代理配置档。
#[tauri::command]
pub fn create_system_proxy_profile(
    state: State<'_, AppState>,
    input: SystemProxyProfileInput,
) -> Result<SystemProxyProfile, String> {
    state
        .db
        .create_system_proxy_profile(&input)
        .map_err(String::from)
}

/// 更新系统代理配置档。
#[tauri::command]
pub fn update_system_proxy_profile(
    state: State<'_, AppState>,
    id: String,
    input: SystemProxyProfileInput,
) -> Result<SystemProxyProfile, String> {
    state
        .db
        .update_system_proxy_profile(&id, &input)
        .map_err(String::from)
}

/// 删除系统代理配置档。
#[tauri::command]
pub fn delete_system_proxy_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .delete_system_proxy_profile(&id)
        .map_err(String::from)
}

/// 设置系统代理。profile_id 可以是系统代理配置 id 或 HTTP forward 服务 id。
#[tauri::command]
pub async fn set_system_proxy(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<SystemProxyStatus, String> {
    if let Ok(service) = state.db.get_service(&profile_id) {
        if service.kind == ServiceKind::HttpForward
            && !matches!(
                state.manager.read().await.runtime_status(&profile_id),
                RuntimeStatus::Running
            )
        {
            return Err("系统代理只能指向运行中的 HTTP Forward 服务".to_string());
        }
    }
    let target = state
        .db
        .get_system_proxy_target(&profile_id)
        .map_err(String::from)?;
    let status = system_proxy::set_system_proxy(&target).map_err(String::from)?;
    state
        .db
        .set_active_system_proxy_profile(Some(&profile_id))
        .map_err(String::from)?;
    state
        .db
        .insert_service_event(None, "info", &status.message, "{}")
        .map_err(String::from)?;
    Ok(status)
}

/// 按手动填写的目标设置系统代理，不要求先创建系统代理配置。
#[tauri::command]
pub fn set_system_proxy_target(
    state: State<'_, AppState>,
    target: SystemProxyTarget,
) -> Result<SystemProxyStatus, String> {
    let status = system_proxy::set_system_proxy(&target).map_err(String::from)?;
    state
        .db
        .set_active_system_proxy_profile(None)
        .map_err(String::from)?;
    state
        .db
        .insert_service_event(None, "info", &status.message, "{}")
        .map_err(String::from)?;
    Ok(status)
}

/// 清理系统代理。
#[tauri::command]
pub fn clear_system_proxy(state: State<'_, AppState>) -> Result<SystemProxyStatus, String> {
    let status = system_proxy::clear_system_proxy().map_err(String::from)?;
    state
        .db
        .set_active_system_proxy_profile(None)
        .map_err(String::from)?;
    state
        .db
        .insert_service_event(None, "info", &status.message, "{}")
        .map_err(String::from)?;
    Ok(status)
}

/// 查询系统代理状态。
#[tauri::command]
pub fn get_system_proxy_status() -> Result<SystemProxyStatus, String> {
    system_proxy::get_system_proxy_status().map_err(String::from)
}

/// 获取当前默认路由对应的局域网 IPv4；无法判断时返回空值，前端会降级为通配地址。
#[tauri::command]
pub fn get_lan_ip() -> Result<Option<String>, String> {
    Ok(resolve_lan_ipv4().map(|ip| ip.to_string()))
}

/// 获取系统和应用信息。
#[tauri::command]
pub fn get_system_info(app: AppHandle) -> Result<SystemInfo, String> {
    let data_dir = crate::resolve_app_data_dir_for_handle(&app)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: app.package_info().version.to_string(),
        data_dir,
    })
}

fn resolve_lan_ipv4() -> Option<Ipv4Addr> {
    choose_lan_ipv4(
        LAN_ROUTE_PROBE_TARGETS.iter().filter_map(|target| {
            local_ipv4_for_udp_target(SocketAddr::new(IpAddr::V4(*target), 80))
        }),
    )
}

fn local_ipv4_for_udp_target(target: SocketAddr) -> Option<Ipv4Addr> {
    let socket = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(target).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

fn choose_lan_ipv4<I>(candidates: I) -> Option<Ipv4Addr>
where
    I: IntoIterator<Item = Ipv4Addr>,
{
    let mut fallback = None;
    for ip in candidates {
        if !is_usable_lan_ipv4(ip) {
            continue;
        }
        if ip.is_private() {
            return Some(ip);
        }
        fallback.get_or_insert(ip);
    }
    fallback
}

fn is_usable_lan_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_broadcast()
        && !ip.is_multicast()
        && !ip.is_link_local()
}

async fn run_test<F, Fut>(f: F) -> AppResult<TestResult>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = AppResult<String>>,
{
    let started = Instant::now();
    match f().await {
        Ok(message) => Ok(TestResult {
            ok: true,
            message,
            duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        }),
        Err(err) => Ok(TestResult {
            ok: false,
            message: err.to_string(),
            duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        }),
    }
}

async fn test_service_detail(detail: &ServiceDetail) -> AppResult<String> {
    if let Some(cfg) = &detail.http_reverse {
        let url = url::Url::parse(&cfg.target_url)?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("目标 URL 缺少 host".to_string()))?;
        let port = url.port_or_known_default().ok_or_else(|| {
            AppError::InvalidInput("目标 URL 缺少端口且无法根据 scheme 推断".to_string())
        })?;
        tcp_connect_test(
            &format!("{host}:{port}"),
            cfg.request_timeout_ms.min(10_000),
        )
        .await?;
        return Ok("HTTP 反向代理目标端口可达".to_string());
    }
    if detail.http_forward.is_some() {
        return Ok("HTTP forward 配置有效，可启动后用浏览器或 curl 验证".to_string());
    }
    if let Some(cfg) = &detail.tcp_forward {
        tcp_connect_test(
            &format!("{}:{}", cfg.target_host, cfg.target_port),
            cfg.connect_timeout_ms,
        )
        .await?;
        return Ok("TCP 目标端口可达".to_string());
    }
    if let Some(cfg) = &detail.udp_forward {
        return Ok(format!(
            "UDP 目标 {}:{} 已通过配置校验",
            cfg.target_host, cfg.target_port
        ));
    }
    if detail.ssh_tunnel.is_some() {
        return Ok("SSH 隧道配置已保存，启动时会建立隧道".to_string());
    }
    Err(AppError::InvalidInput("服务缺少类型专用配置".to_string()))
}

async fn tcp_connect_test(addr: &str, timeout_ms: u64) -> AppResult<()> {
    timeout(
        Duration::from_millis(timeout_ms.max(1)),
        TcpStream::connect(addr),
    )
    .await
    .map_err(|_| AppError::Message(format!("连接 {addr} 超时")))??;
    Ok(())
}

async fn stop_service_for_command(
    app: AppHandle,
    state: &AppState,
    id: &str,
) -> Result<RuntimeStatus, String> {
    let stopping = {
        let mut manager = state.manager.write().await;
        manager.begin_stop_service(id)
    };
    let Some(stopping) = stopping else {
        return Ok(RuntimeStatus::Stopped);
    };

    let outcome = stopping
        .wait(Arc::clone(&state.db), app)
        .await
        .map_err(String::from)?;
    let status = outcome.status();
    {
        let mut manager = state.manager.write().await;
        manager.complete_stop(outcome);
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_lan_ipv4_prefers_private_addresses() {
        let ip = choose_lan_ipv4([
            Ipv4Addr::new(203, 0, 113, 9),
            Ipv4Addr::new(192, 168, 1, 23),
            Ipv4Addr::new(10, 0, 0, 8),
        ]);

        assert_eq!(ip, Some(Ipv4Addr::new(192, 168, 1, 23)));
    }

    #[test]
    fn choose_lan_ipv4_skips_loopback_unspecified_and_link_local() {
        let ip = choose_lan_ipv4([
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::new(169, 254, 1, 2),
            Ipv4Addr::new(10, 10, 0, 2),
        ]);

        assert_eq!(ip, Some(Ipv4Addr::new(10, 10, 0, 2)));
    }
}
