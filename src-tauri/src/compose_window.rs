//! Compose-window management — opens a separate Tauri webview window per
//! draft (AMD-6 / spec §5.4).
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.4
//! bd issue: tuxlink-dm8 (Task 14 — compose window)
//!
//! **Separate window, NOT Radix Dialog** — this is AMD-6 locked decision #2.
//! The compose experience is in its own webview, labeled `compose-<draftId>`,
//! so it:
//!   - Survives main-window hide-to-tray (spec §5.4, Task 8 integration).
//!   - Allows multiple concurrent compose windows.
//!   - Does NOT embed in the AppShell grid.
//!
//! **F7 guard:** compose windows do NOT open further compose windows.
//! The native menu and its `app.emit` broadcast were removed in ng3
//! (`lib.rs` + `menu.rs` deleted); the `caller_is_authorized` guard below now
//! stands as pure defense-in-depth — a misbehaving frontend that somehow
//! invokes `compose_window_open` from a compose context is rejected at the
//! Rust layer regardless of frontend guards.
//!
//! **Window geometry:** `tauri-plugin-window-state` persists per-window size
//! and position keyed by the window label. Each compose window gets a unique
//! label `compose-<draftId>`, so per-draft geometry is remembered across
//! restores. The plugin is registered in `lib.rs`'s `run()` builder (the
//! integration commit, §4.3) — this file only builds the `WebviewWindowBuilder`.
//!
//! **Registration:** `compose_window_open` is a Tauri command appended to
//! `ui_commands.rs`'s append-only command list. The `invoke_handler`
//! registration lands in the orchestrator integration commit (spec §4.3).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The window label authorized to open compose windows. Only the main window
/// may spawn compose windows (Codex integration round, defense-in-depth for F7).
const MAIN_WINDOW_LABEL: &str = "main";

/// Pure guard: is `caller_label` authorized to invoke `compose_window_open`?
///
/// Only the `main` window may open compose windows. Extracted as a pure
/// function so it is unit-testable without a Tauri runtime (the command itself
/// needs a live `WebviewWindow`, which requires the full runtime — verified at
/// M2 operator smoke per testing-pitfalls.md §9). Mirrors the `menu_event_ids`
/// / `tray_event_ids` testable-surface convention.
pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

/// Upper bound on a `draft_id`'s length. Generated ids (`newDraftId` in
/// App.tsx) are `draft-<iso-ts>-<base36>` (~37 chars); 128 is a generous bound.
const MAX_DRAFT_ID_LEN: usize = 128;

/// Pure guard: is `draft_id` safe to interpolate into the window label
/// (`compose-<id>`) and route (`/compose/<id>`)?
///
/// Generated ids are strictly `[A-Za-z0-9-]`. An IPC caller passing `/`, `?`,
/// `#`, `%`, or other characters could otherwise build a label/URL that mounts
/// the wrong draft — or no draft (tuxlink-g3d, Codex integration-round P2).
/// Reject anything outside the generated charset / length bound rather than
/// silently building a malformed label. Pure → unit-testable without a runtime.
pub fn validate_draft_id(draft_id: &str) -> Result<(), String> {
    if draft_id.is_empty() {
        return Err("draft_id must not be empty".into());
    }
    if draft_id.len() > MAX_DRAFT_ID_LEN {
        return Err(format!("draft_id too long (max {MAX_DRAFT_ID_LEN})"));
    }
    if !draft_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("draft_id must be ASCII alphanumeric or '-'".into());
    }
    Ok(())
}

