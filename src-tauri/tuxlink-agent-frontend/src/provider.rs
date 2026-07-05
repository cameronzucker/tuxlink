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
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use url::Url;

use tuxlink_agent_runner::{
    Conversation, Message, ModelTurn, Provider, ProviderError, RunEvent, ToolCall, ToolSpec,
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
// redacted_url — credential-safe request URL for error/log messages
// ---------------------------------------------------------------------------

/// Render a request URL as `scheme://host[:port]/path` for an error message,
/// dropping any userinfo and query string so a credential-in-URL can never leak
/// into a transport error or the session log.
///
/// This is the instrumentation that turns an opaque "HTTP 404" into an
/// actionable one: the operator sees the exact path that was requested, so a
/// base URL missing `/chat/completions` is obvious at a glance instead of a
/// recurring mystery. `AgentEndpoint` validation already rejects
/// credentials-in-URL, so the userinfo strip is defense-in-depth.
pub(crate) fn redacted_url(u: &Url) -> String {
    let mut s = format!("{}://", u.scheme());
    if let Some(host) = u.host_str() {
        s.push_str(host);
    }
    if let Some(port) = u.port() {
        s.push_str(&format!(":{port}"));
    }
    s.push_str(u.path());
    s
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

/// Format an error together with its `source()` cause chain as
/// `"outer: cause: root-cause"`. A bare `format!("{e}")` on a `reqwest::Error`
/// shows only the outermost wrapper (e.g. "error sending request") and hides the
/// real reason (e.g. "connection refused", "certificate verify failed", "dns
/// error") one level down — the "one-deep" error the operator could not diagnose
/// (tuxlink-a1xwx). Walk the chain so the actual cause is surfaced.
///
/// Callers pass a URL-stripped error (`reqwest::Error::without_url()`), so no cause
/// in the chain carries the request URL / a query credential.
pub(crate) fn error_cause_chain(err: &dyn std::error::Error) -> String {
    use std::fmt::Write as _;
    let mut out = err.to_string();
    let mut src = err.source();
    while let Some(e) = src {
        let _ = write!(out, ": {e}");
        src = e.source();
    }
    out
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
    /// Sampling temperature forwarded to the request body as `"temperature"` when
    /// `Some`; omitted entirely when `None` so the server default applies.
    temperature: Option<f32>,
    /// Optional operator-supplied system-prompt override (tuxlink-31tbw). When
    /// `Some`, it replaces [`ELMER_SYSTEM_PROMPT`] as the `role: system` message;
    /// when `None`, the built-in default is used. Threaded from the model-config
    /// snapshot by T4 so a stored override reaches the wire on the compat path too.
    system_prompt: Option<String>,
    /// Optional bearer token (a local llama.cpp / Ollama shim usually needs
    /// none; an OpenAI-compatible proxy may). Stored as [`ApiKey`] so it never
    /// leaks through `Debug`/`Display`; only used via `.expose()` at the HTTP
    /// header boundary.
    api_key: Option<ApiKey>,
    /// Operator-configured context-window budget (tuxlink-evucv). The OpenAI Chat
    /// Completions API has no context-size field (unlike Ollama's
    /// `options.num_ctx`), so this drives a CLIENT-SIDE transcript trim before the
    /// POST: if the estimated prompt exceeds the budget, the oldest turns are
    /// dropped so the request cannot overflow the server context (the 400
    /// `exceed_context_size_error`). `None` = no trim (unbounded, prior behavior).
    /// Set via [`OpenAiProvider::with_num_ctx`] so existing constructors/tests are
    /// unaffected.
    num_ctx: Option<u32>,
}

impl OpenAiProvider {
    /// Build the provider. `endpoint` MUST already have passed
    /// [`crate::endpoint::validate_endpoint`] — this constructor does not
    /// re-validate (the SEC-5 gate is the caller's single chokepoint), but it is
    /// only reachable from `main` after that gate.
    ///
    /// `temperature` is the sampling temperature forwarded to the request body;
    /// `None` leaves the server default unchanged.
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
            num_ctx: None,
        }
    }

    /// Set the client-side context-window budget (tuxlink-evucv). Builder form so
    /// the 6-arg `new` and its many callers/tests stay unchanged; only the
    /// operator-configured construction sites opt in. `None` disables trimming.
    #[must_use]
    pub fn with_num_ctx(mut self, num_ctx: Option<u32>) -> Self {
        self.num_ctx = num_ctx;
        self
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn turn(
        &self,
        conversation: &Conversation,
        tools: &[ToolSpec],
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<ModelTurn, ProviderError> {
        // Phase 1b (tuxlink-e2vw7): request a streamed completion and emit
        // RunEvent deltas as tokens arrive. `on_event` is FIRE-AND-FORGET — what
        // it does never changes which `ModelTurn` this returns.
        //
        // We set `stream` on the already-built request value rather than threading
        // a flag through `build_request_body`, whose tests assert the non-stream
        // body shape. The pure assembly stays untouched.
        let system_prompt = self.system_prompt.as_deref().unwrap_or(ELMER_SYSTEM_PROMPT);
        // tuxlink-evucv: client-side context trim. The OpenAI API has no
        // context-size field, so when the operator configured a window, drop the
        // oldest turns that don't fit rather than letting the request overflow the
        // server context (HTTP 400 exceed_context_size_error). No-op when num_ctx
        // is None (unbounded) or the transcript already fits.
        let trimmed = transcript_budget(self.num_ctx, system_prompt, tools).and_then(|budget| {
            let kept = trim_messages_to_budget(conversation.messages(), budget);
            (kept.len() < conversation.messages().len()).then(|| Conversation::from_messages(kept))
        });
        let conversation = trimmed.as_ref().unwrap_or(conversation);

        let mut body = build_request_body(
            &self.model,
            conversation,
            tools,
            self.temperature,
            system_prompt,
        );
        body["stream"] = json!(true);

        let mut req = self.client.post(self.endpoint.clone()).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose());
        }

        let resp = req.send().await.map_err(|e| {
            // `reqwest::Error`'s Display re-embeds the full request URL ("for url
            // (...)") — which can carry a query credential. Strip it with
            // `without_url()` and use our own credential-safe `redacted_url`
            // instead, so a secret never reaches the error / session log (Codex P1).
            ProviderError::Transport(format!(
                "request to {} failed: {}",
                redacted_url(&self.endpoint),
                error_cause_chain(&e.without_url())
            ))
        })?;

        let status = resp.status();
        if !status.is_success() {
            // Capture a bounded slice of the error body for the operator, but do
            // not let a huge body blow up the message.  Value-scrub the bearer
            // key BEFORE capping — capping first would let a key that straddles
            // the boundary leave a partial prefix in the snippet.
            // `redact_and_cap` enforces scrub-then-cap in the correct order.
            let text = resp.text().await.unwrap_or_default();
            let snippet = redact_and_cap(text, self.api_key.as_ref(), 500);
            // HTTP 429 is classified separately so the frontend can surface the
            // rate-limit callout (rateLimited outcome-kind) rather than the
            // generic NeedsOperator path.  No automatic retry is performed here.
            if status.as_u16() == 429 {
                return Err(ProviderError::RateLimited(format!(
                    "model endpoint {} returned HTTP 429 (rate limited): {snippet}",
                    redacted_url(&self.endpoint)
                )));
            }
            return Err(ProviderError::Transport(format!(
                "model endpoint {} returned HTTP {status}: {snippet}",
                redacted_url(&self.endpoint)
            )));
        }

        // Non-streaming fallback: some OpenAI-compatible endpoints ignore
        // `stream: true` and answer with a single JSON document. Detect that by
        // content-type — an event-stream advertises `text/event-stream`; anything
        // else (typically `application/json`) is a whole completion we parse via
        // the existing `parse_completion` path, emitting no deltas.
        let is_event_stream = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.to_ascii_lowercase().contains("text/event-stream"))
            .unwrap_or(false);

        if !is_event_stream {
            let value: Value = resp.json().await.map_err(|e| {
                ProviderError::Unparseable(format!("response was not JSON: {e}"))
            })?;
            return parse_completion(&value).map_err(ProviderError::Unparseable);
        }

        // Streaming path. The whole read is a SINGLE awaited future inside `turn`
        // — no detached `tokio::spawn`. The run loop races `turn()` against a
        // cancel token and DROPS this future on cancel; keeping the byte-stream
        // read inline means that drop aborts the in-flight reqwest stream rather
        // than leaking a background task that keeps the connection open.
        let mut stream = resp.bytes_stream();
        let mut acc = SseAccumulator::new();

        while let Some(item) = stream.next().await {
            let chunk = item
                .map_err(|e| ProviderError::Transport(format!("stream read failed: {e}")))?;
            // `feed` parses every complete SSE frame currently in the buffer and
            // invokes `on_event` for each delta. A `[DONE]` sentinel ends the
            // stream early; otherwise we keep reading until the body closes. It
            // returns a transport error if the endpoint streams an oversized
            // un-terminated frame or breaches the total-output cap.
            if acc.feed(&chunk, on_event)? {
                break;
            }
        }
        // Flush any trailing frame the server sent without a terminating blank
        // line before closing the connection (rare, but lenient parsing avoids
        // dropping the final delta).
        acc.finish(on_event)?;

        Ok(acc.into_turn())
    }
}

