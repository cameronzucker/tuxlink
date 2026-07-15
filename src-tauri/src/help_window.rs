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
//!
//! **tuxlink-dmwte Task 3:** the get-or-focus + builder + race-guard body
//! previously inlined here now lives in the shared
//! `crate::secondary_window::open_secondary_window` helper; this file only
//! supplies its own label/route/size/decoration constants and the
//! authorization check.

use tauri::{AppHandle, WebviewWindow};

use crate::secondary_window::{open_secondary_window, ClosePolicy, SecondaryWindowSpec};

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

    let spec = SecondaryWindowSpec {
        label: HELP_WINDOW_LABEL.to_string(),
        route: "/help".to_string(),
        title: "Tuxlink Documentation".to_string(),
        inner_size: (1100.0, 700.0),
        min_inner_size: (640.0, 480.0),
        // tuxlink-ew3k: custom in-app titlebar (HelpTitleBar mounted by
        // HelpView). Spec §3.2 had deferred custom chrome to v1.1 to avoid
        // duplicating drag-region wiring; the duplication turned out minimal
        // and OS-native GTK gray looked jarring next to the main client's
        // dark Tuxlink chrome.
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
        assert!(caller_is_authorized("main"));
    }

    #[test]
    fn other_windows_are_not_authorized() {
        assert!(!caller_is_authorized("help"));
        assert!(!caller_is_authorized("compose-draft-foo"));
        assert!(!caller_is_authorized(""));
    }
}
