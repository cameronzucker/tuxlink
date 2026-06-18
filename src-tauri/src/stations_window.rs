//! Station-Data-window management — opens a single separate Tauri webview window
//! for the popped-out source-reactive environmental panel (tuxlink-2phz).
//!
//! Mirrors help_window.rs in shape:
//!   - WebviewWindowBuilder::new(..., WebviewUrl::App("/stations".into()))
//!   - per-label geometry persisted by tauri-plugin-window-state
//!   - registered in lib.rs's invoke_handler list
//!
//! **Single instance.** There is exactly one Station Data window; re-invoking
//! `stations_window_open` when it already exists focuses it (matches help).
//!
//! **Main-window guard.** As with compose/help, only the main window may invoke
//! `stations_window_open` — defense-in-depth against a misbehaving frontend
//! spawning a second one.
//!
//! **Native decorations.** Unlike compose/help (custom dark chrome), this window
//! keeps the OS-native frame. The panel content is the whole window; a bespoke
//! drag-region titlebar is a deferrable polish item, not a functional gap.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const MAIN_WINDOW_LABEL: &str = "main";
const STATIONS_WINDOW_LABEL: &str = "stations";

pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

#[tauri::command]
pub fn stations_window_open(app: AppHandle, caller: WebviewWindow) -> Result<(), String> {
    if !caller_is_authorized(caller.label()) {
        return Err(format!(
            "stations_window_open may only be invoked from the main window (caller: {})",
            caller.label()
        ));
    }

    // Idempotent: focus an already-open Station Data window.
    if let Some(existing) = app.get_webview_window(STATIONS_WINDOW_LABEL) {
        existing.show().map_err(|e| format!("show failed: {e}"))?;
        existing
            .set_focus()
            .map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(());
    }

    let build_result = WebviewWindowBuilder::new(
        &app,
        STATIONS_WINDOW_LABEL,
        WebviewUrl::App("/stations".into()),
    )
    .title("Tuxlink Station Data")
    .inner_size(760.0, 720.0)
    .min_inner_size(420.0, 360.0)
    .resizable(true)
    .build();

    match build_result {
        Ok(_) => Ok(()),
        // Match the compose/help race-guard: a concurrent call may race past the
        // get_webview_window check above and hit AlreadyExists from build().
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            if let Some(existing) = app.get_webview_window(STATIONS_WINDOW_LABEL) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
            Ok(())
        }
        Err(e) => Err(format!("stations window build failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_is_authorized() {
        assert!(caller_is_authorized("main"));
    }

    #[test]
    fn other_windows_are_not_authorized() {
        assert!(!caller_is_authorized("stations"));
        assert!(!caller_is_authorized("compose-draft-foo"));
        assert!(!caller_is_authorized(""));
    }
}
