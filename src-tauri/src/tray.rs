//! System tray icon + window-close-to-tray behaviour.
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.1.
//! bd issue: tuxlink-rit
//!
//! ## Tray menu
//! - Show/Hide Window  (`tray:show_hide`)
//! - New Message       (`tray:new_message`)  — shows window + emits `menu` event `menu:file:new`
//! - ──────────────
//! - Quit              (`tray:quit`)          — calls `app.exit(0)`
//!
//! ## Close-to-tray
//! `on_window_event` CloseRequested → `window.hide()` + `api.prevent_close()`.
//! Only File→Quit / tray→Quit / Ctrl+Q (wired via menu.rs) actually exit the process.
//! This is load-bearing for emcomm: closing the window mid-ARQ must NOT kill the
//! Pat child process.
//!
//! ## Quit pattern
//! `PredefinedMenuItem::quit` is Linux-Unsupported in Tauri 2 / muda (silently
//! no-ops on GTK). The canonical Linux pattern (PR #71 fix) is a custom
//! `MenuItemBuilder` + `on_menu_event` → `app.exit(0)`. This matches the pattern
//! in `menu.rs::wire_menu_events`.
//!
//! ## Event IDs
//! `tray_event_ids()` is the pure-function test surface (same pattern as
//! `menu::menu_event_ids()`). Order matches tray menu layout for human-readability.

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri::menu::MenuEvent;

/// Pure-function manifest of every tray event ID this module handles.
/// Test surface for `tests/tray_test.rs`. Order matches the tray menu layout.
pub fn tray_event_ids() -> Vec<&'static str> {
    vec!["tray:show_hide", "tray:new_message", "tray:quit"]
}

/// Build and install the tray icon + menu, and wire its events.
///
/// Call from `lib.rs`'s `run()` inside the `.setup()` closure.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show_hide = MenuItemBuilder::with_id("tray:show_hide", "Show/Hide Window").build(app)?;
    let new_msg = MenuItemBuilder::with_id("tray:new_message", "New Message").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("tray:quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_hide)
        .item(&new_msg)
        .item(&sep)
        .item(&quit)
        .build()?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("tray: no window icon configured — add icons/32x32.png to tauri.conf.json bundle.icon");

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false) // left-click = show/hide; right-click = menu
        .on_menu_event({
            let app_clone = app.clone();
            move |_app: &AppHandle<R>, event: MenuEvent| {
                let id = event.id().as_ref().to_string();
                match id.as_str() {
                    "tray:quit" => {
                        app_clone.exit(0);
                    }
                    "tray:show_hide" => {
                        toggle_main_window(&app_clone);
                    }
                    "tray:new_message" => {
                        // Show window first, then emit menu:file:new so the
                        // compose-window open handler (Task 14) is live.
                        show_main_window(&app_clone);
                        let _ = app_clone.emit("menu", "menu:file:new");
                    }
                    _ => {}
                }
            }
        })
        .on_tray_icon_event({
            let app_clone = app.clone();
            move |_tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(&app_clone);
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Show the main window and focus it.
fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Toggle the main window visibility.
fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