// --- SSE streaming accumulator (pure, unit-testable) ------------------------

/// One in-flight tool call being assembled from streamed `delta.tool_calls`
/// fragments. The `name` arrives once (on the first fragment for an index); the
/// `arguments` string streams in pieces that must be concatenated in order.
#[derive(Default)]
struct PartialToolCall {
    name: String,
    arguments: String,
    /// Gemini's opaque per-call `extra_content` (carrying
    /// `google.thought_signature`), captured from whichever streamed fragment
    /// carries it so it can be echoed back on the next request (tuxlink-0tuc3).
    extra_content: Option<Value>,
}

/// Maximum size, in bytes, of a single un-terminated SSE frame held in `buf`
/// while waiting for its closing blank line. A legitimate chat-completions frame
/// is one small JSON object (a token or tool-call fragment); 1 MiB is orders of
/// magnitude beyond any real frame, so a `buf` that grows past this without a
/// frame delimiter signals a hostile or broken endpoint streaming an unbounded
/// un-terminated frame. Exceeding it is treated as a transport error rather than
/// letting memory grow until the per-turn timeout.
const MAX_PENDING_FRAME_BYTES: usize = 1024 * 1024; // 1 MiB

/// Maximum total decoded output, in bytes, accumulated across `content` +
/// `reasoning`. A complete answer-plus-reasoning trace for any legitimate model
/// turn fits comfortably under this; 16 MiB bounds an endpoint that streams
/// endless small deltas (which individually terminate their frames, so
/// `MAX_PENDING_FRAME_BYTES` would not catch them) from exhausting memory before
/// the configured timeout. Exceeding it is a transport error.
const MAX_TOTAL_OUTPUT_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

/// Accumulates an OpenAI-style SSE chat-completions stream into a [`ModelTurn`],
/// emitting [`RunEvent`] deltas through a caller-supplied sink as fragments land.
///
/// This is the testable seam: byte chunks from the network are appended via
/// [`SseAccumulator::feed`], which buffers across arbitrary frame boundaries,
/// splits complete blank-line-delimited events, extracts each `data:` payload,
/// and routes the parsed JSON through [`apply_chunk`]. No IO lives here — tests
/// drive it with hand-built byte slices (including mid-frame and mid-codepoint
/// splits) and a recording sink, with no live server.
struct SseAccumulator {
    /// Raw bytes received but not yet forming a complete blank-line-delimited
    /// frame. Buffered as BYTES (not a lossy `String`) so a multi-byte UTF-8
    /// codepoint split across two network chunks is reassembled intact rather
    /// than each half decoding to a U+FFFD replacement char.
    buf: Vec<u8>,
    /// Accumulated answer content (concatenated `delta.content`).
    content: String,
    /// Accumulated reasoning (concatenated `delta.reasoning` / `reasoning_content`).
    /// Reasoning is emitted to the caller delta-by-delta as it streams; no
    /// `ModelTurn` variant carries the assembled trace, so in non-test builds this
    /// field is write-only (read only by the `#[cfg(test)]` `reasoning()` accessor
    /// that asserts the concatenation). Kept because the streaming contract
    /// specifies accumulating reasoning alongside emitting it, and a future caller
    /// may want the full trace.
    #[cfg_attr(not(test), allow(dead_code))]
    reasoning: String,
    /// Tool calls keyed by their stream `index`, assembled across fragments.
    /// `BTreeMap` so the final `ModelTurn::ToolCalls` is ordered by index.
    tool_calls: BTreeMap<i64, PartialToolCall>,
    /// Set once a `data: [DONE]` sentinel is seen.
    done: bool,
}

/// Find the first SSE frame delimiter in `buf` at the BYTE level, returning
/// `Some((start, len))` where `start` is the byte offset of the blank line and
/// `len` is the delimiter width (2 for `\n\n`, 4 for `\r\n\r\n`). Whichever
/// delimiter appears at the earliest offset wins, so a CRLF stream and an LF
/// stream split identically. `None` means no complete frame is buffered yet.
///
/// Scanning at the byte level (rather than decoding to a `String` first) is what
/// lets a multi-byte UTF-8 codepoint split across two network chunks be
/// reassembled into a complete frame before any decode happens.
fn find_frame_delimiter(buf: &[u8]) -> Option<(usize, usize)> {
    let lf = buf.windows(2).position(|w| w == b"\n\n");
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(l), Some(c)) => {
            if l <= c {
                Some((l, 2))
            } else {
                Some((c, 4))
            }
        }
        (Some(l), None) => Some((l, 2)),
        (None, Some(c)) => Some((c, 4)),
        (None, None) => None,
    }
}

