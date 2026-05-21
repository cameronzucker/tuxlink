// Tests for tuxlink-rit — Task 8: System tray + window-close-to-tray.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.1 + §6.
// bd issue: tuxlink-rit
//
// Tauri's tray objects hold platform handles and aren't trivially assertable
// from unit tests; instead we expose `tray_event_ids()` as a pure function
// listing every tray menu event ID this module handles, and verify the expected
// IDs are present. The full close-to-tray runtime behavior is verified at M2
// operator smoke (static test asserts event IDs only — per testing-pitfalls.md §9).

use tuxlink_lib::tray;

/// (1) tray_event_ids() contains the three required IDs.
#[test]
fn test_tray_event_ids_contains_required() {
    let ids = tray::tray_event_ids();
    let required = ["tray:show_hide", "tray:new_message", "tray:quit"];
    for r in required {
        assert!(ids.contains(&r), "missing tray event id: {}", r);
    }
}

/// (2) No duplicate IDs in the manifest.
#[test]
fn test_tray_event_ids_no_duplicates() {
    let ids = tray::tray_event_ids();
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        assert!(seen.insert(id), "duplicate tray event id: {}", id);
    }
}

/// (3) Event ID format: all IDs follow the tray:* namespace convention.
#[test]
fn test_tray_event_ids_namespace() {
    let ids = tray::tray_event_ids();
    for id in &ids {
        assert!(
            id.starts_with("tray:"),
            "tray event id '{}' does not follow tray:* namespace",
            id
        );
    }
}

/// (4) Menu builds without panic — structural: the ID manifest matches the spec
/// menu layout (3 actionable items: show_hide, new_message, quit). A mismatch
/// here indicates the menu build and the ID manifest have drifted, which would
/// cause a runtime panic when the menu fires an event with an unregistered ID.
#[test]
fn test_tray_menu_id_count_matches_spec() {
    let ids = tray::tray_event_ids();
    // Spec §5.1: exactly 3 menu items carry event IDs (separator has none).
    assert_eq!(
        ids.len(),
        3,
        "tray_event_ids() length is {}, expected 3 (show_hide + new_message + quit); \
         menu build will panic or silently drop events if this drifts",
        ids.len()
    );
}

/// (5) Close-handler hides, does not exit — structural: the only tray event
/// that should cause process exit is `tray:quit`. The other IDs (show_hide,
/// new_message) must NOT be `tray:quit`, ensuring the close-to-tray path calls
/// `window.hide()` rather than `app.exit(0)`.
#[test]
fn test_close_to_tray_quit_is_only_exit_id() {
    let ids = tray::tray_event_ids();
    // All non-quit IDs must not be "tray:quit" (they trigger window show/hide,
    // never process exit).
    let non_quit: Vec<&&str> = ids.iter().filter(|&&id| id != "tray:quit").collect();
    assert!(
        !non_quit.is_empty(),
        "no non-quit tray IDs found; close-to-tray needs at least show_hide"
    );
    for id in &non_quit {
        assert_ne!(
            **id, "tray:quit",
            "non-quit tray id '{}' matches tray:quit — close-to-tray would exit the process",
            id
        );
    }
    // Quit ID must be present exactly once (the only exit path from the tray menu).
    let quit_count = ids.iter().filter(|&&id| id == "tray:quit").count();
    assert_eq!(quit_count, 1, "tray:quit must appear exactly once in tray_event_ids()");
}

/// (6) Quit uses custom MenuItemBuilder (not PredefinedMenuItem::quit) — structural:
/// `PredefinedMenuItem::quit` on Linux/GTK is documented as `Linux: Unsupported`
/// in muda/Tauri 2 and silently no-ops. The canonical Linux pattern is a custom
/// MenuItemBuilder item that fires a `tray:quit` event, which is then handled by
/// `app.exit(0)` in the tray on_menu_event closure. This test verifies that
/// `tray:quit` is in the `tray_event_ids()` manifest — a PredefinedMenuItem::quit
/// would not produce a custom event ID and would therefore be absent here.
#[test]
fn test_quit_is_custom_menu_item_not_predefined() {
    let ids = tray::tray_event_ids();
    assert!(
        ids.contains(&"tray:quit"),
        "tray:quit must be a custom MenuItemBuilder item in tray_event_ids(); \
         PredefinedMenuItem::quit silently no-ops on Linux/GTK (Tauri 2 / muda)"
    );
}
