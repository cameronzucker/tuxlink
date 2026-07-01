//! `OllamaProvider` — a [`Provider`] backed by Ollama's NATIVE `/api/chat`
//! endpoint (loopback-only; tuxlink-65qhn).
//!
//! The local/loopback Elmer path historically ran Ollama through its
//! OpenAI-compat shim (`/v1/chat/completions`, [`crate::provider::OpenAiProvider`]).
//! That shim cannot express three things the local edge needs, all of which live
//! on the native `/api/chat`:
//!
//! * **`num_ctx`** — the compat path leaves the context window at Ollama's
//!   default (often 2048/4096), silently truncating agentic prompts and dropping
//!   tool definitions. This adapter sets it via `options.num_ctx`.
//! * **`keep_alive: 0`** — eager unload of the previous model on switch, to
//!   relieve memory pressure on edge hosts. Sent on EVERY request (D4).
//! * **Context-usage meter** — native `/api/chat` returns `prompt_eval_count`
//!   (full-prompt tokens) + `eval_count` (generated tokens) on the final
//!   response. Against the `num_ctx` this adapter set, that drives the
//!   fullness meter ([`RunEvent::ContextUsage`], T2).
//!
//! ## Wire format
//!
//! POST to the configured endpoint (`http://127.0.0.1:11434/api/chat`):
//!
//! ```json
//! {
//!   "model": "qwen3:8b",
//!   "messages": [ { "role": "system" | "user" | "assistant" | "tool", ... } ],
//!   "tools": [ { "type": "function", "function": { "name", "parameters" } } ],
//!   "stream": false,
//!   "keep_alive": 0,
//!   "options": { "num_ctx": 32768, "temperature": 0.7 }
//! }
//! ```
//!
//! [`build_ollama_request`] emits `stream: false` (its unit tests assert that
//! non-stream body shape); [`OllamaProvider::turn`] then overrides
//! `body["stream"] = true` on the already-built value so the wire request
//! streams — mirroring how [`crate::provider::OpenAiProvider::turn`] flips
//! `stream` on its built body without disturbing the pure builder. Streaming is
//! the STREAMING regression fix (tuxlink-b7tkf): the merged non-streaming path
//! showed nothing in the pane for minutes on slow CPU-local models, looking
//! hung; native `/api/chat` streams NDJSON deltas that this adapter emits as
//! [`RunEvent::AssistantDelta`] / [`RunEvent::ReasoningDelta`] as tokens land.
//! `keep_alive` is ALWAYS `0` (D4). The `options` object omits any key whose
//! config value is `None`, and is omitted ENTIRELY when both `num_ctx` and
//! `temperature` are `None` (so the default request shape stays minimal).
//! `tools` is omitted when the tool surface is empty (matching the OpenAI +
//! Anthropic adapters).
//!
//! ### Streaming wire format (NDJSON)
//!
//! With `stream: true` the server responds `application/x-ndjson`: one complete
//! JSON object per `\n`-terminated line (NO `data:` prefix, NO `[DONE]`
//! sentinel, NO blank-line frame delimiter — unlike SSE). A streaming chunk is
//! `{"message":{"role":"assistant","content":"<delta>","thinking":"<reasoning
//! delta>"?,"tool_calls":[...]?},"done":false}`; the final chunk is
//! `{"message":{...},"done":true,"prompt_eval_count":N,"eval_count":N,...}`.
//! Unlike the OpenAI SSE path, native `tool_calls` arrive COMPLETE per chunk
//! (`function.arguments` is a JSON object, not fragmented arguments-strings), so
//! no partial-tool-call reassembly is needed. A server that ignores
//! `stream: true` and answers a single `application/json` document is handled by
//! the non-stream fallback in [`OllamaProvider::turn`] via
//! [`parse_ollama_response`], mirroring the OpenAI adapter's content-type
//! fallback.
//!
//! ### Tool-call protocol (native, NOT OpenAI-compat)
//!
//! The two shapes differ from OpenAI in ways this adapter handles:
//!
//! * **Assistant tool call → in the transcript**: rendered as an `assistant`
//!   message carrying a `tool_calls` array whose `function.arguments` is a JSON
//!   **OBJECT** (OpenAI uses a JSON *string*).
//! * **Tool result → back to the model**: a `tool`-role message with a
//!   `tool_name` field naming the tool (OpenAI uses a `tool_call_id`). Ollama
//!   pairs results to calls by ORDER + name, so — unlike the OpenAI adapter —
//!   NO synthetic `tool_call_id`s are minted on the wire. The runner appends a
//!   `ToolCall` immediately followed by its `ToolResult`, so FIFO order on the
//!   wire IS the call order; rendering each message in transcript order is all
//!   the pairing Ollama needs.
//! * **Response tool call**: `message.tool_calls[].function.name` (string) +
//!   `function.arguments` (an OBJECT — parsed directly, no string-parse; this is
//!   the key divergence from OpenAI and the failure mode the parser guards).
//!
//! ## Response mapping
//!
//! * `message.tool_calls` present and non-empty → [`ModelTurn::ToolCalls`].
//!   `arguments` is already a JSON object; if absent it becomes `Value::Null` so
//!   the runner's COR-3 schema check treats it as malformed and re-prompts
//!   (identical to the OpenAI / Anthropic null-args policy — never silently
//!   fabricated).
//! * Otherwise `message.content` → [`ModelTurn::Text`] (empty string when the
//!   model returned an empty/absent content with no tool calls).
//!
//! ## Security
//!
//! Endpoint vetting (SEC-5 / SSRF-1) is the caller's responsibility — the same
//! `validate_endpoint` → `build_vetted_client` gate as the sibling adapters. A
//! loopback Ollama needs no bearer token, but the constructor accepts an optional
//! [`ApiKey`] for symmetry and sends `Authorization: Bearer …` when present;
//! the key is stored as [`ApiKey`] and never appears in `Debug`/`Display`. Error
//! bodies are scrubbed via [`redact_and_cap`] before propagation.

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use url::Url;

use tuxlink_agent_runner::{
    Conversation, Message, ModelTurn, Provider, ProviderError, RunEvent, ToolCall, ToolSpec,
};

use crate::provider::{redact_and_cap, ApiKey, ELMER_SYSTEM_PROMPT};

/// A [`Provider`] that talks to Ollama's native `/api/chat` endpoint.
///
/// Constructed via [`OllamaProvider::new`] after the endpoint has been validated
/// through `validate_endpoint` / `build_vetted_client` (SEC-5). Only reachable
/// from the loopback constructor (T4's probe-with-fallback selection).
pub struct OllamaProvider {
    client: reqwest::Client,
    /// Pre-validated (SEC-5) endpoint URL — the `/api/chat` path.
    endpoint: Url,
    model: String,
    /// Context window to request via `options.num_ctx`. `None` leaves it at the
    /// Ollama server default (and suppresses the context meter, whose denominator
    /// only exists when this adapter set the window).
    num_ctx: Option<u32>,
    /// Sampling temperature via `options.temperature`. `None` leaves it at the
    /// server default.
    temperature: Option<f32>,
    /// Optional operator-supplied system-prompt override (tuxlink-31tbw). When
    /// `Some`, it replaces [`ELMER_SYSTEM_PROMPT`] as the `role: system` message;
    /// when `None`, the built-in default is used. Threaded from the model-config
    /// snapshot by T4 so a stored override reaches the wire.
    system_prompt: Option<String>,
    /// Optional bearer token. A loopback Ollama needs none; accepted for
    /// symmetry with the sibling adapters. Stored as [`ApiKey`] so it never
    /// leaks through `Debug`/`Display`; only used via `.expose()` at the HTTP
    /// header boundary.
    api_key: Option<ApiKey>,
}