impl SseAccumulator {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            content: String::new(),
            reasoning: String::new(),
            tool_calls: BTreeMap::new(),
            done: false,
        }
    }

    /// Append a network byte chunk and process every COMPLETE SSE frame now in
    /// the buffer. Returns `Ok(true)` once the stream has terminated (a `[DONE]`
    /// sentinel was seen) so the caller can stop reading; `Ok(false)` while more
    /// data is expected.
    ///
    /// Bytes are buffered RAW and frames are split at the byte level, so a
    /// multi-byte UTF-8 codepoint straddling a network-chunk boundary is
    /// reassembled into a complete frame before being decoded — no U+FFFD
    /// corruption. Each complete frame is then decoded with [`std::str::from_utf8`];
    /// a frame that is genuinely not valid UTF-8 (should not happen for a complete
    /// frame from a conforming server) is skipped rather than erroring the turn.
    ///
    /// Returns `Err(ProviderError::Transport)` when an un-terminated frame in
    /// `buf` exceeds [`MAX_PENDING_FRAME_BYTES`], or when total accumulated output
    /// exceeds [`MAX_TOTAL_OUTPUT_BYTES`] — bounding memory against a hostile or
    /// broken endpoint. The error message carries no body/secret content.
    fn feed(
        &mut self,
        bytes: &[u8],
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<bool, ProviderError> {
        self.buf.extend_from_slice(bytes);

        // Frames are separated by a blank line: `\n\n` (LF) or `\r\n\r\n` (CRLF).
        // Scan the BYTE buffer for either delimiter, taking whichever appears
        // first, so the codepoint-reassembly guarantee holds (we never decode a
        // partial codepoint at a chunk boundary).
        while let Some((start, len)) = find_frame_delimiter(&self.buf) {
            // Move out the frame bytes plus its delimiter; the remainder is a
            // (possibly partial) next frame kept for the following chunk.
            let frame_bytes: Vec<u8> = self.buf.drain(..start + len).collect();
            // Decode only the frame body (delimiter excluded). A complete frame is
            // valid UTF-8 since the only split was at a network-chunk boundary; on
            // the rare genuine decode error, skip this frame rather than panicking.
            let Ok(frame) = std::str::from_utf8(&frame_bytes[..start]) else {
                continue;
            };
            if self.process_frame(frame, on_event)? {
                self.done = true;
                return Ok(true);
            }
        }

        // No complete frame remains; what's left is a single pending frame. Guard
        // its size so an endpoint that never sends a blank line cannot grow `buf`
        // without bound.
        if self.buf.len() > MAX_PENDING_FRAME_BYTES {
            return Err(ProviderError::Transport(
                "model stream sent an oversized un-terminated frame".to_string(),
            ));
        }
        Ok(false)
    }

    /// Process whatever remains in the buffer as a final frame once the network
    /// stream has closed. A well-behaved server terminates every frame with a
    /// blank line, so this is usually a no-op; it exists so a trailing frame sent
    /// without the closing blank line is not silently dropped. Returns
    /// `Err(ProviderError::Transport)` if applying the trailing frame breaches
    /// [`MAX_TOTAL_OUTPUT_BYTES`].
    fn finish(&mut self, on_event: &(dyn Fn(RunEvent) + Sync)) -> Result<(), ProviderError> {
        if self.done {
            return Ok(());
        }
        let frame_bytes = std::mem::take(&mut self.buf);
        // Decode leniently for the trailing-frame flush; a malformed tail is
        // dropped rather than erroring the turn.
        let Ok(frame) = std::str::from_utf8(&frame_bytes) else {
            return Ok(());
        };
        if !frame.trim().is_empty() {
            self.process_frame(frame, on_event)?;
        }
        Ok(())
    }

    /// Parse one SSE frame (which may carry multiple `data:` lines per the SSE
    /// spec) and apply each payload. Returns `Ok(true)` if a `[DONE]` sentinel was
    /// seen in this frame; `Err` if applying a payload breached the output cap.
    fn process_frame(
        &mut self,
        frame: &str,
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<bool, ProviderError> {
        for line in frame.lines() {
            // SSE allows `data:foo` and `data: foo`; strip the field name and one
            // optional leading space. Lines that are not `data:` (e.g. `event:`,
            // comments beginning `:`) are ignored.
            let payload = match line.strip_prefix("data:") {
                Some(rest) => rest.strip_prefix(' ').unwrap_or(rest),
                None => continue,
            };

            if payload.trim() == "[DONE]" {
                return Ok(true);
            }

            // A malformed / partial JSON payload is skipped rather than failing the
            // whole turn — a later frame may still complete the answer.
            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                self.apply_chunk(&value, on_event)?;
            }
        }
        Ok(false)
    }

    /// Total accumulated decoded output so far (`content` + `reasoning`), in
    /// bytes. Used to enforce [`MAX_TOTAL_OUTPUT_BYTES`].
    fn total_output_len(&self) -> usize {
        self.content.len() + self.reasoning.len()
    }

    /// Apply a single parsed chunk's `choices[0].delta` to the accumulators and
    /// emit the corresponding deltas. Aside from the `on_event` sink, the only
    /// fallible path is the [`MAX_TOTAL_OUTPUT_BYTES`] cap: appending a delta that
    /// would push `content` + `reasoning` past the cap returns
    /// `Err(ProviderError::Transport)` (no body/secret content in the message).
    fn apply_chunk(
        &mut self,
        chunk: &Value,
        on_event: &(dyn Fn(RunEvent) + Sync),
    ) -> Result<(), ProviderError> {
        let delta = match chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("delta"))
        {
            Some(d) => d,
            None => return Ok(()),
        };

        // Answer content.
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
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

        // Reasoning channel — models spell it `reasoning` OR `reasoning_content`.
        let reasoning = delta
            .get("reasoning")
            .and_then(Value::as_str)
            .or_else(|| delta.get("reasoning_content").and_then(Value::as_str));
        if let Some(text) = reasoning {
            if !text.is_empty() {
                if self.total_output_len() + text.len() > MAX_TOTAL_OUTPUT_BYTES {
                    return Err(ProviderError::Transport(
                        "model stream exceeded the maximum accumulated output size".to_string(),
                    ));
                }
                self.reasoning.push_str(text);
                on_event(RunEvent::ReasoningDelta {
                    chunk: text.to_string(),
                });
            }
        }

        // Tool-call fragments, accumulated by `index`. The `function.name` is sent
        // once; `function.arguments` streams in pieces we concatenate per index.
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                // Fall back to index 0 when the server omits `index` on a
                // single-tool stream.
                let index = call.get("index").and_then(Value::as_i64).unwrap_or(0);
                let entry = self.tool_calls.entry(index).or_default();

                // Gemini streams the opaque `extra_content` (thought_signature) on
                // the tool_call fragment (a sibling of `function`); capture it from
                // whichever fragment carries it so it survives to `into_turn`
                // (tuxlink-0tuc3). Last non-null wins if repeated.
                if let Some(meta) = call.get("extra_content") {
                    if !meta.is_null() {
                        entry.extra_content = Some(meta.clone());
                    }
                }

                if let Some(function) = call.get("function") {
                    if let Some(name) = function.get("name").and_then(Value::as_str) {
                        if !name.is_empty() {
                            entry.name = name.to_string();
                        }
                    }
                    // `arguments` is a JSON string on the OpenAI wire (streamed in
                    // fragments we concatenate). Some OpenAI-compat servers (newer
                    // Gemini via `/v1beta/openai/`) instead send a COMPLETE JSON
                    // object; serialize it into the buffer so `into_turn`'s
                    // `from_str` recovers it identically to the string path
                    // (tuxlink-fzj9a).
                    match function.get("arguments") {
                        Some(Value::String(args)) => entry.arguments.push_str(args),
                        Some(obj @ Value::Object(_)) => {
                            if let Ok(s) = serde_json::to_string(obj) {
                                entry.arguments.push_str(&s);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Finalize into a [`ModelTurn`]. If any tool calls accumulated, they win
    /// (mirroring `parse_completion`'s precedence): each accumulated `arguments`
    /// string is parsed to a `Value`, with an unparseable/empty string becoming
    /// `Value::Null` so the runner's COR-3 re-prompts (identical to the
    /// non-stream contract). Otherwise the concatenated content becomes a `Text`
    /// turn.
    fn into_turn(self) -> ModelTurn {
        if !self.tool_calls.is_empty() {
            let calls = self
                .tool_calls
                .into_values()
                .map(|p| ToolCall {
                    name: p.name,
                    args: serde_json::from_str::<Value>(&p.arguments).unwrap_or(Value::Null),
                    provider_meta: p.extra_content,
                })
                .collect();
            return ModelTurn::ToolCalls(calls);
        }
        ModelTurn::Text(self.content)
    }

    /// The reasoning text accumulated across `ReasoningDelta` fragments. The
    /// finalized [`ModelTurn`] never carries reasoning (it streams delta-only via
    /// the sink), but the concatenated trace is retained so a test can assert the
    /// fragments were concatenated in order.
    #[cfg(test)]
    fn reasoning(&self) -> &str {
        &self.reasoning
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

/// System prompt injected at the front of every Elmer request. Provides
/// station-context framing so the model never asks the operator for information
/// (such as location) that Tuxlink already exposes through its tools.
pub(crate) const ELMER_SYSTEM_PROMPT: &str = "\
You are Elmer, an AI assistant embedded in Tuxlink — a Winlink and amateur-radio \
station application — helping the licensed operator who is running this app. \
You have read-only tools that report the operator's OWN station state: their \
location/grid (position_status), rig, modem, mailbox, nearby stations, \
propagation and solar/space-weather. \
When a request depends on the operator's location or station context, CALL the \
appropriate tool to get it — never ask the operator for information Tuxlink \
already has (for example, never ask 'what is your location?'; call \
position_status). \
\
You can call tools as many times as a request needs, and call several in \
sequence, within one reply. Many useful requests require exactly this: to \
answer 'which nearby VARA stations have the best predicted path', call \
find_stations to get the candidates, then call predict_path for each candidate, \
then rank and present the real results. Work the request with the tools — do \
NOT refuse a multi-step task, cap how many tool calls you will make, or tell the \
operator to run the tools themselves. Building a ranked list, table, or summary \
FROM real tool results is exactly your job and is NOT fabrication. \
\
You STAGE outbound traffic — a Winlink message (message_send), a Request Center \
inquiry (catalog_send_inquiry), a GRIB weather-product request \
(grib_send_request), a form (send_form) — into the local outbox. Staging is \
local and always available regardless of send authority. The Winlink Request \
Center is a large on-demand catalog: call catalog_list to see everything the \
operator can request — propagation forecasts, METAR airport weather, satellite \
keplerian data, aurora and marine forecasts, ARES/RACES bulletins, and much \
more — then stage the matching item(s) with catalog_send_inquiry. Do NOT tell \
the operator the Request Center only offers GRIB or weather; it offers hundreds \
of products. When the operator asks for something a staged request would \
deliver, stage the appropriate request rather than just saying you cannot fetch \
it live. \
\
Sending authority: you can connect and transmit when the operator has ARMED \
send authority. The arm is a time-boxed grant — it IS the operator's Part 97 \
consent for that window. While armed, you may iterate connect attempts \
autonomously: dial a station, read the link result, try the next station or \
band, until a link establishes or options are exhausted; no per-attempt \
approval dialog is required. Egress is DENIED when send authority is disarmed, \
has expired, or when the session is TAINTED (reading an untrusted inbound \
message taints the session and blocks sending until the operator starts a fresh \
authorized session). Do not treat a denial as an error to route around — it \
means you are not currently authorized to transmit. The operator can abort at \
any time; an abort request is sent immediately and stops the active session. You cannot change the CMS \
host, credentials, or other protected configuration — those tools are not on \
your surface. \
\
Do NOT claim a message has been sent or delivered when you have only staged it. \
Do NOT tell the operator to wait for, or poll for, a reply to something that \
has not been transmitted yet. NEVER fabricate data a tool did not return — if a \
tool has not run or returned no real result (for example an actual weather \
forecast that only arrives after the operator transmits a GRIB request), say so \
plainly and never invent values, tables, or station lists out of thin air. This \
rule is about inventing data you do not have; it does NOT mean avoiding tables or \
rankings built from real tool output, which you should produce freely. \
\
Be concise and practical.";

/// Reserve, in estimated tokens, held back from the context budget for the
/// model's response. gpt-oss emits a Harmony reasoning channel on top of the
/// final answer, so this is generous.
const RESPONSE_RESERVE_TOKENS: usize = 4096;

/// Conservative token estimate for a byte string. Real BPE is ~4 chars/token for
/// English and ~3 for dense JSON; dividing by 3 OVER-estimates on purpose so the
/// trim errs toward sending less, never overflowing. `+8` covers per-message
/// role/framing overhead.
fn estimate_text_tokens(s: &str) -> usize {
    s.len() / 3 + 8
}

/// Conservative token estimate for one transcript message. Errs HIGH so the trim
/// sends less rather than overflowing; a `ToolCall` also carries the rendered
/// `tool_calls` JSON envelope (id / type / function wrapper + escaped arguments),
/// so it is charged extra overhead beyond its raw args (Codex P2).
fn estimate_message_tokens(msg: &Message) -> usize {
    match msg {
        Message::User(t) | Message::Assistant(t) => t.len() / 3 + 8,
        Message::ToolResult { name, content, .. } => (name.len() + content.len()) / 3 + 12,
        Message::ToolCall(c) => {
            let args = serde_json::to_string(&c.args).map(|s| s.len()).unwrap_or(0);
            // +40: the assistant tool_calls object (id, "type":"function", nesting,
            // JSON-string-escaped arguments) is materially larger than raw args.
            (c.name.len() + args) / 3 + 40
        }
    }
}

/// Split the transcript into ATOMIC trim units. A text turn (User/Assistant) is a
/// singleton; a contiguous run of tool-activity messages (ToolCall/ToolResult) is
/// ONE unit. Units are kept or dropped whole, so a tool block is never split —
/// which would leave a `ToolResult` with no matching `ToolCall` (FIFO-paired in
/// [`build_request_body`]) and the server 400s it (Codex P1). Returns half-open
/// ranges over `messages`, in order.
fn group_trim_units(messages: &[Message]) -> Vec<std::ops::Range<usize>> {
    let mut units = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if matches!(messages[i], Message::ToolCall(_) | Message::ToolResult { .. }) {
            let start = i;
            while i < messages.len()
                && matches!(messages[i], Message::ToolCall(_) | Message::ToolResult { .. })
            {
                i += 1;
            }
            units.push(start..i);
        } else {
            units.push(i..i + 1);
            i += 1;
        }
    }
    units
}

/// Trim the transcript to fit `budget_tokens`, keeping the MOST RECENT complete
/// units. The system prompt + tool schemas are counted separately by the caller
/// and are NOT part of `budget_tokens`; this only bounds the transcript.
///
/// Correctness invariants (Codex P1):
/// * NEVER splits a tool-activity block — no orphan `ToolResult` can reach the
///   wire (which would 400 on a missing `tool_call_id`).
/// * NEVER returns empty for a non-empty transcript — the newest unit is always
///   kept even if it alone exceeds the budget. A single over-budget unit is a
///   pathological case the (generously sized) server context backstops.
pub fn trim_messages_to_budget(messages: &[Message], budget_tokens: usize) -> Vec<Message> {
    if messages.is_empty() {
        return Vec::new();
    }
    let units = group_trim_units(messages);
    let unit_tokens = |r: &std::ops::Range<usize>| -> usize {
        messages[r.clone()].iter().map(estimate_message_tokens).sum()
    };
    // Always keep the newest unit; then extend backward one whole unit at a time
    // while it fits.
    let mut keep_from = units.len() - 1;
    let mut used = unit_tokens(&units[keep_from]);
    while keep_from > 0 {
        let t = unit_tokens(&units[keep_from - 1]);
        if used + t > budget_tokens {
            break;
        }
        used += t;
        keep_from -= 1;
    }
    messages[units[keep_from].start..].to_vec()
}

/// Compute the transcript token budget for a context window, or `None` when no
/// window is configured (no trim). `overhead` = system prompt + tool schemas +
/// response reserve; the transcript gets whatever remains.
fn transcript_budget(num_ctx: Option<u32>, system_prompt: &str, tools: &[ToolSpec]) -> Option<usize> {
    let nctx = num_ctx? as usize;
    // Estimate the ACTUAL serialized tool JSON (MCP schemas vary widely — a flat
    // per-tool guess is wrong in both directions; Codex P2). Fall back to a
    // generous per-tool constant only if the specs somehow fail to serialize.
    let tools_tokens = serde_json::to_string(tools)
        .map(|s| s.len() / 3)
        .unwrap_or(tools.len() * 300);
    let overhead = estimate_text_tokens(system_prompt) + tools_tokens + RESPONSE_RESERVE_TOKENS;
    Some(nctx.saturating_sub(overhead))
}

/// Build the chat-completions request body from the transcript + tool surface.
/// Pure — no IO. Exposed for unit testing the message + tools shaping.
///
/// `temperature` is forwarded to the request body as `"temperature"` when
/// `Some`; omitted entirely when `None` so the server default applies.
///
/// `system_prompt` is the effective system prompt (the operator override or the
/// built-in [`ELMER_SYSTEM_PROMPT`], resolved by the caller — see
/// [`OpenAiProvider::turn`]).
///
/// Heuristic: does `model` name a Gemini model? Used to gate echoing Gemini's
/// `extra_content` (thought_signature) back — only a Gemini destination expects it.
/// Covers both the direct id (`gemini-3.5-flash`) and namespaced routes
/// (`google/gemini-3.5-flash`, e.g. via OpenRouter).
fn is_gemini_model(model: &str) -> bool {
    model.to_ascii_lowercase().contains("gemini")
}

pub fn build_request_body(
    model: &str,
    conversation: &Conversation,
    tools: &[ToolSpec],
    temperature: Option<f32>,
    system_prompt: &str,
) -> Value {
    let system_message =
        serde_json::json!({ "role": "system", "content": system_prompt });
    let mut messages: Vec<Value> = Vec::with_capacity(conversation.messages().len() + 1);
    messages.push(system_message);
    // Render the transcript into OpenAI chat messages. Tool calls/results use the
    // STANDARD OpenAI tool-calling protocol: the assistant message carries a
    // `tool_calls` array (each with an id and a JSON-STRING `arguments`), and each
    // tool result is a `tool`-role message that references that call's
    // `tool_call_id`. Strict OpenAI-compat providers (Gemini's compat endpoint,
    // OpenAI, ...) reject a `tool` message with no matching `tool_call_id` with an
    // opaque HTTP 400 INVALID_ARGUMENT; only permissive local servers (Ollama)
    // tolerated the old lenient text form. The transcript does not carry the
    // model's original ids, so we mint stable synthetic ones (`call_0`, `call_1`,
    // …) and pair each result to its call FIFO — the runner appends a ToolCall
    // immediately followed by its ToolResult, so FIFO order is the call order.
    let mut next_tool_id: usize = 0;
    let mut pending_tool_ids: std::collections::VecDeque<String> =
        std::collections::VecDeque::new();
    for msg in conversation.messages() {
        match msg {
            Message::ToolCall(call) => {
                let id = format!("call_{next_tool_id}");
                next_tool_id += 1;
                pending_tool_ids.push_back(id.clone());
                // OpenAI requires `arguments` to be a JSON STRING, not an object.
                let arguments = if call.args.is_null() {
                    "{}".to_string()
                } else {
                    serde_json::to_string(&call.args).unwrap_or_else(|_| "{}".to_string())
                };
                // Gemini 3.x "thinking" models require their per-tool-call
                // `extra_content` (carrying `google.thought_signature`) echoed back
                // verbatim on the assistant `tool_calls[]`, or the NEXT turn is
                // rejected with HTTP 400 INVALID_ARGUMENT (tuxlink-0tuc3). We stored
                // it in `provider_meta` when parsing the response; re-emit it here —
                // but ONLY to a Gemini destination. If the operator switches Elmer to
                // another OpenAI-compat provider mid-conversation, the persisted
                // provider_meta from an earlier Gemini turn must NOT be echoed to the
                // new endpoint (strict endpoints reject unknown fields, and it is
                // meaningless there) — Codex adrev 2026-07-05 P2.
                let mut tool_call = json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": call.name, "arguments": arguments }
                });
                if is_gemini_model(model) {
                    if let Some(meta) = &call.provider_meta {
                        tool_call["extra_content"] = meta.clone();
                    }
                }
                messages.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [tool_call]
                }));
            }
            Message::ToolResult { ok, content, .. } => {
                // Match this result to the most recent unmatched call. If none
                // (a result with no preceding call — not produced by the runner,
                // but keep the message well-formed), mint a standalone id.
                let id = pending_tool_ids.pop_front().unwrap_or_else(|| {
                    let orphan = format!("call_{next_tool_id}");
                    next_tool_id += 1;
                    orphan
                });
                let label = if *ok { "result" } else { "error" };
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": format!("[{label}] {content}")
                }));
            }
            other => messages.push(render_message(other)),
        }
    }

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

    // `temperature` is omitted entirely when `None` (server default); present
    // and typed as a JSON number when `Some`.
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }

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
        // Tool call/result messages are rendered STATEFULLY in build_request_body:
        // they need paired synthetic tool_call_ids for the OpenAI tool-calling
        // protocol, which a per-message stateless render cannot produce.
        Message::ToolCall(_) | Message::ToolResult { .. } => {
            unreachable!("tool messages are rendered in build_request_body's loop")
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
/// Coerce a tool call's `function.arguments` field into a `Value`, accepting
/// BOTH the OpenAI wire form (a JSON *string*) and the object form that many
/// OpenAI-*compatible* servers emit.
///
/// The OpenAI spec specifies `arguments` as a stringified JSON object. Ollama's
/// native path and Google's `/v1beta/openai/` layer on newer Gemini models
/// (3.1-pro, 3.5-flash) instead return a native JSON *object*; 2.5-flash
/// returned a string. Accepting both keeps a conforming string working while
/// unbreaking the object emitters (tuxlink-fzj9a):
///
/// * `String` → parse it (unparseable → `Null`).
/// * `Object` → use it directly.
/// * absent / any other shape → `Null`.
///
/// A genuinely malformed payload still yields `Null` so the runner's COR-3
/// schema check treats it as a malformed call and re-prompts — we never
/// silently fabricate a valid-looking object.
fn coerce_tool_arguments(argsv: Option<&Value>) -> Value {
    match argsv {
        Some(Value::String(s)) => serde_json::from_str::<Value>(s).unwrap_or(Value::Null),
        Some(obj @ Value::Object(_)) => obj.clone(),
        _ => Value::Null,
    }
}

/// Parse one `choices[].message.tool_calls[]` entry into a [`ToolCall`].
///
/// `function.arguments` may be a JSON string (OpenAI wire form) or a JSON
/// object (Ollama / newer-Gemini compat form); both are accepted via
/// [`coerce_tool_arguments`].
fn parse_tool_call(tc: &Value) -> ToolCall {
    let function = tc.get("function");
    let name = function
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let args = coerce_tool_arguments(function.and_then(|f| f.get("arguments")));

    // Preserve Gemini's opaque per-call `extra_content` (carries
    // `google.thought_signature`) so it can be echoed back on the next request
    // (tuxlink-0tuc3). Any provider that omits the field leaves this `None`.
    let provider_meta = tc.get("extra_content").cloned();

    ToolCall {
        name,
        args,
        provider_meta,
    }
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

    // --- trim_messages_to_budget: client-side context trim (tuxlink-evucv) ---

    fn user(s: &str) -> Message {
        Message::User(s.to_string())
    }
    fn asst(s: &str) -> Message {
        Message::Assistant(s.to_string())
    }
    fn tcall(name: &str) -> Message {
        Message::ToolCall(ToolCall::new(name, serde_json::json!({})))
    }
    fn tresult(name: &str, content: &str) -> Message {
        Message::ToolResult {
            name: name.to_string(),
            ok: true,
            content: content.to_string(),
        }
    }

    /// A generous budget keeps the whole transcript (no trim).
    #[test]
    fn trim_keeps_everything_under_budget() {
        let msgs = vec![user("hi"), asst("hello"), user("more")];
        let kept = trim_messages_to_budget(&msgs, 100_000);
        assert_eq!(kept, msgs, "nothing should be dropped under a large budget");
    }

    /// A tiny budget keeps at least the most recent message (never returns empty).
    #[test]
    fn trim_always_keeps_latest() {
        let msgs = vec![user("old"), asst("mid"), user("newest")];
        let kept = trim_messages_to_budget(&msgs, 1);
        assert_eq!(kept, vec![user("newest")], "must keep the newest turn even over budget");
    }

    /// Trimming keeps the newest turns and drops the oldest.
    #[test]
    fn trim_drops_oldest_first() {
        // Each message ~ len/3 + 8 tokens. Use long strings so budget bites.
        let msgs = vec![
            user(&"a".repeat(300)), // ~108 tokens
            asst(&"b".repeat(300)), // ~108
            user(&"c".repeat(300)), // ~108
        ];
        let kept = trim_messages_to_budget(&msgs, 150); // room for ~1 msg
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0], msgs[2], "the newest message is kept");
    }

    /// A ToolResult whose ToolCall was trimmed off must NOT survive alone — an
    /// orphan `tool` message has no `tool_call_id` and the server 400s it.
    #[test]
    fn trim_drops_orphan_leading_tool_result() {
        let msgs = vec![
            user(&"q".repeat(600)),      // old, will be dropped
            tcall("find_stations"),      // old, dropped
            tresult("find_stations", &"r".repeat(600)), // would be the oldest kept -> orphan
            asst("here you go"),
        ];
        // Budget large enough to keep the last ~2 messages but not the tcall.
        let kept = trim_messages_to_budget(&msgs, 230);
        assert!(
            !matches!(kept.first(), Some(Message::ToolResult { .. })),
            "a leading orphan ToolResult must be dropped, got: {kept:?}"
        );
        assert_eq!(kept.last(), Some(&asst("here you go")));
    }

    /// A paired ToolCall+ToolResult that both fit are kept together and in order.
    #[test]
    fn trim_keeps_intact_tool_pair() {
        let msgs = vec![user("do it"), tcall("t"), tresult("t", "ok"), asst("done")];
        let kept = trim_messages_to_budget(&msgs, 100_000);
        assert_eq!(kept, msgs);
    }

    /// No budget configured => transcript_budget is None => caller does not trim.
    #[test]
    fn transcript_budget_none_when_num_ctx_unset() {
        assert_eq!(transcript_budget(None, "sys", &[]), None);
    }

    /// A configured window subtracts system-prompt + response-reserve overhead.
    #[test]
    fn transcript_budget_subtracts_overhead() {
        let b = transcript_budget(Some(32_768), "short system", &[]).unwrap();
        assert!(b < 32_768 && b > 32_768 - RESPONSE_RESERVE_TOKENS - 100,
                "budget {b} should be nctx minus a small overhead");
    }

    /// Codex P1: even a zero budget never yields an empty transcript, and never a
    /// leading orphan ToolResult (the newest whole unit is kept).
    #[test]
    fn trim_never_returns_empty_or_orphan() {
        let msgs = vec![user("q"), tcall("t"), tresult("t", "r")];
        let kept = trim_messages_to_budget(&msgs, 0);
        assert!(!kept.is_empty(), "must never send an empty transcript");
        assert!(
            !matches!(kept.first(), Some(Message::ToolResult { .. })),
            "must never start with an orphan ToolResult, got: {kept:?}"
        );
    }

    /// Codex P1: a tool-activity block is atomic — trimming keeps it whole (both
    /// call and result) or drops it whole, never splitting to orphan a result.
    /// Even back-to-back calls (parallel tool use) stay together.
    #[test]
    fn trim_never_splits_a_tool_block() {
        let msgs = vec![
            user("old"),
            tcall("a"),
            tcall("b"),
            tresult("a", "ra"),
            tresult("b", "rb"),
            asst("done"),
        ];
        // Budget that fits the tail but pressures the tool block.
        for budget in [0, 20, 60, 200, 100_000] {
            let kept = trim_messages_to_budget(&msgs, budget);
            // If any ToolResult is kept, its whole block (starting at a ToolCall) is kept.
            let has_result = kept.iter().any(|m| matches!(m, Message::ToolResult { .. }));
            if has_result {
                let first_tool = kept.iter().position(|m| {
                    matches!(m, Message::ToolCall(_) | Message::ToolResult { .. })
                });
                assert!(
                    matches!(kept[first_tool.unwrap()], Message::ToolCall(_)),
                    "a kept tool block must begin with a ToolCall (budget={budget}): {kept:?}"
                );
            }
            assert!(!kept.is_empty());
        }
    }

    // --- redacted_url: credential-safe request URL for error/log lines -------

    /// The transport error must name the URL that was requested (so a base URL
    /// missing `/chat/completions` is obvious), but must NEVER leak userinfo or a
    /// query string into the message. This is the instrumentation fix for the
    /// recurring custom-endpoint 404 (tuxlink-1hv4j).
    #[test]
    fn redacted_url_shows_path_but_hides_credentials() {
        // A base URL (the exact misconfiguration that 404s) — the path must show.
        let base = Url::parse("https://elmer-pod.example.ts.net/v1").unwrap();
        assert_eq!(redacted_url(&base), "https://elmer-pod.example.ts.net/v1");

        // Port is preserved; userinfo and query are stripped.
        let with_creds =
            Url::parse("https://user:sk-secret@host.example:8443/v1/chat/completions?token=abc")
                .unwrap();
        let shown = redacted_url(&with_creds);
        assert_eq!(shown, "https://host.example:8443/v1/chat/completions");
        assert!(!shown.contains("sk-secret"), "must not leak userinfo: {shown}");
        assert!(!shown.contains("token=abc"), "must not leak query: {shown}");
    }

    // --- error_cause_chain: surface the real transport cause (tuxlink-a1xwx) ---

    /// A reqwest-style wrapper ("error sending request") must not hide its real
    /// cause ("connection refused") — the chain is walked and joined.
    #[test]
    fn error_cause_chain_walks_source_chain() {
        #[derive(Debug)]
        struct Inner;
        impl std::fmt::Display for Inner {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "connection refused")
            }
        }
        impl std::error::Error for Inner {}
        #[derive(Debug)]
        struct Outer(Inner);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "error sending request")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        assert_eq!(
            error_cause_chain(&Outer(Inner)),
            "error sending request: connection refused"
        );
    }

    /// A single error with no `source()` renders as just its own message.
    #[test]
    fn error_cause_chain_single_error_has_no_suffix() {
        let e = std::io::Error::other("boom");
        assert_eq!(error_cause_chain(&e), "boom");
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

    // --- Gemini 3.x thought_signature round-trip (tuxlink-0tuc3) -------------
    // Gemini's OpenAI-compat layer returns a per-tool-call
    // `extra_content.google.thought_signature` and REJECTS the next turn (HTTP 400)
    // unless it is echoed back verbatim on the assistant `tool_calls[]`. The
    // transport must (a) capture it into `provider_meta` when parsing, and (b)
    // re-emit it when building the next request.

    #[test]
    fn parses_gemini_extra_content_into_provider_meta() {
        let extra = json!({ "google": { "thought_signature": "SIG_ABC123" } });
        let recorded = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "find_stations", "arguments": "{}" },
                        "extra_content": extra,
                    }]
                }
            }]
        });
        match parse_completion(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].provider_meta.as_ref(), Some(&extra),
                    "the tool_call's extra_content must be captured into provider_meta");
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn parse_leaves_provider_meta_none_when_extra_content_absent() {
        let recorded = json!({
            "choices": [{ "message": { "role": "assistant", "tool_calls": [
                { "function": { "name": "a", "arguments": "{}" } }
            ] } }]
        });
        match parse_completion(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => assert!(calls[0].provider_meta.is_none()),
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn build_request_body_echoes_provider_meta_as_extra_content() {
        let extra = json!({ "google": { "thought_signature": "SIG_XYZ" } });
        let mut convo = Conversation::new("go");
        convo.push_tool_call(
            ToolCall::new("find_stations", json!({})).with_provider_meta(Some(extra.clone())),
        );
        let body = build_request_body("gemini-3.5-flash", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        // messages[0]=system, [1]=user "go", [2]=the assistant tool_call.
        let tc = &body["messages"][2]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], json!("find_stations"), "wrong message indexed");
        assert_eq!(tc["extra_content"], extra,
            "provider_meta must be echoed back verbatim as tool_calls[].extra_content");
    }

    #[test]
    fn build_request_body_omits_extra_content_when_provider_meta_none() {
        let mut convo = Conversation::new("go");
        convo.push_tool_call(ToolCall::new("find_stations", json!({})));
        let body = build_request_body("gpt-4o", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let tc = &body["messages"][2]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], json!("find_stations"), "wrong message indexed");
        assert!(tc.get("extra_content").is_none(),
            "non-Gemini tool calls must not carry an extra_content field");
    }

    #[test]
    fn build_request_body_does_not_leak_provider_meta_to_non_gemini_destination() {
        // A conversation that HAS a Gemini thought_signature, then the operator
        // switches Elmer to a non-Gemini OpenAI-compat endpoint mid-conversation:
        // the signature must NOT be echoed to the new provider (Codex adrev P2).
        let extra = json!({ "google": { "thought_signature": "SIG_LEAK" } });
        let mut convo = Conversation::new("go");
        convo.push_tool_call(
            ToolCall::new("find_stations", json!({})).with_provider_meta(Some(extra)),
        );
        let body = build_request_body("gpt-4o", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let tc = &body["messages"][2]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], json!("find_stations"), "wrong message indexed");
        assert!(tc.get("extra_content").is_none(),
            "provider_meta must NOT leak to a non-Gemini endpoint after a provider switch");
    }

    #[test]
    fn is_gemini_model_matches_direct_and_namespaced_ids() {
        assert!(is_gemini_model("gemini-3.5-flash"));
        assert!(is_gemini_model("google/gemini-3.5-flash")); // OpenRouter-style route
        assert!(is_gemini_model("GEMINI-3-PRO")); // case-insensitive
        assert!(!is_gemini_model("gpt-4o"));
        assert!(!is_gemini_model("claude-haiku-4-5"));
    }

    #[test]
    fn stream_captures_gemini_extra_content_into_provider_meta() {
        let extra = json!({ "google": { "thought_signature": "SIG_STREAM" } });
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        // Gemini streams extra_content alongside the tool_call fragment (sibling of
        // `function`); the signature may land on any fragment for the index.
        let body = format!(
            "{}{}",
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "name": "find_stations", "arguments": "{}" },
                  "extra_content": extra }
            ] } }] })),
            sse_frame(json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] })),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();
        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].provider_meta.as_ref(), Some(&extra),
                    "streamed extra_content must survive into provider_meta");
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
    fn parses_tool_call_arguments_as_object() {
        // Newer Gemini (3.1-pro / 3.5-flash) via `/v1beta/openai/` returns
        // `function.arguments` as a native JSON OBJECT, not the OpenAI wire
        // string. It MUST parse to the object, NOT Null — Null makes the runner
        // treat every call as malformed and re-prompt in a loop (tuxlink-fzj9a).
        let recorded = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "find_stations",
                            "arguments": { "grid": "DM79", "band": "20m" }
                        }
                    }]
                }
            }]
        });
        match parse_completion(&recorded).unwrap() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79", "band": "20m" }));
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

    /// The first message in every request MUST be the Elmer system prompt.
    /// The conversation messages follow it, shifted by one index.
    #[test]
    fn request_body_first_message_is_system_prompt() {
        let convo = Conversation::new("where am I?");
        let body = build_request_body("local-model", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        assert_eq!(
            body["messages"][0]["role"], "system",
            "messages[0] must be the system prompt"
        );
        let system_content = body["messages"][0]["content"].as_str().unwrap_or("");
        assert!(
            system_content.contains("position_status"),
            "system prompt must mention position_status so the model calls it for location questions; got: {system_content:?}"
        );
        assert!(
            system_content.contains("operator"),
            "system prompt must reference the operator; got: {system_content:?}"
        );
        // The prompt must teach the armed-send model: staging is always
        // available, transmission requires ARMED authority, tainted sessions
        // block egress. Anchor on the three load-bearing tokens.
        assert!(
            system_content.contains("STAGE")
                && system_content.contains("ARMED")
                && system_content.contains("TAINTED"),
            "system prompt must explain staging + armed send-authority + taint gate; got: {system_content:?}"
        );
        // The prompt must authorize iterative, multi-step tool use so the model
        // does not refuse tasks like 'rank the top-5 stations by predicted path'
        // (tuxlink-5cj61). Anchor on the worked example (predict_path) plus the
        // explicit not-a-refusal directive.
        assert!(
            system_content.contains("predict_path") && system_content.contains("multi-step"),
            "system prompt must authorize iterative multi-step tool use (predict_path example + 'multi-step'); got: {system_content:?}"
        );
        // The conversation's first user message is now at index 1.
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "where am I?");
    }

    /// A system-prompt OVERRIDE replaces the built-in default as messages[0]
    /// (tuxlink-31tbw). Proves a stored override reaches the compat wire.
    #[test]
    fn request_body_system_prompt_override_replaces_default() {
        let convo = Conversation::new("hi");
        let body = build_request_body("m", &convo, &[], None, "CUSTOM ELMER PROMPT");
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

    #[test]
    fn request_body_includes_model_and_tools() {
        let convo = Conversation::new("find a station near DM79");
        let body = build_request_body("local-model", &convo, &[echo_tool()], None, ELMER_SYSTEM_PROMPT);
        assert_eq!(body["model"], "local-model");
        // messages[0] is the system prompt; the first user message is at index 1.
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "find a station near DM79");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "echo");
        // The schema is passed through verbatim as `parameters`.
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn request_body_omits_tools_when_none() {
        let convo = Conversation::new("hi");
        let body = build_request_body("m", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        assert!(body.get("tools").is_none(), "tools should be absent: {body}");
    }

    #[test]
    fn tool_result_renders_as_tool_role() {
        let mut convo = Conversation::new("go");
        convo.push_tool_result("find_stations", "{\"count\":3}");
        let body = build_request_body("m", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let tool_msg = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .expect("a tool-role message");
        // New OpenAI protocol: the tool message references a `tool_call_id` (not a
        // `name`). This result has no preceding ToolCall, so it gets a standalone
        // synthetic id; the key invariant is that a tool_call_id is present.
        assert!(
            tool_msg["tool_call_id"].is_string(),
            "tool message must carry a tool_call_id (strict providers 400 without it)"
        );
        assert!(tool_msg["content"].as_str().unwrap().contains("result"));
    }

    #[test]
    fn tool_error_result_labels_error() {
        let mut convo = Conversation::new("go");
        convo.push_tool_error("message_send", "tool denied: session is tainted");
        let body = build_request_body("m", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let tool_msg = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .unwrap();
        assert!(tool_msg["content"].as_str().unwrap().contains("error"));
        assert!(tool_msg["content"].as_str().unwrap().contains("tainted"));
    }

    /// A tool call + its result render as the STANDARD OpenAI tool-calling
    /// protocol: an assistant message with a `tool_calls` array (id + JSON-STRING
    /// arguments) followed by a `tool` message whose `tool_call_id` MATCHES the
    /// call's id. Verified against Gemini's live OpenAI-compat endpoint: the prior
    /// lenient form (assistant text note + `tool` role without a tool_call_id)
    /// produced HTTP 400 INVALID_ARGUMENT; this paired form is accepted. Regression
    /// guard for tuxlink-2k0gz (Elmer cloud tool-calling).
    #[test]
    fn tool_call_and_result_use_openai_protocol_with_matching_ids() {
        let mut convo = Conversation::new("what is my position?");
        convo.push_tool_call(ToolCall::new("position_status", json!({})));
        convo.push_tool_result("position_status", "{\"grid\":\"CN87\"}");
        let body = build_request_body("gemini-2.5-flash", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        let msgs = body["messages"].as_array().expect("messages array");

        // The assistant tool-call message: tool_calls[0] with an id, function name,
        // and arguments as a JSON STRING (not an object).
        let assistant = msgs
            .iter()
            .find(|m| m["role"] == "assistant" && m["tool_calls"].is_array())
            .expect("an assistant message carrying tool_calls");
        let tc = &assistant["tool_calls"][0];
        assert_eq!(tc["type"], "function");
        assert_eq!(tc["function"]["name"], "position_status");
        assert!(
            tc["function"]["arguments"].is_string(),
            "OpenAI requires function.arguments to be a JSON string; got: {:?}",
            tc["function"]["arguments"]
        );
        let call_id = tc["id"].as_str().expect("tool call must have an id");

        // The tool result message must reference that exact id.
        let tool_msg = msgs
            .iter()
            .find(|m| m["role"] == "tool")
            .expect("a tool-role result message");
        assert_eq!(
            tool_msg["tool_call_id"].as_str(),
            Some(call_id),
            "the tool result's tool_call_id MUST match the assistant tool call's id \
             (Gemini/OpenAI 400 without this linkage)"
        );
        assert!(tool_msg["content"].as_str().unwrap().contains("CN87"));
    }

    // --- temperature forwarding -------------------------------------------

    /// `temperature: Some(0.8)` must appear in the body as a JSON number.
    #[test]
    fn request_body_includes_temperature_when_some() {
        let convo = Conversation::new("hi");
        let body = build_request_body("m", &convo, &[], Some(0.8_f32), ELMER_SYSTEM_PROMPT);
        let temp = body
            .get("temperature")
            .and_then(Value::as_f64)
            .expect("temperature must be present when Some");
        assert!(
            (temp - 0.8).abs() < 1e-6,
            "temperature must be ~0.8; got: {temp}"
        );
    }

    /// `temperature: None` must NOT add a `temperature` key to the body, so the
    /// server default is left unchanged.
    #[test]
    fn request_body_omits_temperature_when_none() {
        let convo = Conversation::new("hi");
        let body = build_request_body("m", &convo, &[], None, ELMER_SYSTEM_PROMPT);
        assert!(
            body.get("temperature").is_none(),
            "temperature must be absent when None; got: {body}"
        );
    }

    // --- SSE streaming accumulator (no network) ---------------------------

    use std::sync::Mutex;

    /// A shared recording buffer the test sink pushes `RunEvent`s into. Wrapped in
    /// `Arc<Mutex<…>>` so the `Fn(RunEvent) + Sync` sink closure can own a clone
    /// while the test still asserts on the original.
    fn recorder() -> std::sync::Arc<Mutex<Vec<RunEvent>>> {
        std::sync::Arc::new(Mutex::new(Vec::new()))
    }

    /// Wrap one OpenAI streaming chunk object as an `data: {json}\n\n` SSE frame.
    fn sse_frame(chunk: Value) -> String {
        format!("data: {chunk}\n\n")
    }

    /// Content deltas accumulate in order, each emits an `AssistantDelta`, and the
    /// finalized turn is the concatenation.
    #[test]
    fn stream_content_deltas_accumulate_and_emit_in_order() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let body = format!(
            "{}{}{}",
            sse_frame(json!({ "choices": [{ "delta": { "content": "Hello" } }] })),
            sse_frame(json!({ "choices": [{ "delta": { "content": ", " } }] })),
            sse_frame(json!({ "choices": [{ "delta": { "content": "world" } }] })),
        );
        let done = acc.feed(body.as_bytes(), &sink).unwrap();
        assert!(!done, "no [DONE] sentinel yet");

        let turn = acc.into_turn();
        assert_eq!(turn, ModelTurn::Text("Hello, world".into()));

        let recorded = events.lock().unwrap();
        assert_eq!(
            *recorded,
            vec![
                RunEvent::AssistantDelta { chunk: "Hello".into() },
                RunEvent::AssistantDelta { chunk: ", ".into() },
                RunEvent::AssistantDelta { chunk: "world".into() },
            ],
            "AssistantDelta events must arrive in stream order"
        );
    }

    /// `reasoning` spelling: reasoning fragments emit `ReasoningDelta` and never
    /// pollute the answer content.
    #[test]
    fn stream_reasoning_spelling_emits_reasoning_delta() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let body = format!(
            "{}{}{}",
            sse_frame(json!({ "choices": [{ "delta": { "reasoning": "weigh " } }] })),
            sse_frame(json!({ "choices": [{ "delta": { "reasoning": "options" } }] })),
            sse_frame(json!({ "choices": [{ "delta": { "content": "Answer" } }] })),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();
        // The reasoning trace is concatenated in order and kept distinct from the
        // answer content.
        assert_eq!(acc.reasoning(), "weigh options");
        assert_eq!(acc.into_turn(), ModelTurn::Text("Answer".into()));

        let recorded = events.lock().unwrap();
        assert_eq!(
            *recorded,
            vec![
                RunEvent::ReasoningDelta { chunk: "weigh ".into() },
                RunEvent::ReasoningDelta { chunk: "options".into() },
                RunEvent::AssistantDelta { chunk: "Answer".into() },
            ],
            "reasoning fragments must emit ReasoningDelta, separate from the answer"
        );
    }

    /// `reasoning_content` spelling (gpt-oss-style): same behaviour as `reasoning`.
    #[test]
    fn stream_reasoning_content_spelling_emits_reasoning_delta() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let body = sse_frame(
            json!({ "choices": [{ "delta": { "reasoning_content": "thinking..." } }] }),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();

        let recorded = events.lock().unwrap();
        assert_eq!(
            *recorded,
            vec![RunEvent::ReasoningDelta { chunk: "thinking...".into() }],
            "the `reasoning_content` spelling must also emit ReasoningDelta"
        );
    }

    /// Tool calls streamed in fragments — name once, arguments in 2+ pieces, keyed
    /// by index — accumulate into `ModelTurn::ToolCalls` with the parsed args.
    #[test]
    fn stream_tool_call_fragments_accumulate_into_tool_calls() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let body = format!(
            "{}{}{}",
            // Fragment 1: name + first slice of arguments.
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "name": "find_stations", "arguments": "{\"gr" } }
            ] } }] })),
            // Fragment 2: more arguments (no name re-sent).
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "arguments": "id\":\"DM" } }
            ] } }] })),
            // Fragment 3: final arguments slice.
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "arguments": "79\"}" } }
            ] } }] })),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
        // Tool-call fragments emit no deltas (only content/reasoning do).
        assert!(
            events.lock().unwrap().is_empty(),
            "tool-call fragments must not emit Assistant/Reasoning deltas"
        );
    }

    /// Two concurrent tool calls (distinct indices) each assemble independently
    /// and come out ordered by index.
    #[test]
    fn stream_multiple_tool_calls_by_index() {
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        let body = format!(
            "{}{}",
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "name": "a", "arguments": "{\"x\":" } },
                { "index": 1, "function": { "name": "b", "arguments": "{}" } }
            ] } }] })),
            sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
                { "index": 0, "function": { "arguments": "1}" } }
            ] } }] })),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();

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

    /// An unparseable / empty accumulated arguments string becomes `Value::Null`,
    /// matching the non-stream `parse_completion` contract (runner COR-3 reprompts).
    #[test]
    fn stream_tool_call_unparseable_args_become_null() {
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        let body = sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
            { "index": 0, "function": { "name": "echo", "arguments": "{not json" } }
        ] } }] }));
        acc.feed(body.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls[0].name, "echo");
                assert_eq!(calls[0].args, Value::Null);
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// Newer Gemini (3.1-pro / 3.5-flash) via `/v1beta/openai/` streams
    /// `function.arguments` as a COMPLETE JSON object rather than fragmented
    /// strings. It must assemble into the object, not `Null` (which would make
    /// the runner treat the call as malformed and loop). tuxlink-fzj9a.
    #[test]
    fn stream_tool_call_object_arguments() {
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        let body = sse_frame(json!({ "choices": [{ "delta": { "tool_calls": [
            { "index": 0, "function": {
                "name": "find_stations",
                "arguments": { "grid": "DM79", "band": "20m" }
            } }
        ] } }] }));
        acc.feed(body.as_bytes(), &sink).unwrap();

        match acc.into_turn() {
            ModelTurn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "find_stations");
                assert_eq!(calls[0].args, json!({ "grid": "DM79", "band": "20m" }));
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    /// `[DONE]` terminates: `feed` returns `true` and any frames AFTER the
    /// sentinel in the same byte chunk are not processed.
    #[test]
    fn stream_done_sentinel_terminates() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let body = format!(
            "{}{}{}",
            sse_frame(json!({ "choices": [{ "delta": { "content": "kept" } }] })),
            "data: [DONE]\n\n",
            sse_frame(json!({ "choices": [{ "delta": { "content": "dropped" } }] })),
        );
        let done = acc.feed(body.as_bytes(), &sink).unwrap();
        assert!(done, "feed must report termination once [DONE] is seen");

        assert_eq!(acc.into_turn(), ModelTurn::Text("kept".into()));
        assert_eq!(
            *events.lock().unwrap(),
            vec![RunEvent::AssistantDelta { chunk: "kept".into() }],
            "frames after [DONE] in the same chunk must not be processed"
        );
    }

    /// A frame split across two byte chunks (mid-JSON boundary) is buffered and
    /// parsed once the remainder arrives — the buffer/split logic must not drop or
    /// double-emit the delta.
    #[test]
    fn stream_frame_split_across_chunks_is_buffered() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        let full = sse_frame(json!({ "choices": [{ "delta": { "content": "split-token" } }] }));
        // Cut the frame in the MIDDLE of the JSON payload (not on a frame
        // boundary) so the first chunk has no complete `\n\n` frame.
        let cut = full.len() / 2;
        let (first, second) = full.split_at(cut);

        let done_a = acc.feed(first.as_bytes(), &sink).unwrap();
        assert!(!done_a, "partial frame must not terminate");
        assert!(
            events.lock().unwrap().is_empty(),
            "no delta should emit while the frame is still incomplete"
        );

        acc.feed(second.as_bytes(), &sink).unwrap();
        assert_eq!(acc.into_turn(), ModelTurn::Text("split-token".into()));
        assert_eq!(
            *events.lock().unwrap(),
            vec![RunEvent::AssistantDelta { chunk: "split-token".into() }],
            "the buffered frame must emit exactly one delta once completed"
        );
    }

    /// CRLF frame delimiters (`\r\n\r\n`) are normalised and parsed identically to
    /// `\n\n`, and a trailing frame without a closing blank line is flushed by
    /// `finish`.
    #[test]
    fn stream_crlf_and_trailing_frame_without_blank_line() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        // First frame CRLF-terminated; second frame has NO trailing blank line.
        let body = format!(
            "data: {}\r\n\r\ndata: {}",
            json!({ "choices": [{ "delta": { "content": "a" } }] }),
            json!({ "choices": [{ "delta": { "content": "b" } }] }),
        );
        acc.feed(body.as_bytes(), &sink).unwrap();
        // The second frame is still buffered (no blank line yet).
        assert_eq!(
            *events.lock().unwrap(),
            vec![RunEvent::AssistantDelta { chunk: "a".into() }]
        );
        // Stream closes: finish() flushes the trailing frame.
        acc.finish(&sink).unwrap();
        assert_eq!(acc.into_turn(), ModelTurn::Text("ab".into()));
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                RunEvent::AssistantDelta { chunk: "a".into() },
                RunEvent::AssistantDelta { chunk: "b".into() },
            ]
        );
    }

    /// Empty content deltas (some servers send `{"content":""}` keep-alive-ish
    /// frames) emit nothing and contribute nothing.
    #[test]
    fn stream_empty_content_delta_emits_nothing() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };
        let mut acc = SseAccumulator::new();
        let body = sse_frame(json!({ "choices": [{ "delta": { "content": "" } }] }));
        acc.feed(body.as_bytes(), &sink).unwrap();
        assert!(events.lock().unwrap().is_empty(), "empty content must emit no delta");
        assert_eq!(acc.into_turn(), ModelTurn::Text(String::new()));
    }

    /// REGRESSION GUARD — UTF-8 codepoint split across two `feed()` calls.
    ///
    /// A multi-byte UTF-8 codepoint whose bytes straddle a network-chunk boundary
    /// must be reassembled into a complete frame and decoded intact — the old
    /// lossy-`String` buffer decoded each half separately, turning the split byte
    /// into U+FFFD replacement chars and corrupting non-ASCII tokens (emoji, CJK,
    /// smart punctuation). Here the frame carries "héllo 🌍"; the byte buffer is
    /// cut in the MIDDLE of the emoji (a 4-byte codepoint) so neither half is
    /// valid UTF-8 on its own.
    #[test]
    fn stream_multibyte_codepoint_split_across_feeds_is_not_corrupted() {
        let events = recorder();
        let sink = {
            let events = events.clone();
            move |e: RunEvent| events.lock().unwrap().push(e)
        };

        let mut acc = SseAccumulator::new();
        // "héllo 🌍" — 'é' is 2 bytes (0xC3 0xA9), '🌍' is 4 bytes (0xF0 0x9F 0x8C 0x8D).
        let payload = "héllo 🌍";
        let full = sse_frame(json!({ "choices": [{ "delta": { "content": payload } }] }));
        let full_bytes = full.as_bytes();

        // Find a cut point INSIDE the emoji's 4-byte sequence so the split lands
        // mid-codepoint (neither half is valid UTF-8 alone).
        let emoji_start = full.find('🌍').expect("emoji present in frame");
        let cut = emoji_start + 2; // 2 bytes into the 4-byte emoji
        let (first, second) = full_bytes.split_at(cut);
        assert!(
            std::str::from_utf8(first).is_err(),
            "test precondition: the first chunk must end mid-codepoint (invalid UTF-8 alone)"
        );

        let done_a = acc.feed(first, &sink).unwrap();
        assert!(!done_a, "partial frame must not terminate");
        assert!(
            events.lock().unwrap().is_empty(),
            "no delta should emit while the frame (and its codepoint) is incomplete"
        );

        acc.feed(second, &sink).unwrap();
        // The reassembled content must be byte-for-byte the original string — no
        // U+FFFD replacement chars anywhere.
        assert_eq!(acc.into_turn(), ModelTurn::Text(payload.into()));
        assert_eq!(
            *events.lock().unwrap(),
            vec![RunEvent::AssistantDelta { chunk: payload.into() }],
            "the reassembled multi-byte content must decode intact (no U+FFFD)"
        );
    }

    /// An un-terminated frame (no blank line) larger than `MAX_PENDING_FRAME_BYTES`
    /// must surface a `ProviderError::Transport` rather than growing `buf`
    /// unbounded. The error message must carry no body content.
    #[test]
    fn stream_oversized_pending_frame_errors() {
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        // A single `data:` line with no terminating blank line, longer than the cap.
        let mut oversized = b"data: ".to_vec();
        oversized.extend(vec![b'x'; MAX_PENDING_FRAME_BYTES + 16]);

        let err = acc
            .feed(&oversized, &sink)
            .expect_err("an oversized un-terminated frame must error");
        match err {
            ProviderError::Transport(msg) => {
                assert!(
                    msg.contains("oversized"),
                    "expected an oversized-frame transport error; got: {msg:?}"
                );
                assert!(
                    !msg.contains('x'),
                    "the error message must not echo any frame body content; got: {msg:?}"
                );
            }
            other => panic!("expected ProviderError::Transport, got {other:?}"),
        }
    }

    /// A stream of terminated frames whose decoded content totals more than
    /// `MAX_TOTAL_OUTPUT_BYTES` must surface a `ProviderError::Transport` — this is
    /// the abuse vector that `MAX_PENDING_FRAME_BYTES` alone does not catch (each
    /// frame terminates, but the accumulator grows without bound). To keep the
    /// test fast we drive `apply_chunk` directly with a single over-cap delta
    /// rather than streaming millions of small frames.
    #[test]
    fn stream_total_output_cap_errors() {
        let sink = |_e: RunEvent| {};
        let mut acc = SseAccumulator::new();
        let big = "y".repeat(MAX_TOTAL_OUTPUT_BYTES + 1);
        let chunk = json!({ "choices": [{ "delta": { "content": big } }] });

        let err = acc
            .apply_chunk(&chunk, &sink)
            .expect_err("output past the total cap must error");
        match err {
            ProviderError::Transport(msg) => {
                assert!(
                    msg.contains("accumulated output"),
                    "expected a total-output transport error; got: {msg:?}"
                );
                assert!(
                    !msg.contains('y'),
                    "the error message must not echo accumulated content; got: {msg:?}"
                );
            }
            other => panic!("expected ProviderError::Transport, got {other:?}"),
        }
    }
}
