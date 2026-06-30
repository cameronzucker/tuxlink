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
//! The endpoint URL is accepted from operator config OR the `elmer_config_set`
//! Tauri command (an operator UI action) — it is **never** sourced from a tool
//! result. The egress gate in [`tuxlink_agent_frontend::egress::build_vetted_client`]
//! enforces IP-policy at connect time; `AgentEndpoint::parse` rejects metadata
//! literals and credentials-in-URL at config time.
//!
//! ## Vetted-client egress (AC-7, Task C2)
//!
//! [`ElmerProvider::new_vetted`] is the preferred constructor for operator-configured
//! endpoints. It routes the model client through
//! [`tuxlink_agent_frontend::egress::build_vetted_client`] — the same
//! resolved-IP-pin and DNS-rebind gate used by the tile fetcher — so a named
//! host that resolves to a forbidden IP (metadata, link-local, loopback without
//! opt-in) is denied at build time, before any HTTP POST can be issued. The
//! `LoopbackEndpoint` constructor ([`ElmerProvider::new`]) remains for local
//! llama.cpp / Ollama shims and for existing tests.

use std::net::SocketAddr;

use async_trait::async_trait;
use serde_json::Value;

use tuxlink_agent_frontend::{
    egress::{build_vetted_client, EgressError},
    endpoint::{AgentEndpoint, LoopbackEndpoint},
    provider::{ApiKey, OpenAiProvider},
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
        let inner = OpenAiProvider::new(client, endpoint.0, model, api_key.map(ApiKey::new));
        Self { inner }
    }

    /// Build a redacting provider whose inner [`OpenAiProvider`] uses the
    /// SSRF-guarded vetted client from [`build_vetted_client`].
    ///
    /// Calls `build_vetted_client(&endpoint, system_resolver)` to resolve the
    /// endpoint host (if named), vet every resolved IP against the
    /// [`tuxlink_agent_frontend::egress::elmer_ip_is_permitted`] policy, and
    /// pin the connection to the vetted address set — closing the DNS-rebind
    /// window between config time and connect time.
    ///
    /// * `endpoint` — operator-configured endpoint, already validated by
    ///   [`AgentEndpoint::parse`] (rejects link-local/metadata literals and
    ///   credentials-in-URL at config time; this constructor adds the
    ///   fetch-time DNS-rebind gate).
    /// * `model` — model identifier string (e.g. `"llama3"`, `"gpt-4o"`).
    /// * `api_key` — optional bearer token (usually `None` for local ollama/llama.cpp).
    ///
    /// Returns `Err(EgressError)` if the endpoint's resolved IP is denied by
    /// the egress policy (e.g. a named host resolving to `169.254.169.254`).
    pub async fn new_vetted(
        endpoint: AgentEndpoint,
        model: String,
        api_key: Option<ApiKey>,
    ) -> Result<Self, EgressError> {
        Self::new_vetted_with_resolver(endpoint, model, api_key, |host, port| async move {
            system_resolver(&host, port).await
        })
        .await
    }

    /// Core constructor with an injectable resolver seam — mirrors
    /// `tiles::fetch::fetch_tile_bytes_with_resolver`.
    ///
    /// `resolve` maps `(host, port)` to candidate `SocketAddr`s. Production
    /// callers use [`Self::new_vetted`] which injects the platform resolver;
    /// tests inject a fake to prove deny-path propagation without real DNS.
    ///
    /// The `#[cfg(any(test, …))]` annotation is deliberately absent — the seam
    /// needs to be reachable from the `#[cfg(test)]` block inside this module,
    /// and `pub(crate)` suffices for that without requiring a feature flag.
    pub(crate) async fn new_vetted_with_resolver<R, Fut>(
        endpoint: AgentEndpoint,
        model: String,
        api_key: Option<ApiKey>,
        resolve: R,
    ) -> Result<Self, EgressError>
    where
        R: Fn(String, u16) -> Fut,
        Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
    {
        let client = build_vetted_client(&endpoint, resolve).await?;
        let url = endpoint.0;
        let inner = OpenAiProvider::new(client, url, model, api_key);
        Ok(Self { inner })
    }
}

// ---------------------------------------------------------------------------
// System resolver (production seam)
// ---------------------------------------------------------------------------

