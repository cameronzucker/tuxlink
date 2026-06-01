//! Pin the HTML Forms child-webview capability ACL.
//!
//! The `forms-webview.json` capability (P1 Task 7) intentionally grants the
//! minimum surface: `core:default` and nothing else. The child webview
//! talks to tuxlink only via the loopback `forms::http_server`; no Tauri
//! IPC, no fs, no shell, no window control.
//!
//! This test guards against accidental widening — a future agent adding
//! a fs / event / shell permission to the forms-webview capability would
//! break the design's §10 threat-model assumption (custom-form HTML in
//! the webview cannot reach tuxlink internals).
//!
//! Refs: spec §5.6, §10; plan §Task 7.

use std::path::PathBuf;

#[test]
fn forms_webview_capability_has_only_core_default() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities/forms-webview.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let perms = json["permissions"]
        .as_array()
        .expect("permissions must be an array");
    let perms: Vec<&str> = perms.iter().filter_map(|v| v.as_str()).collect();

    // Hardcoded minimum allowlist at P1: ONLY core:default.
    // If you're widening this, you're changing the security model — file
    // a bd issue and route through Codex adrev before flipping the assert.
    assert_eq!(
        perms,
        vec!["core:default"],
        "forms-webview capability widened beyond the design's §10 threat model"
    );
}

#[test]
fn forms_webview_capability_scoped_to_compose_form_label_prefix() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities/forms-webview.json");
    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();

    let webviews = json["webviews"]
        .as_array()
        .expect("webviews scope must be an array");
    let webviews: Vec<&str> = webviews.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(
        webviews,
        vec!["compose-form-*"],
        "forms-webview must scope to the compose-form-* label prefix only"
    );
}
