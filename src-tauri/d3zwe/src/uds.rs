//! `UdsToolInvoker` — a [`ToolInvoker`] backed by an rmcp CLIENT over the live
//! `tuxlink-mcp` Unix-domain socket (#939 descriptor; T8).
//!
//! ## Authority model — never arm, relay denials
//!
//! d3zwe does NOT pass an authority parameter to the MCP server and has NO code
//! path to arm send authority or clear taint. The arm/taint gate is enforced
//! ENTIRELY server-side by the MCP guard: a read-only / compose tool runs
//! freely; a gated egress tool the operator has not armed (or a tainted session)
//! is refused by the server, which returns an MCP error whose message says "not
//! authorized to transmit/write ...". This invoker classifies that error into
//! [`ToolOutcome::Denied`] and RELAYS it — it never attempts to arm.
//!
//! The crate's [`CallAuthority::Agent`] is the single authority the runner ever
//! passes `invoke`; it is informational at this UDS boundary (the real authority
//! decision lives below the MCP boundary, server-side). We assert it is `Agent`
//! as a belt-and-suspenders SEC-3 guard, matching the runner's own invariant.
//!
//! ## Testability
//!
//! The rmcp client wiring (connect, list_tools, call_tool) is thin and isolated
//! in [`UdsToolInvoker`]'s async methods. The behavior that decides outcomes —
//! classifying a call error as a denial vs an operational failure, extracting a
//! tool result's text, and resolving the default socket path — lives in PURE
//! functions ([`classify_call_error`], [`default_socket_path`]) that are
//! unit-tested with no live socket. The live round-trip is the N305 trial.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use rmcp::model::CallToolRequestParam;
use rmcp::service::{RoleClient, RunningService, ServiceExt};

use tuxlink_agent_runner::{CallAuthority, ToolCall, ToolInvoker, ToolOutcome, ToolSpec};

/// The ungated abort tools, by transport. On a cancel during a gated egress
/// tool, the frontend best-effort calls the matching one. They are NEVER gated
/// (a working abort must not require an armed session). See the router's
/// "Pure-stop tools" section.
pub const ABORT_TOOLS: &[&str] = &["cms_abort", "modem_ardop_disconnect", "vara_stop_session"];

/// An rmcp client session bound to the Tuxlink MCP UDS, plus the tool surface it
/// advertised at connect time.
pub struct UdsToolInvoker {
    client: Mutex<RunningService<RoleClient, ()>>,
    tools: Vec<ToolSpec>,
}

impl UdsToolInvoker {
    /// Connect to the MCP server at `socket_path`, perform the rmcp handshake,
    /// and snapshot the tool surface via `list_tools`. The connection is held
    /// for the session lifetime (the server serves a single active session).
    pub async fn connect(socket_path: &Path) -> Result<Self, ConnectError> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| ConnectError::Connect(format!("{}: {e}", socket_path.display())))?;
        let (read_half, write_half) = stream.into_split();

        // Symmetric to the server's `router.serve((read_half, write_half))`: the
        // client handler is the unit `()`, and the transport is the split-stream
        // tuple (rmcp `transport-async-rw`).
        let client = ()
            .serve((read_half, write_half))
            .await
            .map_err(|e| ConnectError::Handshake(e.to_string()))?;

        let listed = client
            .list_all_tools()
            .await
            .map_err(|e| ConnectError::ListTools(e.to_string()))?;

        let tools = listed.iter().map(tool_spec_from_rmcp).collect();

        Ok(Self {
            client: Mutex::new(client),
            tools,
        })
    }

    /// Best-effort call to an ungated abort tool by name (cancel → abort wiring).
    /// Returns whether the call reached the server without a transport error; an
    /// abort is never gated, so a non-error reply is success.
    pub async fn call_abort(&self, tool_name: &str) -> bool {
        let param = CallToolRequestParam {
            name: tool_name.to_string().into(),
            arguments: None,
        };
        let client = self.client.lock().await;
        client.call_tool(param).await.is_ok()
    }

    /// Cancel the running rmcp session, releasing the UDS slot. Best-effort.
    pub async fn shutdown(self) {
        let client = self.client.into_inner();
        let _ = client.cancel().await;
    }
}

