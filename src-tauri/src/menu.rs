//! Native OS menu bar.
//!
//! Spec: docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md Task 7 (with AMD-10).
//! bd issue: tuxlink-6vi
//!
//! Categories per docs/ux-anti-patterns.md: File / Message / Session / Mailbox /
//! View / Tools / Help. Menu item click events fire Tauri events of the form
//! `menu:{category}:{action}` (e.g., `menu:file:new`, `menu:session:connect`)
//! consumed by the React frontend.
//!
//! AMD-10 (2026-05-17) added the wizard-half (`menu:session:test_send`) and
//! the runtime-half (show_transport, radio_dock, raw_log, settings_*).
//!
//! `menu_event_ids()` is a pure function exposing every event ID for
//! regression testing — Tauri's platform-handle-holding menu objects aren't
//! trivially assertable from unit tests, so the event-id manifest is the
//! tested contract.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

/// Pure-function manifest of every menu event ID this module emits. Test
/// surface for `tests/menu_test.rs`. Order matches the menu layout for
/// human-readability when diffing.
pub fn menu_event_ids() -> Vec<&'static str> {
    vec![
        "menu:file:new", "menu:file:quit",
        "menu:message:reply", "menu:message:reply_all", "menu:message:forward", "menu:message:print",
        "menu:session:connect", "menu:session:disconnect", "menu:session:log",
        "menu:session:test_send",         // AMD-10 wizard half
        "menu:session:show_transport",    // AMD-10 runtime half
        "menu:mailbox:inbox", "menu:mailbox:sent", "menu:mailbox:outbox",
        "menu:view:session_log", "menu:view:status_bar",
        "menu:view:raw_log",              // AMD-10 runtime half
        "menu:view:radio_dock",           // AMD-10 runtime half
        "menu:tools:templates", "menu:tools:rig_control", "menu:tools:preferences",
        "menu:tools:settings_connection",         // AMD-10 runtime half
        "menu:tools:settings_privacy_gps",        // AMD-10 runtime half
        "menu:tools:settings_privacy_position",   // AMD-10 runtime half
        "menu:tools:settings_gps",                // AMD-10 runtime half
        "menu:help:about", "menu:help:docs", "menu:help:report_issue",
    ]
}

pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let file = SubmenuBuilder::new(app, "File")
        .item(&MenuItemBuilder::with_id("menu:file:new", "New Message").accelerator("CmdOrCtrl+N").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu:file:quit", "Quit").accelerator("CmdOrCtrl+Q").build(app)?)
        .build()?;

    let message = SubmenuBuilder::new(app, "Message")
        .item(&MenuItemBuilder::with_id("menu:message:reply", "Reply").accelerator("CmdOrCtrl+R").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:reply_all", "Reply All").accelerator("CmdOrCtrl+Shift+R").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:forward", "Forward").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:message:print", "Print").accelerator("CmdOrCtrl+P").build(app)?)
        .build()?;

    let session = SubmenuBuilder::new(app, "Session")
        .item(&MenuItemBuilder::with_id("menu:session:connect", "Connect").accelerator("F5").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:session:disconnect", "Disconnect").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("menu:session:log", "Session Log").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:session:test_send", "Test send").build(app)?)             // AMD-10
        .item(&MenuItemBuilder::with_id("menu:session:show_transport", "Show transport").build(app)?)   // AMD-10
        .build()?;

    let mailbox = SubmenuBuilder::new(app, "Mailbox")
        .item(&MenuItemBuilder::with_id("menu:mailbox:inbox", "Inbox").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:mailbox:sent", "Sent").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:mailbox:outbox", "Outbox").build(app)?)
        .build()?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&MenuItemBuilder::with_id("menu:view:session_log", "Toggle Session Log").accelerator("CmdOrCtrl+Shift+L").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:view:raw_log", "Show Raw Session Log").build(app)?)        // AMD-10
        .item(&MenuItemBuilder::with_id("menu:view:status_bar", "Toggle Status Bar").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:view:radio_dock", "Show Radio Dock").accelerator("CmdOrCtrl+Shift+M").build(app)?)  // AMD-10
        .build()?;

    // Settings submenu under Tools (AMD-10) — nested for the Connection / Privacy
    // / GPS groupings.
    let settings_privacy = SubmenuBuilder::new(app, "Privacy")
        .item(&MenuItemBuilder::with_id("menu:tools:settings_privacy_gps", "GPS state").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:tools:settings_privacy_position", "Position precision").build(app)?)
        .build()?;
    let settings = SubmenuBuilder::new(app, "Settings")
        .item(&MenuItemBuilder::with_id("menu:tools:settings_connection", "Connection").build(app)?)
        .item(&settings_privacy)
        .item(&MenuItemBuilder::with_id("menu:tools:settings_gps", "GPS").build(app)?)
        .build()?;

    let tools = SubmenuBuilder::new(app, "Tools")
        .item(&MenuItemBuilder::with_id("menu:tools:templates", "Templates").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:tools:rig_control", "Rig Control").build(app)?)
        .separator()
        .item(&settings)                                                                    // AMD-10
        .item(&MenuItemBuilder::with_id("menu:tools:preferences", "Preferences").build(app)?)
        .build()?;

    let help = SubmenuBuilder::new(app, "Help")
        .item(&MenuItemBuilder::with_id("menu:help:about", "About Tuxlink").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:help:docs", "Documentation").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:help:report_issue", "Report Issue").build(app)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[&file, &message, &session, &mailbox, &view, &tools, &help])
        .build()
}

/// Wire menu events to Tauri IPC so the React frontend can listen.
///
/// Quit is special-cased to exit natively because the frontend listener
/// doesn't exist yet (Tasks 9+ will add it). The event still emits, so a
/// future listener can observe and — if needed — intercept by calling a
/// Tauri command to do cleanup (e.g., "discard unsaved draft?" dialog)
/// before triggering final exit. Apps must always be Quit-able.
pub fn wire_menu_events<R: Runtime>(app: &AppHandle<R>) {
    let app_for_handler = app.clone();
    app.on_menu_event(move |_app, event| {
        let id = event.id().as_ref().to_string();
        let _ = app_for_handler.emit("menu", &id);
        if id == "menu:file:quit" {
            app_for_handler.exit(0);
        }
    });
}
