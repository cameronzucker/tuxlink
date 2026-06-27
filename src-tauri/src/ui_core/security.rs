//! Egress authorization — thin re-export of the standalone [`tuxlink_security`]
//! crate.
//!
//! `EgressGuard`, `EgressAuthority`, `EgressDenied`, and the pure `decide()`
//! heart were extracted into the `tuxlink-security` crate (MCP phase 3.1) so the
//! Tauri monolith AND the standalone tier-2 testserver can depend on the SAME
//! real authority without pulling in the Tauri app. This module preserves the
//! historical `crate::ui_core::security::*` import paths used across the
//! monolith (e.g. `security_commands.rs`, the MCP router) via a glob re-export.

pub use tuxlink_security::*;