impl OllamaProvider {
    /// Build the provider. `endpoint` MUST already have passed
    /// [`crate::endpoint::validate_endpoint`] — this constructor does not
    /// re-validate (the SEC-5 gate is the caller's single chokepoint).
    ///
    /// `system_prompt` is the operator override (tuxlink-31tbw); `None` uses the
    /// built-in [`ELMER_SYSTEM_PROMPT`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: reqwest::Client,
        endpoint: Url,
        model: impl Into<String>,
        num_ctx: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<String>,
        api_key: Option<ApiKey>,
    ) -> Self {
        Self {
            client,
            endpoint,
            model: model.into(),
            num_ctx,
            temperature,
            system_prompt,
            api_key,
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<ModelTurn, ProviderError> {
        // Streaming (tuxlink-b7tkf): request a streamed NDJSON completion and
        // emit RunEvent deltas as tokens arrive. `on_event` is FIRE-AND-FORGET —
        // what it does never changes which `ModelTurn` this returns. We set
        // `stream` on the already-built request value rather than threading a
        // flag through `build_ollama_request`, whose tests assert the non-stream
        // body shape (mirroring the OpenAI adapter's approach). The pure assembly
        // stays untouched.
        let mut body = build_ollama_request(
            &self.model,
            conversation,
            tools,
            self.num_ctx,
            self.temperature,
            self.system_prompt.as_deref().unwrap_or(ELMER_SYSTEM_PROMPT),
        );
        body["stream"] = json!(true);

        let mut req = self
            .client
            .post(self.endpoint.clone())
            .header("content-type", "application/json")
            .json(&body);

        // A loopback Ollama needs no auth; send a bearer only if one is set.
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            // Scrub any bearer key BEFORE capping (a key straddling the cap
            // boundary would otherwise leak a prefix); `redact_and_cap` enforces
            // that order.
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

        // Non-streaming fallback: some servers (or a llama.cpp shim behind the
        // same loopback port) ignore `stream: true` and answer with a single
        // JSON document. Detect that by content-type — a native stream advertises
        // `application/x-ndjson`; anything else (typically `application/json`) is
        // a whole response we parse via `parse_ollama_response`, emitting no
        // deltas. This mirrors the OpenAI adapter's `is_event_stream` fallback.
        let is_ndjson = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.to_ascii_lowercase().contains("application/x-ndjson"))
            .unwrap_or(false);

        if !is_ndjson {
            let value: Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Unparseable(format!("response was not JSON: {e}")))?;

            // Same ContextUsage emit as the streaming path below: fire-and-forget
            // when the model reported counts AND this adapter set a known
            // `num_ctx` (the denominator). Absent counts or a server-default
            // window simply produces no event — the meter stays hidden (T2).
            if let (Some((prompt_tokens, eval_tokens)), Some(num_ctx)) =
                (parse_ollama_counts(&value), self.num_ctx)
            {
                on_event(RunEvent::ContextUsage {
                    prompt_tokens,
                    eval_tokens,
                    num_ctx,
                });
            }

            return parse_ollama_response(&value).map_err(ProviderError::Unparseable);
        }

        // Streaming path. The whole read is a SINGLE awaited future inside `turn`
        // — no detached `tokio::spawn`. The run loop races `turn()` against a
        // cancel token and DROPS this future on cancel; keeping the byte-stream
        // read inline means that drop aborts the in-flight reqwest stream rather
        // than leaking a background task that keeps the connection open.
        let mut stream = resp.bytes_stream();
        let mut acc = OllamaStreamAccumulator::new();

        while let Some(item) = stream.next().await {
            let chunk = item
                .map_err(|e| ProviderError::Transport(format!("stream read failed: {e}")))?;
            // `feed` parses every complete NDJSON line currently in the buffer and
            // invokes `on_event` for each delta. A `done: true` line ends the
            // stream; otherwise we keep reading until the body closes. It returns
            // a transport error if the endpoint streams an oversized un-terminated
            // line or breaches the total-output cap.
            if acc.feed(&chunk, on_event)? {
                break;
            }
        }
        // Flush any trailing line the server sent without a terminating newline
        // before closing the connection (rare, but lenient parsing avoids
        // dropping the final delta or the counts on the `done` line).
        acc.finish(on_event)?;

        // Emit the context-usage meter event (fire-and-forget) when the model
        // reported token counts on the `done` line AND this adapter set a known
        // `num_ctx` (the denominator) — same contract as the non-stream path.
        if let (Some((prompt_tokens, eval_tokens)), Some(num_ctx)) =
            (acc.counts(), self.num_ctx)
        {
            on_event(RunEvent::ContextUsage {
                prompt_tokens,
                eval_tokens,
                num_ctx,
            });
        }

        Ok(acc.into_turn())
    }
}

// ---------------------------------------------------------------------------
// NDJSON streaming accumulator (pure, unit-testable)
// ---------------------------------------------------------------------------

/// Maximum size, in bytes, of a single un-terminated NDJSON line held in `buf`
/// while waiting for its closing `\n`. A legitimate `/api/chat` chunk is one
/// small JSON object (a token delta or a complete tool call); 1 MiB is orders of
/// magnitude beyond any real line, so a `buf` that grows past this without a
/// newline signals a hostile or broken endpoint streaming an unbounded
/// un-terminated line. Exceeding it is a transport error rather than letting
/// memory grow until the per-turn timeout. Defined file-local rather than reusing
/// the OpenAI adapter's private `MAX_PENDING_FRAME_BYTES` (not visible across the
/// module boundary), with the same rationale.
const MAX_PENDING_LINE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Maximum total decoded output, in bytes, accumulated across `content` +
/// `thinking`. A complete answer-plus-reasoning trace for any legitimate model
/// turn fits comfortably under this; 16 MiB bounds an endpoint that streams
/// endless small deltas (which individually terminate their lines, so
/// `MAX_PENDING_LINE_BYTES` would not catch them) from exhausting memory before
/// the configured timeout. Exceeding it is a transport error. File-local twin of
/// the OpenAI adapter's private `MAX_TOTAL_OUTPUT_BYTES`.
const MAX_TOTAL_OUTPUT_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

/// Accumulates a native Ollama `/api/chat` NDJSON stream into a [`ModelTurn`],
/// emitting [`RunEvent`] deltas through a caller-supplied sink as chunks land.
///
/// This is the NDJSON analogue of [`crate::provider`]'s SSE accumulator and the
/// testable seam: byte chunks from the network are appended via
/// [`OllamaStreamAccumulator::feed`], which buffers across arbitrary line
/// boundaries, splits complete `\n`-terminated lines, parses each as a JSON
/// object, and routes it through [`OllamaStreamAccumulator::apply_line`]. No IO
/// lives here — tests drive it with hand-built byte slices (including mid-line
/// and mid-codepoint splits) and a recording sink, with no live server.
///
/// Unlike the OpenAI SSE path there is NO partial-tool-call reassembly: native
/// `tool_calls` arrive COMPLETE per chunk (`function.arguments` is a JSON
/// object), so each is collected via the shared [`parse_ollama_tool_call`] as
/// chunks bring it.
struct OllamaStreamAccumulator {
    /// Raw bytes received but not yet forming a complete `\n`-terminated line.
    /// Buffered as BYTES (not a lossy `String`) so a multi-byte UTF-8 codepoint
    /// split across two network chunks is reassembled intact rather than each
    /// half decoding to a U+FFFD replacement char.
    buf: Vec<u8>,
    /// Accumulated answer content (concatenated `message.content` deltas).
    content: String,
    /// Accumulated reasoning (concatenated `message.thinking` deltas). Reasoning
    /// is emitted to the caller delta-by-delta as it streams; no `ModelTurn`
    /// variant carries the assembled trace, so in non-test builds this field is
    /// write-only (read only by the `#[cfg(test)]` `thinking()` accessor that
    /// asserts the concatenation). Kept because the streaming contract specifies
    /// accumulating reasoning alongside emitting it, and a future caller may want
    /// the full trace. Mirrors the SSE accumulator's `reasoning` field exactly.
    #[cfg_attr(not(test), allow(dead_code))]
    thinking: String,
    /// Tool calls collected across chunks, in arrival order. Native tool calls
    /// are complete per chunk (no fragment reassembly), so this is a plain `Vec`.
    tool_calls: Vec<ToolCall>,
    /// Running byte tally of accumulated tool calls (name + serialized args),
    /// folded into [`Self::total_output_len`] so tool-call growth is bounded by
    /// the same [`MAX_TOTAL_OUTPUT_BYTES`] cap as content/thinking. Without it a
    /// stream of endless or oversized `tool_calls` (which never touch content or
    /// thinking) would grow memory unbounded past the caps.
    tool_calls_bytes: usize,
    /// Set once a line with top-level `done: true` is seen.
    done: bool,
    /// `prompt_eval_count` from the `done` line, when present.
    prompt_eval_count: Option<u32>,
    /// `eval_count` from the `done` line, when present.
    eval_count: Option<u32>,
}

