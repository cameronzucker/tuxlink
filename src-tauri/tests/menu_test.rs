// Tests for tuxlink-6vi — Task 7: Native OS menu bar.
//
// Spec: docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md Task 7 (with AMD-10 wizard+runtime halves).
// bd issue: tuxlink-6vi
//
// Tauri's menu objects hold platform handles and aren't trivially assertable from
// unit tests; instead we expose `menu_event_ids()` as a pure function listing every
// event ID the menu emits, and verify the expected IDs are present. The Tauri
// `tauri::menu::*` API itself is exercised at runtime via `pnpm tauri dev` (Step 5,
// manual operator verification per the plan's "Manual verification tax" section).

use tuxlink_lib::menu;

#[test]
fn test_menu_exposes_required_event_ids() {
    let ids = menu::menu_event_ids();
    let required = [
        "menu:file:new", "menu:file:quit",
        "menu:message:reply", "menu:message:reply_all", "menu:message:forward", "menu:message:print",
        "menu:session:connect", "menu:session:disconnect", "menu:session:log",
        "menu:session:test_send",         // AMD-10 wizard half
        "menu:session:show_transport",    // AMD-10 runtime half
        "menu:mailbox:inbox", "menu:mailbox:sent", "menu:mailbox:outbox",
        "menu:view:session_log", "menu:view:status_bar",
        "menu:view:raw_log",              // AMD-10 runtime half
        "menu:view:radio_dock",           // AMD-10 runtime half
        "menu:view:scheme:default", "menu:view:scheme:night-red", "menu:view:scheme:grayscale",  // tuxlink-8za
        "menu:tools:templates", "menu:tools:rig_control", "menu:tools:preferences",
        "menu:tools:settings_connection",         // AMD-10 runtime half
        "menu:tools:settings_privacy_gps",        // AMD-10 runtime half
        "menu:tools:settings_privacy_position",   // AMD-10 runtime half
        "menu:tools:settings_gps",                // AMD-10 runtime half
        "menu:help:about", "menu:help:docs", "menu:help:report_issue",
    ];
    for r in required {
        assert!(ids.contains(&r), "missing menu event id: {}", r);
    }
}