/// Open a compose window for the given draft id.
///
/// The window is labeled `compose-<draftId>` and loads
/// `/compose/<draftId>` inside the app's devUrl/frontendDist.
/// `tauri-plugin-window-state` persists geometry per label so each
/// draft's window remembers its last position.
///
/// Multiple compose windows are allowed (Winlink emcomm workflow
/// commonly has several drafts open simultaneously).
///
/// The command is idempotent: if a window with the same label already
/// exists, `WebviewWindowBuilder::build` returns an `AlreadyExists`
/// error — we swallow it and the existing window is revealed via a
/// `window.set_focus()` call. If the window is visible but not focused,
/// focus is restored; if it is hidden (main-window-hide-to-tray flow),
/// it is shown and focused.
///
/// **Main-window-only guard (defense-in-depth, ng3).** Compose windows carry a
/// Tauri capability (`compose.json`) and therefore have an IPC bridge; a
/// malicious/buggy frontend could invoke `compose_window_open` directly to
/// spawn nested compose windows. The native-menu broadcast that previously
/// triggered this was removed in ng3 (`menu.rs` deleted), but this Rust-side
/// check remains as a pure structural guard. `caller` is the invoking
/// [`WebviewWindow`], injected by Tauri's command runtime.
#[tauri::command]
pub fn compose_window_open(
    app: AppHandle,
    caller: WebviewWindow,
    draft_id: String,
) -> Result<(), String> {
    if !caller_is_authorized(caller.label()) {
        return Err(format!(
            "compose_window_open may only be invoked from the main window (caller: {})",
            caller.label()
        ));
    }

    // tuxlink-g3d: reject a draft_id that would build a malformed label/route.
    validate_draft_id(&draft_id)?;

    let label = format!("compose-{}", draft_id);
    let url = format!("/compose/{}", draft_id);

    // Attempt to focus an already-open window first (idempotent open).
    if let Some(existing) = app.get_webview_window(&label) {
        existing
            .show()
            .map_err(|e| format!("show failed: {e}"))?;
        existing
            .set_focus()
            .map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(());
    }

    // Build a new compose window. `tauri-plugin-window-state` hooks into
    // the WebviewWindow lifecycle via `.on_window_event` registered in
    // `lib.rs`'s `run()` builder (integration commit). The builder does
    // not need to call the plugin explicitly — the plugin's `Builder` hook
    // restores + saves window state automatically once registered.
    //
    // Race guard (Codex P2): a concurrent call that races past the
    // `get_webview_window` check above can hit `build()` and receive an
    // `AlreadyExists` error. Treat that as success — the window exists,
    // attempt to focus it before returning.
    // Centered placement. A Gmail-style bottom-right DOCK was attempted (ng3
    // smoke #8) but the Pi's labwc/Wayland compositor owns window placement —
    // a client can neither set nor query a separate window's absolute position,
    // so `.position()` is ignored and the window lands center regardless.
    // Operator accepted centered for v0.0.1; the true in-window docked panel
    // (which CAN position itself, and is also faster) is filed for revisit.
    let build_result = WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::App(url.into()),
    )
    .title("New Message — Tuxlink")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 360.0)
    .resizable(true)
    .decorations(false)
    .center()
    .build();

    match build_result {
        Ok(_) => {}
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            // Concurrent open race: another call already created the window.
            // Focus it and return success (same as the sequential-dupe path
            // handled by `get_webview_window` at the top of this function).
            // `WebviewWindowBuilder` creates both a window and a webview, so
            // either variant can be emitted depending on which layer fires
            // first (Codex P2 fix).
            if let Some(existing) = app.get_webview_window(&label) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
        }
        Err(e) => return Err(format!("compose window build failed: {e}")),
    }

    Ok(())
}

/// Close ONLY the calling window (tuxlink-h2y). The compose frontend invokes
/// this for every close path (the in-app Close button + the native titlebar X
/// via `onCloseRequested`) instead of the JS `window.close()` / `window.destroy()`
/// APIs. Those JS APIs take a caller-supplied label resolved server-side, so
/// they are window-CLASS — an XSS'd compose window could close/destroy the main
/// window. Here `window` is the INVOKING window, injected by Tauri, so the
/// surface is scoped to self by construction.
///
/// `destroy()` (force-close) is used rather than `close()` so it does NOT
/// re-fire `CloseRequested` (which the frontend intercepts) — avoiding a close
/// loop. Routing every close path through this command lets compose.json drop
/// the window-class `core:window:allow-close` + `allow-destroy` grants (Codex
/// integration-round P3). Self-close is always safe, so — unlike
/// `compose_window_open` — no caller-authorization guard is needed.
#[tauri::command]
pub fn compose_close_self(window: WebviewWindow) -> Result<(), String> {
    window
        .destroy()
        .map_err(|e| format!("compose_close_self: destroy failed: {e}"))
}

