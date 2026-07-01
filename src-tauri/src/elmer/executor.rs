//! In-process MCP invoker for the Elmer agent pane.
//!
//! `InProcessMcpInvoker` implements [`ToolInvoker`] by spinning an in-memory
//! rmcp duplex pair against a real [`TuxlinkMcp`] router instance.  This means
//! every tool call goes through the SAME router code paths that the UDS transport
//! uses — including the taint side-effects in the router's `#[tool]` methods
//! (`router.rs:204/223/261/371`) — so the arm/taint gate is never bypassed.
//!
//! ## Security invariants (AC-1, AC-3)
//!
//! **AC-1 — dispatch through the router.**  Taint is a router side-effect set
//! inside the `#[tool]` method bodies (`guard.taint()` calls in `router.rs`).
//! Calling ports directly would read mailbox content WITHOUT tainting — a silent
//! security collapse.  The duplex approach ensures the invoker goes through
//! `TuxlinkMcp` exactly as the UDS transport does.
//!
//! **AC-3 — egress gate via `guarded_egress(Agent)`.**  Every EGRESS-marked router
//! tool is gated in the port implementation (`mcp_ports.rs` / `MonolithEgressPort`)
//! through `guarded_egress(&guard, EgressAuthority::Agent, ...)`.  A disarmed,
//! expired, or tainted session is denied at the operation.  The invoker exposes
//! the FULL router tool surface; arming is enforced at the operation, not by
//! hiding tools (spec C1: gate at the operation, not the list).
//!
//! ## Lifecycle (P2-D)
//!
//! The server task runs for the session's lifetime.  It returns when the client
//! half drops at session teardown.  `rearm` (Task 8) resets the conversation +
//! guard but does NOT rebuild the invoker — there is no reconnect cost per re-arm.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use rmcp::model::CallToolRequestParam;
use rmcp::service::{RoleClient, RunningService, ServiceExt};

use tuxlink_agent_frontend::mcp_client::{classify_call_error, list_tools_as_specs, map_call_result};
use tuxlink_agent_runner::{CallAuthority, ToolCall, ToolInvoker, ToolOutcome, ToolSpec};
use tuxlink_mcp_core::{McpState, TuxlinkMcp};

/// Elmer-local error returned when `InProcessMcpInvoker::connect` fails.
#[derive(Debug)]
pub enum ConnectError {
    /// The in-process rmcp handshake failed.
    Serve(String),
    /// Listing tools from the freshly-connected router failed.
    ListTools(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Serve(e) => write!(f, "in-process MCP serve error: {e}"),
            ConnectError::ListTools(e) => write!(f, "failed to list MCP tools: {e}"),
        }
    }
}

impl std::error::Error for ConnectError {}

/// An in-process MCP tool invoker backed by a [`TuxlinkMcp`] router served
/// over a `tokio::io::duplex` stream pair.
///
/// Construct with [`InProcessMcpInvoker::connect`].  The invoker holds the
/// server task alive for its lifetime; drop the invoker to end the session.
pub struct InProcessMcpInvoker {
    /// The live rmcp client session.  Wrapped in a Mutex because `call_tool`
    /// takes `&self` on `RunningService` but the method is not `Sync`.
    client: Mutex<RunningService<RoleClient, ()>>,
    /// Full router tool surface, egress tools included.  This is the only schema
    /// source the runner uses; arming is enforced at each operation via
    /// `guarded_egress(Agent)` in the port implementations, not by filtering here.
    tools: Vec<ToolSpec>,
    /// Keeps the server task alive.  The task returns when the client transport
    /// drops at session teardown.
    _server: tokio::task::JoinHandle<()>,
}

impl InProcessMcpInvoker {
    /// Spin up a `TuxlinkMcp` router server over an in-memory duplex, perform
    /// the rmcp handshake from the client side, and snapshot the full tool
    /// surface.  The server and client share the same `Arc<McpState>` (and
    /// therefore the same `Arc<EgressGuard>`), so taint set by the router's
    /// `#[tool]` methods is immediately visible to the caller's guard handle.
    pub async fn connect(state: Arc<McpState>) -> Result<Self, ConnectError> {
        // 256 KiB buffer — large enough for typical MCP message bursts.
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);

