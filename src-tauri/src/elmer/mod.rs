//! Elmer — Tuxlink's on-device agent pane.
//!
//! This module is a re-export hub.  Tasks 4-8 progressively append submodules:
//! - Task 4 (`executor`): `InProcessMcpInvoker` — the canonical in-process tool
//!   executor.
//! - Task 5 (`provider`): the Anthropic API-backed `Provider` impl.
//! - Task 6 (`session`): the `ElmerSession` lifecycle wrapper (arm/rearm/cancel).
//! - Task 7 (`pane`): the Tauri commands wiring `ElmerSession` to the UI.
//! - Task 8 (`approval`): the operator-approval flush for gated egress tools.

pub mod executor;