#[cfg(test)]
mod tests {
    // Window tests are structural/doc-only: `tauri::test` helpers require a
    // full Tauri runtime which is not available in unit-test mode. The M2
    // browser smoke (spec §6, testing-pitfalls.md §9) is the runtime gate
    // for compose-window open/focus/multi-window behavior.
    //
    // What IS testable here: the label format contract.

    #[test]
    fn compose_label_format() {
        let draft_id = "draft-2026-05-19-001";
        let label = format!("compose-{}", draft_id);
        assert_eq!(label, "compose-draft-2026-05-19-001");
        // Label must be non-empty and free of path separators (Tauri rejects
        // labels that look like filesystem paths).
        assert!(!label.is_empty());
        assert!(!label.contains('/'));
        assert!(!label.contains('\\'));
    }

    #[test]
    fn compose_url_format() {
        let draft_id = "draft-abc";
        let url = format!("/compose/{}", draft_id);
        assert_eq!(url, "/compose/draft-abc");
    }

    // ── validate_draft_id (tuxlink-g3d) — pure charset/length guard ──────────

    #[test]
    fn validate_draft_id_accepts_generated_ids() {
        // Shape of newDraftId(): draft-<iso-ts-with-:.replaced>-<base36>
        assert!(super::validate_draft_id("draft-2026-05-20T19-30-00-000Z-a1b2c3").is_ok());
        assert!(super::validate_draft_id("draft-abc").is_ok());
        assert!(super::validate_draft_id("ABC123").is_ok());
    }

    #[test]
    fn validate_draft_id_rejects_empty() {
        assert!(super::validate_draft_id("").is_err());
    }

    #[test]
    fn validate_draft_id_rejects_path_and_url_metachars() {
        // '/' and '\\' (label/path injection), '?' '#' (route/query), '%' (encoding)
        for bad in ["a/b", "../etc", "a\\b", "a?b", "a#b", "a%2f", "a b", "a.b", "a:b"] {
            assert!(
                super::validate_draft_id(bad).is_err(),
                "expected reject for {bad:?}"
            );
        }
    }

    #[test]
    fn validate_draft_id_rejects_too_long() {
        let long = "a".repeat(super::MAX_DRAFT_ID_LEN + 1);
        assert!(super::validate_draft_id(&long).is_err());
        // boundary: exactly MAX is allowed
        assert!(super::validate_draft_id(&"a".repeat(super::MAX_DRAFT_ID_LEN)).is_ok());
    }

    // Codex integration round: `compose_window_open` is gated to the main
    // window so an IPC-enabled compose window cannot recursively spawn nested
    // compose windows (defense-in-depth for F7). The command's runtime path
    // needs a live `WebviewWindow`; the authorization decision is factored into
    // the pure `caller_is_authorized` helper so it is unit-testable here.
    #[test]
    fn caller_is_authorized_only_for_main() {
        assert!(super::caller_is_authorized("main"));
    }

    #[test]
    fn caller_is_authorized_rejects_compose_windows() {
        // A compose window must NOT be able to open further compose windows.
        assert!(!super::caller_is_authorized("compose-draft-2026-05-19-001"));
        assert!(!super::caller_is_authorized("compose-draft-abc"));
    }

    #[test]
    fn caller_is_authorized_rejects_other_labels() {
        assert!(!super::caller_is_authorized(""));
        assert!(!super::caller_is_authorized("wizard"));
        assert!(!super::caller_is_authorized("MAIN")); // case-sensitive
        assert!(!super::caller_is_authorized("main "));
    }
}