        // Server half: TuxlinkMcp router served over the server side.
        let (sr, sw) = tokio::io::split(server_io);
        let mcp = TuxlinkMcp::new(state);
        let server = tokio::spawn(async move {
            // `serve` handshakes and returns a `RunningService`.  We must call
            // `waiting()` on it (or keep it alive) for the duration of the
            // session — dropping `RunningService` immediately fires its
            // `DropGuard` and cancels the background task, ending the session
            // before any tool calls can complete.
            if let Ok(running) = mcp.serve((sr, sw)).await {
                let _ = running.waiting().await;
            }
        });

        // Client half: unit handler `()` served over the client side.
        let (cr, cw) = tokio::io::split(client_io);
        let client = ()
            .serve((cr, cw))
            .await
            .map_err(|e| ConnectError::Serve(e.to_string()))?;

        // The full router tool surface, egress tools included. Arming is enforced
        // at the operation via guarded_egress(Agent) in the router's port impls,
        // not by hiding tools here (spec C1: gate at the operation, not the list).
        let tools = list_tools_as_specs(&client)
            .await
            .map_err(|e| ConnectError::ListTools(e.to_string()))?;

        Ok(Self {
            client: Mutex::new(client),
            tools,
            _server: server,
        })
    }
}

#[async_trait]
impl ToolInvoker for InProcessMcpInvoker {
    fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        // SEC-3: the runner only ever passes Agent; assert belt-and-suspenders.
        debug_assert_eq!(authority, CallAuthority::Agent);

        let param = CallToolRequestParam {
            name: call.name.clone().into(),
            arguments: call.args.as_object().cloned(),
        };