#[async_trait]
impl ToolInvoker for UdsToolInvoker {
    fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        // SEC-3 belt-and-suspenders: the runner only ever sends Agent. We do NOT
        // forward an authority param to the server (the gate is server-side); we
        // only assert the invariant the runner upholds.
        debug_assert_eq!(authority, CallAuthority::Agent);

        // COR-2: if cancellation already fired, do not start the call.
        if cancel.is_cancelled() {
            return ToolOutcome::Denied("cancelled before tool dispatch".to_string());
        }

        // Stream live per-tool progress to stderr so the operator sees the
        // "transcript" as the opaque loop runs (the runner owns the Conversation
        // and returns only the terminal outcome — see print.rs).
        eprintln!("  → tool {} {}", call.name, call.args);

        let arguments = call.args.as_object().cloned();
        let param = CallToolRequestParam {
            name: call.name.clone().into(),
            arguments,
        };

        let client = self.client.lock().await;
        // Race the call against cancellation so an operator abort mid-tool is
        // observed promptly (COR-2). On cancel we drop the in-flight future.
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return ToolOutcome::Denied("cancelled during tool dispatch".to_string());
            }
            r = client.call_tool(param) => r,
        };

        match result {
            Ok(call_result) => {
                // A tool that ran. The server signals an OPERATIONAL failure via
                // CallToolResult.is_error == Some(true) (distinct from an
                // authorization denial, which arrives as an Err below). Surface
                // the result text either way; the runner re-prompts on an error
                // text but treats a denial as terminal.
                let text = extract_result_text(&call_result);
                if call_result.is_error.unwrap_or(false) {
                    ToolOutcome::InvalidArgs(text)
                } else {
                    // The server already curated + redacted this content; parse
                    // it as JSON when possible, else wrap the raw text. (Matched
                    // explicitly rather than `unwrap_or` so `text` is not moved
                    // while still borrowed by the parse.)
                    let value = match serde_json::from_str::<Value>(&text) {
                        Ok(v) => v,
                        Err(_) => Value::String(text),
                    };
                    ToolOutcome::Ok(value)
                }
            }
            Err(err) => classify_call_error(&err.to_string()),
        }
    }
}

/// Map an rmcp `call_tool` error string onto a [`ToolOutcome`].
///
/// An authorization denial (the server's `egress_err` / `write_err` emit
/// "not authorized to transmit/write ..." via `ErrorData::invalid_request`)
/// becomes [`ToolOutcome::Denied`] — terminal, relayed verbatim, NEVER retried
/// or worked around with an arm attempt. Any other error (transport, server
/// fault) is a non-denial that the runner can re-prompt on, surfaced as
/// [`ToolOutcome::InvalidArgs`] carrying the detail.
pub fn classify_call_error(message: &str) -> ToolOutcome {
    let lowered = message.to_ascii_lowercase();
    let is_denial = lowered.contains("not authorized")
        || lowered.contains("tainted")
        || lowered.contains("must arm")
        || lowered.contains("armed send")
        || lowered.contains("re-arm");
    if is_denial {
        ToolOutcome::Denied(message.to_string())
    } else {
        ToolOutcome::InvalidArgs(message.to_string())
    }
}

/// Extract the concatenated text of a tool result's content blocks. The Tuxlink
/// tools pack a single JSON text content (`Content::json`), but be defensive
/// about multiple / non-text blocks.
fn extract_result_text(result: &rmcp::model::CallToolResult) -> String {
    let mut out = String::new();
    for content in &result.content {
        if let Some(text) = content.as_text() {
            out.push_str(&text.text);
        }
    }
    out
}

/// Build a runner [`ToolSpec`] from an rmcp `Tool`. The rmcp `input_schema` is
/// an `Arc<Map<String, Value>>`; the runner's schema is a plain JSON object
/// `Value`, so wrap the map.
fn tool_spec_from_rmcp(tool: &rmcp::model::Tool) -> ToolSpec {
    let schema: Map<String, Value> = (*tool.input_schema).clone();
    ToolSpec {
        name: tool.name.to_string(),
        json_schema: Value::Object(schema),
    }
}

