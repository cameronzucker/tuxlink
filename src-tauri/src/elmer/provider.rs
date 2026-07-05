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
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use tuxlink_agent_frontend::{
    anthropic_provider::{is_anthropic_endpoint, AnthropicProvider},
    egress::{build_vetted_client, EgressError},
    endpoint::{AgentEndpoint, LoopbackEndpoint},
    ollama_provider::OllamaProvider,
    provider::{ApiKey, OpenAiProvider},
};
use tuxlink_agent_runner::{
    Conversation, Message, ModelTurn, Provider, ProviderError, RunEvent, ToolCall, ToolSpec,
};

// ---------------------------------------------------------------------------
// Public type
// ---------------------------------------------------------------------------

/// A [`Provider`] that redacts every message in the conversation before
/// forwarding to the inner model provider.
///
/// The inner provider is selected by endpoint host at construction time:
/// * `api.anthropic.com` → [`AnthropicProvider`] (native Messages API).
/// * All other endpoints → [`OpenAiProvider`] (OpenAI-compatible `/v1/chat/completions`).
///
/// Construct via [`ElmerProvider::new`] (loopback) or [`ElmerProvider::new_vetted`]
/// (SSRF-guarded vetted client, preferred for remote endpoints). The inner
/// provider does the actual HTTP POST; this wrapper is purely a redaction pass.
pub struct ElmerProvider {
    /// The concrete model adapter, selected by endpoint at build time.
    inner: Box<dyn Provider + Send + Sync>,
    /// Which concrete adapter `inner` is, recorded at build time. Used only by
    /// tests to assert the probe-with-fallback selection (D1) picked the right
    /// adapter without a `dyn`-downcast; carries no behavior. Written on every
    /// build but READ only by the `#[cfg(test)]` `kind()` accessor, so it is
    /// genuinely dead in non-test builds — allow it there (in test builds the
    /// attribute is absent, so there is no unused-allow warning).
    #[cfg_attr(not(test), allow(dead_code))]
    kind: ProviderKind,
}

/// The concrete adapter an [`ElmerProvider`] wraps. Recorded so tests can assert
/// the probe-with-fallback selection (D1) without downcasting `dyn Provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Native Ollama `/api/chat` — chosen when a loopback probe of `/api/tags`
    /// succeeds (200 + `{"models": [...]}`).
    Ollama,
    /// OpenAI-compatible `/v1/chat/completions` — the loopback fallback (llama.cpp
    /// or any non-Ollama compat server) and the default remote adapter.
    OpenAi,
    /// Anthropic Messages API — chosen for the `api.anthropic.com` remote host.
    Anthropic,
}

impl ElmerProvider {
    /// Construct a redacting provider for a loopback endpoint.
    ///
    /// Loopback endpoints are always OpenAI-compatible (local llama.cpp / Ollama);
    /// `AnthropicProvider` is never chosen for loopback.
    ///
    /// * `endpoint` — pre-validated loopback endpoint (SEC-5, AC-7).
    /// * `model` — model identifier string (e.g. `"llama3"`, `"mistral"`).
    /// * `api_key` — optional bearer token (usually `None` for local ollama/llama.cpp).
    pub fn new(endpoint: LoopbackEndpoint, model: String, api_key: Option<String>) -> Self {
        let client = reqwest::Client::new();
        let inner: Box<dyn Provider + Send + Sync> = Box::new(
            OpenAiProvider::new(client, endpoint.0, model, None, None, api_key.map(ApiKey::new))
        );
        Self { inner, kind: ProviderKind::OpenAi }
    }

