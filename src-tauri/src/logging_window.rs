//! Logging-window management — mirrors help_window.rs (spec §8.1).
//!
//! Single-instance Tauri webview at `/logging` (label "logging"); geometry
//! persisted by tauri-plugin-window-state. Re-invoking focuses the existing
//! window. Only the main window may invoke `logging_window_open`.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const MAIN_WINDOW_LABEL: &str = "main";
const LOGGING_WINDOW_LABEL: &str = "logging";

pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

#[tauri::command]
pub fn logging_window_open(app: AppHandle, caller: WebviewWindow) -> Result<(), String> {
    if !caller_is_authorized(caller.label()) {
        return Err(format!(
            "logging_window_open may only be invoked from the main window (caller: {})",
            caller.label()
        ));
    }

    // Idempotent: focus an already-open logging window.
    if let Some(existing) = app.get_webview_window(LOGGING_WINDOW_LABEL) {
        existing.show().map_err(|e| format!("show failed: {e}"))?;
        existing
            .set_focus()
            .map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(());
    }

    let build_result = WebviewWindowBuilder::new(
        &app,
        LOGGING_WINDOW_LABEL,
        WebviewUrl::App("/logging".into()),
    )
    .title("Tuxlink Logging")
    .inner_size(820.0, 720.0)
    .min_inner_size(600.0, 480.0)
    .resizable(true)
    // Custom in-app titlebar — matches help_window.rs convention; spec §8.1
    // deferred custom chrome to v1.1 but both help + logging windows use the
    // same dark Tuxlink chrome as the main window.
    .decorations(false)
    .build();

    match build_result {
        Ok(_) => Ok(()),
        // Race-guard: a concurrent call may have raced past get_webview_window
        // above and hit AlreadyExists from build(). Mirror compose_window pattern.
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            if let Some(existing) = app.get_webview_window(LOGGING_WINDOW_LABEL) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
            Ok(())
        }
        Err(e) => Err(format!("logging window build failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_is_authorized() {
        assert!(caller_is_authorized(MAIN_WINDOW_LABEL));
    }

    #[test]
    fn other_windows_are_unauthorized() {
        assert!(!caller_is_authorized("compose-draft-1"));
        assert!(!caller_is_authorized("help"));
        assert!(!caller_is_authorized("logging")); // window cannot invoke itself
        assert!(!caller_is_authorized(""));
    }
}