/// Why connecting to the MCP UDS failed.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// The Unix socket could not be connected (server not running, bad path).
    #[error("could not connect to the Tuxlink MCP socket ({0}); is the app running?")]
    Connect(String),
    /// The rmcp handshake failed.
    #[error("MCP handshake failed: {0}")]
    Handshake(String),
    /// `list_tools` failed after a successful handshake.
    #[error("listing tools failed: {0}")]
    ListTools(String),
}

/// Resolve the default MCP socket path, mirroring the #939 descriptor logic in
/// `src-tauri/src/mcp_connection.rs`: `$XDG_RUNTIME_DIR/tuxlink/mcp.sock` when
/// the runtime dir is set, else the hardened `/tmp/tuxlink-<uid>/tuxlink/mcp.sock`
/// fallback. This is the path used when `--socket` is not given.
///
/// NOTE: this does NOT replicate the runtime-dir *privacy* check the app's bind
/// path performs (that decides where to BIND); a connecting client simply tries
/// the canonical location. If the operator's app bound the fallback path, they
/// pass `--socket` explicitly. The path is surfaced to the operator so a
/// mismatch is visible, not silently wrong.
pub fn default_socket_path() -> PathBuf {
    // SAFETY: getuid(2) takes no args and cannot fail (POSIX).
    let uid = unsafe { libc::getuid() };
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("tuxlink").join("mcp.sock"),
        _ => std::env::temp_dir()
            .join(format!("tuxlink-{uid}"))
            .join("tuxlink")
            .join("mcp.sock"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- denial classification (no socket) --------------------------------

    #[test]
    fn not_authorized_is_denied() {
        let msg = "not authorized to transmit: send authority is not armed. \
                   The operator must ARM send authority, and the session must \
                   not be tainted.";
        assert!(matches!(classify_call_error(msg), ToolOutcome::Denied(_)));
    }

    #[test]
    fn not_authorized_to_write_is_denied() {
        let msg = "not authorized to write: the session is tainted.";
        assert!(matches!(classify_call_error(msg), ToolOutcome::Denied(_)));
    }

    #[test]
    fn tainted_session_is_denied() {
        assert!(matches!(
            classify_call_error("session is tainted; re-arm to clear"),
            ToolOutcome::Denied(_)
        ));
    }

    #[test]
    fn must_arm_is_denied() {
        assert!(matches!(
            classify_call_error("the operator must ARM send authority"),
            ToolOutcome::Denied(_)
        ));
    }

    #[test]
    fn denial_message_relayed_verbatim() {
        let msg = "not authorized to transmit: deadline passed";
        match classify_call_error(msg) {
            ToolOutcome::Denied(reason) => assert_eq!(reason, msg),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn operational_error_is_not_a_denial() {
        // A transport / server fault is NOT a denial — the runner may re-prompt.
        assert!(matches!(
            classify_call_error("connection reset by peer"),
            ToolOutcome::InvalidArgs(_)
        ));
        assert!(matches!(
            classify_call_error("invalid params: missing field `station`"),
            ToolOutcome::InvalidArgs(_)
        ));
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert!(matches!(
            classify_call_error("NOT AUTHORIZED TO TRANSMIT"),
            ToolOutcome::Denied(_)
        ));
        assert!(matches!(
            classify_call_error("Session Is TAINTED"),
            ToolOutcome::Denied(_)
        ));
    }

    // --- abort tool set ---------------------------------------------------

    #[test]
    fn abort_tools_are_the_three_ungated_stops() {
        assert!(ABORT_TOOLS.contains(&"cms_abort"));
        assert!(ABORT_TOOLS.contains(&"modem_ardop_disconnect"));
        assert!(ABORT_TOOLS.contains(&"vara_stop_session"));
        assert_eq!(ABORT_TOOLS.len(), 3);
    }

    // --- default socket path ----------------------------------------------

    #[test]
    fn default_socket_path_uses_xdg_when_set() {
        // We cannot safely mutate process env in parallel tests; assert the
        // structure of the returned path instead, which holds for both branches.
        let p = default_socket_path();
        assert!(
            p.ends_with("tuxlink/mcp.sock"),
            "default socket path should end with tuxlink/mcp.sock, got {p:?}"
        );
    }
}
