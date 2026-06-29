//! Elmer — Tuxlink's on-device agent pane.
//!
//! This module is a re-export hub.  Tasks 4-8 progressively append submodules:
//! - Task 4 (`executor`): `InProcessMcpInvoker` — the canonical in-process tool
//!   executor.
//! - Task 5 (`OutboxReadPort`): non-tainting staged-outbox read — see
//!   [`crate::mcp_ports::MonolithOutboxReadPort`].
//! - Task 6 (`approval`): scoped one-shot outbox approval + digest-gated flush.
//! - Task 7 (`provider` / `session`): `ElmerSession` + Tauri commands.
//! - Task 8 (`pane`): full Tauri command surface.

pub mod approval;
pub mod executor;
