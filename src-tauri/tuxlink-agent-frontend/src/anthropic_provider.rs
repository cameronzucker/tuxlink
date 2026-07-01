//! `AnthropicProvider` — a [`Provider`] backed by Anthropic's native Messages API.
//!
//! The Anthropic Messages API differs from OpenAI's `/v1/chat/completions` in three
//! key ways this adapter handles:
//!
//! * **Auth header**: `x-api-key: <key>` instead of `Authorization: Bearer <key>`.
//! * **Request shape**: `system` is a top-level string (not a message), `max_tokens`
//!   is REQUIRED, tool schema uses `input_schema` (not `parameters`), and tool
//!   results use a special content-block structure.
//! * **Response shape**: `content` is an array of typed blocks (`text` | `tool_use`);
//!   there are no `choices`.
//!
//! ## Wire format
//!
//! POST to the configured endpoint (e.g. `https://api.anthropic.com/v1/messages`):
//!
//! ```json
//! {
//!   "model": "claude-haiku-4-5",
//!   "max_tokens": 8192,
//!   "system": "<ELMER_SYSTEM_PROMPT>",
//!   "messages": [ { "role": "user" | "assistant", "content": ... } ],
//!   "tools": [ { "name": "...", "description": "...", "input_schema": {...} } ]
//! }
//! ```
//!
//! `ToolCall` messages in the conversation become assistant content blocks with
//! `type: "tool_use"` and `input: <args object>`. `ToolResult` messages become
//! user content blocks with `type: "tool_result"`. Synthetic ids (`toulu_0`,
//! `toulu_1`, …) are assigned FIFO so each `tool_result.tool_use_id` matches the
//! preceding `tool_use.id` in the same conversation.
//!
//! ## Response mapping
//!
//! * Any `type == "tool_use"` block in `content` → `ModelTurn::ToolCalls`.
//!   `input` is already a JSON object (no string-parse needed, unlike OpenAI).
//! * Otherwise concatenate all `type == "text"` block `text` fields →
//!   `ModelTurn::Text`.
//!
//! ## Security
//!
//! The `x-api-key` header value is the ONLY auth difference from `OpenAiProvider`.
//! Endpoint vetting (SEC-5 / AC-7) is the caller's responsibility (the same
//! `validate_endpoint` → `AgentEndpoint` → `build_vetted_client` gate applies).
//! The key is stored as [`crate::provider::ApiKey`] and never appears in
//! `Debug`/`Display` output. Error bodies are scrubbed via `redact_and_cap`
//! before propagation.

use async_trait::async_trait;
use serde_json::{json, Value};
use url::Url;

use tuxlink_agent_runner::{Conversation, Message, ModelTurn, Provider, ProviderError, RunEvent, ToolCall, ToolSpec};

use crate::provider::{ApiKey, redact_and_cap, ELMER_SYSTEM_PROMPT};

/// Choose `max_tokens` for the request. The Messages API requires it, and it
/// bounds the WHOLE assistant turn — INCLUDING extended-thinking tokens, which
/// count against it on Claude 4.x models. The old flat 4096 truncated
/// thinking-heavy models (Sonnet 5) mid-thought, before any answer `text` block
/// was emitted, yielding an empty assistant turn (tuxlink-uig9f). Give generous
/// headroom for thinking + a multi-step synthesis answer, clamped below each
/// model's output ceiling: Haiku's max output is smaller than Sonnet/Opus.
fn anthropic_max_tokens(model: &str) -> u32 {
    if model.to_ascii_lowercase().contains("haiku") {
        8192
    } else {
        // Sonnet / Opus (and any non-Haiku Claude) support well beyond this;
        // 16384 covers heavy adaptive thinking plus the answer without risking
        // a per-model over-limit rejection.
        16384
    }
}

