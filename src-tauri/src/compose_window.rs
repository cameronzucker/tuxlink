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
//! **Codex F7 guard:** compose windows do NOT wire a `menu:file:new` listener.
//! `menu.rs:123` emits that event via `app.emit` which broadcasts to EVERY
//! webview (including compose windows). If a compose window listened for that
//! event it would spawn nested compose windows. The listener lives ONLY in
//! `App.tsx`'s main-window code path, gated to the main window (integration
//! commit §4.3).
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
/// **Main-window-only guard (Codex integration round, defense-in-depth for
/// F7).** Now that compose windows carry a Tauri capability (`compose.json`),
/// they have an IPC bridge and could in principle invoke `compose_window_open`
/// themselves — recursively spawning nested compose windows. The frontend
/// already guards this (`App.tsx`: a compose route never listens for
/// `menu:file:new`), but a malicious/buggy frontend could still issue the
/// invoke directly. This Rust-side check rejects any call NOT originating from
/// the `main` window. `caller` is the invoking [`WebviewWindow`], injected by
/// Tauri's command runtime.
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
    let build_result = WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::App(url.into()),
    )
    .title("New Message — Tuxlink")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 360.0)
    .resizable(true)
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