    /// Build a redacting provider whose inner adapter uses the SSRF-guarded
    /// vetted client from [`build_vetted_client`].
    ///
    /// The inner adapter is selected by endpoint host:
    /// * `api.anthropic.com` → [`AnthropicProvider`] (native Messages API with
    ///   `x-api-key` auth and `/v1/messages` wire format).
    /// * All other endpoints → [`OpenAiProvider`] (OpenAI-compatible).
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
    /// * `model` — model identifier string (e.g. `"llama3"`, `"gpt-4o"`, `"claude-haiku-4-5"`).
    /// * `num_ctx` — context window. Native Ollama requests it via
    ///   `options.num_ctx`; the OpenAI-compat adapter has no wire field for it so
    ///   it drives a CLIENT-SIDE transcript trim instead (tuxlink-evucv); the
    ///   Anthropic adapter ignores it. `None` = server default / no trim (T3/T4).
    /// * `temperature` — sampling temperature threaded to ALL three adapters
    ///   (Ollama `options.temperature`; OpenAI / Anthropic top-level
    ///   `"temperature"`). `None` leaves the server default unchanged (T9).
    /// * `system_prompt` — operator override applied to EVERY adapter
    ///   (tuxlink-31tbw); `None` = the built-in `ELMER_SYSTEM_PROMPT`.
    /// * `api_key` — optional credential (`x-api-key` for Anthropic; bearer for others).
    ///
    /// Returns `Err(EgressError)` if the endpoint's resolved IP is denied by
    /// the egress policy (e.g. a named host resolving to `169.254.169.254`).
    pub async fn new_vetted(
        endpoint: AgentEndpoint,
        model: String,
        num_ctx: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<String>,
        api_key: Option<ApiKey>,
    ) -> Result<Self, EgressError> {
        Self::new_vetted_with_resolver(
            endpoint,
            model,
            num_ctx,
            temperature,
            system_prompt,
            api_key,
            |host, port| async move { system_resolver(&host, port).await },
        )
        .await
    }

