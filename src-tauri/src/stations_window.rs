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
//! **Main-window guard.** As with compose/help, only the main window may
//! invoke `stations_window_open` — defense-in-depth against a misbehaving
//! frontend spawning a second one.
//!
//! **Native decorations.** Unlike compose/help (custom dark chrome), this
//! window keeps the OS-native frame. The panel content is the whole window; a
//! bespoke drag-region titlebar is a deferrable polish item, not a functional
//! gap.
//!
//! **tuxlink-dmwte Task 3:** the get-or-focus + builder + race-guard body that
//! used to live here directly is now the shared
//! `crate::secondary_window::open_secondary_window` helper (this file was its
//! source transplant); this file only supplies its own label/route/size/
//! decoration constants and the authorization check.

use tauri::{AppHandle, WebviewWindow};

use crate::secondary_window::{open_secondary_window, ClosePolicy, SecondaryWindowSpec};

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

    let spec = SecondaryWindowSpec {
        label: STATIONS_WINDOW_LABEL.to_string(),
        route: "/stations".to_string(),
        title: "Tuxlink Station Data".to_string(),
        inner_size: (760.0, 720.0),
        min_inner_size: (420.0, 360.0),
        // Native decorations (unlike compose/help/logging's custom chrome) —
        // see the module docstring's "Native decorations" note.
        decorations: true,
        centered: false,
        close_policy: ClosePolicy::CloseSelf,
    };
    // Station Data ignores the spawn outcome (get-or-focus is all it needs).
    open_secondary_window(&app, caller.label(), &spec).map(|_| ())
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
