//! Help-window management — opens a single separate Tauri webview window for
//! the user-guide documentation (tuxlink-0gsy / spec §3).
//!
//! Mirrors compose_window.rs in shape:
//!   - WebviewWindowBuilder::new(..., WebviewUrl::App("/help".into()))
//!   - per-label geometry persisted by tauri-plugin-window-state
//!   - registered in lib.rs's invoke_handler list
//!
//! **Single instance.** Unlike compose (which permits many windows for many
//! drafts), there is exactly one help window. Re-invoking `help_window_open`
//! when the window already exists focuses it.
//!
//! **Main-window guard.** As with compose, only the main window is permitted
//! to invoke `help_window_open`. Defense-in-depth against a misbehaving help
//! frontend trying to spawn a second help window.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const MAIN_WINDOW_LABEL: &str = "main";
const HELP_WINDOW_LABEL: &str = "help";

pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

#[tauri::command]
pub fn help_window_open(app: AppHandle, caller: WebviewWindow) -> Result<(), String> {
    if !caller_is_authorized(caller.label()) {
        return Err(format!(
            "help_window_open may only be invoked from the main window (caller: {})",
            caller.label()
        ));
    }

    // Idempotent: focus an already-open help window.
    if let Some(existing) = app.get_webview_window(HELP_WINDOW_LABEL) {
        existing.show().map_err(|e| format!("show failed: {e}"))?;
        existing.set_focus().map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(());
    }

    let build_result = WebviewWindowBuilder::new(
        &app,
        HELP_WINDOW_LABEL,
        WebviewUrl::App("/help".into()),
    )
    .title("Tuxlink Documentation")
    .inner_size(1100.0, 700.0)
    .min_inner_size(640.0, 480.0)
    .resizable(true)
    // tuxlink-ew3k: custom in-app titlebar (HelpTitleBar mounted by
    // HelpView). Spec §3.2 had deferred custom chrome to v1.1 to avoid
    // duplicating drag-region wiring; the duplication turned out minimal
    // and OS-native GTK gray looked jarring next to the main client's
    // dark Tuxlink chrome.
    .decorations(false)
    .build();

    match build_result {
        Ok(_) => Ok(()),
        // Match the compose race-guard pattern: a concurrent call may race past
        // the get_webview_window check above and hit AlreadyExists from build().
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            if let Some(existing) = app.get_webview_window(HELP_WINDOW_LABEL) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
            Ok(())
        }
        Err(e) => Err(format!("help window build failed: {e}")),
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
        assert!(!caller_is_authorized("help"));
        assert!(!caller_is_authorized("compose-draft-foo"));
        assert!(!caller_is_authorized(""));
    }
}
