//! MCP server for the agent caller (phase 3.1 transport spine).
//!
//! This module exposes the Tuxlink MCP endpoint. Phase 3.1 ships exactly ONE
//! inert tool — [`router::TuxlinkMcp::server_info`] — which reports the app
//! name/version plus the live [`EgressGuard`] arm/taint state, proving the
//! spine reaches real security state. No real capability is wired here; later
//! phases add tools, redaction, taint-setting, and the egress gate.
//!
//! Design seam: all of `server_info`'s logic lives in the pure, transport-free
//! [`server_info_view`] free function so it is unit-testable WITHOUT the rmcp
//! transport. The `#[tool]` method in [`router`] is a thin wrapper over it,
//! mirroring the project's core-fn + thin-adapter pattern.

use std::sync::Arc;

use serde::Serialize;

use crate::ui_core::security::EgressGuard;

pub mod router;

/// The live handles the MCP router needs. Phase 3.1's only tool (`server_info`)
/// reads the [`EgressGuard`]; later phases (3.2+) extend this bundle with the
/// backend, session-log, modem, and position handles as tools are added.
#[derive(Clone)]
pub struct McpState {
    /// The armed-grant + taint authority, shared with the Tauri-managed
    /// `Arc<EgressGuard>` (lib.rs `.manage()`).
    pub guard: Arc<EgressGuard>,
}

/// Serializable shape returned by the `server_info` tool.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ServerInfoDto {
    /// Package name (`CARGO_PKG_NAME`).
    pub name: String,
    /// Package version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// True when send authority is currently armed (grant not expired).
    pub armed: bool,
    /// True when the session is tainted by untrusted content.
    pub tainted: bool,
}

/// Pure view of `server_info`: reads the live guard state and the compile-time
/// package identity. Transport-free so it can be unit-tested directly. `armed`
/// is `armed_remaining() > 0` (a live, un-expired grant); `tainted` mirrors the
/// guard's taint flag.
pub fn server_info_view(state: &McpState) -> ServerInfoDto {
    ServerInfoDto {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        armed: state.guard.armed_remaining() > 0,
        tainted: state.guard.is_tainted(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic clock so an armed grant has a known, un-expired deadline.
    fn fixed_1000() -> u64 {
        1000
    }

    fn state_with(guard: EgressGuard) -> McpState {
        McpState {
            guard: Arc::new(guard),
        }
    }

    #[test]
    fn view_reports_package_identity() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        let dto = server_info_view(&state);
        assert_eq!(dto.name, env!("CARGO_PKG_NAME"));
        assert_eq!(dto.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn fresh_guard_is_not_armed_and_not_tainted() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        let dto = server_info_view(&state);
        assert!(!dto.armed, "a fresh guard must report armed=false");
        assert!(!dto.tainted, "a fresh guard must report tainted=false");
    }

    #[test]
    fn arming_makes_view_report_armed() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(30); // deadline 1030, now 1000 -> 30s remaining
        let dto = server_info_view(&state);
        assert!(dto.armed, "after arm(30) the view must report armed=true");
        assert!(!dto.tainted);
    }

    #[test]
    fn expired_grant_is_not_armed() {
        // arm(0): deadline == now == 1000 -> armed_remaining() == 0 -> not armed.
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(0);
        let dto = server_info_view(&state);
        assert!(!dto.armed, "an expired/zero grant must report armed=false");
    }

    #[test]
    fn tainting_makes_view_report_tainted() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.taint();
        let dto = server_info_view(&state);
        assert!(dto.tainted, "after taint() the view must report tainted=true");
    }

    #[test]
    fn armed_and_tainted_are_independent() {
        // Taint must not clear the arm grant, and vice versa: both can be true.
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(30);
        state.guard.taint();
        let dto = server_info_view(&state);
        assert!(dto.armed);
        assert!(dto.tainted);
    }
}
