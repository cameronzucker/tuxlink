//! Transport-agnostic rmcp-client helpers for Elmer frontends.
//!
//! These functions convert between rmcp's wire types and the runner's value
//! types, classify call errors as denials vs. operational failures, extract
//! tool-result text, and collect the tool surface from a live client.
//!
//! They are deliberately transport-agnostic: the same helpers work whether the
//! transport is a Unix-domain socket (d3zwe / `UdsToolInvoker`) or an in-process
//! duplex stream (the future Elmer pane frontend). The UDS-specific `connect`
//! call and `ToolInvoker` impl remain in d3zwe's `uds.rs`.

use serde_json::{Map, Value};

use tuxlink_agent_runner::{ToolOutcome, ToolSpec};

// --- Error classification ---------------------------------------------------

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

// --- Result extraction ------------------------------------------------------

/// Extract the concatenated text of a tool result's content blocks. The Tuxlink
/// tools pack a single JSON text content (`Content::json`), but be defensive
/// about multiple / non-text blocks.
pub fn extract_result_text(result: &rmcp::model::CallToolResult) -> String {
    let mut out = String::new();
    for content in &result.content {
        if let Some(text) = content.as_text() {
            out.push_str(&text.text);
        }
    }
    out
}

/// Map a successful [`rmcp::model::CallToolResult`] onto a [`ToolOutcome`].
///
/// A tool that ran but set `is_error = Some(true)` is an OPERATIONAL failure
/// (distinct from an authorization denial, which arrives as an `Err` from
/// `call_tool` and is handled via [`classify_call_error`]). On operational
/// failure the result text is surfaced as [`ToolOutcome::InvalidArgs`] so the
/// runner can re-prompt. On success the result text is parsed as JSON when
/// possible, else wrapped as a string, and returned as [`ToolOutcome::Ok`].
pub fn map_call_result(result: rmcp::model::CallToolResult) -> ToolOutcome {
    let text = extract_result_text(&result);
    if result.is_error.unwrap_or(false) {
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

// --- Tool surface -----------------------------------------------------------

/// Build a runner [`ToolSpec`] from an rmcp `Tool`. The rmcp `input_schema` is
/// an `Arc<Map<String, Value>>`; the runner's schema is a plain JSON object
/// `Value`, so wrap the map.
pub fn tool_spec_from_rmcp(tool: &rmcp::model::Tool) -> ToolSpec {
    let schema: Map<String, Value> = (*tool.input_schema).clone();
    ToolSpec {
        name: tool.name.to_string(),
        json_schema: Value::Object(schema),
    }
}

/// Fetch and convert the full tool surface from a running rmcp client session.
///
/// Calls `list_all_tools` (handles pagination internally) and maps each rmcp
/// `Tool` to a runner [`ToolSpec`] via [`tool_spec_from_rmcp`].
pub async fn list_tools_as_specs(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<Vec<ToolSpec>, rmcp::ServiceError> {
    let listed = client.list_all_tools().await?;
    Ok(listed.iter().map(tool_spec_from_rmcp).collect())
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
}