/// Anthropic API version header value. Required on every request.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A [`Provider`] that talks to Anthropic's native Messages API
/// (`POST /v1/messages`).
///
/// Constructed via `AnthropicProvider::new` after the endpoint has been
/// validated through `validate_endpoint` / `build_vetted_client`.
pub struct AnthropicProvider {
    client: reqwest::Client,
    /// Pre-validated (SEC-5) endpoint URL — the `/v1/messages` path.
    endpoint: Url,
    model: String,
    /// Sampling temperature forwarded to the request body as top-level
    /// `"temperature"` when `Some`; omitted entirely when `None` so the server
    /// default applies.
    temperature: Option<f32>,
    /// Optional operator-supplied system-prompt override (tuxlink-31tbw). When
    /// `Some`, it replaces [`ELMER_SYSTEM_PROMPT`] in the top-level `system`
    /// field; when `None`, the built-in default is used. Threaded from the
    /// model-config snapshot by T4 so a stored override reaches the wire.
    system_prompt: Option<String>,
    /// The `x-api-key` credential. Stored as [`ApiKey`] so it never leaks
    /// through `Debug`/`Display`; accessed via `.expose()` only at the HTTP
    /// header boundary.
    api_key: Option<ApiKey>,
}

impl AnthropicProvider {
    /// Build the provider. `endpoint` MUST already have passed
    /// [`crate::endpoint::validate_endpoint`] — this constructor does not
    /// re-validate. The same SEC-5 gate as `OpenAiProvider::new` applies.
    ///
    /// `temperature` is the sampling temperature forwarded as a top-level
    /// `"temperature"` field; `None` leaves the server default unchanged.
    ///
    /// `system_prompt` is the operator override (tuxlink-31tbw); `None` uses the
    /// built-in [`ELMER_SYSTEM_PROMPT`].
    pub fn new(
        client: reqwest::Client,
        endpoint: Url,
        model: impl Into<String>,
        temperature: Option<f32>,
        system_prompt: Option<String>,
        api_key: Option<ApiKey>,
    ) -> Self {
        Self {
            client,
            endpoint,
            model: model.into(),
            temperature,
            system_prompt,
            api_key,
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<ModelTurn, ProviderError> {
        // `on_event` is fire-and-forget — we do not use it (non-streaming). The
        // parameter is accepted for trait compatibility; this implementation is
        // non-streaming (Anthropic streaming SSE has different framing than
        // OpenAI SSE; a streaming path is a follow-up).
        let _ = on_event;

        let body = build_anthropic_request(
            &self.model,
            conversation,
            tools,
            self.temperature,
            self.system_prompt.as_deref().unwrap_or(ELMER_SYSTEM_PROMPT),
        );

        let mut req = self
            .client
            .post(self.endpoint.clone())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body);

        // Anthropic uses `x-api-key`, NOT `Authorization: Bearer`.
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key.expose());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet = redact_and_cap(text, self.api_key.as_ref(), 500);
            if status.as_u16() == 429 {
                return Err(ProviderError::RateLimited(format!(
                    "model endpoint returned HTTP 429 (rate limited): {snippet}"
                )));
            }
            return Err(ProviderError::Transport(format!(
                "model endpoint returned HTTP {status}: {snippet}"
            )));
        }

        let value: Value = resp.json().await.map_err(|e| {
            ProviderError::Unparseable(format!("response was not JSON: {e}"))
        })?;

        parse_messages_response(&value).map_err(ProviderError::Unparseable)
    }
}

// ---------------------------------------------------------------------------
// Pure request assembly
// ---------------------------------------------------------------------------