impl OllamaStreamAccumulator {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            content: String::new(),
            thinking: String::new(),
            tool_calls: Vec::new(),
            tool_calls_bytes: 0,
            done: false,
            prompt_eval_count: None,
            eval_count: None,
        }
    }

    /// Append a network byte chunk and process every COMPLETE `\n`-terminated
    /// line now in the buffer. Returns `Ok(true)` once the stream has terminated
    /// (a line with `done: true` was seen) so the caller can stop reading;
    /// `Ok(false)` while more data is expected.
    ///
    /// Bytes are buffered RAW and lines are split at the byte level, so a
    /// multi-byte UTF-8 codepoint straddling a network-chunk boundary is
    /// reassembled into a complete line before being decoded — no U+FFFD
    /// corruption. Each complete line is then decoded with
    /// [`std::str::from_utf8`]; a line that is genuinely not valid UTF-8 (should
    /// not happen for a complete line from a conforming server) is skipped rather
    /// than erroring the turn.
    ///
    /// Returns `Err(ProviderError::Transport)` when an un-terminated line in
    /// `buf` exceeds [`MAX_PENDING_LINE_BYTES`], or when total accumulated output
    /// exceeds [`MAX_TOTAL_OUTPUT_BYTES`] — bounding memory against a hostile or
    /// broken endpoint. The error message carries no body/secret content.
    fn feed(
        &mut self,
        bytes: &[u8],
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<bool, ProviderError> {
        self.buf.extend_from_slice(bytes);

        // Lines are separated by a single `\n`. Scan the BYTE buffer for the next
        // newline, taking the line body before it, so the codepoint-reassembly
        // guarantee holds (we never decode a partial codepoint at a chunk
        // boundary).
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            // Move out the line bytes plus its newline; the remainder is a
            // (possibly partial) next line kept for the following chunk.
            let line_bytes: Vec<u8> = self.buf.drain(..pos + 1).collect();
            // Decode only the line body (newline excluded). A complete line is
            // valid UTF-8 since the only split was at a network-chunk boundary; on
            // the rare genuine decode error, skip this line rather than panicking.
            let Ok(line) = std::str::from_utf8(&line_bytes[..pos]) else {
                continue;
            };
            if self.apply_line(line, on_event)? {
                self.done = true;
                return Ok(true);
            }
        }

        // No complete line remains; what's left is a single pending line. Guard
        // its size so an endpoint that never sends a newline cannot grow `buf`
        // without bound.
        if self.buf.len() > MAX_PENDING_LINE_BYTES {
            return Err(ProviderError::Transport(
                "model stream sent an oversized un-terminated line".to_string(),
            ));
        }
        Ok(false)
    }

    /// Process whatever remains in the buffer as a final line once the network
    /// stream has closed. A well-behaved server terminates every line with a
    /// newline, so this is usually a no-op; it exists so a trailing line sent
    /// without the closing newline (which may be the `done` line carrying the
    /// counts) is not silently dropped. Returns `Err(ProviderError::Transport)`
    /// if applying the trailing line breaches [`MAX_TOTAL_OUTPUT_BYTES`].
    fn finish(&mut self, on_event: &(dyn Fn(RunEvent) + Sync)) -> Result<(), ProviderError> {
        if self.done {
            return Ok(());
        }
        let line_bytes = std::mem::take(&mut self.buf);
        // Decode leniently for the trailing-line flush; a malformed tail is
        // dropped rather than erroring the turn.
        let Ok(line) = std::str::from_utf8(&line_bytes) else {
            return Ok(());
        };
        if !line.trim().is_empty() {
            self.apply_line(line, on_event)?;
        }
        Ok(())
    }

    /// Total accumulated decoded output so far (`content` + `thinking` +
    /// serialized `tool_calls`), in bytes. Used to enforce
    /// [`MAX_TOTAL_OUTPUT_BYTES`] across ALL accumulation paths so no single
    /// channel (including tool calls) can grow memory unbounded.
    fn total_output_len(&self) -> usize {
        self.content.len() + self.thinking.len() + self.tool_calls_bytes
    }

    /// Parse one NDJSON line as a JSON object and apply it: append + emit answer
    /// content and reasoning deltas, collect any complete tool calls, and capture
    /// the token counts on the final `done` line. Returns `Ok(true)` if the line
    /// carried top-level `done: true` (stream terminates); `Err` if appending a
    /// delta breached [`MAX_TOTAL_OUTPUT_BYTES`].
    ///
    /// A blank line or a line that is not parseable JSON is ignored (a
    /// conforming server sends one complete object per line, but leniency avoids
    /// failing the whole turn on a stray keep-alive newline).
    fn apply_line(
        &mut self,
        line: &str,
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<bool, ProviderError> {
        if line.trim().is_empty() {
            return Ok(false);
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Ok(false);
        };

        if let Some(message) = value.get("message") {
            // Answer content delta.
            if let Some(text) = message.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    if self.total_output_len() + text.len() > MAX_TOTAL_OUTPUT_BYTES {
                        return Err(ProviderError::Transport(
                            "model stream exceeded the maximum accumulated output size".to_string(),
                        ));
                    }
                    self.content.push_str(text);
                    on_event(RunEvent::AssistantDelta {
                        chunk: text.to_string(),
                    });
                }
            }

            // Reasoning / "thinking" delta (qwen3 and other thinking models).
            if let Some(text) = message.get("thinking").and_then(Value::as_str) {
                if !text.is_empty() {
                    if self.total_output_len() + text.len() > MAX_TOTAL_OUTPUT_BYTES {
                        return Err(ProviderError::Transport(
                            "model stream exceeded the maximum accumulated output size".to_string(),
                        ));
                    }
                    self.thinking.push_str(text);
                    on_event(RunEvent::ReasoningDelta {
                        chunk: text.to_string(),
                    });
                }
            }

            // Native tool calls arrive COMPLETE per chunk (function.arguments is
            // a JSON object) — no fragment reassembly. Collect each via the shared
            // parser as chunks bring them.
            if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let tc = parse_ollama_tool_call(call);
                    // Bound accumulated tool-call bytes under the same total-output
                    // cap so an endless/oversized tool_calls stream (which never
                    // touches content/thinking) cannot grow memory without bound.
                    let sz = tc.name.len() + tc.args.to_string().len();
                    if self.total_output_len() + sz > MAX_TOTAL_OUTPUT_BYTES {
                        return Err(ProviderError::Transport(
                            "model stream exceeded the maximum accumulated output size".to_string(),
                        ));
                    }
                    self.tool_calls_bytes += sz;
                    self.tool_calls.push(tc);
                }
            }
        }

        // Final chunk: capture the token counts and signal termination.
        if value.get("done").and_then(Value::as_bool) == Some(true) {
            if let Some((prompt, eval)) = parse_ollama_counts(&value) {
                self.prompt_eval_count = Some(prompt);
                self.eval_count = Some(eval);
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// The token counts captured from the `done` line, when both were present.
    /// Mirrors [`parse_ollama_counts`]'s all-or-nothing contract.
    fn counts(&self) -> Option<(u32, u32)> {
        Some((self.prompt_eval_count?, self.eval_count?))
    }

    /// Finalize into a [`ModelTurn`]. If any tool calls were collected, they win
    /// (mirroring [`parse_ollama_response`]'s precedence); otherwise the
    /// concatenated content becomes a `Text` turn.
    fn into_turn(self) -> ModelTurn {
        if !self.tool_calls.is_empty() {
            return ModelTurn::ToolCalls(self.tool_calls);
        }
        ModelTurn::Text(self.content)
    }

    /// The reasoning text accumulated across `ReasoningDelta` fragments. The
    /// finalized [`ModelTurn`] never carries reasoning (it streams delta-only via
    /// the sink), but the concatenated trace is retained so a test can assert the
    /// fragments were concatenated in order. Mirrors the SSE accumulator's
    /// `reasoning()` accessor.
    #[cfg(test)]
    fn thinking(&self) -> &str {
        &self.thinking
    }
}

// ---------------------------------------------------------------------------
// Pure request assembly
// ---------------------------------------------------------------------------

/// Build the native Ollama `/api/chat` request body from the transcript + tool
/// surface + the config-supplied `num_ctx` / `temperature`.
///
/// Pure — no IO. Exported so Rust tests can drive it directly.
///
/// Invariants:
/// * `stream` is ALWAYS `false` (D2).
/// * `keep_alive` is ALWAYS `0` — eager unload of the previous model (D4).
/// * `options` omits any `None` key, and is omitted ENTIRELY when both
///   `num_ctx` and `temperature` are `None`.
/// * `tools` is omitted when the surface is empty (matching the sibling
///   adapters).
/// * Tool call/result messages render FIFO in transcript order; Ollama pairs by
///   order + `tool_name`, so no synthetic ids are needed on the wire.
pub fn build_ollama_request(
    model: &str,
    conversation: &Conversation,
    tools: &[ToolSpec],
    num_ctx: Option<u32>,
    temperature: Option<f32>,
    system_prompt: &str,
) -> Value {
    // The system prompt is a message (role: system) on the native path, same as
    // the OpenAI-compat adapter (Anthropic is the outlier that hoists it to a
    // top-level field). `system_prompt` is the effective prompt (operator
    // override or the built-in default, resolved by the caller). Prepend it,
    // then render the transcript in order.
    let mut messages: Vec<Value> =
        Vec::with_capacity(conversation.messages().len() + 1);
    messages.push(json!({ "role": "system", "content": system_prompt }));
    for msg in conversation.messages() {
        messages.push(render_ollama_message(msg));
    }

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": false,
        // D4: always unload the previous model on switch. `0` (an integer) is
        // Ollama's "unload immediately after this request" sentinel.
        "keep_alive": 0,
    });

    // Only include `tools` when there is a surface — an empty array is not
    // helpful and some servers object to it.
    if !tools.is_empty() {
        let tool_entries: Vec<Value> = tools
            .iter()
            .map(|t| {
                // Mirror the OpenAI adapter's `ToolFunction`: `name` + `parameters`
                // only. `ToolSpec` carries no description, and a synthetic
                // "Tool: <name>" placeholder is strictly worse than none (burns
                // tokens, tells the model nothing). Any real description the tool
                // schema carries rides inside `parameters` (its top-level
                // `description`), which the model still sees.
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "parameters": t.json_schema,
                    }
                })
            })
            .collect();
        body["tools"] = Value::Array(tool_entries);
    }

    // Assemble `options` from the set knobs; omit the object entirely when both
    // are None so the default request shape carries no `options` key.
    let mut options = serde_json::Map::new();
    if let Some(ctx) = num_ctx {
        options.insert("num_ctx".to_string(), json!(ctx));
    }
    if let Some(temp) = temperature {
        options.insert("temperature".to_string(), json!(temp));
    }
    if !options.is_empty() {
        body["options"] = Value::Object(options);
    }

    body
}

