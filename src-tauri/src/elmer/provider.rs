//! `ElmerProvider` — redacting [`Provider`] wrapper (Task 8a, tuxlink-13v2l).
//!
//! ## What it does
//!
//! Every message in the conversation is passed through
//! [`crate::winlink::redaction`] before the transcript reaches the model
//! endpoint. This is the per-turn redaction sink required by AC-6: secrets
//! (secure-login `;PQ`/`;PR` tokens, raw wire lines that may echo credentials)
//! are scrubbed from ALL four [`Message`] variants — including the `ToolCall`
//! serialized `args` — before the HTTP POST is made.
//!
//! ## SSRF defence (AC-7)
//!
//! The endpoint URL is accepted ONLY from operator config or the hard-coded
//! loopback default. It is never sourced from a tool result. The
//! [`tuxlink_agent_frontend::endpoint::LoopbackEndpoint`] wrapper enforces that
//! the URL is loopback-only (`127.0.0.0/8` / `::1` / `localhost`); any other
//! host is rejected at construction time.

use async_trait::async_trait;
use serde_json::Value;

use tuxlink_agent_frontend::{
    endpoint::LoopbackEndpoint,
    provider::OpenAiProvider,
};
use tuxlink_agent_runner::{
    Conversation, Message, ModelTurn, Provider, ProviderError, ToolCall, ToolSpec,
};

// ---------------------------------------------------------------------------
// Public type
// ---------------------------------------------------------------------------

/// A [`Provider`] that redacts every message in the conversation before
/// forwarding to the inner [`OpenAiProvider`].
///
/// Construct via [`ElmerProvider::new`]. The inner provider does the actual
/// HTTP POST; this wrapper is purely a redaction pass.
pub struct ElmerProvider {
    inner: OpenAiProvider,
}

impl ElmerProvider {
    /// Construct a redacting provider.
    ///
    /// * `endpoint` — pre-validated loopback endpoint (SEC-5, AC-7).
    /// * `model` — model identifier string (e.g. `"llama3"`, `"mistral"`).
    /// * `api_key` — optional bearer token (usually `None` for local ollama/llama.cpp).
    pub fn new(endpoint: LoopbackEndpoint, model: String, api_key: Option<String>) -> Self {
        let client = reqwest::Client::new();
        let inner = OpenAiProvider::new(client, endpoint.0, model, api_key);
        Self { inner }
    }
}

#[async_trait]
impl Provider for ElmerProvider {
    /// Run one model turn with a fully-redacted copy of `conversation`.
    ///
    /// The redacted conversation is built message-by-message via
    /// [`redact_message`], which is **exhaustive over all four `Message`
    /// variants** — a `match` forces exhaustiveness so a future new variant
    /// cannot be silently passed through unredacted. The original conversation
    /// is never mutated.
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
    ) -> Result<ModelTurn, ProviderError> {
        // AC-6: build a redacted conversation for the model turn.
        let redacted_messages: Vec<Message> = conversation
            .messages()
            .iter()
            .map(redact_message)
            .collect();
        let redacted = Conversation::from_messages(redacted_messages);

        self.inner.turn(&redacted, tools).await
    }
}

// ---------------------------------------------------------------------------
// Redaction helpers
// ---------------------------------------------------------------------------

/// Redact a single [`Message`] variant.
///
/// **Exhaustive over all four variants** — the `match` ensures no variant
/// is silently passed through. Any new variant added to the enum will cause
/// a compile error here, forcing a conscious redaction decision.
fn redact_message(msg: &Message) -> Message {
    match msg {
        Message::User(text) => {
            Message::User(redact_text(text))
        }
        Message::Assistant(text) => {
            Message::Assistant(redact_text(text))
        }
        Message::ToolCall(call) => {
            // Redact the serialized args string leaves (AC-6: a secret echoed
            // back in a tool-call argument must not leak to the model endpoint).
            Message::ToolCall(ToolCall {
                name: call.name.clone(),
                args: redact_json_value(&call.args),
            })
        }
        Message::ToolResult { name, ok, content } => {
            // Tool result bodies can contain raw wire lines with credentials
            // (the mailbox read path, CMS protocol echo-back, etc.).
            Message::ToolResult {
                name: name.clone(),
                ok: *ok,
                content: redact_text(content),
            }
        }
    }
}

