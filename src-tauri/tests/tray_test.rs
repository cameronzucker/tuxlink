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
