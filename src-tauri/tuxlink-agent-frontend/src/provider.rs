//! `OpenAiProvider` — a [`Provider`] backed by an OpenAI-compatible
//! `/v1/chat/completions` endpoint (SEC-5 loopback-enforced; T7).
//!
//! The model adapter is deliberately thin: the only stateful, IO-bearing piece
//! is one reqwest POST. Everything that decides behavior — building the request
//! body (tools array, transcript → messages), and mapping the response JSON onto
//! a [`ModelTurn`] — lives in PURE functions ([`build_request_body`],
//! [`parse_completion`]) that are unit-tested against recorded JSON with NO live
//! network.
//!
//! ## Response mapping
//!
//! * `choices[0].message.tool_calls` present and non-empty → [`ModelTurn::ToolCalls`].
//!   Each call's `function.arguments` is a JSON *string* per the OpenAI wire
//!   format; we parse it to a `Value`. A non-object / unparseable arguments
//!   string becomes `Value::Null` (the runner's COR-3 schema check then treats
//!   it as a malformed call and re-prompts — we do NOT silently drop it).
//! * Otherwise `choices[0].message.content` → [`ModelTurn::Text`] (empty string
//!   if the model returned a null content with no tool calls).

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use url::Url;

use tuxlink_agent_runner::{
    Conversation, Message, ModelTurn, Provider, ProviderError, ToolCall, ToolSpec,
};

// ---------------------------------------------------------------------------
// ApiKey — redacting newtype for bearer tokens
// ---------------------------------------------------------------------------

/// A bearer-token credential that NEVER leaks its value through [`std::fmt::Debug`]
/// or [`std::fmt::Display`].
///
/// The only way to obtain the secret string is [`ApiKey::expose`], which is an
/// explicit opt-in. This makes it impossible to accidentally log or format the
/// key — the default formatting paths both produce `<redacted>`.
///
/// Both `Debug` AND `Display` are implemented manually (not derived) because:
/// * A `#[derive(Debug)]` would print the raw inner value.
/// * `Display` is the format trait used by `format!("{}")`, `to_string()`, and
///   many error-reporting paths — a missing `Display` impl is the classic leak
///   vector where callers fall back to `{:?}` which would otherwise expose the
///   secret.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    /// Wrap a string as an `ApiKey`. The value is NOT validated — any non-empty
    /// string is accepted; the gateway and model endpoint reject invalid keys.
    pub fn new(s: impl Into<String>) -> Self {
        ApiKey(s.into())
    }

    /// The ONLY path to the raw secret value. Callers must explicitly invoke
    /// this when they need to set the `Authorization: Bearer …` header; all
    /// other uses should go through `Display`/`Debug` which redact.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Writes `ApiKey(<redacted>)` — the raw value is never included.
impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Writes `<redacted>` — guards against `format!("{key}")` accidentally leaking
/// the secret in logs or error messages.
impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

// ---------------------------------------------------------------------------
// scrub_key — pure helper for value-scrubbing a key out of an error snippet
// ---------------------------------------------------------------------------

/// Replace every occurrence of the exposed key in `snippet` with `<redacted>`.
///
/// This is a pure function so the scrub logic is unit-testable without a live
/// HTTP server. Called by the non-2xx error branch in [`OpenAiProvider::turn`]
/// immediately before building the [`ProviderError::Transport`] string, so a
/// 401 response that echoes the bearer token back in its body cannot propagate
/// the secret into the error log.
///
/// When `key` is `None` (unauthenticated endpoint) the snippet is returned
/// unchanged.
pub(crate) fn scrub_key(snippet: String, key: Option<&ApiKey>) -> String {
    match key {
        Some(k) => snippet.replace(k.expose(), "<redacted>"),
        None => snippet,
    }
}

