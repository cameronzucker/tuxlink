//! System tray icon + window-close-to-tray behaviour.
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.1.
//! bd issue: tuxlink-rit
//!
//! ## Tray menu
//! - Show Window       (`tray:show_hide`)     — unconditional show + unminimize + focus (tuxlink-9zd)
//! - New Message       (`tray:new_message`)  — shows window + emits `menu` event `menu:file:new`
//! - ──────────────
//! - Quit              (`tray:quit`)          — calls `app.exit(0)`
//!
//! ## Close-to-tray
//! `on_window_event` CloseRequested → `api.prevent_close()` + (Linux) `minimize()`
//! / (other) `hide()`. See `lib.rs` for the per-OS rationale (tuxlink-9zd).
//! Only File→Quit / tray→Quit / Ctrl+Q (wired via menu.rs) actually exit the process.
//! This is load-bearing for emcomm: closing the window mid-ARQ must NOT kill the
//! Pat child process.
//!
//! ## Restore reliability (tuxlink-9zd)
//! The tray menu's restore item is an **unconditional** "Show Window" — it never
//! branches on `window.is_visible()` (which is unreliable after a Wayland
//! hide/minimize and could invert into hiding again). It always
//! `unminimize() + show() + set_focus()`, so restore works on every cycle.
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

/// Tray icon embedded at compile time (RGBA pixels, no runtime file I/O).
/// `iconAsTemplate` behaviour preserved on macOS via `icon_as_template(true)` below.
const TRAY_ICON: tauri::image::Image<'_> = tauri::include_image!("icons/tray-icon.png");

/// Pure-function manifest of every tray event ID this module handles.
/// Test surface for `tests/tray_test.rs`. Order matches the tray menu layout.
pub fn tray_event_ids() -> Vec<&'static str> {
    vec!["tray:show_hide", "tray:new_message", "tray:quit"]
}

/// Build and install the tray icon + menu, and wire its events.
///
/// Call from `lib.rs`'s `run()` inside the `.setup()` closure.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // tuxlink-9zd: an explicit "Show Window" (unconditional show), NOT a
    // visibility toggle. Event id stays `tray:show_hide` (tray_test.rs manifest).
    let show_hide = MenuItemBuilder::with_id("tray:show_hide", "Show Window").build(app)?;
    let new_msg = MenuItemBuilder::with_id("tray:new_message", "New Message").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("tray:quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_hide)
        .item(&new_msg)
        .item(&sep)
        .item(&quit)
        .build()?;

    // Use the compile-time embedded tray icon. `icon_as_template(true)` preserves the
    // macOS template-image behaviour that was previously set in tauri.conf.json's
    // now-removed `app.trayIcon` block (no-op on Linux/Windows; safe to keep).
    let _tray = TrayIconBuilder::new()
        .icon(TRAY_ICON)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false) // left-click = show/hide; right-click = menu
        .on_menu_event({
            let app_clone = app.clone();
            move |_app: &AppHandle<R>, event: MenuEvent| {
                let id = event.id().as_ref().to_string();
                match id.as_str() {
                    "tray:quit" => {
                        tracing::info!(
                            target: "tuxlink::tray",
                            event = "quit",
                            "tray menu event",
                        );
                        app_clone.exit(0);
                    }
                    "tray:show_hide" => {
                        tracing::debug!(
                            target: "tuxlink::tray",
                            event = "show_window",
                            "tray menu event",
                        );
                        // Unconditional show (tuxlink-9zd) — never toggle on the
                        // unreliable is_visible() state.
                        show_main_window(&app_clone);
                    }
                    "tray:new_message" => {
                        tracing::debug!(
                            target: "tuxlink::tray",
                            event = "new_message",
                            "tray menu event",
                        );
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

/// Show the main window and focus it. Unconditional + idempotent: unminimize
/// first (the Linux close-to-tray path minimizes — tuxlink-9zd), then show
/// (covers the macOS/Windows hide path), then focus. Safe to call when already
/// visible. Never branches on is_visible() (unreliable on Wayland after
/// hide/minimize).
fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