        let client = self.client.lock().await;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                ToolOutcome::Cancelled(format!("cancelled during {}", call.name))
            }
            res = client.call_tool(param) => match res {
                Ok(r) => map_call_result(r),
                Err(e) => classify_call_error(&e.to_string()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;
    use tuxlink_agent_runner::{CallAuthority, ToolCall};
    use tuxlink_mcp_core::{test_support, McpState, TuxlinkMcp};
    use tuxlink_security::EgressGuard;

    /// Minimal valid arguments for each EGRESS-marked tool.
    ///
    /// rmcp decodes arguments BEFORE the `#[tool]` method body runs, so
    /// dispatching a tool with missing required fields returns a decode error —
    /// NOT `Denied` — and the arm gate is never reached.  Pass these args to
    /// guarantee the dispatch reaches `guarded_egress` and produces a true
    /// guard-driven denial (or success on armed+untainted).
    ///
    /// Fields marked `#[serde(default)]` in the router param structs
    /// (`intent`, `path`, `freq_hz` on vara/rig, `qsy`) are omitted
    /// intentionally — they carry a language-level default and need not be
    /// present.
    fn minimal_args_for_tool(name: &str) -> serde_json::Value {
        match name {
            // No required args.
            "cms_connect" | "verify_cms_connection" => serde_json::json!({}),
            // freq_hz is the one required field (serde NOT default).
            "rig_tune" => serde_json::json!({ "freq_hz": 7_104_000u64 }),
            // target is required; freq_hz + qsy are serde(default).
            "ardop_connect" => serde_json::json!({ "target": "KX4Z-10" }),
            // target is required; intent is serde(default).
            "ardop_b2f_exchange" => serde_json::json!({ "target": "KX4Z-10" }),
            // target is required; freq_hz + qsy are serde(default).
            "vara_b2f_exchange" => serde_json::json!({ "target": "KX4Z-10" }),
            // call is required; path is serde(default).
            "packet_connect" => serde_json::json!({ "call": "KX4Z-10" }),
            // Unknown / no-arg tools: safe default.
            _ => serde_json::json!({}),
        }
    }

    /// A recognisable id that the seeded mock mailbox will echo back.
    const SEEDED_ID: &str = "MSG001";

    // -----------------------------------------------------------------------
    // AC-1: taint-parity gate
    // -----------------------------------------------------------------------

    /// **Taint-parity test (AC-1 #1 gate).**  Calling `message_read` through the
    /// in-process invoker must taint the shared `EgressGuard` — because the
    /// dispatch goes through `TuxlinkMcp::message_read` which calls
    /// `self.state.guard.taint()`.  If the invoker bypassed the router and hit
    /// the port directly, the guard would remain un-tainted and this test would
    /// fail.
    #[tokio::test]
    async fn in_proc_invoker_taints_on_message_read() {
        let guard = Arc::new(EgressGuard::new());
        let state = test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
        let invoker = InProcessMcpInvoker::connect(state).await.unwrap();

        assert!(!guard.is_tainted(), "guard must start un-tainted");

        let call = ToolCall {
            name: "message_read".into(),
            args: serde_json::json!({ "folder": "inbox", "id": SEEDED_ID }),
        };
        let _ = invoker
            .invoke(&call, CallAuthority::Agent, &CancellationToken::new())
            .await;

        assert!(
            guard.is_tainted(),
            "in-proc invoker must taint the shared guard via the router"
        );
    }

    // -----------------------------------------------------------------------
    // AC-3: arm-gate trip-wires (C1/C2 — replacing the old withhold tests)
    // -----------------------------------------------------------------------

    /// A tool name + description pair extracted from the live router tool list.
    struct RouterToolDesc {
        name: String,
        description: String,
    }

    /// List ALL router tools (including egress ones) with their descriptions by
    /// spinning a short-lived in-process duplex against the real router.
    async fn list_all_router_tools_with_desc(state: Arc<McpState>) -> Vec<RouterToolDesc> {
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let (sr, sw) = tokio::io::split(server_io);
        let mcp = TuxlinkMcp::new(state);
        let _server = tokio::spawn(async move {
            if let Ok(running) = mcp.serve((sr, sw)).await {
                let _ = running.waiting().await;
            }
        });
        let (cr, cw) = tokio::io::split(client_io);
        let client = ().serve((cr, cw)).await.expect("client handshake");
        let all_tools = client.list_all_tools().await.expect("list_all_tools");
        all_tools
            .into_iter()
            .map(|t| RouterToolDesc {
                name: t.name.to_string(),
                description: t
                    .description
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// **C2 trip-wire (replaces the denylist-lock test):** every EGRESS-marked
    /// router tool is visible on the surface AND is gated — disarmed dispatch
    /// returns `Denied`.  Drives off the router's "EGRESS" description marker, so
    /// a new egress tool auto-joins this assertion and FAILS CI if it is not
    /// arm-gated.
    #[tokio::test]
    async fn every_egress_marked_tool_is_visible_and_arm_gated() {
        let guard = Arc::new(EgressGuard::new()); // disarmed, un-tainted

        // Discover EGRESS-marked tool names via the raw router tool list (which
        // includes descriptions — ToolSpec does not carry descriptions).
        let desc_state = test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
        let all = list_all_router_tools_with_desc(desc_state).await;
        let egress_names: Vec<String> = all
            .iter()
            .filter(|t| t.description.contains("EGRESS"))
            .map(|t| t.name.clone())
            .collect();
        assert!(
            !egress_names.is_empty(),
            "EGRESS-marked tools must be present in the router; none found"
        );

        // Build an invoker over the same guard (disarmed).
        let state = test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
        let invoker = InProcessMcpInvoker::connect(state).await.unwrap();

        // Every EGRESS tool must be visible on the surface (no longer filtered).
        let surface_names: std::collections::HashSet<&str> =
            invoker.tools().iter().map(|t| t.name.as_str()).collect();
        for name in &egress_names {
            assert!(
                surface_names.contains(name.as_str()),
                "EGRESS tool `{name}` must be visible on the invoker surface (withhold removed)"
            );
        }

        // Disarmed: each EGRESS tool must be Denied by the guard.
        //
        // We pass minimal valid args so that rmcp's argument decode succeeds and
        // the dispatch actually reaches `guarded_egress`.  Without valid args an
        // serde decode error arrives before the guard runs, and the test would
        // pass vacuously (the decode error is not a `Denied`).
        //
        // Additionally assert the Denied message contains the guard's own wording
        // so a future decode error cannot masquerade as a pass.  The egress error
        // chain is:
        //   guarded_egress → EgressDenied::NotArmed (Display: "send authority is
        //   not armed") → EgressPortError::Denied → router egress_err →
        //   "not authorized to transmit: ..." → classify_call_error →
        //   ToolOutcome::Denied(message).
        let cancel = CancellationToken::new();
        for name in &egress_names {
            let call = ToolCall { name: name.clone(), args: minimal_args_for_tool(name) };
            let out = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
            match &out {
                ToolOutcome::Denied(msg) => {
                    // "not authorized to transmit" is the prefix egress_err() adds
                    // around the EgressDenied::Display string.  If this sub-assert
                    // fires, the Denied came from somewhere other than the arm gate
                    // (e.g. a leftover decode error that somehow classified as
                    // Denied) — the gate itself was not exercised.
                    assert!(
                        msg.contains("not authorized"),
                        "disarmed EGRESS tool `{name}` was Denied but the message \
                         does not contain the guard's denial wording ('not authorized'): \
                         {msg:?}.  Ensure minimal_args_for_tool passes valid required \
                         fields so the guard — not the serde decoder — produces the Denied."
                    );
                }
                _ => panic!(
                    "disarmed EGRESS tool `{name}` must be Denied by the guard, got {out:?}"
                ),
            }
        }
    }

    /// **Armed + un-tainted: egress dispatch crosses the gate.**  The mock
    /// `EgressPort` in `test_support` calls the REAL `guarded_egress(Agent)` gate;
    /// when armed+untainted it returns `Ok(())` (gate opened), which the router
    /// maps to a success outcome — NOT `Denied`.  We assert not-Denied as the
    /// correct sufficient check (no real modem in CI; connect errors are
    /// `InvalidArgs`, not `Denied`).
    ///
    /// Per-tool `op_ran` resets confirm that EACH individual tool's gate opens
    /// and the mock op runs, not just that at least one tool in the loop set
    /// the flag.
    #[tokio::test]
    async fn armed_untainted_egress_is_not_denied() {
        use std::sync::atomic::Ordering;

        // Use state_with_all_probes so we can observe op_ran after the call.
        let (state, op_ran, _aborted, _staged) =
            test_support::state_with_all_probes(EgressGuard::new());
        // Arm the guard that was captured into the state.
        state.guard.arm(30); // armed, un-tainted

        let state = Arc::new(state);
        let invoker = InProcessMcpInvoker::connect(state).await.unwrap();
        let cancel = CancellationToken::new();

        // One representative tool per transport class.  Minimal valid args are
        // required: rmcp decodes arguments before the method body runs, so a
        // missing required field errors on decode (not on the guard), and the
        // armed test would pass vacuously (the guard was never reached).
        for name in &["cms_connect", "ardop_connect", "vara_b2f_exchange", "packet_connect"] {
            // Reset op_ran before EACH tool call so we can assert this specific
            // tool's gate opened — not just that any previous tool set the flag.
            op_ran.store(false, Ordering::SeqCst);

            let call = ToolCall {
                name: (*name).into(),
                args: minimal_args_for_tool(name),
            };
            let out = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
            assert!(
                !matches!(out, ToolOutcome::Denied(_)),
                "armed+untainted `{name}` must NOT be Denied (gate opened), got {out:?}"
            );

            // The mock EgressPort flips op_ran inside the gated closure.
            // Asserting this per-tool proves the gate opened AND the op actually
            // ran for that specific tool — not merely that a Denied was avoided.
            assert!(
                op_ran.load(Ordering::SeqCst),
                "armed+untainted `{name}` must reach the mock egress op \
                 (op_ran flag not set after the call — the gate may not have opened \
                 or the mock port was not called)"
            );
        }
    }
}
