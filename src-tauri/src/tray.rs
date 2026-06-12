//! @author kongweiguang
//! 系统托盘与主窗口显隐行为。

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Runtime, Window, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "net-power-main-tray";
const MENU_SHOW: &str = "show-main";
const MENU_HIDE: &str = "hide-main";
const MENU_QUIT: &str = "quit-app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayMenuAction {
    Show,
    Hide,
    Quit,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrayMenuItemSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub action: TrayMenuAction,
}

const TRAY_MENU_ITEMS: &[TrayMenuItemSpec] = &[
    TrayMenuItemSpec {
        id: MENU_SHOW,
        label: "显示窗口",
        action: TrayMenuAction::Show,
    },
    TrayMenuItemSpec {
        id: MENU_HIDE,
        label: "隐藏到托盘",
        action: TrayMenuAction::Hide,
    },
    TrayMenuItemSpec {
        id: MENU_QUIT,
        label: "退出 net-power",
        action: TrayMenuAction::Quit,
    },
];

/// 初始化系统托盘菜单。
pub fn setup(app: &App) -> tauri::Result<()> {
    let show = MenuItem::with_id(
        app,
        TRAY_MENU_ITEMS[0].id,
        TRAY_MENU_ITEMS[0].label,
        true,
        None::<&str>,
    )?;
    let hide = MenuItem::with_id(
        app,
        TRAY_MENU_ITEMS[1].id,
        TRAY_MENU_ITEMS[1].label,
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        TRAY_MENU_ITEMS[2].id,
        TRAY_MENU_ITEMS[2].label,
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("net-power 代理工具")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match tray_menu_action(event.id().as_ref()) {
            TrayMenuAction::Show => show_main_window(app),
            TrayMenuAction::Hide => hide_main_window(app),
            TrayMenuAction::Quit => app.exit(0),
            TrayMenuAction::Ignore => {}
        })
        .on_tray_icon_event(|tray, event| {
            if should_show_window_from_tray(&event) {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    bind_main_window_close_behavior(app);
    Ok(())
}

pub(crate) fn tray_menu_items() -> &'static [TrayMenuItemSpec] {
    TRAY_MENU_ITEMS
}

pub(crate) fn tray_menu_action(id: &str) -> TrayMenuAction {
    tray_menu_items()
        .iter()
        .find(|item| item.id == id)
        .map(|item| item.action)
        .unwrap_or(TrayMenuAction::Ignore)
}

fn bind_main_window_close_behavior(app: &App) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let window_for_close = window.clone();
        // 直接绑定到主 WebView 窗口，确保 Windows 原生关闭按钮先拦截再隐藏到托盘。
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window_for_close.hide();
            }
        });
    }
}

/// 处理主窗口关闭请求：关闭按钮默认隐藏到托盘，保持后台代理服务继续运行。
pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

pub(crate) fn should_show_window_from_tray(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } | TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    )
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.hide();
    }
}