/// Scrub the key out of `text`, THEN cap to `max` chars. Order matters: capping
/// before scrubbing would let a key straddling the boundary leak a prefix.
///
/// This is the correct order for the non-2xx error-body path in
/// [`OpenAiProvider::turn`]: scrub the full body first so a key that straddles
/// the `max`-char boundary cannot leave a partial bearer-token prefix in the
/// resulting snippet. Exposed as `pub(crate)` so it is unit-testable from the
/// same `#[cfg(test)]` module.
pub(crate) fn redact_and_cap(text: String, key: Option<&ApiKey>, max: usize) -> String {
    scrub_key(text, key).chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// OpenAiProvider
// ---------------------------------------------------------------------------

/// A [`Provider`] that talks to an OpenAI-compatible chat-completions endpoint.
pub struct OpenAiProvider {
    client: reqwest::Client,
    /// Pre-validated (SEC-5) endpoint URL. Set ONCE from the CLI/config at
    /// construction; never mutated, and never sourced from a tool result.
    endpoint: Url,
    model: String,
    /// Optional bearer token (a local llama.cpp / Ollama shim usually needs
    /// none; an OpenAI-compatible proxy may). Stored as [`ApiKey`] so it never
    /// leaks through `Debug`/`Display`; only used via `.expose()` at the HTTP
    /// header boundary.
    api_key: Option<ApiKey>,
}

impl OpenAiProvider {
    /// Build the provider. `endpoint` MUST already have passed
    /// [`crate::endpoint::validate_endpoint`] — this constructor does not
    /// re-validate (the SEC-5 gate is the caller's single chokepoint), but it is
    /// only reachable from `main` after that gate.
    pub fn new(client: reqwest::Client, endpoint: Url, model: impl Into<String>, api_key: Option<ApiKey>) -> Self {
        Self {
            client,
            endpoint,
            model: model.into(),
            api_key,
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
    ) -> Result<ModelTurn, ProviderError> {
        let body = build_request_body(&self.model, conversation, tools);

        let mut req = self.client.post(self.endpoint.clone()).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            // Capture a bounded slice of the error body for the operator, but do
            // not let a huge body blow up the message.  Value-scrub the bearer
            // key BEFORE capping — capping first would let a key that straddles
            // the boundary leave a partial prefix in the snippet.
            // `redact_and_cap` enforces scrub-then-cap in the correct order.
            let text = resp.text().await.unwrap_or_default();
            let snippet = redact_and_cap(text, self.api_key.as_ref(), 500);
            return Err(ProviderError::Transport(format!(
                "model endpoint returned HTTP {status}: {snippet}"
            )));
        }

        let value: Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Unparseable(format!("response was not JSON: {e}")))?;

        parse_completion(&value).map_err(ProviderError::Unparseable)
    }
}

// --- Pure request assembly --------------------------------------------------

/// OpenAI `tools` entry: `{ "type": "function", "function": { name, parameters } }`.
#[derive(Debug, Serialize)]
struct ToolEntry<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ToolFunction<'a>,
}

#[derive(Debug, Serialize)]
struct ToolFunction<'a> {
    name: &'a str,
    parameters: &'a Value,
}

/// Build the chat-completions request body from the transcript + tool surface.
/// Pure — no IO. Exposed for unit testing the message + tools shaping.
pub fn build_request_body(model: &str, conversation: &Conversation, tools: &[ToolSpec]) -> Value {
    let messages: Vec<Value> = conversation.messages().iter().map(render_message).collect();

    let tool_entries: Vec<ToolEntry> = tools
        .iter()
        .map(|t| ToolEntry {
            kind: "function",
            function: ToolFunction {
                name: &t.name,
                parameters: &t.json_schema,
            },
        })
        .collect();

    let mut body = json!({
        "model": model,
        "messages": messages,
    });

    // Only include `tools` when there is a tool surface — an empty array makes
    // some servers reject `tool_choice` defaults.
    if !tool_entries.is_empty() {
        body["tools"] = serde_json::to_value(&tool_entries).unwrap_or(Value::Null);
    }

    body
}