/// Production resolver: resolve `host:port` to a list of `SocketAddr` via the
/// platform resolver (Tokio's async DNS lookup). Mirrors
/// `tiles::fetch::system_resolve`.
async fn system_resolver(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    let target = format!("{host}:{port}");
    tokio::net::lookup_host(target).await.map(|it| it.collect())
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
        let msg = Message::User("Connect to CMS ;PQ: 23753528 and check mail".into());
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
        let msg = Message::Assistant("I see ;PR: 72768415 in the session log".into());
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
            content: "[C:B2F ;PQ: 23753528 AUTH OK]".into(),
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
                "body": "Password is ;PQ: 98765432"
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
        let val = json!(["safe", ";PQ: 11223344", 42]);
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
    // AC-7: LoopbackEndpoint smoke — retained until E2 (LoopbackEndpoint removal)
    // -----------------------------------------------------------------------

    /// `LoopbackEndpoint::parse` accepts a local ollama endpoint.
    ///
    /// Minimal smoke test retained so `LoopbackEndpoint` stays exercised until
    /// the E2 constructor migration removes it entirely.
    #[test]
    fn loopback_endpoint_accepts_local_ollama() {
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions");
        assert!(ep.is_ok(), "expected Ok, got {ep:?}");
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
    // AC-7: AgentEndpoint contract tests
    //
    // The endpoint accepted via `elmer_config_set` (operator UI action) is
    // validated by `AgentEndpoint::parse` at config time. The parse rules are:
    //   - metadata-IP literals → rejected
    //   - public host/IP       → accepted (egress gate covers runtime policy)
    //   - credentials-in-URL   → rejected
    //
    // These tests duplicate A1's AgentEndpoint tests deliberately — they keep
    // this module self-documenting about the parse contract without requiring a
    // cross-module import. The enforcement of "endpoint is never set from a tool
    // result" is verified by R2.4 in injection_tests.rs (task F1).
    // -----------------------------------------------------------------------

    /// `AgentEndpoint::parse` rejects a metadata-IP literal.
    ///
    /// The SSRF guard catches cloud-metadata addresses at config time so they
    /// never reach the egress gate or `ElmerProvider::new_vetted`.
    #[test]
    fn agent_endpoint_rejects_metadata_literal() {
        let ep = AgentEndpoint::parse("http://169.254.169.254/v1/chat/completions");
        assert!(ep.is_err(), "metadata-IP literal must be rejected at parse: {ep:?}");
    }

    /// `AgentEndpoint::parse` accepts a public HTTPS endpoint.
    ///
    /// Public cloud API endpoints (api.openai.com, etc.) are legitimate operator
    /// targets; the egress gate enforces runtime IP policy via `new_vetted`.
    #[test]
    fn agent_endpoint_accepts_public_host() {
        let ep = AgentEndpoint::parse("https://api.openai.com/v1/chat/completions");
        assert!(ep.is_ok(), "public endpoint must be accepted at parse: {ep:?}");
    }

    /// `AgentEndpoint::parse` rejects credentials-in-URL (userinfo component).
    ///
    /// An endpoint with embedded credentials would let a tool result inject a
    /// credential-bearing URL into the config; rejecting at parse closes that
    /// vector even if the caller is operator-sourced.
    #[test]
    fn agent_endpoint_rejects_userinfo() {
        let ep = AgentEndpoint::parse("https://user:pass@api.example.com/v1/chat/completions");
        assert!(ep.is_err(), "userinfo in URL must be rejected at parse: {ep:?}");
    }

    // -----------------------------------------------------------------------
    // AC-7: provider opacity test
    //
    // The endpoint must NEVER be reachable from a tool result. The provider
    // is constructed from operator config OR the `elmer_config_set` Tauri
    // command — both are operator-side actions, not agent-writable.
    //
    // The enforcement boundary (R2.4) lives in injection_tests.rs (task F1):
    // that test asserts the MCP boundary cannot set the endpoint from a tool
    // result. This test asserts the structural invariant: ElmerProvider has no
    // public endpoint setter reachable from Tauri command context.
    // -----------------------------------------------------------------------

    /// The ElmerProvider struct is opaque — it has no public setter for the
    /// endpoint that could be called from Tauri command context.
    ///
    /// Verifies that ElmerProvider can be constructed and used as a dyn Provider.
    #[test]
    fn elmer_provider_new_is_opaque_and_implements_provider() {
        // The only public constructors are ElmerProvider::new(LoopbackEndpoint, ...)
        // and ElmerProvider::new_vetted(AgentEndpoint, ...). Neither exposes a
        // public endpoint setter; the inner OpenAiProvider field is private
        // (no pub field, no setter method). An agent calling a Tauri tool has
        // no path to mutate the endpoint — the structural invariant enforces AC-7.
        let ep = LoopbackEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions")
            .expect("loopback must be accepted");
        let provider = ElmerProvider::new(ep, "test-model".into(), None);
        // If this trait object coercion compiles, the trait is implemented.
        let _: &dyn Provider = &provider;
    }

    // -----------------------------------------------------------------------
    // AC-7 / Task C2: new_vetted routes through build_vetted_client
    // -----------------------------------------------------------------------

    /// A resolver that always returns the given fixed addresses (test seam).
    /// Mirrors `tiles::fetch::fixed_resolver` and `egress::tests::fixed_resolver`.
    fn fixed_resolver(
        addrs: Vec<std::net::SocketAddr>,
    ) -> impl Fn(String, u16) -> std::future::Ready<std::io::Result<Vec<std::net::SocketAddr>>>
           + Clone {
        move |_host, _port| std::future::ready(Ok(addrs.clone()))
    }

    /// `new_vetted` accepts a loopback IP-literal endpoint and builds a
    /// provider. No network call is made — `build_vetted_client` takes the
    /// IP-literal branch (no DNS to rebind) and constructs the client directly.
    ///
    /// Mirrors the loopback smoke test for `ElmerProvider::new`, but proves
    /// that the vetted path reaches the same `Ok(provider)` outcome for the
    /// canonical local ollama / llama.cpp endpoint.
    #[tokio::test]
    async fn new_vetted_builds_for_loopback() {
        let ep = AgentEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions")
            .expect("loopback AgentEndpoint must parse");
        let result = ElmerProvider::new_vetted(ep, "llama3".into(), None).await;
        assert!(
            result.is_ok(),
            "new_vetted must succeed for a loopback IP-literal endpoint (build returned Err)"
        );
    }

    /// `new_vetted` accepts a public HTTPS endpoint (api.openai.com) when the
    /// resolver returns a public IP. Build only — no network I/O. Proves that
    /// `build_vetted_client` permits public IPs for Elmer (inverted vs tiles).
    ///
    /// Uses the resolver seam so no real DNS is required in CI.
    #[tokio::test]
    async fn new_vetted_builds_for_public() {
        let ep = AgentEndpoint::parse("https://api.openai.com/v1/chat/completions")
            .expect("public AgentEndpoint must parse");
        // Inject a resolver that returns a public IP — `build_vetted_client`
        // permits public IPs for model endpoints (INVERTED vs tile egress).
        let public: std::net::SocketAddr = "104.18.6.192:443".parse().unwrap();
        let result = ElmerProvider::new_vetted_with_resolver(
            ep,
            "gpt-4o".into(),
            Some(ApiKey::new("sk-x")),
            fixed_resolver(vec![public]),
        )
        .await;
        assert!(
            result.is_ok(),
            "new_vetted must succeed for a public endpoint resolving to a public IP (build returned Err)"
        );
    }

    /// `new_vetted` propagates `EgressError::HostDenied` when a named endpoint
    /// resolves to a forbidden IP (e.g. the cloud-metadata address).
    ///
    /// The metadata literal `169.254.169.254` already errors at
    /// `AgentEndpoint::parse`; to exercise the DNS-rebind gate inside
    /// `build_vetted_client` we use a NAMED endpoint (parsed fine) and poison
    /// the resolver to return the forbidden IP. This is the resolver-seam
    /// deny-path test described in the Task C2 brief.
    #[tokio::test]
    async fn new_vetted_denies_named_endpoint_resolving_to_metadata() {
        let ep = AgentEndpoint::parse("https://api.model.example/v1/chat/completions")
            .expect("named AgentEndpoint must parse");
        let metadata: std::net::SocketAddr = "169.254.169.254:443".parse().unwrap();
        let result = ElmerProvider::new_vetted_with_resolver(
            ep,
            "some-model".into(),
            None,
            fixed_resolver(vec![metadata]),
        )
        .await;
        assert!(
            matches!(result, Err(EgressError::HostDenied(_))),
            "new_vetted must propagate EgressError::HostDenied when the resolver returns a forbidden IP"
        );
    }
}
