//! @author kongweiguang
//! Net Power Tauri 应用入口。这里只负责组装插件、状态和 Command。

#[cfg(test)]
mod app_path_tests;
mod app_state;
mod commands;
mod crypto;
mod database;
mod error;
mod manager;
mod models;
mod proxy;
mod system_proxy;
mod tray;
#[cfg(test)]
mod tray_tests;

use app_state::AppState;
use database::Database;
use models::{AppSetting, ServiceDetail};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

const APP_DATA_DIR_ENV: &str = "NET_POWER_APP_DATA_DIR";

/// 启动 Tauri 应用。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("net-power")
                .build(),
        )
        .on_window_event(tray::handle_window_event)
        .setup(|app| {
            tray::setup(app)?;
            let data_dir = resolve_app_data_dir(app)?;
            let db_path = data_dir.join("proxy-tool.db");
            let db = Database::init(&db_path)?;
            let state = AppState::new(db);
            if let Err(err) = state.db.prune_logs_from_settings() {
                let _ = state.db.insert_service_event(
                    None,
                    "warn",
                    &format!("日志保留清理失败: {err}"),
                    "{}",
                );
            }
            let db_for_auto_start = Arc::clone(&state.db);
            let settings = state.db.list_settings().unwrap_or_default();
            let services_to_start = if service_auto_start_enabled(&settings) {
                state
                    .db
                    .list_services()
                    .map(services_selected_for_auto_start)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            app.manage(state);

            if !services_to_start.is_empty() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    for service in services_to_start {
                        let _ = state
                            .manager
                            .write()
                            .await
                            .start_service(service, Arc::clone(&db_for_auto_start), handle.clone())
                            .await;
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_settings,
            commands::update_app_setting,
            commands::get_autostart_status,
            commands::set_autostart,
            commands::list_services,
            commands::get_service,
            commands::create_service,
            commands::update_service,
            commands::delete_service,
            commands::duplicate_service,
            commands::start_service,
            commands::stop_service,
            commands::restart_service,
            commands::list_runtime_status,
            commands::test_service,
            commands::list_ssh_profiles,
            commands::get_ssh_profile,
            commands::create_ssh_profile,
            commands::update_ssh_profile,
            commands::delete_ssh_profile,
            commands::test_ssh_profile,
            commands::list_logs,
            commands::clear_logs,
            commands::list_system_proxy_profiles,
            commands::create_system_proxy_profile,
            commands::update_system_proxy_profile,
            commands::delete_system_proxy_profile,
            commands::set_system_proxy,
            commands::set_system_proxy_target,
            commands::clear_system_proxy,
            commands::get_system_proxy_status,
            commands::get_system_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn resolve_app_data_dir(app: &tauri::App) -> tauri::Result<PathBuf> {
    if let Some(path) = app_data_dir_override(std::env::var_os(APP_DATA_DIR_ENV)) {
        return Ok(path);
    }
    app.path().app_data_dir()
}

fn app_data_dir_override(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn service_auto_start_enabled(settings: &[AppSetting]) -> bool {
    settings.iter().any(|setting| {
        setting.key == "services.auto_start_enabled" && setting.value_json.trim() == "true"
    })
}

fn services_selected_for_auto_start(services: Vec<ServiceDetail>) -> Vec<ServiceDetail> {
    services
        .into_iter()
        .filter(|service| service.enabled && service.auto_start)
        .collect()
}