/// Build the Anthropic Messages API request body.
///
/// Pure — no IO. Exported so Rust tests can drive it directly.
///
/// `temperature` is forwarded as a top-level `"temperature"` field when `Some`;
/// omitted entirely when `None` so the server default applies. The Anthropic
/// Messages API accepts `temperature` as a top-level key (not nested).
///
/// `system_prompt` is the effective system prompt (the operator override or the
/// built-in [`ELMER_SYSTEM_PROMPT`], resolved by the caller — see
/// [`AnthropicProvider::turn`]). It is hoisted to the top-level `system` field
/// (Anthropic's Messages API places the system prompt there, not as a message).
pub fn build_anthropic_request(
    model: &str,
    conversation: &Conversation,
    tools: &[ToolSpec],
    temperature: Option<f32>,
    system_prompt: &str,
) -> Value {
    // Assign synthetic tool_use ids by FIFO position: the first ToolCall in
    // the conversation gets `toolu_0`, the second `toulu_1`, etc.
    // We need to pre-scan the conversation to number them so that each
    // tool_result.tool_use_id matches the corresponding tool_use.id we emit.
    let mut tool_call_counter: u32 = 0;

    let mut messages: Vec<Value> = Vec::with_capacity(conversation.messages().len());
    for msg in conversation.messages() {
        let rendered = render_anthropic_message(msg, &mut tool_call_counter);
        if let Some(v) = rendered {
            messages.push(v);
        }
    }

    let mut body = json!({
        "model": model,
        "max_tokens": anthropic_max_tokens(model),
        "system": system_prompt,
        "messages": messages,
    });

    // `temperature` is omitted entirely when `None` (server default); present
    // as a top-level JSON number when `Some` (Anthropic Messages API placement).
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }

    // Only include `tools` when there is a surface (empty array is allowed but
    // not helpful; some providers object to it).
    if !tools.is_empty() {
        let tool_entries: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": format!("Tool: {}", t.name),
                    "input_schema": t.json_schema,
                })
            })
            .collect();
        body["tools"] = Value::Array(tool_entries);
    }

    body
}

/// Render one transcript [`Message`] into an Anthropic messages-array entry.
///
/// Returns `None` only in the degenerate case where a message would produce
/// empty content (discarded rather than producing an invalid message).
///
/// The `tool_call_counter` is incremented each time a `ToolCall` is rendered
/// so that each `tool_use` block gets a unique synthetic id (`toolu_0`,
/// `toulu_1`, …) and the immediately-following `ToolResult` uses the same id.
/// The runner always appends a ToolCall immediately before its ToolResult, so
/// FIFO pairing is sound.
fn render_anthropic_message(msg: &Message, counter: &mut u32) -> Option<Value> {
    match msg {
        Message::User(text) => Some(json!({ "role": "user", "content": text })),
        Message::Assistant(text) => Some(json!({ "role": "assistant", "content": text })),
        Message::ToolCall(call) => {
            let id = format!("toulu_{}", *counter);
            *counter += 1;
            // `input` is a JSON object (NOT a string — differs from OpenAI).
            // Null args become an empty object so the wire value is always an object.
            let input = match &call.args {
                Value::Object(_) => call.args.clone(),
                Value::Null => json!({}),
                other => {
                    // Non-object, non-null — wrap in an object so the shape is valid.
                    json!({ "value": other })
                }
            };
            Some(json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": id,
                    "name": call.name,
                    "input": input,
                }]
            }))
        }
        Message::ToolResult { name, ok, content } => {
            // The matching ToolCall was at (counter - 1) since we increment
            // AFTER emitting the tool_use; the result uses the SAME counter
            // value (which has already been incremented by the preceding ToolCall
            // render). So the result id is `toulu_{counter - 1}`.
            //
            // Safety: if counter is 0 here the conversation is malformed (a
            // ToolResult without a preceding ToolCall); use 0 as the id so we
            // don't panic (Anthropic will surface a validation error).
            let call_idx = counter.saturating_sub(1);
            let tool_use_id = format!("toulu_{call_idx}");
            let label = if *ok { "result" } else { "error" };
            Some(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": format!("[{label}] {content} (tool: {name})")
                }]
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Pure response mapping
// ---------------------------------------------------------------------------

/// Map an Anthropic Messages API response JSON onto a [`ModelTurn`].
///
/// Returns `Err(detail)` when the response is structurally unusable (missing
/// `content` array). A present-but-empty `content` maps to an empty `Text`
/// turn rather than an error.
pub fn parse_messages_response(value: &Value) -> Result<ModelTurn, String> {
    let content = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "response had no content array".to_string())?;

    // Any `tool_use` block → ToolCalls (takes precedence, matching OpenAI convention).
    let tool_calls: Vec<ToolCall> = content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|block| {
            let name = block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // `input` is already a JSON object on the Anthropic wire.
            // If absent or not an object, use Null so the runner's COR-3
            // schema check catches it as malformed (same as OpenAI null-args policy).
            let args = block
                .get("input")
                .cloned()
                .unwrap_or(Value::Null);
            ToolCall { name, args }
        })
        .collect();

    if !tool_calls.is_empty() {
        return Ok(ModelTurn::ToolCalls(tool_calls));
    }

    // Concatenate all `text` blocks.
    let text: String = content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");

    Ok(ModelTurn::Text(text))
}

