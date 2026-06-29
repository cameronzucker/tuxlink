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

/// A [`Provider`] that talks to an OpenAI-compatible chat-completions endpoint.
pub struct OpenAiProvider {
    client: reqwest::Client,
    /// Pre-validated (SEC-5) endpoint URL. Set ONCE from the CLI/config at
    /// construction; never mutated, and never sourced from a tool result.
    endpoint: Url,
    model: String,
    /// Optional bearer token (a local llama.cpp / Ollama shim usually needs
    /// none; an OpenAI-compatible proxy may). Read from env, never logged.
    api_key: Option<String>,
}

impl OpenAiProvider {
    /// Build the provider. `endpoint` MUST already have passed
    /// [`crate::endpoint::validate_endpoint`] — this constructor does not
    /// re-validate (the SEC-5 gate is the caller's single chokepoint), but it is
    /// only reachable from `main` after that gate.
    pub fn new(client: reqwest::Client, endpoint: Url, model: impl Into<String>, api_key: Option<String>) -> Self {
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
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            // Capture a bounded slice of the error body for the operator, but do
            // not let a huge body blow up the message.
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(500).collect();
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