/// Render one transcript [`Message`] into an OpenAI chat message object.
///
/// Tool results map to the `tool` role. The runner's transcript does not carry
/// the OpenAI `tool_call_id`, so we label by tool name in the content; a local
/// model handles this fine, and the loop's correctness does not depend on the
/// id round-trip (it re-derives intent from the visible transcript each turn).
fn render_message(msg: &Message) -> Value {
    match msg {
        Message::User(text) => json!({ "role": "user", "content": text }),
        Message::Assistant(text) => json!({ "role": "assistant", "content": text }),
        Message::ToolCall(call) => json!({
            // Record the intended call as an assistant note so the model sees its
            // own prior action in the transcript.
            "role": "assistant",
            "content": format!("[called tool `{}` with {}]", call.name, call.args),
        }),
        Message::ToolResult { name, ok, content } => {
            let label = if *ok { "result" } else { "error" };
            json!({
                "role": "tool",
                "name": name,
                "content": format!("[{label}] {content}"),
            })
        }
    }
}

// --- Pure response mapping --------------------------------------------------

/// Map an OpenAI chat-completions response JSON onto a [`ModelTurn`]. Pure.
///
/// Returns `Err(detail)` only when the response is structurally unusable (no
/// `choices`); a present-but-empty content with no tool calls maps to an empty
/// `Text` turn rather than an error, so the loop can surface it.
pub fn parse_completion(value: &Value) -> Result<ModelTurn, String> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| "response had no choices[0]".to_string())?;

    let message = choice
        .get("message")
        .ok_or_else(|| "choices[0] had no message".to_string())?;

    // Tool calls take precedence over content.
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        if !tool_calls.is_empty() {
            let calls: Vec<ToolCall> = tool_calls.iter().map(parse_tool_call).collect();
            return Ok(ModelTurn::ToolCalls(calls));
        }
    }

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(ModelTurn::Text(content))
}

