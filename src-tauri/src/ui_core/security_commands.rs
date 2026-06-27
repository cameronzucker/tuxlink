//! Thin Tauri command adapters over [`crate::ui_core::security::EgressGuard`].
//! The operator arms/disarms send-authority delegation here; the GUI (Plan 5)
//! renders the status. Enforcement at egress operations is Plan 3.

use std::sync::Arc;
use serde::Serialize;
use crate::ui_core::security::EgressGuard;
use crate::session_log::SessionLogState;
use crate::winlink_backend::{LogLevel, LogSource};

/// Serializable snapshot of the egress-grant state for the GUI.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EgressStatusDto {
    pub armed: bool,
    pub armed_remaining_secs: u64,
    pub tainted: bool,
}

impl EgressStatusDto {
    fn from_guard(g: &EgressGuard) -> Self {
        let remaining = g.armed_remaining();
        EgressStatusDto {
            armed: remaining > 0,
            armed_remaining_secs: remaining,
            tainted: g.is_tainted(),
        }
    }
}

#[tauri::command]
pub fn egress_arm(
    duration_secs: u64,
    state: tauri::State<'_, Arc<EgressGuard>>,
    log: tauri::State<'_, Arc<SessionLogState>>,
) -> Result<EgressStatusDto, String> {
    if duration_secs == 0 {
        return Err("arm duration must be greater than zero".to_string());
    }
    state.arm(duration_secs);
    log.append_operator_line(
        LogLevel::Info,
        LogSource::Backend,
        format!("[egress] send authority armed for {duration_secs}s"),
    );
    Ok(EgressStatusDto::from_guard(&state))
}

#[tauri::command]
pub fn egress_disarm(
    state: tauri::State<'_, Arc<EgressGuard>>,
    log: tauri::State<'_, Arc<SessionLogState>>,
) -> Result<EgressStatusDto, String> {
    state.disarm();
    log.append_operator_line(
        LogLevel::Info,
        LogSource::Backend,
        "[egress] send authority disarmed",
    );
    Ok(EgressStatusDto::from_guard(&state))
}

#[tauri::command]
pub fn egress_status(state: tauri::State<'_, Arc<EgressGuard>>) -> EgressStatusDto {
    EgressStatusDto::from_guard(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The DTO projection is the testable unit (commands are thin State wrappers).
    #[test]
    fn status_dto_reflects_armed_then_disarmed() {
        let g = EgressGuard::new();
        let before = EgressStatusDto::from_guard(&g);
        assert!(!before.armed);
        assert_eq!(before.armed_remaining_secs, 0);

        g.arm(30);
        let armed = EgressStatusDto::from_guard(&g);
        assert!(armed.armed);
        assert!(armed.armed_remaining_secs > 0 && armed.armed_remaining_secs <= 30);

        g.disarm();
        assert!(!EgressStatusDto::from_guard(&g).armed);
    }

    #[test]
    fn status_dto_reflects_taint() {
        let g = EgressGuard::new();
        assert!(!EgressStatusDto::from_guard(&g).tainted);
        g.taint();
        assert!(EgressStatusDto::from_guard(&g).tainted);
    }
}
