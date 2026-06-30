//! MCP-boundary regression tests for the Elmer model-config feature (Task F1).
//!
//! ## Purpose
//!
//! These are **structural trip-wires**, not behaviour assertions.  They fail the
//! moment the wrong thing appears — or disappears — from the router's tool surface
//! and its source code.
//!
//! ### Trip-wire 1: config commands absent from the model tool list
//!
//! [`config_commands_absent_from_model_tool_list`] spins the same in-process
//! invoker that [`executor::InProcessMcpInvoker::connect`] builds and asserts
//! that none of the three Tauri-only commands appear in `invoker.tools()`.
//! Because those commands are registered ONLY in `lib.rs`'s `invoke_handler`,
//! they CANNOT be present — but if someone were to add one to the MCP router
//! in future this test would catch it immediately.
//!
//! ### Trip-wire 2: egress denylist still intact after the feature
//!
//! [`egress_tools_still_withheld_after_feature`] re-asserts the existing
//! [`executor::WITHHELD_EGRESS_TOOLS`] invariant post-feature-merge.  Any merge
//! that widened the MCP tool surface (e.g. by accidentally un-withholding an
//! egress tool) would be detected before CI goes green.
//!
//! ### Trip-wire 3: config command names absent from router source
//!
//! [`config_command_names_not_in_router_source`] uses `include_str!` to pull the
//! router source files into the compiled test binary and asserts that the three
//! config command names do not appear there.  Any addition of a `#[tool]`
//! annotation or `register` call adjacent to one of those names in the router
//! would cause the literal string to appear in the source and fail this test.
//!
//! ## How each test would fail if a config command were router-registered
//!
//! - Trip-wire 1: `invoker.tools()` would now contain the name → assertion fails.
//! - Trip-wire 2: no direct effect, but serves as a post-merge regression guard
//!   for the egress surface that coexists with the config surface.
//! - Trip-wire 3: the `#[tool]` expansion or `register` call would emit the
//!   command name into executor.rs or router.rs → `include_str!` pulls it in →
//!   assertion fails.
//!
//! ## MSRV
//!
//! Code uses only stable APIs available on Rust 1.75 (project MSRV).
//! `std::collections::HashSet`, `tokio::test`, and `include_str!` are all
//! stable on 1.75.  No `inspect_err` or other post-1.75 additions are used.

use std::collections::HashSet;
use std::sync::Arc;

use tuxlink_mcp_core::{test_support, McpState};
use tuxlink_security::EgressGuard;

use crate::elmer::executor::{InProcessMcpInvoker, WITHHELD_EGRESS_TOOLS};

/// A seeded message id recognised by `MockMailbox` — required to construct a
/// valid `McpState` via `test_support::state_with_seeded_inbox`.
const SEEDED_ID: &str = "MSG001";

/// The three Elmer model-config Tauri commands.  These are registered ONLY in
/// `lib.rs`'s `invoke_handler`; they must NEVER appear in the MCP router.
const CONFIG_COMMAND_NAMES: &[&str] = &[
    "elmer_config_read",
    "elmer_config_set",
    "elmer_detect_models",
];

// ---------------------------------------------------------------------------
// Helper: build a minimal in-process invoker (mirrors executor.rs tests)
// ---------------------------------------------------------------------------

async fn connect_invoker() -> InProcessMcpInvoker {
    let guard = Arc::new(EgressGuard::new());
    let state: Arc<McpState> =
        test_support::state_with_seeded_inbox(guard, SEEDED_ID);
    InProcessMcpInvoker::connect(state)
        .await
        .expect("in-process invoker must connect")
}

// ---------------------------------------------------------------------------
// Trip-wire 1: config commands absent from the MCP tool list
// ---------------------------------------------------------------------------

/// **Trip-wire 1 (F1-T1).**  None of the three Elmer model-config Tauri
/// commands appear in the tool surface exposed by the in-process MCP invoker.
///
/// Because `elmer_config_read`, `elmer_config_set`, and `elmer_detect_models`
/// are registered only via `tauri::generate_handler!` in `lib.rs`, they cannot
/// be MCP tools — but this test will FAIL the moment someone registers one on
/// the router (e.g. adds `#[tool]` or wires it through `TuxlinkMcp`).
///
/// ### Inverse sanity-check
///
/// If `elmer_config_set` were added to the router, `invoker.tools()` would
/// contain `ToolSpec { name: "elmer_config_set", … }` and the `any` assertion
/// below would return `true`, failing the `assert!(!…)`.
#[tokio::test]
async fn config_commands_absent_from_model_tool_list() {
    let invoker = connect_invoker().await;
    let tool_names: HashSet<&str> = invoker.tools().iter().map(|t| t.name.as_str()).collect();

    for name in CONFIG_COMMAND_NAMES {
        assert!(
            !tool_names.contains(*name),
            "config command `{name}` must NOT appear in the MCP tool list — \
             it is a Tauri-only command; if this fails, it was accidentally \
             registered on the MCP router"
        );
    }
}