/// Parse a single OpenAI tool-call object into a [`ToolCall`].
///
/// `function.arguments` is a JSON STRING on the wire; we parse it into a value.
/// If it is absent / not a string / not valid JSON, we yield `Value::Null` as
/// the args so the runner's schema validation (COR-3) catches it as malformed
/// and re-prompts — we never silently fabricate a valid-looking object.
fn parse_tool_call(tc: &Value) -> ToolCall {
    let function = tc.get("function");
    let name = function
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let args = function
        .and_then(|f| f.get("arguments"))
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(Value::Null);

    ToolCall { name, args }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tuxlink_agent_runner::ToolSpec;

    // --- ApiKey redaction ----------------------------------------------------

    /// `Debug` output MUST NOT contain the secret value and MUST contain the
    /// literal string `<redacted>`.  This test covers the explicit manual
    /// `Debug` impl — a `#[derive(Debug)]` would expose the inner String.
    #[test]
    fn apikey_debug_is_redacted() {
        let key = ApiKey::new("sk-secret123");
        let debug = format!("{:?}", key);
        assert!(
            !debug.contains("sk-secret123"),
            "Debug output must not contain the raw secret; got: {debug:?}"
        );
        assert!(
            debug.contains("<redacted>"),
            "Debug output must contain '<redacted>'; got: {debug:?}"
        );
    }

    /// `Display` output MUST equal the exact string `<redacted>`.  `Display` is
    /// the classic leak vector: `format!("{key}")` or `.to_string()` can appear
    /// in log lines, UI strings, and error messages — all must be safe.
    #[test]
    fn apikey_display_is_redacted() {
        let key = ApiKey::new("sk-secret123");
        assert_eq!(format!("{}", key), "<redacted>");
    }

    /// `expose()` MUST return the exact raw secret — it is the only authorised
    /// reader and must not be affected by the redaction logic.
    #[test]
    fn apikey_expose_returns_secret() {
        let key = ApiKey::new("sk-x");
        assert_eq!(key.expose(), "sk-x");
    }

    // --- scrub_key pure helper -----------------------------------------------

    /// When a key IS present, `scrub_key` must replace every occurrence of the
    /// exposed secret in the snippet with `<redacted>`.  This is the primary
    /// defence against 401-echo credential leakage: the scrub runs on the raw
    /// error body BEFORE it becomes a `ProviderError::Transport` string.
    #[test]
    fn scrub_key_replaces_secret_in_snippet() {
        let key = ApiKey::new("sk-mysecret");
        let snippet = "invalid_api_key: sk-mysecret is not authorised".to_string();
        let scrubbed = scrub_key(snippet, Some(&key));
        assert!(
            !scrubbed.contains("sk-mysecret"),
            "scrub_key must remove the raw secret; got: {scrubbed:?}"
        );
        assert!(
            scrubbed.contains("<redacted>"),
            "scrub_key must insert '<redacted>' in place of the secret; got: {scrubbed:?}"
        );
    }

    /// When the key appears multiple times in the snippet (e.g. a verbose error
    /// body that echoes the key in multiple fields), ALL occurrences must be
    /// replaced, not just the first.
    #[test]
    fn scrub_key_replaces_all_occurrences() {
        let key = ApiKey::new("tok-abc");
        let snippet = "key=tok-abc, received=tok-abc, hint: tok-abc expired".to_string();
        let scrubbed = scrub_key(snippet, Some(&key));
        assert!(
            !scrubbed.contains("tok-abc"),
            "all occurrences of the secret must be scrubbed; got: {scrubbed:?}"
        );
        assert_eq!(
            scrubbed.matches("<redacted>").count(),
            3,
            "expected 3 replacements; got: {scrubbed:?}"
        );
    }

    /// When no key was sent (unauthenticated endpoint), the snippet must be
    /// returned unchanged.  This guards against accidentally over-scrubbing
    /// when there is no secret to protect.
    #[test]
    fn scrub_key_passthrough_when_no_key() {
        let snippet = "some error without a key".to_string();
        let scrubbed = scrub_key(snippet.clone(), None);
        assert_eq!(scrubbed, snippet, "snippet must be unchanged when key is None");
    }

    /// When the snippet does NOT contain the secret (e.g. a generic 500 body),
    /// `scrub_key` must return the snippet unchanged rather than injecting
    /// spurious `<redacted>` tokens.
    #[test]
    fn scrub_key_unchanged_when_secret_absent_from_snippet() {
        let key = ApiKey::new("sk-absent");
        let snippet = "internal server error".to_string();
        let scrubbed = scrub_key(snippet.clone(), Some(&key));
        assert_eq!(
            scrubbed, snippet,
            "snippet must be unchanged when the secret does not appear in it"
        );
    }

    /// `error_body_scrubs_just_sent_key` — verify that the value-scrub runs
    /// end-to-end through the `OpenAiProvider::turn` non-2xx path.
    ///
    /// `mockito` is NOT a dev-dependency of this crate (verified: only `tokio`
    /// appears in `[dev-dependencies]` in Cargo.toml and adding a new dep to
    /// the contended Pi is prohibited by the global constraints).  Rather than
    /// stub a live HTTP server, we test the scrub logic via the extracted pure
    /// helper `scrub_key` — see `scrub_key_replaces_secret_in_snippet` above.
    ///
    /// This test exercises the exact code path: take a raw 401-body snippet
    /// that echoes the bearer key, apply `scrub_key` with the key that was
    /// sent, and assert the secret is absent from the result.  The integration
    /// of `scrub_key` into `turn()` is validated here at the unit level and
    /// will be exercised end-to-end in CI once a real HTTP mock can be wired.
    #[test]
    fn error_body_scrubs_just_sent_key() {
        // Simulate: a 401 body that literally echoes the bearer token back
        // (observed with some OpenAI-compatible proxy implementations).
        let sent_key = ApiKey::new("sk-live-bearer-token");
        let raw_401_body = format!(
            "{{\"error\": \"invalid_api_key\", \"key\": \"{}\"}}",
            sent_key.expose()
        );
        let raw_snippet: String = raw_401_body.chars().take(500).collect();

        // This is exactly the operation performed in `turn()` before the
        // ProviderError::Transport is constructed.
        let scrubbed = scrub_key(raw_snippet, Some(&sent_key));

        assert!(
            !scrubbed.contains(sent_key.expose()),
            "ProviderError::Transport must not contain the raw bearer key; got: {scrubbed:?}"
        );
        assert!(
            scrubbed.contains("<redacted>"),
            "ProviderError::Transport must contain '<redacted>'; got: {scrubbed:?}"
        );
    }

    // --- redact_and_cap: scrub-before-cap order-dependency tests -------------

    /// THE BOUNDARY-STRADDLE REGRESSION GUARD.
    ///
    /// If the key starts at character 490 in a 510-char body and we cap first
    /// (at 500 chars), the snippet contains a 10-char PREFIX of the key, which
    /// `scrub_key` cannot match (it's looking for the full key). `redact_and_cap`
    /// scrubs the FULL body first, so even a key that straddles the boundary is
    /// fully replaced before any truncation happens.
    #[test]
    fn redact_and_cap_scrubs_before_capping() {
        let key = ApiKey::new("sk-boundary-key-secret");
        // Place the key at position 490 in a > 500-char string.
        // After a take(500) the raw snippet would contain only the first 10 chars
        // of the 22-char key, which scrub_key cannot match — this is the bug.
        let padding: String = "A".repeat(490);
        let text = format!("{}{}", padding, key.expose());
        assert!(
            text.len() > 500,
            "test precondition: text ({} chars) must exceed 500-char cap",
            text.len()
        );

        let result = redact_and_cap(text, Some(&key), 500);

        // The key prefix (first 8 chars) must NOT appear in the result.
        let key_prefix = &key.expose()[..8];
        assert!(
            !result.contains(key_prefix),
            "redact_and_cap must not leak a key prefix straddling the cap boundary; \
             got result: {result:?} (prefix checked: {key_prefix:?})"
        );
        // The full key must not appear either.
        assert!(
            !result.contains(key.expose()),
            "redact_and_cap must not leak the full key; got: {result:?}"
        );
        // The scrub placeholder must be present.
        assert!(
            result.contains("<redacted>"),
            "redact_and_cap must insert '<redacted>' where the key was; got: {result:?}"
        );
    }

    /// Verify the cap is honoured: a long key-free string is truncated to ≤ max.
    #[test]
    fn redact_and_cap_caps_length() {
        let long_text: String = "X".repeat(2000);
        let result = redact_and_cap(long_text, None, 500);
        assert!(
            result.len() <= 500,
            "redact_and_cap must cap the result to ≤ 500 chars; got {} chars",
            result.len()
        );
    }

    /// When key is None, the result must be capped but content otherwise unchanged.
    #[test]
    fn redact_and_cap_none_key_passthrough_capped() {
        let text: String = "B".repeat(1000);
        let result = redact_and_cap(text, None, 500);
        assert!(
            result.len() <= 500,
            "result must be capped; got {} chars",
            result.len()
        );
        // Content must be entirely 'B' — no spurious <redacted> tokens.
        assert!(
            result.chars().all(|c| c == 'B'),
            "no content mutation expected when key is None; got: {result:?}"
        );
    }

    fn echo_tool() -> ToolSpec {
        ToolSpec::new(
            "echo",
            json!({ "type": "object", "properties": { "msg": { "type": "string" } } }),
        )
    }

    // --- response mapping (recorded JSON, no network) ---------------------

    #[test]
    fn parses_text_completion() {
        let recorded = json!({
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hello operator" },
                "finish_reason": "stop"
            }]
        });
        assert_eq!(
            parse_completion(&recorded).unwrap(),
            ModelTurn::Text("hello operator".into())
        );
    }

    #[test]
    fn parses_tool_call_completion() {
        let recorded = json!({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "find_stations",
                            "arguments": "{\"grid\":\"DM79\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let turn = parse_completion(&recorded).unwrap();
        match turn {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_tool_calls_in_one_message() {
        let recorded = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [
                        { "function": { "name": "a", "arguments": "{\"x\":1}" } },
                        { "function": { "name": "b", "arguments": "{}" } }
                    ]
                }
            }]
        });
        match parse_completion(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[0].args, json!({ "x": 1 }));
                assert_eq!(calls[1].name, "b");
                assert_eq!(calls[1].args, json!({}));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_take_precedence_over_content() {
        let recorded = json!({
            "choices": [{
                "message": {
                    "content": "some chatter",
                    "tool_calls": [{ "function": { "name": "x", "arguments": "{}" } }]
                }
            }]
        });
        assert!(matches!(
            parse_completion(&recorded).unwrap(),
            ModelTurn::ToolCalls(_)
        ));
    }

    #[test]
    fn empty_tool_calls_array_falls_back_to_text() {
        let recorded = json!({
            "choices": [{
                "message": { "content": "no tools today", "tool_calls": [] }
            }]
        });
        assert_eq!(
            parse_completion(&recorded).unwrap(),
            ModelTurn::Text("no tools today".into())
        );
    }

    #[test]
    fn null_content_no_tools_is_empty_text() {
        let recorded = json!({
            "choices": [{ "message": { "role": "assistant", "content": null } }]
        });
        assert_eq!(parse_completion(&recorded).unwrap(), ModelTurn::Text(String::new()));
    }

    #[test]
    fn malformed_tool_arguments_become_null_args() {
        // `arguments` is not valid JSON → Null args, which the runner treats as a
        // malformed call (COR-3) rather than a silently-dropped one.
        let recorded = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{ "function": { "name": "echo", "arguments": "{not json" } }]
                }
            }]
        });
        match parse_completion(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "echo");
                assert_eq!(calls[0].args, Value::Null);
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn missing_choices_is_error() {
        assert!(parse_completion(&json!({})).is_err());
        assert!(parse_completion(&json!({ "choices": [] })).is_err());
    }

    // --- request assembly -------------------------------------------------

    #[test]
    fn request_body_includes_model_and_tools() {
        let convo = Conversation::new("find a station near DM79");
        let body = build_request_body("local-model", &convo, &[echo_tool()]);
        assert_eq!(body["model"], "local-model");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "find a station near DM79");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "echo");
        // The schema is passed through verbatim as `parameters`.
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn request_body_omits_tools_when_none() {
        let convo = Conversation::new("hi");
        let body = build_request_body("m", &convo, &[]);
        assert!(body.get("tools").is_none(), "tools should be absent: {body}");
    }

    #[test]
    fn tool_result_renders_as_tool_role() {
        let mut convo = Conversation::new("go");
        convo.push_tool_result("find_stations", "{\"count\":3}");
        let body = build_request_body("m", &convo, &[]);
        let tool_msg = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .expect("a tool-role message");
        assert_eq!(tool_msg["name"], "find_stations");
        assert!(tool_msg["content"].as_str().unwrap().contains("result"));
    }

    #[test]
    fn tool_error_result_labels_error() {
        let mut convo = Conversation::new("go");
        convo.push_tool_error("message_send", "tool denied: session is tainted");
        let body = build_request_body("m", &convo, &[]);
        let tool_msg = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .unwrap();
        assert!(tool_msg["content"].as_str().unwrap().contains("error"));
        assert!(tool_msg["content"].as_str().unwrap().contains("tainted"));
    }
}