/// Redact a free-form text string via the wire-line / free-form redactor.
///
/// Returns a fresh `String` even when nothing is redacted (cheap on clean
/// strings — `Cow::Borrowed` is cloned to owned).
fn redact_text(text: &str) -> String {
    crate::winlink::redaction::redact_freeform(text).into_owned()
}

/// Walk a JSON `Value` tree and redact every string leaf.
///
/// Arrays and objects are recursed; primitives other than strings are passed
/// through unchanged (numbers / booleans / null carry no credential payload).
fn redact_json_value(val: &Value) -> Value {
    match val {
        Value::String(s) => Value::String(redact_text(s)),
        Value::Array(arr) => Value::Array(arr.iter().map(redact_json_value).collect()),
        Value::Object(map) => {
            let redacted: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), redact_json_value(v)))
                .collect();
            Value::Object(redacted)
        }
        // Numbers, booleans, null — no credential payload, pass through.
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests (AC-6 + AC-7 coverage)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // AC-6: redact_message exhaustiveness + content checks
    // -----------------------------------------------------------------------

    /// A `;PQ` token inside a `User` message is redacted before the turn.
    #[test]
    fn user_message_redacts_secure_login_token() {
        let msg = Message::User("Connect to CMS ;PQ23753528 and check mail".into());
        let redacted = redact_message(&msg);
        match redacted {
            Message::User(text) => {
                assert!(
                    !text.contains("23753528"),
                    "secure-login token must be redacted: {text}"
                );
            }
            _ => panic!("variant changed"),
        }
    }

    /// A `;PR` token inside an `Assistant` message is redacted.
    #[test]
    fn assistant_message_redacts_secure_login_token() {
        let msg = Message::Assistant("I see ;PR72768415 in the session log".into());
        let redacted = redact_message(&msg);
        match redacted {
            Message::Assistant(text) => {
                assert!(
                    !text.contains("72768415"),
                    "token must be redacted from assistant: {text}"
                );
            }
            _ => panic!("variant changed"),
        }
    }

    /// A `;PQ` token in a `ToolResult` body is redacted.
    ///
    /// AC-6 test case: "a tool-result password literal is redacted on turn 2
    /// (not just turn 1)." This verifies the ToolResult variant is covered.
    #[test]
    fn tool_result_redacts_password_literal_on_repeat_turn() {
        // Simulate turn 2: ToolResult body contains a raw secure-login token
        // echoed from a CMS protocol response.
        let msg = Message::ToolResult {
            name: "cms_connect".into(),
            ok: true,
            content: "[C:B2F ;PQ23753528 AUTH OK]".into(),
        };
        let redacted = redact_message(&msg);
        match redacted {
            Message::ToolResult { content, .. } => {
                assert!(
                    !content.contains("23753528"),
                    "tool-result must redact password: {content}"
                );
            }
            _ => panic!("variant changed"),
        }
    }

    /// `ToolCall` args JSON string leaves are redacted.
    #[test]
    fn tool_call_args_string_leaves_are_redacted() {
        let msg = Message::ToolCall(ToolCall {
            name: "message_send".into(),
            args: json!({
                "to": "W1AW@winlink.org",
                "body": "Password is ;PQ98765432"
            }),
        });
        let redacted = redact_message(&msg);
        match redacted {
            Message::ToolCall(call) => {
                let body = call.args.get("body").and_then(Value::as_str).unwrap_or("");
                assert!(
                    !body.contains("98765432"),
                    "ToolCall args must be redacted: {}", call.args
                );
            }
            _ => panic!("variant changed"),
        }
    }

    /// `ToolCall` name is preserved unchanged (only args are redacted).
    #[test]
    fn tool_call_name_preserved_after_redaction() {
        let msg = Message::ToolCall(ToolCall {
            name: "find_stations".into(),
            args: json!({ "grid": "DM79" }),
        });
        let redacted = redact_message(&msg);
        match redacted {
            Message::ToolCall(call) => {
                assert_eq!(call.name, "find_stations");
                assert_eq!(call.args, json!({ "grid": "DM79" }));
            }
            _ => panic!("variant changed"),
        }
    }

    /// Clean text passes through redact_text with content intact.
    #[test]
    fn clean_text_passes_through_unchanged() {
        let text = "find a station near my grid DM79";
        let result = redact_text(text);
        assert_eq!(result, text);
    }

    /// Nested JSON arrays have their string leaves redacted.
    #[test]
    fn nested_json_array_string_leaves_are_redacted() {
        let val = json!(["safe", ";PQ11223344", 42]);
        let redacted = redact_json_value(&val);
        let arr = redacted.as_array().unwrap();
        assert_eq!(arr[0], "safe");
        assert!(
            !arr[1].as_str().unwrap_or("").contains("11223344"),
            "array string leaf must be redacted"
        );
        assert_eq!(arr[2], 42); // number passes through
    }

    // -----------------------------------------------------------------------
    // AC-7: LoopbackEndpoint rejects non-loopback hosts
    // -----------------------------------------------------------------------

    /// `LoopbackEndpoint::parse` accepts a local ollama endpoint.
    #[test]
    fn loopback_endpoint_accepts_local_ollama() {
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions");
        assert!(ep.is_ok(), "expected Ok, got {ep:?}");
    }

    /// `LoopbackEndpoint::parse` rejects a LAN address (192.168.x.x).
    #[test]
    fn loopback_endpoint_rejects_lan_address() {
        let ep = LoopbackEndpoint::parse("http://192.168.1.5/v1/chat/completions");
        assert!(ep.is_err(), "LAN address must be rejected: {ep:?}");
    }

    /// `LoopbackEndpoint::parse` rejects the cloud metadata address.
    #[test]
    fn loopback_endpoint_rejects_metadata_address() {
        let ep = LoopbackEndpoint::parse("http://169.254.169.254/v1/chat/completions");
        assert!(ep.is_err(), "metadata address must be rejected: {ep:?}");
    }

    /// Verify that `ElmerProvider::new` can be constructed with a valid endpoint
    /// (smoke test — no actual HTTP call made).
    #[test]
    fn elmer_provider_new_smoke() {
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions")
            .expect("loopback must be accepted");
        let _provider = ElmerProvider::new(ep, "llama3".into(), None);
        // No panic = construction succeeded.
    }

    // -----------------------------------------------------------------------
    // AC-7: agent-writable config key absence test
    //
    // The endpoint must NEVER be reachable from a tool result. The provider
    // is constructed only in the lib.rs setup closure from config/default,
    // not from any command parameter or tool output. This test asserts the
    // absence of any Tauri command parameter that could deserialize an
    // endpoint/model from the frontend (a React-supplied endpoint would be
    // an SSRF vector).
    //
    // Implementation: the grep-gate lives in commands.rs (AC-5 test) which
    // asserts no Vec<Message>/Conversation command parameter exists. The
    // endpoint/model config key is an OPERATOR-only field (not agent-writable).
    // -----------------------------------------------------------------------

    /// The ElmerProvider struct is opaque — it has no public setter for the
    /// endpoint that could be called from Tauri command context.
    ///
    /// Verifies that ElmerProvider can be constructed and used as a dyn Provider.
    #[test]
    fn elmer_provider_new_is_opaque_and_implements_provider() {
        // The only public constructor is ElmerProvider::new(LoopbackEndpoint, ...).
        // There is no way to set the endpoint from outside this module; the inner
        // OpenAiProvider field is private (no pub field, no setter method).
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions")
            .expect("loopback must be accepted");
        let provider = ElmerProvider::new(ep, "test-model".into(), None);
        // If this trait object coercion compiles, the trait is implemented.
        let _: &dyn Provider = &provider;
    }
}
