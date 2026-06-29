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
//! **AC-3 P0-1 — egress-tool withholding.**  The seven gated egress tools are
//! removed from the tool surface returned by `tools()` and short-circuit to
//! [`ToolOutcome::Denied`] in `invoke()` before touching the MCP channel.
//! Transmission is operator-gated via Task 8b's approval flush.
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

/// The seven gated egress tools whose descriptions contain "EGRESS" in
/// `router.rs:451-561`.  These are withheld from the agent's tool surface;
/// force-dispatching them returns [`ToolOutcome::Denied`].  Task 8b's approval
/// flush is the only path that fires them (under operator consent).
///
/// **DENYLIST LOCK:** the `withheld_set_equals_every_egress_marked_tool` test
/// asserts this constant equals the set of EGRESS-marked router tools at all
/// times.  Adding a new egress tool to the router without updating this list
/// FAILS the test — which is the desired trip-wire.
pub const WITHHELD_EGRESS_TOOLS: &[&str] = &[
    "cms_connect",
    "verify_cms_connection",
    "rig_tune",
    "ardop_connect",
    "ardop_b2f_exchange",
    "vara_b2f_exchange",
    "packet_connect",
];

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
    /// Router tools MINUS [`WITHHELD_EGRESS_TOOLS`].  This is the only schema
    /// source the runner uses; egress tools are absent.
    tools: Vec<ToolSpec>,
    /// Keeps the server task alive.  The task returns when the client transport
    /// drops at session teardown.
    _server: tokio::task::JoinHandle<()>,
}

impl InProcessMcpInvoker {
    /// Spin up a `TuxlinkMcp` router server over an in-memory duplex, perform
    /// the rmcp handshake from the client side, and snapshot the (filtered) tool
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

        // Snapshot the full tool surface, then filter out the egress tools.
        let all = list_tools_as_specs(&client)
            .await
            .map_err(|e| ConnectError::ListTools(e.to_string()))?;
        let tools = all
            .into_iter()
            .filter(|t| !WITHHELD_EGRESS_TOOLS.contains(&t.name.as_str()))
            .collect();

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

        // AC-3 P0-1: withheld egress tools are denied before touching the MCP
        // channel.  Task 8b's approval flush is the only authorised path for
        // these tools.
        if WITHHELD_EGRESS_TOOLS.contains(&call.name.as_str()) {
            return ToolOutcome::Denied(
                "Transmitting is operator-gated. Stage the message, then ask \
                 the operator to review and send via the approval dialog."
                    .into(),
            );
        }

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
    // AC-3 P0-1: egress-tool withholding
    // -----------------------------------------------------------------------

    /// **Withholding test.**  None of the seven gated egress tools appear in
    /// `tools()`; force-dispatching one returns `ToolOutcome::Denied`.
    #[tokio::test]
    async fn in_proc_invoker_withholds_egress_tools() {
        let guard = Arc::new(EgressGuard::new());
        let state = test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
        let invoker = InProcessMcpInvoker::connect(state).await.unwrap();

        for name in WITHHELD_EGRESS_TOOLS {
            assert!(
                !invoker.tools().iter().any(|t| t.name.as_str() == *name),
                "{name} must be withheld from tools()"
            );
        }

        // Force-dispatch a withheld tool → must be Denied.
        let call = ToolCall {
            name: "cms_connect".into(),
            args: serde_json::json!({}),
        };
        let out = invoker
            .invoke(&call, CallAuthority::Agent, &CancellationToken::new())
            .await;
        assert!(
            matches!(out, ToolOutcome::Denied(_)),
            "force-dispatching a withheld tool must return Denied, got {out:?}"
        );
    }

    // -----------------------------------------------------------------------
    // AC-3 P1-A: denylist equals EGRESS-marked tools (trip-wire)
    // -----------------------------------------------------------------------

    /// A tool name + description pair extracted from the live router tool list.
    struct RouterToolDesc {
        name: String,
        description: String,
    }

    /// List ALL router tools (including egress ones) with their descriptions by
    /// spinning a short-lived in-process duplex against the real router.
    async fn list_all_router_tools_with_desc(state: Arc<McpState>) -> Vec<RouterToolDesc> {
        use rmcp::service::ServiceExt;

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

    /// **Denylist-lock test (P1-A).**  The withheld set must EQUAL the set of
    /// router tools whose description contains the literal "EGRESS".  A new egress
    /// tool added to the router without updating [`WITHHELD_EGRESS_TOOLS`] will
    /// fail this test, preventing the security gap from shipping silently.
    #[tokio::test]
    async fn withheld_set_equals_every_egress_marked_tool() {
        let guard = Arc::new(EgressGuard::new());
        let state = test_support::state_with_seeded_inbox(guard, SEEDED_ID);
        let all = list_all_router_tools_with_desc(state).await;

        let egress_marked: std::collections::BTreeSet<String> = all
            .iter()
            .filter(|t| t.description.contains("EGRESS"))
            .map(|t| t.name.clone())
            .collect();

        let withheld: std::collections::BTreeSet<String> = WITHHELD_EGRESS_TOOLS
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(
            egress_marked, withheld,
            "every EGRESS-marked router tool must be withheld and vice-versa; \
             add new egress tools to WITHHELD_EGRESS_TOOLS"
        );
    }
}
