//! Adapter between this crate's PHY and subsystem #1's `hf-channel-sim`
//! crate.
//!
//! The j10k subsystem-#1 crate exists in-repo as a sibling top-level
//! package; this file documents the integration point but does not
//! wire the dependency yet because the workspace topology between the
//! standalone-release `hf-channel-sim` and the workspace-internal
//! `tuxmodem-phy` is owned by the post-merge integration PR. Once
//! both branches land on main, the follow-up swaps this scaffold for:
//!
//! ```ignore
//! use hf_channel_sim::{Channel, ChannelCondition};
//! ```
//!
//! and writes the per-mode round-trip tests under
//! `#[cfg(feature = "sim")]`. Until then, this file is the single
//! place a future maintainer needs to touch for the wire-up.

#![allow(dead_code)]

#[test]
fn sim_adapter_integration_point_marker() {
    // No assertion: the file's existence is the integration-point
    // signal. The marker test ensures the file is compiled by
    // `cargo test` so the placeholder doesn't bit-rot under feature
    // changes elsewhere in the crate.
}
