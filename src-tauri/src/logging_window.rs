//! Logging-window management — mirrors help_window.rs (spec §8.1).
//!
//! Single-instance Tauri webview at `/logging` (label "logging"); geometry
//! persisted by tauri-plugin-window-state. Re-invoking focuses the existing
//! window. Only the main window may invoke `logging_window_open`.
//!
//! **tuxlink-dmwte Task 3:** the get-or-focus + builder + race-guard body
//! previously inlined here now lives in the shared
//! `crate::secondary_window::open_secondary_window` helper; this file only
//! supplies its own label/route/size/decoration constants and the
//! authorization check.

use tauri::{AppHandle, WebviewWindow};

use crate::secondary_window::{open_secondary_window, ClosePolicy, SecondaryWindowSpec};

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

    let spec = SecondaryWindowSpec {
        label: LOGGING_WINDOW_LABEL.to_string(),
        route: "/logging".to_string(),
        title: "Tuxlink Logging".to_string(),
        inner_size: (820.0, 720.0),
        min_inner_size: (600.0, 480.0),
        // Custom in-app titlebar — matches help_window.rs convention; spec
        // §8.1 deferred custom chrome to v1.1 but both help + logging windows
        // use the same dark Tuxlink chrome as the main window.
        decorations: false,
        close_policy: ClosePolicy::CloseSelf,
    };
    open_secondary_window(&app, caller.label(), &spec)
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
