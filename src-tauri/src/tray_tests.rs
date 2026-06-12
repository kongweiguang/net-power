//! @author kongweiguang
//! 系统托盘事件和菜单结构测试。

use crate::tray::{
    should_show_window_from_tray, tray_menu_action, tray_menu_items, TrayMenuAction,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{PhysicalPosition, PhysicalSize, Position, Rect, Size};

const TRAY_ID: &str = "net-power-main-tray";

fn tray_rect() -> Rect {
    Rect {
        position: Position::Physical(PhysicalPosition::new(0, 0)),
        size: Size::Physical(PhysicalSize::new(16, 16)),
    }
}

#[test]
fn left_click_release_shows_main_window() {
    let event = TrayIconEvent::Click {
        id: TRAY_ID.into(),
        position: PhysicalPosition::new(0.0, 0.0),
        rect: tray_rect(),
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
    };

    assert!(should_show_window_from_tray(&event));
}

#[test]
fn right_click_does_not_show_main_window() {
    let event = TrayIconEvent::Click {
        id: TRAY_ID.into(),
        position: PhysicalPosition::new(0.0, 0.0),
        rect: tray_rect(),
        button: MouseButton::Right,
        button_state: MouseButtonState::Up,
    };

    assert!(!should_show_window_from_tray(&event));
}

#[test]
fn left_double_click_shows_main_window() {
    let event = TrayIconEvent::DoubleClick {
        id: TRAY_ID.into(),
        position: PhysicalPosition::new(0.0, 0.0),
        rect: tray_rect(),
        button: MouseButton::Left,
    };

    assert!(should_show_window_from_tray(&event));
}

#[test]
fn tray_menu_uses_stable_chinese_items() {
    let items = tray_menu_items();

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].id, "show-main");
    assert_eq!(items[0].label, "显示窗口");
    assert_eq!(items[0].action, TrayMenuAction::Show);
    assert_eq!(items[1].id, "hide-main");
    assert_eq!(items[1].label, "隐藏到托盘");
    assert_eq!(items[1].action, TrayMenuAction::Hide);
    assert_eq!(items[2].id, "quit-app");
    assert_eq!(items[2].label, "退出 net-power");
    assert_eq!(items[2].action, TrayMenuAction::Quit);
}

#[test]
fn tray_menu_action_ignores_unknown_ids() {
    assert_eq!(tray_menu_action("show-main"), TrayMenuAction::Show);
    assert_eq!(tray_menu_action("hide-main"), TrayMenuAction::Hide);
    assert_eq!(tray_menu_action("quit-app"), TrayMenuAction::Quit);
    assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
}