/// Render one transcript [`Message`] into a native `/api/chat` messages-array
/// entry.
///
/// Exhaustive over every [`Message`] variant. Unlike the OpenAI adapter (which
/// mints synthetic `tool_call_id`s and therefore must render tool messages in a
/// stateful loop), Ollama pairs tool results to calls by ORDER + `tool_name`, so
/// each message renders independently and in-order — no counter, no
/// unreachable arm.
///
/// * `User` / `Assistant` → the corresponding role with a plain `content` string.
/// * `ToolCall` → an `assistant` message carrying a `tool_calls` array whose
///   `function.arguments` is a JSON **OBJECT** (null args become `{}` so the
///   wire value is always an object).
/// * `ToolResult` → a `tool`-role message with `tool_name` naming the tool and
///   the (ok/error-labelled) result as `content`.
fn render_ollama_message(msg: &Message) -> Value {
    match msg {
        Message::User(text) => json!({ "role": "user", "content": text }),
        Message::Assistant(text) => json!({ "role": "assistant", "content": text }),
        Message::ToolCall(call) => {
            // `arguments` is a JSON OBJECT on the native wire (NOT a string as in
            // OpenAI). Null / non-object args become an empty object so the shape
            // stays valid.
            let arguments = match &call.args {
                Value::Object(_) => call.args.clone(),
                Value::Null => json!({}),
                other => json!({ "value": other }),
            };
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": call.name,
                        "arguments": arguments,
                    }
                }]
            })
        }
        Message::ToolResult { name, ok, content } => {
            let label = if *ok { "result" } else { "error" };
            json!({
                "role": "tool",
                "tool_name": name,
                "content": format!("[{label}] {content}"),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Pure response mapping
// ---------------------------------------------------------------------------

/// Map a native Ollama `/api/chat` response JSON onto a [`ModelTurn`]. Pure.
///
/// Returns `Err(detail)` only when the response is structurally unusable (no
/// `message` object). A present-but-empty `content` with no tool calls maps to
/// an empty `Text` turn rather than an error, so the loop can surface it —
/// mirroring [`crate::provider::parse_completion`]'s contract.
///
/// Tool calls take precedence over content. Each call's `function.arguments` is
/// already a JSON object on the native wire; an absent/missing `arguments`
/// becomes `Value::Null` so the runner's COR-3 schema check treats it as
/// malformed and re-prompts (never silently fabricated).
pub fn parse_ollama_response(value: &Value) -> Result<ModelTurn, String> {
    let message = value
        .get("message")
        .ok_or_else(|| "response had no message object".to_string())?;

    // Tool calls take precedence over content.
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        if !tool_calls.is_empty() {
            let calls: Vec<ToolCall> = tool_calls.iter().map(parse_ollama_tool_call).collect();
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

/// Parse a single native Ollama tool-call object into a [`ToolCall`].
///
/// `function.arguments` is normally a JSON OBJECT on the native `/api/chat`
/// wire. However, some Ollama models and quants (especially smaller/quantized
/// variants) emit `function.arguments` as a JSON **STRING** even on the native
/// path — e.g. `"{\"grid\":\"DM79\"}"`. Without handling, a string value passes
/// through as `Value::String`, the runner's schema check rejects it (root is not
/// an object), and the agent enters a malformed-retry loop (COR-3).
///
/// Fix C tolerance (mirrors the OpenAI adapter):
///
/// * `arguments` is a JSON **object** → use as-is (the normal case).
/// * `arguments` is a JSON **string** → attempt `serde_json::from_str`. On
///   success, use the parsed value; on failure, fall back to `Value::Null` so
///   COR-3 re-prompts (malformed → re-prompt, never silently fabricated).
/// * `arguments` is **absent** → `Value::Null` (COR-3 re-prompts).
/// * `arguments` is any other JSON type (number, bool, array) → pass through
///   as-is; the runner's schema check will catch it and COR-3 re-prompts.
fn parse_ollama_tool_call(tc: &Value) -> ToolCall {
    let function = tc.get("function");
    let name = function
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let raw_args = function.and_then(|f| f.get("arguments"));
    let args = match raw_args {
        // Normal case: already a JSON object — use verbatim.
        Some(v @ Value::Object(_)) => v.clone(),
        // String-encoded arguments: some Ollama quants serialize the object as
        // a JSON string. Attempt to parse; on failure fall to Null so COR-3
        // re-prompts rather than letting a malformed value propagate.
        Some(Value::String(s)) => {
            match serde_json::from_str::<Value>(s) {
                Ok(parsed) => parsed,
                Err(_) => Value::Null,
            }
        }
        // Absent: Null → COR-3 re-prompts.
        None => Value::Null,
        // Any other type (number, bool, array): pass through; runner rejects.
        Some(other) => other.clone(),
    };

    ToolCall { name, args }
}

/// Extract the context-usage counts from a native `/api/chat` response.
///
/// Returns `Some((prompt_eval_count, eval_count))` when BOTH top-level counts
/// are present as numbers, else `None` (some models / early responses omit them).
/// Pure — no IO. Exposed so [`OllamaProvider::turn`] and tests share one reader.
pub fn parse_ollama_counts(value: &Value) -> Option<(u32, u32)> {
    let prompt = value.get("prompt_eval_count").and_then(Value::as_u64)?;
    let eval = value.get("eval_count").and_then(Value::as_u64)?;
    Some((prompt as u32, eval as u32))
}

// ---------------------------------------------------------------------------
// Endpoint heuristic (light — the authoritative discrimination is T4's probe)
// ---------------------------------------------------------------------------

/// A LIGHT heuristic for "does this endpoint look like native Ollama?".
///
/// This is NOT the authoritative discriminator. Loopback Ollama-vs-llama.cpp
/// selection is decided at construction by T4's runtime probe of
/// `GET {origin}/api/tags` (200 + parseable ⇒ native), because a URL alone
/// cannot tell an Ollama server from a llama.cpp OpenAI-compat server on the
/// same loopback port. This helper only answers the cheap textual question —
/// the path already points at the native chat route, or the host is loopback —
/// for callers that want a pre-probe hint. Pure — no IO.
///
/// Takes `&str` (not `&Url`) so callers in crates that do not depend on the
/// `url` crate can use it without importing `url::Url`; parsing happens here.
pub fn is_ollama_endpoint(endpoint: &str) -> bool {
    // Path explicitly names the native chat route.
    if endpoint.contains("/api/chat") {
        return true;
    }
    // Otherwise a loopback host is a candidate (the probe decides for real).
    Url::parse(endpoint)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| {
                h.eq_ignore_ascii_case("localhost")
                    || h == "127.0.0.1"
                    || h == "::1"
                    || h == "[::1]"
            })
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

    fn echo_tool() -> ToolSpec {
        ToolSpec::new(
            "echo",
            json!({ "type": "object", "properties": { "msg": { "type": "string" } } }),
        )
    }

    // -----------------------------------------------------------------------
    // build_ollama_request — wire shape
    // -----------------------------------------------------------------------

    /// `keep_alive: 0` and `stream: false` are present on EVERY request, and the
    /// first message is the Elmer system prompt (D2 + D4).
    #[test]
    fn request_always_has_keep_alive_zero_stream_false_and_system_prompt() {
        let convo = Conversation::new("where am I?");
        let body = build_ollama_request("qwen3:8b", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);

        // D4: keep_alive is always the integer 0.
        assert_eq!(
            body.get("keep_alive").and_then(Value::as_i64),
            Some(0),
            "keep_alive must always be 0 (eager unload); got: {body}"
        );
        // D2: non-streaming.
        assert_eq!(
            body.get("stream").and_then(Value::as_bool),
            Some(false),
            "stream must always be false (non-streaming v1); got: {body}"
        );
        assert_eq!(body["model"], "qwen3:8b");

        // messages[0] is the system prompt; the first user message follows it.
        assert_eq!(body["messages"][0]["role"], "system");
        let system = body["messages"][0]["content"].as_str().unwrap_or("");
        assert!(
            system.contains("position_status"),
            "system prompt must mention position_status; got: {system:?}"
        );
        assert!(
            system.contains("STAGE") && system.contains("ARMED send authority"),
            "system prompt must explain staging + Arm-to-send; got: {system:?}"
        );
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "where am I?");
    }

    /// A system-prompt OVERRIDE replaces the built-in default as messages[0]
    /// (tuxlink-31tbw). This is the end-to-end proof that a stored override
    /// reaches the wire, not just the config store.
    #[test]
    fn request_system_prompt_override_replaces_default() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], None, None, "CUSTOM ELMER PROMPT");
        assert_eq!(body["messages"][0]["role"], "system");
        let system = body["messages"][0]["content"].as_str().unwrap_or("");
        assert_eq!(
            system, "CUSTOM ELMER PROMPT",
            "the override must appear verbatim as the system message; got: {system:?}"
        );
        assert!(
            !system.contains("position_status"),
            "the built-in default must NOT be present when overridden; got: {system:?}"
        );
    }

    /// Passing `ELMER_SYSTEM_PROMPT` (the `None` → default resolution) yields the
    /// built-in prompt, unchanged from the pre-override behavior.
    #[test]
    fn request_default_system_prompt_when_not_overridden() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);
        let system = body["messages"][0]["content"].as_str().unwrap_or("");
        assert!(
            system.contains("position_status") && system.contains("ARMED send authority"),
            "the built-in default must be used when not overridden; got: {system:?}"
        );
    }

    /// When both `num_ctx` and `temperature` are set, `options` carries both.
    #[test]
    fn request_options_carries_num_ctx_and_temperature_when_set() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], Some(32_768), Some(0.7), ELMER_SYSTEM_PROMPT);
        let options = body.get("options").expect("options must be present");
        assert_eq!(
            options.get("num_ctx").and_then(Value::as_u64),
            Some(32_768),
            "options.num_ctx must be set; got: {options}"
        );
        // temperature round-trips as a float ~0.7.
        let temp = options
            .get("temperature")
            .and_then(Value::as_f64)
            .expect("options.temperature must be present");
        assert!(
            (temp - 0.7).abs() < 1e-6,
            "options.temperature must be ~0.7; got: {temp}"
        );
    }

    /// `options` omits `temperature` when only `num_ctx` is set (per-key omit).
    #[test]
    fn request_options_omits_unset_key() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], Some(8192), None, ELMER_SYSTEM_PROMPT);
        let options = body.get("options").expect("options must be present");
        assert_eq!(options.get("num_ctx").and_then(Value::as_u64), Some(8192));
        assert!(
            options.get("temperature").is_none(),
            "options must omit temperature when it is None; got: {options}"
        );
    }

    /// `options` is omitted ENTIRELY when both knobs are None.
    #[test]
    fn request_omits_options_when_both_none() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);
        assert!(
            body.get("options").is_none(),
            "options must be absent when both num_ctx and temperature are None; got: {body}"
        );
    }

    /// Tools serialize in the native `{type:function, function:{name,parameters}}`
    /// shape (name + parameters only, mirroring the OpenAI adapter), and `tools`
    /// is omitted when the surface is empty.
    #[test]
    fn request_serializes_tools_in_native_shape() {
        let convo = Conversation::new("find a station near DM79");
        let body = build_ollama_request("m", &convo, &[echo_tool()], None, None, ELMER_SYSTEM_PROMPT);
        let tools = body["tools"].as_array().expect("tools must be an array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "echo");
        // Schema is passed through verbatim as `parameters` (NOT `input_schema`).
        assert_eq!(tools[0]["function"]["parameters"]["type"], "object");
        assert!(
            tools[0]["function"].get("input_schema").is_none(),
            "native shape uses `parameters`, not `input_schema`; got: {}",
            tools[0]
        );
    }

    #[test]
    fn request_omits_tools_when_none() {
        let convo = Conversation::new("hi");
        let body = build_ollama_request("m", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);
        assert!(body.get("tools").is_none(), "tools should be absent: {body}");
    }

    /// A ToolCall renders as an assistant message whose `function.arguments` is a
    /// JSON OBJECT (the key divergence from OpenAI's string arguments), and a
    /// ToolResult renders as a `tool`-role message with a `tool_name` (no
    /// tool_call_id). FIFO order is transcript order.
    #[test]
    fn request_tool_call_and_result_native_fifo_shape() {
        let mut convo = Conversation::new("find a station near DM79");
        convo.push_tool_call(ToolCall::new("find_stations", json!({ "grid": "DM79" })));
        convo.push_tool_result("find_stations", r#"{"count":3}"#);

        let body = build_ollama_request("m", &convo, &[echo_tool()], None, None, ELMER_SYSTEM_PROMPT);
        let msgs = body["messages"].as_array().expect("messages array");

        // messages[0]=system, [1]=user, [2]=assistant tool_call, [3]=tool result.
        let asst = &msgs[2];
        assert_eq!(asst["role"], "assistant");
        let tc = &asst["tool_calls"][0];
        assert_eq!(tc["function"]["name"], "find_stations");
        // arguments MUST be a JSON object (not a string).
        assert!(
            tc["function"]["arguments"].is_object(),
            "native arguments must be a JSON object, not a string; got: {}",
            tc["function"]["arguments"]
        );
        assert_eq!(tc["function"]["arguments"]["grid"], "DM79");

        let result = &msgs[3];
        assert_eq!(result["role"], "tool");
        // Native pairing is by tool_name, NOT tool_call_id.
        assert_eq!(result["tool_name"], "find_stations");
        assert!(
            result.get("tool_call_id").is_none(),
            "native tool result must NOT carry an OpenAI tool_call_id; got: {result}"
        );
        assert!(result["content"].as_str().unwrap().contains("result"));
    }

    /// An error tool result labels its content "error" and preserves the detail.
    #[test]
    fn request_tool_error_result_labels_error() {
        let mut convo = Conversation::new("go");
        convo.push_tool_error("message_send", "tool denied: session is tainted");
        let body = build_ollama_request("m", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);
        let tool_msg = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .expect("a tool-role message");
        let content = tool_msg["content"].as_str().unwrap();
        assert!(content.contains("error"));
        assert!(content.contains("tainted"));
    }

    /// A ToolCall with `Value::Null` args renders `arguments` as `{}` (never a
    /// null or a string) so the wire value is always an object.
    #[test]
    fn request_null_args_become_empty_object() {
        let mut convo = Conversation::new("go");
        convo.push_tool_call(ToolCall::new("noop", Value::Null));
        let body = build_ollama_request("m", &convo, &[], None, None, ELMER_SYSTEM_PROMPT);
        let asst = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "assistant" && m["tool_calls"].is_array())
            .expect("assistant tool_calls message");
        assert_eq!(
            asst["tool_calls"][0]["function"]["arguments"],
            json!({}),
            "null args must render as an empty object"
        );
    }

    // -----------------------------------------------------------------------
    // parse_ollama_response
    // -----------------------------------------------------------------------

    /// A text turn: `message.content` with no tool_calls → `ModelTurn::Text`.
    #[test]
    fn parse_text_turn() {
        let recorded = json!({
            "model": "qwen3:8b",
            "message": { "role": "assistant", "content": "hello operator" },
            "done": true
        });
        assert_eq!(
            parse_ollama_response(&recorded).unwrap(),
            ModelTurn::Text("hello operator".into())
        );
    }

    /// A single tool_call whose `arguments` is a JSON OBJECT parses directly (no
    /// string-parse) — the critical divergence from the OpenAI shape.
    #[test]
    fn parse_single_tool_call_object_args() {
        let recorded = json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "find_stations",
                        "arguments": { "grid": "DM79" }
                    }
                }]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                // args are the object AS-IS, not a re-parsed string.
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Multiple tool_calls → multiple ToolCalls, in order.
    #[test]
    fn parse_multiple_tool_calls() {
        let recorded = json!({
            "message": {
                "role": "assistant",
                "tool_calls": [
                    { "function": { "name": "a", "arguments": { "x": 1 } } },
                    { "function": { "name": "b", "arguments": {} } }
                ]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
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

    /// Tool calls take precedence over content (matching the sibling adapters).
    #[test]
    fn parse_tool_calls_take_precedence_over_content() {
        let recorded = json!({
            "message": {
                "content": "some chatter",
                "tool_calls": [{ "function": { "name": "x", "arguments": {} } }]
            }
        });
        assert!(matches!(
            parse_ollama_response(&recorded).unwrap(),
            ModelTurn::ToolCalls(_)
        ));
    }

    /// An empty tool_calls array falls back to the content text.
    #[test]
    fn parse_empty_tool_calls_falls_back_to_text() {
        let recorded = json!({
            "message": { "content": "no tools today", "tool_calls": [] }
        });
        assert_eq!(
            parse_ollama_response(&recorded).unwrap(),
            ModelTurn::Text("no tools today".into())
        );
    }

    /// A tool_call with MISSING `arguments` → `Value::Null` args (COR-3 re-prompts).
    #[test]
    fn parse_tool_call_missing_args_becomes_null() {
        let recorded = json!({
            "message": {
                "tool_calls": [{ "function": { "name": "echo" } }]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "echo");
                assert_eq!(calls[0].args, Value::Null);
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Fix C: string-encoded arguments tolerance
    // -----------------------------------------------------------------------

    /// Fix C: a tool_call whose `arguments` is a JSON STRING containing a
    /// valid JSON object is parsed into the object, NOT left as a string.
    ///
    /// Some Ollama quants emit `function.arguments` as a JSON string even on
    /// the native `/api/chat` path (e.g. `"{\"grid\":\"DM79\"}"`). Without this
    /// fix the value stayed as `Value::String`, the runner's schema check
    /// rejected it, and the agent entered a malformed-retry loop (COR-3).
    #[test]
    fn parse_tool_call_string_encoded_object_args_are_parsed() {
        let recorded = json!({
            "message": {
                "tool_calls": [{
                    "function": {
                        "name": "find_stations",
                        // arguments is a JSON STRING (not an object) — the Fix C case.
                        "arguments": "{\"grid\":\"DM79\"}"
                    }
                }]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "find_stations");
                assert!(
                    calls[0].args.is_object(),
                    "string-encoded args must be parsed to an object; got: {:?}",
                    calls[0].args
                );
                assert_eq!(
                    calls[0].args["grid"], "DM79",
                    "parsed object must contain the expected key"
                );
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Fix C: object `arguments` (the normal case) still work unchanged after
    /// the string-tolerance branch is added.
    ///
    /// Regression guard: the happy-path (object args) must still parse
    /// as-is; the Fix C logic must not disturb it.
    #[test]
    fn parse_tool_call_object_args_unchanged_by_fix_c() {
        let recorded = json!({
            "message": {
                "tool_calls": [{
                    "function": {
                        "name": "find_stations",
                        "arguments": { "grid": "DM79" }
                    }
                }]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Fix C: a tool_call whose `arguments` is an UNPARSEABLE JSON string
    /// (malformed JSON) falls back to `Value::Null` so COR-3 can re-prompt.
    /// The agent never gets a fabricated value — malformed input → explicit Null.
    #[test]
    fn parse_tool_call_unparseable_string_args_become_null() {
        let recorded = json!({
            "message": {
                "tool_calls": [{
                    "function": {
                        "name": "find_stations",
                        // Malformed JSON string — cannot be parsed to a Value.
                        "arguments": "{ not valid json !!!"
                    }
                }]
            }
        });
        match parse_ollama_response(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(
                    calls[0].args,
                    Value::Null,
                    "unparseable string args must fall back to Null (COR-3 re-prompts)"
                );
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// An empty/absent content with no tool calls → empty `Text` turn (not an error).
    #[test]
    fn parse_empty_message_is_empty_text() {
        let recorded = json!({ "message": { "role": "assistant", "content": "" } });
        assert_eq!(
            parse_ollama_response(&recorded).unwrap(),
            ModelTurn::Text(String::new())
        );
        // Absent content too.
        let no_content = json!({ "message": { "role": "assistant" } });
        assert_eq!(
            parse_ollama_response(&no_content).unwrap(),
            ModelTurn::Text(String::new())
        );
    }

    /// Missing `message` object → parse error.
    #[test]
    fn parse_missing_message_is_error() {
        assert!(parse_ollama_response(&json!({})).is_err());
        assert!(parse_ollama_response(&json!({ "done": true })).is_err());
    }

    // -----------------------------------------------------------------------
    // parse_ollama_counts
    // -----------------------------------------------------------------------

    /// Both counts present → `Some((prompt, eval))`.
    #[test]
    fn parse_counts_present() {
        let recorded = json!({
            "message": { "content": "hi" },
            "prompt_eval_count": 1234,
            "eval_count": 56,
            "done": true
        });
        assert_eq!(parse_ollama_counts(&recorded), Some((1234, 56)));
    }

    /// A response missing either count → `None` (meter stays hidden).
    #[test]
    fn parse_counts_absent() {
        // Neither present.
        assert_eq!(parse_ollama_counts(&json!({ "message": {} })), None);
        // Only prompt present.
        assert_eq!(
            parse_ollama_counts(&json!({ "prompt_eval_count": 10 })),
            None
        );
        // Only eval present.
        assert_eq!(parse_ollama_counts(&json!({ "eval_count": 5 })), None);
    }

    // -----------------------------------------------------------------------
    // is_ollama_endpoint (light heuristic — probe is authoritative in T4)
    // -----------------------------------------------------------------------

    /// The `/api/chat` path is recognized regardless of host.
    #[test]
    fn is_ollama_endpoint_true_for_api_chat_path() {
        assert!(is_ollama_endpoint("http://127.0.0.1:11434/api/chat"));
        assert!(is_ollama_endpoint("http://some-host:11434/api/chat"));
    }

    /// A loopback host is a candidate even without the native path.
    #[test]
    fn is_ollama_endpoint_true_for_loopback_host() {
        assert!(is_ollama_endpoint("http://127.0.0.1:11434/v1/chat/completions"));
        assert!(is_ollama_endpoint("http://localhost:11434/"));
    }

    /// A remote OpenAI/Anthropic host with no native path is not a candidate.
    #[test]
    fn is_ollama_endpoint_false_for_remote_compat() {
        assert!(!is_ollama_endpoint("https://api.openai.com/v1/chat/completions"));
        assert!(!is_ollama_endpoint("https://api.anthropic.com/v1/messages"));
    }

    // -----------------------------------------------------------------------
    // OllamaStreamAccumulator — NDJSON streaming (recorded bytes, no network)
    // -----------------------------------------------------------------------

    use std::sync::{Arc, Mutex};

    /// A shared recording buffer the test sink pushes `RunEvent`s into. Wrapped in
    /// `Arc<Mutex<…>>` so the `Fn(RunEvent) + Sync` sink closure can own a clone
    /// while the test still asserts on the original — mirroring the SSE
    /// accumulator's `recorder()` helper (the sink MUST be `Sync`, so a
    /// single-threaded `RefCell` will not compile against `feed`'s bound).
    fn recorder() -> Arc<Mutex<Vec<RunEvent>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// Build a `Fn(RunEvent) + Sync` sink that records into `events`.
    fn recording_sink(events: &Arc<Mutex<Vec<RunEvent>>>) -> impl Fn(RunEvent) + Sync {
        let events = events.clone();
        move |e: RunEvent| events.lock().unwrap().push(e)
    }

    /// Every `AssistantDelta` chunk in the recorder, in order.
    fn assistant_deltas(events: &Arc<Mutex<Vec<RunEvent>>>) -> Vec<String> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                RunEvent::AssistantDelta { chunk } => Some(chunk.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every `ReasoningDelta` chunk in the recorder, in order.
    fn reasoning_deltas(events: &Arc<Mutex<Vec<RunEvent>>>) -> Vec<String> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                RunEvent::ReasoningDelta { chunk } => Some(chunk.clone()),
                _ => None,
            })
            .collect()
    }

    /// Multiple content chunks → one `AssistantDelta` per non-empty delta, and
    /// `into_turn` concatenates them into a `Text` turn.
    #[test]
    fn stream_text_deltas_emit_and_concatenate() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\", \"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"operator\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"prompt_eval_count\":10,\"eval_count\":3}\n",
        );
        let done = acc.feed(ndjson.as_bytes(), &sink).unwrap();
        assert!(done, "the done:true line must terminate the stream");

        assert_eq!(
            assistant_deltas(&events),
            vec!["Hello".to_string(), ", ".to_string(), "operator".to_string()],
            "one AssistantDelta per non-empty content delta"
        );
        assert_eq!(
            acc.into_turn(),
            ModelTurn::Text("Hello, operator".into()),
            "into_turn concatenates the content deltas"
        );
    }

    /// The empty final-chunk content ("") must NOT emit an AssistantDelta.
    #[test]
    fn stream_skips_empty_content_deltas() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"content\":\"\"},\"done\":false}\n",
            "{\"message\":{\"content\":\"hi\"},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();
        assert_eq!(
            assistant_deltas(&events),
            vec!["hi".to_string()],
            "empty content deltas must not emit AssistantDelta"
        );
    }

    /// A blank line in the stream is ignored (no panic, no spurious event).
    #[test]
    fn stream_ignores_blank_lines() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "\n",
            "{\"message\":{\"content\":\"a\"},\"done\":false}\n",
            "   \n",
            "{\"message\":{\"content\":\"b\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();
        assert_eq!(assistant_deltas(&events), vec!["a".to_string(), "b".to_string()]);
    }

    /// `thinking` deltas emit `ReasoningDelta` and accumulate; content still
    /// accumulates independently. The `thinking()` accessor sees the concatenation.
    #[test]
    fn stream_thinking_deltas_emit_reasoning_and_content_accumulates() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"thinking\":\"let me \"},\"done\":false}\n",
            "{\"message\":{\"thinking\":\"think\",\"content\":\"answer \"},\"done\":false}\n",
            "{\"message\":{\"content\":\"here\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();

        assert_eq!(
            reasoning_deltas(&events),
            vec!["let me ".to_string(), "think".to_string()],
            "one ReasoningDelta per non-empty thinking delta"
        );
        assert_eq!(assistant_deltas(&events), vec!["answer ".to_string(), "here".to_string()]);
        assert_eq!(acc.thinking(), "let me think", "thinking is concatenated");
        assert_eq!(acc.into_turn(), ModelTurn::Text("answer here".into()));
    }

    /// A chunk carrying a native tool_call (object arguments) → `into_turn` is
    /// `ToolCalls` with the correct name + OBJECT args, taking precedence over
    /// any content.
    #[test]
    fn stream_tool_call_object_args_becomes_tool_calls_turn() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"content\":\"some chatter\",\"tool_calls\":[{\"function\":{\"name\":\"find_stations\",\"arguments\":{\"grid\":\"DM79\"}}}]},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// A tool_call whose `arguments` is a JSON STRING is parsed to an object,
    /// reusing `parse_ollama_tool_call`'s Fix C tolerance.
    #[test]
    fn stream_tool_call_string_encoded_args_are_parsed() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"tool_calls\":[{\"function\":{\"name\":\"find_stations\",\"arguments\":\"{\\\"grid\\\":\\\"DM79\\\"}\"}}]},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "find_stations");
                assert!(
                    calls[0].args.is_object(),
                    "string-encoded args must be parsed to an object; got {:?}",
                    calls[0].args
                );
                assert_eq!(calls[0].args["grid"], "DM79");
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Tool calls that arrive across SEPARATE chunks are all collected, in order.
    #[test]
    fn stream_multiple_tool_calls_across_chunks() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"tool_calls\":[{\"function\":{\"name\":\"a\",\"arguments\":{\"x\":1}}}]},\"done\":false}\n",
            "{\"message\":{\"tool_calls\":[{\"function\":{\"name\":\"b\",\"arguments\":{}}}]},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true}\n",
        );
        acc.feed(ndjson.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
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

    /// The `done` line's `prompt_eval_count` + `eval_count` are captured and
    /// exposed via `counts()`.
    #[test]
    fn stream_done_line_captures_counts() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = concat!(
            "{\"message\":{\"content\":\"hi\"},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true,\"prompt_eval_count\":1234,\"eval_count\":56}\n",
        );
        assert!(acc.feed(ndjson.as_bytes(), &sink).unwrap());
        assert_eq!(acc.counts(), Some((1234, 56)));
    }

    /// A `done` line missing the counts yields `None` (meter stays hidden).
    #[test]
    fn stream_done_line_without_counts_is_none() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let ndjson = "{\"message\":{\"content\":\"hi\"},\"done\":true}\n";
        acc.feed(ndjson.as_bytes(), &sink).unwrap();
        assert_eq!(acc.counts(), None);
    }

    /// A single NDJSON line split across TWO feed() calls at an arbitrary byte
    /// boundary reassembles into one complete line.
    #[test]
    fn stream_line_split_across_feeds_reassembles() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let full = "{\"message\":{\"content\":\"hello operator\"},\"done\":false}\n";
        let bytes = full.as_bytes();
        let split = 20; // arbitrary mid-line byte offset
        // First half carries no complete line yet.
        assert!(!acc.feed(&bytes[..split], &sink).unwrap());
        assert!(assistant_deltas(&events).is_empty(), "no delta until the line completes");
        // Second half completes the line.
        assert!(!acc.feed(&bytes[split..], &sink).unwrap());
        assert_eq!(assistant_deltas(&events), vec!["hello operator".to_string()]);
    }

    /// A multi-byte UTF-8 codepoint split across two feed() calls decodes intact
    /// (mirrors the SSE mid-codepoint test). The 'é' (U+00E9) is 0xC3 0xA9.
    #[test]
    fn stream_multibyte_codepoint_split_across_feeds_decodes_intact() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        let full = "{\"message\":{\"content\":\"café\"},\"done\":false}\n";
        let bytes = full.as_bytes();
        // Find the byte index of the first byte of 'é' (0xC3) and split BETWEEN
        // its two UTF-8 bytes so the boundary bisects the codepoint.
        let pos = bytes.iter().position(|&b| b == 0xC3).expect("é first byte");
        assert!(!acc.feed(&bytes[..pos + 1], &sink).unwrap());
        assert!(!acc.feed(&bytes[pos + 1..], &sink).unwrap());
        assert_eq!(
            assistant_deltas(&events),
            vec!["café".to_string()],
            "the é must decode intact across the chunk boundary, not as U+FFFD"
        );
    }

    /// `finish` flushes a trailing `done` line sent WITHOUT a terminating newline
    /// so its content delta and counts are not dropped.
    #[test]
    fn stream_finish_flushes_unterminated_trailing_line() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        // No trailing newline on the final line.
        let ndjson = "{\"message\":{\"content\":\"tail\"},\"done\":true,\"prompt_eval_count\":7,\"eval_count\":2}";
        // feed sees no complete line (no newline) → returns false.
        assert!(!acc.feed(ndjson.as_bytes(), &sink).unwrap());
        assert!(assistant_deltas(&events).is_empty(), "nothing flushed until finish()");
        acc.finish(&sink).unwrap();
        assert_eq!(assistant_deltas(&events), vec!["tail".to_string()]);
        assert_eq!(acc.counts(), Some((7, 2)));
    }

    /// An oversized un-terminated line (no newline, past the pending cap) → a
    /// Transport error rather than unbounded memory growth.
    #[test]
    fn stream_oversized_unterminated_line_errors() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        // A megabyte-plus of non-newline bytes with no line delimiter.
        let big = vec![b'x'; MAX_PENDING_LINE_BYTES + 1];
        let err = acc.feed(&big, &sink).unwrap_err();
        assert!(
            matches!(err, ProviderError::Transport(_)),
            "oversized un-terminated line must be a Transport error; got {err:?}"
        );
    }

    /// Accumulated output past the total cap → a Transport error. A single
    /// complete line whose content delta exceeds the cap trips it on apply.
    #[test]
    fn stream_total_output_cap_errors() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        // Build one complete line whose content string exceeds the total cap.
        let huge = "z".repeat(MAX_TOTAL_OUTPUT_BYTES + 1);
        let line = format!("{{\"message\":{{\"content\":\"{huge}\"}},\"done\":false}}\n");
        let err = acc.feed(line.as_bytes(), &sink).unwrap_err();
        assert!(
            matches!(err, ProviderError::Transport(_)),
            "exceeding the total-output cap must be a Transport error; got {err:?}"
        );
    }

    /// Codex-adrev regression: accumulated tool calls must be bounded by the SAME
    /// total-output cap as content/thinking. A tool_calls stream that never
    /// touches content/thinking must still error once its serialized bytes exceed
    /// the cap, rather than growing memory unbounded.
    #[test]
    fn stream_tool_call_bytes_are_capped() {
        let events = recorder();
        let sink = recording_sink(&events);
        let mut acc = OllamaStreamAccumulator::new();

        // One tool call whose serialized arguments alone exceed the total cap.
        let huge = "z".repeat(MAX_TOTAL_OUTPUT_BYTES + 1);
        let line = serde_json::json!({
            "message": { "tool_calls": [ { "function": { "name": "x", "arguments": { "a": huge } } } ] },
            "done": false
        })
        .to_string()
            + "\n";
        let err = acc.feed(line.as_bytes(), &sink).unwrap_err();
        assert!(
            matches!(err, ProviderError::Transport(_)),
            "exceeding the total-output cap via tool_calls must be a Transport error; got {err:?}"
        );
    }
}