// ---------------------------------------------------------------------------
// Anthropic models-list response parsing (Step 4 — detect path)
// ---------------------------------------------------------------------------

/// Parse an Anthropic `GET /v1/models` response into a list of model ids.
///
/// Anthropic's models endpoint returns `{ "data": [ { "id": "...", ... }, … ], … }`.
/// This is the SAME outer shape as the OpenAI `/v1/models` response, so the
/// existing `map_models_response` in `config_commands.rs` works without changes
/// when given the Anthropic response body.
///
/// This function is provided as a pure testable seam specifically for the Anthropic
/// response shape. It mirrors what `map_models_response` does for the 2xx path,
/// parsing `data[].id`.
///
/// Returns `Err` only when `data` is absent or empty.
pub fn parse_anthropic_models_list(body: &str) -> Result<Vec<String>, String> {
    let parsed: Value =
        serde_json::from_str(body).map_err(|e| format!("response is not JSON: {e}"))?;

    let data = parsed
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "response missing `data` array".to_string())?;

    if data.is_empty() {
        return Err("no models returned".to_string());
    }

    let ids: Vec<String> = data
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(String::from))
        .collect();

    if ids.is_empty() {
        return Err("data array had entries but none had an `id` string field".to_string());
    }

    Ok(ids)
}

// ---------------------------------------------------------------------------
// Provider selector — pick AnthropicProvider or OpenAiProvider by host
// ---------------------------------------------------------------------------