// ---------------------------------------------------------------------------
// Trip-wire 2: egress denylist still intact after the feature merge
// ---------------------------------------------------------------------------

/// **Trip-wire 2 (F1-T2).**  Every name in [`WITHHELD_EGRESS_TOOLS`] is still
/// absent from `invoker.tools()` after the elmer-model-config feature is
/// merged.
///
/// This re-asserts the pre-existing AC-3 P0-1 invariant at the post-merge
/// boundary, so a merge that accidentally widened the MCP egress surface is
/// caught before the PR goes green.
///
/// ### Inverse sanity-check
///
/// If `cms_connect` were removed from `WITHHELD_EGRESS_TOOLS`, the filtering
/// step in `InProcessMcpInvoker::connect` would no longer strip it, and
/// `tool_names.contains("cms_connect")` would be `true`, failing the assertion.
#[tokio::test]
async fn egress_tools_still_withheld_after_feature() {
    let invoker = connect_invoker().await;
    let tool_names: HashSet<&str> = invoker.tools().iter().map(|t| t.name.as_str()).collect();

    for name in WITHHELD_EGRESS_TOOLS {
        assert!(
            !tool_names.contains(*name),
            "egress tool `{name}` must remain withheld from the MCP tool list; \
             this may have been caused by a merge that removed the name from \
             WITHHELD_EGRESS_TOOLS or added it as a non-egress router tool"
        );
    }
}

// ---------------------------------------------------------------------------
// Trip-wire 3: config command names absent from router source (grep-gate)
// ---------------------------------------------------------------------------

/// Strip the `#[cfg(test)]` block and comment lines from a source file so the
/// grep-gate cannot trip on its own needle strings.
///
/// Mirrors the `production_src` helper in `commands.rs::security_gate_tests`.
fn production_src(s: &str) -> String {
    s.split("#[cfg(test)]")
        .next()
        .unwrap_or(s)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// **Trip-wire 3 (F1-T3).**  The three config command names do NOT appear in
/// the production portion of the executor or MCP router source files.
///
/// Uses `include_str!` to compile the source files into the test binary, then
/// asserts the three command name literals are absent from the production code
/// portion of each file (test blocks and comment lines are stripped to avoid
/// false positives from documentation or test strings).
///
/// ### Scope of the grep-gate
///
/// - `executor.rs` — the in-process invoker; must not gain a special-case
///   dispatch path for config commands.
/// - `tuxlink-mcp-core/src/router.rs` — the `TuxlinkMcp` router; must not
///   gain a `#[tool]`-annotated handler for any config command.
///
/// Files intentionally excluded from the grep:
/// - `config_commands.rs` — defines the commands; their names appear there by
///   design.
/// - `lib.rs` — the `invoke_handler!` registration; names appear there by
///   design.
/// - `injection_tests.rs` (this file) — the grep needle strings would
///   self-trip the test; excluded by the `split("#[cfg(test)]")` strip above
///   plus the fact that this file is not in the checked set.
///
/// ### Inverse sanity-check
///
/// If `#[tool] async fn elmer_config_set(…)` were added to `router.rs`, the
/// function name `elmer_config_set` would appear in the production portion of
/// that file and the assertion below would fail.
#[test]
fn config_command_names_not_in_router_source() {
    // The router and invoker sources to grep.
    // `include_str!` paths are relative to THIS file's location
    // (src-tauri/src/elmer/injection_tests.rs).
    let sources_to_check: &[(&str, &str)] = &[
        (
            "executor.rs",
            include_str!("executor.rs"),
        ),
        (
            "tuxlink-mcp-core/src/router.rs",
            include_str!("../../tuxlink-mcp-core/src/router.rs"),
        ),
    ];

    for (label, src) in sources_to_check {
        let prod = production_src(src);
        for name in CONFIG_COMMAND_NAMES {
            assert!(
                !prod.contains(name),
                "config command name `{name}` found in production source of \
                 `{label}` — this file must not register or dispatch config \
                 commands as MCP tools; check for a misplaced `#[tool]` \
                 annotation or router registration"
            );
        }
    }
}
