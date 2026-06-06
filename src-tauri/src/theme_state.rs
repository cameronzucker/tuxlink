//! Theme-state sharing across windows (tuxlink-0gsy / spec §8.2).
//!
//! The main window owns the operator's chosen color scheme (it persists it
//! to localStorage on its side). When the scheme changes, main calls
//! `theme_broadcast_scheme(scheme)` which (a) stores the value in this
//! Tauri-managed state and (b) emits a `color_scheme_changed` event so any
//! other windows (currently: help) re-apply.
//!
//! New (help) windows opened mid-session bootstrap with `theme_get_scheme()`
//! to read whatever the main window last broadcast — typically the value it
//! applied at startup.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// Singleton holding the last scheme broadcast by the main window.
/// `None` until the main window calls `theme_broadcast_scheme` at least once.
pub struct ThemeState(pub Mutex<Option<String>>);

impl Default for ThemeState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
pub fn theme_get_scheme(state: tauri::State<'_, ThemeState>) -> Option<String> {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn theme_broadcast_scheme(
    app: AppHandle,
    state: tauri::State<'_, ThemeState>,
    scheme: String,
) -> Result<(), String> {
    if scheme.is_empty() {
        return Err("scheme must not be empty".into());
    }
    *state.0.lock().unwrap() = Some(scheme.clone());
    tracing::debug!(
        target: "tuxlink::theme",
        scheme = %scheme,
        "theme scheme broadcast",
    );
    app.emit("color_scheme_changed", scheme).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_state_starts_empty() {
        let s = ThemeState::default();
        assert!(s.0.lock().unwrap().is_none());
    }

    #[test]
    fn theme_state_stores_scheme_after_direct_set() {
        // theme_broadcast_scheme needs a live AppHandle for the emit call;
        // its storage half is exercised here directly via the Mutex so we
        // verify the state container works without a runtime.
        let s = ThemeState::default();
        *s.0.lock().unwrap() = Some("night-red".into());
        assert_eq!(s.0.lock().unwrap().clone(), Some("night-red".into()));
    }
}