/// Returns `true` when `endpoint` routes to `api.anthropic.com`.
///
/// The selector is host-based: any endpoint on `api.anthropic.com` uses the
/// native Messages API; all other endpoints fall through to `OpenAiProvider`.
/// Pure — no I/O.
/// Host-based selector: true iff `endpoint` is the Anthropic Messages API host.
///
/// Takes `&str` (not `&Url`) so callers in crates that do not depend on the
/// `url` crate directly — e.g. the main `tuxlink` crate — can select without
/// importing `url::Url`. Parsing happens here, in the crate that owns `url`.
pub fn is_anthropic_endpoint(endpoint: &str) -> bool {
    Url::parse(endpoint)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.eq_ignore_ascii_case("api.anthropic.com"))
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tuxlink_agent_runner::{Conversation, ToolCall, ToolSpec};

    // -----------------------------------------------------------------------
    // Wire-shape test — request body
    // -----------------------------------------------------------------------

    /// A Conversation with User + ToolCall + ToolResult + a ToolSpec should
    /// produce the correct Anthropic wire shape:
    ///   - top-level `system` string (not a message)
    ///   - top-level `max_tokens`
    ///   - `messages` list with correctly typed blocks
    ///   - tool_use block has `input` as an OBJECT (not a string)
    ///   - tool_result block has `tool_use_id` matching the tool_use's `id`
    ///   - `tools[0].input_schema` present; NO `parameters`/`function` wrapper
    #[test]
    fn request_wire_shape_tool_call_and_result() {
        let mut convo = Conversation::new("find a station near DM79");
        // Append a ToolCall followed by its ToolResult (runner pattern).
        convo.push_tool_call(ToolCall::new("find_stations", json!({ "grid": "DM79" })));
        convo.push_tool_result("find_stations", r#"{"count":3}"#);

        let spec = ToolSpec::new(
            "find_stations",
            json!({ "type": "object", "properties": { "grid": { "type": "string" } } }),
        );

        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[spec], None, ELMER_SYSTEM_PROMPT);

        // 1. Top-level `system` must be a string (not a message object).
        assert!(
            body.get("system").and_then(Value::as_str).is_some(),
            "system must be a top-level string; got: {}",
            body.get("system").map(|v| v.to_string()).unwrap_or_default()
        );
        assert!(
            body["system"].as_str().unwrap().contains("Elmer"),
            "system prompt must contain 'Elmer'"
        );

        // 2. top-level `max_tokens` must be present AND generous enough that
        //    extended-thinking tokens don't truncate the answer (tuxlink-uig9f).
        //    Haiku gets its smaller ceiling; Sonnet/Opus get more.
        assert_eq!(
            body.get("max_tokens").and_then(Value::as_u64),
            Some(8192),
            "haiku max_tokens must be 8192 (was built with claude-haiku-4-5)"
        );
        assert_eq!(
            build_anthropic_request("claude-sonnet-5", &convo, &[], None, ELMER_SYSTEM_PROMPT)
                .get("max_tokens")
                .and_then(Value::as_u64),
            Some(16384),
            "sonnet/opus max_tokens must be 16384 so thinking + answer fit (Sonnet 5 empty-output bug)"
        );

        let msgs = body["messages"].as_array().expect("messages must be array");

        // 3. First message is the user turn (no system message in array).
        assert_eq!(msgs[0]["role"], "user", "first message must be user");
        assert_eq!(msgs[0]["content"], "find a station near DM79");

        // 4. Second message is the assistant with a tool_use content block.
        let asst_msg = &msgs[1];
        assert_eq!(asst_msg["role"], "assistant");
        let asst_content = asst_msg["content"].as_array().expect("assistant content must be array");
        assert_eq!(asst_content.len(), 1);
        let tool_use_block = &asst_content[0];
        assert_eq!(tool_use_block["type"], "tool_use");
        let tool_use_id = tool_use_block["id"].as_str().expect("tool_use must have id");
        assert!(
            tool_use_id.starts_with("toulu_"),
            "tool_use id must start with 'toulu_'; got: {tool_use_id}"
        );
        // `input` must be a JSON OBJECT (not a string — the key difference from OpenAI).
        assert!(
            tool_use_block["input"].is_object(),
            "input must be a JSON object, not a string; got: {}",
            tool_use_block["input"]
        );
        assert_eq!(tool_use_block["input"]["grid"], "DM79");

        // 5. Third message is the user with a tool_result content block.
        let result_msg = &msgs[2];
        assert_eq!(result_msg["role"], "user");
        let result_content = result_msg["content"].as_array().expect("result content must be array");
        assert_eq!(result_content.len(), 1);
        let tool_result_block = &result_content[0];
        assert_eq!(tool_result_block["type"], "tool_result");
        let result_tool_use_id = tool_result_block["tool_use_id"].as_str().expect("tool_result must have tool_use_id");
        // THE KEY INVARIANT: tool_result.tool_use_id must match the tool_use.id.
        assert_eq!(
            result_tool_use_id,
            tool_use_id,
            "tool_result.tool_use_id ({result_tool_use_id}) must match tool_use.id ({tool_use_id})"
        );

        // 6. Tools array must use `input_schema`, NOT `parameters` or a `function` wrapper.
        let tools = body["tools"].as_array().expect("tools must be array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "find_stations");
        assert!(
            tools[0].get("input_schema").is_some(),
            "tools[0] must have `input_schema`; got: {}",
            tools[0]
        );
        assert!(
            tools[0].get("parameters").is_none(),
            "tools[0] must NOT have `parameters` (OpenAI shape); got: {}",
            tools[0]
        );
        assert!(
            tools[0].get("function").is_none(),
            "tools[0] must NOT have a `function` wrapper (OpenAI shape); got: {}",
            tools[0]
        );
    }

    // -----------------------------------------------------------------------
    // Response parsing tests
    // -----------------------------------------------------------------------

    /// A text block in the response maps to `ModelTurn::Text`.
    #[test]
    fn parse_text_response() {
        let recorded = json!({
            "id": "msg_01",
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "text", "text": "hello operator" }
            ],
            "model": "claude-haiku-4-5",
            "stop_reason": "end_turn"
        });
        assert_eq!(
            parse_messages_response(&recorded).unwrap(),
            ModelTurn::Text("hello operator".into())
        );
    }

    /// Multiple text blocks are concatenated into one `ModelTurn::Text`.
    #[test]
    fn parse_multiple_text_blocks_concatenated() {
        let recorded = json!({
            "content": [
                { "type": "text", "text": "Hello" },
                { "type": "text", "text": " world" }
            ]
        });
        assert_eq!(
            parse_messages_response(&recorded).unwrap(),
            ModelTurn::Text("Hello world".into())
        );
    }

    /// A tool_use block maps to `ModelTurn::ToolCalls` with the correct name
    /// and args. `input` is already a JSON object (no string-parse needed).
    #[test]
    fn parse_tool_use_response() {
        let recorded = json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toulu_0",
                    "name": "find_stations",
                    "input": { "grid": "DM79" }
                }
            ]
        });
        match parse_messages_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Multiple tool_use blocks → multiple ToolCalls.
    #[test]
    fn parse_multiple_tool_use_blocks() {
        let recorded = json!({
            "content": [
                { "type": "tool_use", "id": "toulu_0", "name": "a", "input": { "x": 1 } },
                { "type": "tool_use", "id": "toulu_1", "name": "b", "input": {} }
            ]
        });
        match parse_messages_response(&recorded).unwrap() {
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

    /// tool_use takes precedence over text blocks (mirrors OpenAI convention).
    #[test]
    fn tool_use_takes_precedence_over_text() {
        let recorded = json!({
            "content": [
                { "type": "text", "text": "some chatter" },
                { "type": "tool_use", "id": "toulu_0", "name": "x", "input": {} }
            ]
        });
        assert!(matches!(
            parse_messages_response(&recorded).unwrap(),
            ModelTurn::ToolCalls(_)
        ));
    }

    /// Missing `input` in a tool_use block → `Value::Null` args (COR-3 re-prompts).
    #[test]
    fn tool_use_missing_input_becomes_null_args() {
        let recorded = json!({
            "content": [
                { "type": "tool_use", "id": "toulu_0", "name": "echo" }
            ]
        });
        match parse_messages_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "echo");
                assert_eq!(calls[0].args, Value::Null);
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Empty content array → empty `Text` turn (not an error).
    #[test]
    fn empty_content_is_empty_text() {
        let recorded = json!({ "content": [] });
        assert_eq!(
            parse_messages_response(&recorded).unwrap(),
            ModelTurn::Text(String::new())
        );
    }

    /// Missing `content` → parse error.
    #[test]
    fn missing_content_is_error() {
        assert!(parse_messages_response(&json!({})).is_err());
    }

    // -----------------------------------------------------------------------
    // Provider selector test
    // -----------------------------------------------------------------------

    /// Selector returns true for an api.anthropic.com endpoint.
    #[test]
    fn is_anthropic_endpoint_true_for_anthropic() {
        assert!(
            is_anthropic_endpoint("https://api.anthropic.com/v1/messages"),
            "api.anthropic.com must be detected as Anthropic"
        );
    }

    /// Selector returns false for an OpenAI endpoint.
    #[test]
    fn is_anthropic_endpoint_false_for_openai() {
        assert!(
            !is_anthropic_endpoint("https://api.openai.com/v1/chat/completions"),
            "api.openai.com must NOT be detected as Anthropic"
        );
    }

    /// Selector returns false for a loopback endpoint.
    #[test]
    fn is_anthropic_endpoint_false_for_loopback() {
        assert!(
            !is_anthropic_endpoint("http://127.0.0.1:11434/v1/chat/completions"),
            "loopback must NOT be detected as Anthropic"
        );
    }

    // -----------------------------------------------------------------------
    // Step 4: Anthropic models-list parse
    // -----------------------------------------------------------------------

    /// `{"data":[{"id":"claude-haiku-4-5"},{"id":"claude-sonnet-5"}]}` parses
    /// to both model ids.
    #[test]
    fn parse_anthropic_models_list_two_models() {
        let body = r#"{"data":[{"id":"claude-haiku-4-5","display_name":"Claude Haiku 4.5"},{"id":"claude-sonnet-5","display_name":"Claude Sonnet 5"}],"has_more":false}"#;
        let ids = parse_anthropic_models_list(body).unwrap();
        assert!(
            ids.contains(&"claude-haiku-4-5".to_string()),
            "expected claude-haiku-4-5 in ids; got: {ids:?}"
        );
        assert!(
            ids.contains(&"claude-sonnet-5".to_string()),
            "expected claude-sonnet-5 in ids; got: {ids:?}"
        );
        assert_eq!(ids.len(), 2);
    }

    /// Empty `data` array → error (matches `map_models_response` ZeroModels).
    #[test]
    fn parse_anthropic_models_list_empty_data_errors() {
        let body = r#"{"data":[]}"#;
        assert!(parse_anthropic_models_list(body).is_err());
    }

    /// Missing `data` key → error.
    #[test]
    fn parse_anthropic_models_list_missing_data_errors() {
        let body = r#"{"models":[]}"#;
        assert!(parse_anthropic_models_list(body).is_err());
    }

    // -----------------------------------------------------------------------
    // Null args → empty object in input (no panic)
    // -----------------------------------------------------------------------

    /// A ToolCall with `Value::Null` args becomes `input: {}` in the wire body.
    #[test]
    fn null_args_become_empty_object_in_input() {
        let mut convo = Conversation::new("go");
        convo.push_tool_call(ToolCall::new("noop", Value::Null));
        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let msgs = body["messages"].as_array().unwrap();
        let asst = &msgs[1];
        let content = asst["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(
            content[0]["input"],
            json!({}),
            "null args must become empty object"
        );
    }

    // -----------------------------------------------------------------------
    // System prompt content sanity check
    // -----------------------------------------------------------------------

    /// The system prompt must contain key anchors (matches OpenAI provider tests).
    #[test]
    fn system_prompt_contains_key_anchors() {
        let convo = Conversation::new("where am I?");
        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let system = body["system"].as_str().unwrap_or("");
        assert!(system.contains("position_status"), "system must mention position_status");
        assert!(system.contains("operator"), "system must reference operator");
        assert!(
            system.contains("STAGE") && system.contains("ARMED") && system.contains("TAINTED"),
            "system must explain staging + armed send-authority + taint gate"
        );
    }

    /// A system-prompt OVERRIDE replaces the built-in default in the top-level
    /// `system` field (tuxlink-31tbw). Proves a stored override reaches the wire.
    #[test]
    fn system_prompt_override_replaces_default() {
        let convo = Conversation::new("where am I?");
        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[], None, "CUSTOM ELMER PROMPT");
        let system = body["system"].as_str().unwrap_or("");
        assert_eq!(
            system, "CUSTOM ELMER PROMPT",
            "the override must appear verbatim in the system field; got: {system:?}"
        );
        assert!(
            !system.contains("position_status"),
            "the built-in default must NOT be present when overridden; got: {system:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Temperature forwarding
    // -----------------------------------------------------------------------

    /// `temperature: Some(0.6)` must appear as a top-level JSON number in the
    /// Anthropic request body (Messages API placement).
    #[test]
    fn request_body_includes_temperature_when_some() {
        let convo = Conversation::new("hi");
        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[], Some(0.6_f32), ELMER_SYSTEM_PROMPT);
        let temp = body
            .get("temperature")
            .and_then(Value::as_f64)
            .expect("temperature must be present when Some");
        assert!(
            (temp - 0.6).abs() < 1e-6,
            "temperature must be ~0.6; got: {temp}"
        );
    }

    /// `temperature: None` must NOT add a `temperature` key to the body, so the
    /// Anthropic server default is left unchanged.
    #[test]
    fn request_body_omits_temperature_when_none() {
        let convo = Conversation::new("hi");
        let body = build_anthropic_request("claude-haiku-4-5", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        assert!(
            body.get("temperature").is_none(),
            "temperature must be absent when None; got: {body}"
        );
    }
}