    /// Core constructor with an injectable resolver seam — mirrors
    /// `tiles::fetch::fetch_tile_bytes_with_resolver`. Uses the PRODUCTION probe
    /// ([`probe_ollama`]) for the loopback native-vs-compat selection (D1).
    ///
    /// `resolve` maps `(host, port)` to candidate `SocketAddr`s. Production
    /// callers use [`Self::new_vetted`] which injects the platform resolver;
    /// tests inject a fake to prove deny-path propagation without real DNS.
    ///
    /// ### Temperature scope (T9)
    ///
    /// `temperature` is threaded to all three adapters: the native Ollama adapter
    /// (`options.temperature`), the OpenAI-compat adapter (top-level
    /// `"temperature"`), and the Anthropic adapter (top-level `"temperature"`).
    /// `None` leaves each server's default unchanged.
    ///
    /// The `#[cfg(any(test, …))]` annotation is deliberately absent — the seam
    /// needs to be reachable from the `#[cfg(test)]` block inside this module,
    /// and `pub(crate)` suffices for that without requiring a feature flag.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn new_vetted_with_resolver<R, Fut>(
        endpoint: AgentEndpoint,
        model: String,
        num_ctx: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<String>,
        api_key: Option<ApiKey>,
        resolve: R,
    ) -> Result<Self, EgressError>
    where
        R: Fn(String, u16) -> Fut,
        Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
    {
        Self::new_vetted_with_resolver_and_probe(
            endpoint,
            model,
            num_ctx,
            temperature,
            system_prompt,
            api_key,
            resolve,
            |client, origin| async move { probe_ollama(&client, origin).await },
        )
        .await
    }

    /// Full constructor seam: injectable resolver AND injectable Ollama probe.
    ///
    /// This is the D1 probe-with-fallback core. The `probe` closure decides, for
    /// a LOOPBACK endpoint, whether a native Ollama server is present:
    /// `true` ⇒ build [`OllamaProvider`] (native `/api/chat`); `false` ⇒ fall back
    /// to [`OpenAiProvider`] (compat `/v1/chat/completions`, the pre-T4 behavior).
    /// The probe REUSES the same vetted client (SSRF-1 / SEC-5 gate) — it is never
    /// a fresh client. NON-loopback endpoints are never probed: they take the
    /// existing `is_anthropic_endpoint` host-based selection unchanged.
    ///
    /// Tests inject a probe that returns a forced result so the selection is
    /// exercised without a real server; production injects [`probe_ollama`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn new_vetted_with_resolver_and_probe<R, Fut, P, PFut>(
        endpoint: AgentEndpoint,
        model: String,
        num_ctx: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<String>,
        api_key: Option<ApiKey>,
        resolve: R,
        probe: P,
    ) -> Result<Self, EgressError>
    where
        R: Fn(String, u16) -> Fut,
        Fut: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
        // The probe origin is passed as a `String` (scheme+host+port). The URL
        // parse for the native endpoint uses `reqwest::Url::parse` (reqwest
        // re-exports `url::Url`), avoiding a direct `url` dep in this crate.
        P: Fn(reqwest::Client, String) -> PFut,
        PFut: std::future::Future<Output = bool>,
    {
        // ONE vetted client, built through the SSRF/DNS-rebind gate. Both the
        // probe and the eventual provider use THIS client — never a fresh one.
        let client = build_vetted_client(&endpoint, resolve).await?;
        let is_loopback = endpoint.is_loopback();
        // The canonical scheme+host+port origin (path/query stripped). Derived
        // from the validated endpoint via A1's `origin()` so the probe targets
        // the right host+port regardless of the configured path (`/api/chat` vs
        // `/v1/chat/completions`).
        let origin = endpoint.origin();
        let url = endpoint.0;

        let (inner, kind): (Box<dyn Provider + Send + Sync>, ProviderKind) = if is_loopback {
            // D1 — loopback discrimination via probe-with-fallback. A loopback
            // host may be Ollama (native `/api/*`) OR llama.cpp (compat-only,
            // 404s on `/api/*`). Probe `GET {origin}/api/tags` on the vetted
            // client; a positive probe means native Ollama is present.
            let is_ollama = probe(client.clone(), origin.clone()).await;
            if is_ollama {
                // FIX B: build the native URL from origin + /api/chat, NOT from
                // the operator-configured `url`. The operator tile default is
                // `/v1/chat/completions` (the OpenAI-compat path); posting an
                // Ollama `/api/chat` body to that path fails. `origin` is the
                // scheme+host+port from the validated endpoint (no trailing
                // slash), so appending `/api/chat` is always correct here.
                //
                // Defensive parse: origin came from a validated AgentEndpoint so
                // it will always parse, but we never unwrap in prod. A parse
                // failure here falls back to the OpenAI-compat adapter using the
                // original `url`; this trades the incorrect-URL failure mode for
                // a tolerated compat fallback rather than a panic.
                let native_url_str = format!("{origin}/api/chat");
                // Use reqwest::Url (re-exports url::Url) — no direct `url`
                // dep needed in this crate.
                match reqwest::Url::parse(&native_url_str) {
                    Ok(native_url) => {
                        tracing::debug!(
                            target: "elmer",
                            endpoint = %url,
                            native_url = %native_url,
                            "loopback probe found native Ollama (/api/tags) — using OllamaProvider"
                        );
                        (
                            Box::new(OllamaProvider::new(
                                client,
                                native_url,
                                model,
                                num_ctx,
                                temperature,
                                system_prompt,
                                api_key,
                            )),
                            ProviderKind::Ollama,
                        )
                    }
                    Err(e) => {
                        // Defensive: should never happen given a validated endpoint,
                        // but fall back to compat rather than panic.
                        tracing::warn!(
                            target: "elmer",
                            origin = %origin,
                            error = %e,
                            "loopback probe found Ollama but native URL parse failed — falling back to compat"
                        );
                        (
                            Box::new(OpenAiProvider::new(client, url, model, temperature, system_prompt, api_key)
                    .with_num_ctx(num_ctx)),
                            ProviderKind::OpenAi,
                        )
                    }
                }
            } else {
                // Any non-positive probe (404 / refused / timeout / unparseable)
                // preserves the current compat behavior. LOG it so the fallback
                // is not silent (a user who expected native num_ctx to apply can
                // see why it didn't).
                tracing::info!(
                    target: "elmer",
                    endpoint = %url,
                    "loopback probe did not find native Ollama — using OpenAI-compat (num_ctx drives a client-side transcript trim)"
                );
                (
                    Box::new(OpenAiProvider::new(client, url, model, temperature, system_prompt, api_key)
                    .with_num_ctx(num_ctx)),
                    ProviderKind::OpenAi,
                )
            }
        } else if is_anthropic_endpoint(url.as_str()) {
            // Remote: host-based selection, UNCHANGED from the pre-T4 behavior.
            (
                Box::new(AnthropicProvider::new(client, url, model, temperature, system_prompt, api_key)),
                ProviderKind::Anthropic,
            )
        } else {
            (
                Box::new(OpenAiProvider::new(client, url, model, temperature, system_prompt, api_key)
                    .with_num_ctx(num_ctx)),
                ProviderKind::OpenAi,
            )
        };

        Ok(Self { inner, kind })
    }

    /// The concrete adapter this provider wraps — test-only accessor for the D1
    /// probe-with-fallback selection assertions.
    #[cfg(test)]
    pub(crate) fn kind(&self) -> ProviderKind {
        self.kind
    }
}

// ---------------------------------------------------------------------------
// Ollama loopback probe (D1) — production default for the probe seam
// ---------------------------------------------------------------------------

/// Short timeout for the loopback `/api/tags` probe. A local Ollama answers this
/// in single-digit milliseconds; 2s tolerates a cold-but-running server while
/// still failing fast to the compat fallback when nothing is listening.
const OLLAMA_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Production probe: is a native Ollama server present at `origin`?
///
/// `origin` is the canonical scheme+host+port string from
/// [`AgentEndpoint::origin`] (no trailing slash, e.g. `http://127.0.0.1:11434`).
/// Issues `GET {origin}/api/tags` on the ALREADY-VETTED `client` (SSRF-1 / SEC-5
/// gate is upstream; this reuses that client, never a fresh one) with a short
/// timeout. Returns `true` ONLY when the response is 200 AND the body parses as
/// JSON containing a `"models"` array — Ollama's `/api/tags` shape. Any other
/// outcome (404 from llama.cpp, connection refused, timeout, non-JSON) returns
/// `false`, driving the compat fallback. Never panics; all errors → `false`.
///
/// `reqwest`'s `IntoUrl` re-parses the `String` URL, so this crate needs no
/// direct `url` dependency.
async fn probe_ollama(client: &reqwest::Client, origin: String) -> bool {
    let tags_url = format!("{origin}/api/tags");
    let resp = match client
        .get(tags_url)
        .timeout(OLLAMA_PROBE_TIMEOUT)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    if !resp.status().is_success() {
        return false;
    }
    match resp.json::<Value>().await {
        Ok(body) => body.get("models").map(Value::is_array).unwrap_or(false),
        Err(_) => false,
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
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<ModelTurn, ProviderError> {
        // AC-6: build a redacted conversation for the model turn.
        let redacted_messages: Vec<Message> = conversation
            .messages()
            .iter()
            .map(redact_message)
            .collect();
        let redacted = Conversation::from_messages(redacted_messages);

        // Pass the streaming sink straight through to the wrapped provider: this
        // decorator only redacts the conversation, it does not consume events.
        // (The wrapped OpenAiProvider is non-streaming until phase 1b.)
        self.inner.turn(&redacted, tools, on_event).await
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
        // Force the probe to report NO Ollama so this build takes the compat
        // fallback deterministically (no real server on this Pi). The point of
        // this test is that the loopback build reaches `Ok`, not which adapter.
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "llama3".into(),
            None,
            None,
            None,
            None,
            fixed_resolver(vec!["127.0.0.1:11434".parse().unwrap()]),
            never_ollama_probe(),
        )
        .await;
        assert!(
            result.is_ok(),
            "new_vetted must succeed for a loopback IP-literal endpoint (build returned Err)"
        );
        assert_eq!(
            result.unwrap().kind(),
            ProviderKind::OpenAi,
            "a negative probe must fall back to the compat adapter"
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
            None,
            None,
            None,
            Some(ApiKey::new("sk-x")),
            fixed_resolver(vec![public]),
        )
        .await;
        assert!(
            result.is_ok(),
            "new_vetted must succeed for a public endpoint resolving to a public IP (build returned Err)"
        );
        assert_eq!(
            result.unwrap().kind(),
            ProviderKind::OpenAi,
            "a non-anthropic public endpoint must select the compat adapter"
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
            None,
            None,
            None,
            fixed_resolver(vec![metadata]),
        )
        .await;
        assert!(
            matches!(result, Err(EgressError::HostDenied(_))),
            "new_vetted must propagate EgressError::HostDenied when the resolver returns a forbidden IP"
        );
    }

    // -----------------------------------------------------------------------
    // Provider selection: Anthropic vs OpenAI by endpoint host
    // -----------------------------------------------------------------------

    /// `new_vetted` for an api.anthropic.com endpoint succeeds and builds
    /// the AnthropicProvider path. Build only — no network I/O (the resolver
    /// seam returns a public IP; the result is always an ElmerProvider).
    ///
    /// We cannot inspect the `inner` field (it is `Box<dyn Provider>`) but we
    /// CAN assert the build succeeds for the Anthropic endpoint and that it
    /// also implements `Provider`. The `is_anthropic_endpoint` function (tested
    /// directly in `tuxlink_agent_frontend::anthropic_provider::tests`) is the
    /// per-host selector; this test ensures the full `new_vetted_with_resolver`
    /// path reaches `Ok` for the Anthropic host.
    #[tokio::test]
    async fn new_vetted_builds_for_anthropic_endpoint() {
        use tuxlink_agent_frontend::anthropic_provider::is_anthropic_endpoint;

        // Verify the selector itself first (pure function, no IO).
        assert!(
            is_anthropic_endpoint("https://api.anthropic.com/v1/messages"),
            "api.anthropic.com must be identified as an Anthropic endpoint"
        );
        assert!(
            !is_anthropic_endpoint("https://api.openai.com/v1/chat/completions"),
            "api.openai.com must NOT be identified as an Anthropic endpoint"
        );

        // Build through new_vetted_with_resolver — resolver returns a public IP.
        let ep = AgentEndpoint::parse("https://api.anthropic.com/v1/messages")
            .expect("Anthropic AgentEndpoint must parse");
        let public: std::net::SocketAddr = "18.208.0.0:443".parse().unwrap();
        let result = ElmerProvider::new_vetted_with_resolver(
            ep,
            "claude-haiku-4-5".into(),
            None,
            None,
            None,
            Some(ApiKey::new("sk-ant-x")),
            fixed_resolver(vec![public]),
        )
        .await;
        let provider = result.expect("new_vetted must succeed for api.anthropic.com with a public IP");
        assert_eq!(
            provider.kind(),
            ProviderKind::Anthropic,
            "the api.anthropic.com host must select the AnthropicProvider (remote selection unchanged)"
        );
    }

    // -----------------------------------------------------------------------
    // D1: probe-with-fallback loopback selection (native Ollama vs compat)
    // -----------------------------------------------------------------------

    /// A probe that ALWAYS reports native Ollama present (forces the native path).
    fn always_ollama_probe(
    ) -> impl Fn(reqwest::Client, String) -> std::future::Ready<bool> + Clone {
        move |_client, _origin| std::future::ready(true)
    }

    /// A probe that NEVER reports Ollama (forces the compat fallback).
    fn never_ollama_probe(
    ) -> impl Fn(reqwest::Client, String) -> std::future::Ready<bool> + Clone {
        move |_client, _origin| std::future::ready(false)
    }

    /// D1: a loopback endpoint whose probe reports Ollama → native OllamaProvider.
    #[tokio::test]
    async fn loopback_probe_positive_selects_ollama() {
        let ep = AgentEndpoint::parse("http://127.0.0.1:11434/api/chat")
            .expect("loopback AgentEndpoint must parse");
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "qwen3:8b".into(),
            Some(32_768),
            Some(0.7),
            None,
            None,
            fixed_resolver(vec!["127.0.0.1:11434".parse().unwrap()]),
            always_ollama_probe(),
        )
        .await
        .expect("loopback native build must succeed");
        assert_eq!(
            result.kind(),
            ProviderKind::Ollama,
            "a positive probe must select the native Ollama adapter"
        );
    }

    /// Fix B: a loopback endpoint configured with the OpenAI-compat path
    /// (`/v1/chat/completions`, the default localOllama tile value) still
    /// selects the native OllamaProvider when the probe is positive. This is
    /// the core regression guard: before Fix B, OllamaProvider was constructed
    /// with the configured URL (the compat path), so it POSTed an `/api/chat`
    /// body to `/v1/chat/completions` and failed. After Fix B, the URL is built
    /// from origin + `/api/chat` regardless of the configured path.
    #[tokio::test]
    async fn loopback_probe_positive_with_compat_input_url_still_selects_ollama() {
        // The operator configured the DEFAULT tile URL — the OpenAI-compat path.
        let ep = AgentEndpoint::parse("http://127.0.0.1:11434/v1/chat/completions")
            .expect("loopback AgentEndpoint must parse");
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "qwen3:8b".into(),
            Some(32_768),
            None,
            None,
            None,
            fixed_resolver(vec!["127.0.0.1:11434".parse().unwrap()]),
            always_ollama_probe(),
        )
        .await
        .expect("loopback native build must succeed even with compat input URL");
        assert_eq!(
            result.kind(),
            ProviderKind::Ollama,
            "a positive probe must select native Ollama even when the input URL is the compat path"
        );
        // (The inner OllamaProvider endpoint is Box<dyn Provider> — path cannot
        // be inspected here. The path assertion is in ollama_provider::tests via
        // OllamaProvider::endpoint_url(). ProviderKind::Ollama is the primary gate.)
    }

    /// D1: a loopback endpoint whose probe reports NO Ollama → compat fallback.
    /// This preserves the pre-T4 llama.cpp behavior.
    #[tokio::test]
    async fn loopback_probe_negative_falls_back_to_compat() {
        let ep = AgentEndpoint::parse("http://127.0.0.1:8080/v1/chat/completions")
            .expect("loopback AgentEndpoint must parse");
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "some-local-model".into(),
            Some(8192),
            None,
            None,
            None,
            fixed_resolver(vec!["127.0.0.1:8080".parse().unwrap()]),
            never_ollama_probe(),
        )
        .await
        .expect("loopback compat build must succeed");
        assert_eq!(
            result.kind(),
            ProviderKind::OpenAi,
            "a negative probe must fall back to the OpenAI-compat adapter"
        );
    }

    /// D1: a REMOTE endpoint is NEVER probed — even a probe that would report
    /// Ollama does not divert a remote host to the native adapter. The remote
    /// host-based selection is unchanged.
    #[tokio::test]
    async fn remote_endpoint_is_not_probed() {
        let ep = AgentEndpoint::parse("https://api.openai.com/v1/chat/completions")
            .expect("public AgentEndpoint must parse");
        let public: std::net::SocketAddr = "104.18.6.192:443".parse().unwrap();
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "gpt-4o".into(),
            None,
            None,
            None,
            Some(ApiKey::new("sk-x")),
            fixed_resolver(vec![public]),
            // Even a probe that WOULD say "Ollama present" must be ignored for a
            // remote host — remote endpoints are not probed at all.
            always_ollama_probe(),
        )
        .await
        .expect("remote build must succeed");
        assert_eq!(
            result.kind(),
            ProviderKind::OpenAi,
            "a remote endpoint must NOT be diverted to Ollama by the probe"
        );
    }

    /// D1: the egress deny-path still propagates on the loopback probe path — a
    /// forbidden resolved IP fails BEFORE the probe runs (the vetted-client build
    /// is the first step).
    #[tokio::test]
    async fn loopback_probe_path_still_propagates_egress_deny() {
        // A NAMED loopback (`localhost`) whose resolver is poisoned to a metadata
        // IP must be denied by build_vetted_client before any probe.
        let ep = AgentEndpoint::parse("http://localhost:11434/api/chat")
            .expect("localhost AgentEndpoint must parse");
        let metadata: std::net::SocketAddr = "169.254.169.254:11434".parse().unwrap();
        let result = ElmerProvider::new_vetted_with_resolver_and_probe(
            ep,
            "qwen3:8b".into(),
            None,
            None,
            None,
            None,
            fixed_resolver(vec![metadata]),
            always_ollama_probe(),
        )
        .await;
        assert!(
            matches!(result, Err(EgressError::HostDenied(_))),
            "a poisoned loopback name must be HostDenied before the probe runs"
        );
    }
}
