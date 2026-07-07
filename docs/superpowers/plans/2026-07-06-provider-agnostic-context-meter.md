# Provider-agnostic context meter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Elmer context meter render for OpenAI-compat providers (vLLM/cloud), not just native Ollama, by parsing the compat `usage` object and probing `/v1/models` for the context window.

**Architecture:** Additive to the compat adapter (`OpenAiProvider`). The numerator comes from the universal `usage.prompt_tokens`; the denominator comes from a memoized, best-effort `GET /v1/models` probe (`max_model_len`/`context_length`), and drives both the meter and the client-side trim. The event's denominator becomes `Option<u32>` end to end so the meter degrades to a bare token counter when the window is unknown. Native Ollama is untouched; `num_ctx` stays an Ollama-only causal control.

**Tech Stack:** Rust (reqwest, serde_json, async_trait, tokio) in `src-tauri`; React + TypeScript + Vitest in `src/elmer`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-06-provider-agnostic-context-meter-design.md` (authoritative).
- **No new crate dependencies** — `Cargo.lock` MUST stay unchanged (probe reuses the existing `reqwest::Client`; JSON via existing `serde_json`).
- Credential safety: any new HTTP call reuses the existing `redacted_url` / `scrub_key` / bearer-auth machinery in `provider.rs`; no key or URL-credential may reach a log or error string.
- Pure-function TDD: request/response shaping stays in pure functions unit-tested against recorded JSON with NO live network, matching the `parse_completion` / `build_request_body` pattern in `provider.rs`.
- A probe or `usage`-parse failure is **fire-and-forget**: it never changes the returned `ModelTurn` and never fails the turn.
- CI runs both arches + full vitest + `cargo clippy --all-targets -D warnings`; no intermediate dead-code state between committed tasks.
- Commit trailer on every commit: `Agent: falcon-oriole-canyon` + the `Co-Authored-By` line.
- Work is on branch `bd-tuxlink-xnenf/ctx-meter` in worktree `worktrees/bd-tuxlink-xnenf-ctx-meter`.

## File Structure

- `src-tauri/tuxlink-agent-runner/src/types.rs` — `RunEvent::ContextUsage.num_ctx: u32 → Option<u32>` (Task 1).
- `src-tauri/src/elmer/events.rs` — `ElmerEvent::Context.num_ctx: u32 → Option<u32>` (Task 1).
- `src-tauri/src/elmer/session.rs:469` — bridge pass-through (unchanged shape, recompiles) (Task 1).
- `src-tauri/tuxlink-agent-frontend/src/ollama_provider.rs` — two emit sites wrap `Some(num_ctx)` (Task 1).
- `src/elmer/elmerEvents.ts` — `ElmerContextPayload.numCtx: number → number | null` (Task 2).
- `src/elmer/ContextMeter.tsx` + `.test.tsx` — counter-mode when `numCtx == null` (Task 2).
- `src/elmer/ElmerPane.tsx` — context-pressure null guard (`:1225`), meter render (`:1410`) (Task 2); config `num_ctx` gate (`:781`) (Task 5).
- `src-tauri/tuxlink-agent-frontend/src/provider.rs` — usage parsing + emit (Task 3); `/v1/models` probe + trim rewire + drop compat `num_ctx` (Task 4).
- `src-tauri/src/elmer/provider.rs` — drop `.with_num_ctx(num_ctx)` at the 3 compat sites (Task 4).
- `src/elmer/GetKeyCard.tsx` — hide the num_ctx control on non-Ollama tiles (Task 5).

---

### Task 1: Make the meter denominator optional (Rust schema + bridge)

Makes `num_ctx` nullable end to end in Rust so a provider can emit usage without a window. Native Ollama still always sends `Some`, so its behavior is unchanged.

**Files:**
- Modify: `src-tauri/tuxlink-agent-runner/src/types.rs` (`ContextUsage` variant + its test ~296–330)
- Modify: `src-tauri/src/elmer/events.rs` (`ElmerEvent::Context` variant)
- Modify: `src-tauri/src/elmer/session.rs:469` (bridge)
- Modify: `src-tauri/tuxlink-agent-frontend/src/ollama_provider.rs` (two emit sites ~255, ~294)

**Interfaces:**
- Produces: `RunEvent::ContextUsage { prompt_tokens: u32, eval_tokens: u32, num_ctx: Option<u32> }`; `ElmerEvent::Context { prompt_tokens: u32, eval_tokens: u32, num_ctx: Option<u32> }` (serializes `numCtx: number | null`).

- [ ] **Step 1: Update the runner unit test to the optional shape (failing)**

In `types.rs`, edit `context_usage_variant_constructs_and_relays_counts` to construct with `Some` and add a `None` case:

```rust
        let event = RunEvent::ContextUsage {
            prompt_tokens: 1234,
            eval_tokens: 56,
            num_ctx: Some(32_768),
        };
```
and after the existing assertions add:
```rust
        // A windowless emit (compat path with no probed window) carries None.
        let counterless = RunEvent::ContextUsage {
            prompt_tokens: 900,
            eval_tokens: 12,
            num_ctx: None,
        };
        match counterless {
            RunEvent::ContextUsage { num_ctx, .. } => assert_eq!(num_ctx, None),
            other => panic!("expected ContextUsage, got {other:?}"),
        }
```
Also update the inner match arm `num_ctx,` assertion to `assert_eq!(*num_ctx, Some(32_768));`.

- [ ] **Step 2: Run test — verify it fails to compile**

Run: `cargo test -p tuxlink-agent-runner context_usage 2>&1 | tail -20`
Expected: FAIL — `expected u32, found Option<{integer}>` (field still `u32`).

- [ ] **Step 3: Change the field type**

In `types.rs`, in the `ContextUsage` variant, change:
```rust
        /// The context window the provider requested (`options.num_ctx`), the
        /// denominator for the fullness meter.
        num_ctx: u32,
```
to:
```rust
        /// The context window the provider requested (native Ollama
        /// `options.num_ctx`) or discovered (compat `/v1/models`), the
        /// denominator for the fullness meter. `None` when the window is
        /// unknown (compat endpoint that advertises no context length) — the
        /// meter then renders a bare token counter with no percentage.
        num_ctx: Option<u32>,
```

- [ ] **Step 4: Run test — verify it passes**

Run: `cargo test -p tuxlink-agent-runner context_usage 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Propagate through the bridge event + Ollama emits**

In `src-tauri/src/elmer/events.rs`, change the `Context` variant field `num_ctx: u32` → `num_ctx: Option<u32>` (keep the existing `#[serde(rename = "numCtx")]`). Update its doc comment to note `None → numCtx: null`.

In `src-tauri/src/elmer/session.rs:469`, the bridge arm is a field pass-through and needs no shape change; confirm it still reads:
```rust
                    RunEvent::ContextUsage { prompt_tokens, eval_tokens, num_ctx } => {
                        ElmerEvent::Context { prompt_tokens, eval_tokens, num_ctx }
                    }
```

In `ollama_provider.rs`, both emit sites currently do `num_ctx,` (an unwrapped `u32` from the `if let (Some(..), Some(num_ctx)) = ...` bindings). Change each `num_ctx,` in the `RunEvent::ContextUsage { .. }` construction to `num_ctx: Some(num_ctx),`.

- [ ] **Step 6: Build the whole backend — verify no dead code / type errors**

Run: `cargo clippy -p tuxlink-agent-frontend -p tuxlink-agent-runner --all-targets 2>&1 | tail -20 && cargo clippy -p tuxlink --all-targets 2>&1 | tail -20`
Expected: no errors, no warnings. (Native Ollama meter unchanged; only the type widened.)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/tuxlink-agent-runner/src/types.rs src-tauri/src/elmer/events.rs src-tauri/src/elmer/session.rs src-tauri/tuxlink-agent-frontend/src/ollama_provider.rs
git commit -m "feat(elmer): make context-meter denominator Option<u32> end to end

RunEvent::ContextUsage.num_ctx and ElmerEvent::Context.num_ctx become
Option<u32> so a provider can emit token usage without a known window (compat
path, next tasks). Ollama always emits Some(); native meter unchanged.

Agent: falcon-oriole-canyon
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Counter-mode meter (TS/UI)

Renders the meter as a bare token count when `numCtx` is null (window unknown), and keeps the windowed bar+% when it is present.

**Files:**
- Modify: `src/elmer/elmerEvents.ts` (`ElmerContextPayload.numCtx`)
- Modify: `src/elmer/ContextMeter.tsx`
- Modify: `src/elmer/ContextMeter.test.tsx`
- Modify: `src/elmer/ElmerPane.tsx:1225` (pressure guard), `:1410-1411` (render)

**Interfaces:**
- Consumes: event `numCtx: number | null` (Task 1's serde shape).
- Produces: `<ContextMeter promptTokens={number} numCtx={number | null} />` — windowed when a number, counter when null.

- [ ] **Step 1: Write failing counter-mode tests**

Append to `ContextMeter.test.tsx`:
```tsx
  it('counter-mode: renders bare token count when numCtx is null', () => {
    render(<ContextMeter promptTokens={12000} numCtx={null} />);
    expect(screen.getByTestId('elmer-context-meter-left').textContent).toBe('Context 12k');
    // No "/ 32k", no percentage suffix, no fill track.
    expect(screen.queryByTestId('elmer-context-meter-right')).toBeNull();
    expect(screen.queryByTestId('elmer-context-meter-track')).toBeNull();
  });

  it('counter-mode: aria-label states the window is unknown', () => {
    render(<ContextMeter promptTokens={12000} numCtx={null} />);
    const el = screen.getByTestId('elmer-context-meter');
    expect(el.getAttribute('aria-label')).toBe('Context usage: 12k tokens (window unknown)');
  });

  it('windowed mode still renders the bar when numCtx is a number', () => {
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    expect(screen.getByTestId('elmer-context-meter-track')).toBeTruthy();
    expect(screen.getByTestId('elmer-context-meter-left').textContent).toBe('Context 12k / 32k');
  });
```

- [ ] **Step 2: Run tests — verify they fail**

Run: `pnpm vitest run src/elmer/ContextMeter.test.tsx 2>&1 | tail -25`
Expected: FAIL — counter-mode cases (prop type rejects `null`, and the null branch not implemented).

- [ ] **Step 3: Implement counter-mode**

In `ContextMeter.tsx`, change the prop type and add the null branch:
```tsx
interface ContextMeterProps {
  promptTokens: number;
  numCtx: number | null;
}

export function ContextMeter({ promptTokens, numCtx }: ContextMeterProps) {
  // Counter-mode: window unknown (compat endpoint that advertises no context
  // length). Show tokens consumed only — no percentage, no fill bar — so the
  // gauge stays present and honest without a denominator it cannot trust.
  if (numCtx == null) {
    return (
      <div
        className="elmer-context-meter elmer-context-meter--counter"
        data-testid="elmer-context-meter"
        aria-label={`Context usage: ${formatK(promptTokens)} tokens (window unknown)`}
      >
        <div className="elmer-context-meter-labels">
          <span
            className="elmer-context-meter-left"
            data-testid="elmer-context-meter-left"
          >
            Context {formatK(promptTokens)}
          </span>
        </div>
      </div>
    );
  }

  // Windowed mode (unchanged below this point).
  const pct = numCtx > 0 ? Math.round((promptTokens / numCtx) * 100) : 0;
```
(Leave the rest of the windowed render exactly as-is.)

- [ ] **Step 4: Run tests — verify pass**

Run: `pnpm vitest run src/elmer/ContextMeter.test.tsx 2>&1 | tail -25`
Expected: PASS (all, including the unchanged windowed tests).

- [ ] **Step 5: Update the event type + consumers**

In `elmerEvents.ts`, change `numCtx: number;` → `numCtx: number | null;` in `ElmerContextPayload`, and update its doc line to: `numCtx — context window (Ollama num_ctx / compat probe); null when unknown → counter-mode.`

In `ElmerPane.tsx:1225`, make the pressure guard explicit about null:
```tsx
    context !== null && context.numCtx != null && context.numCtx > 0 && context.promptTokens / context.numCtx >= 0.85;
```

In `ElmerPane.tsx:1410-1411`, the render already guards `context !== null`; the prop now accepts `number | null`, so no change is needed beyond confirming it type-checks:
```tsx
      {context !== null && (
        <ContextMeter promptTokens={context.promptTokens} numCtx={context.numCtx} />
      )}
```

- [ ] **Step 6: Typecheck + full vitest for the elmer surface**

Run: `pnpm tsc --noEmit 2>&1 | tail -20 && pnpm vitest run src/elmer 2>&1 | tail -25`
Expected: no type errors; all elmer tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/elmer/ContextMeter.tsx src/elmer/ContextMeter.test.tsx src/elmer/elmerEvents.ts src/elmer/ElmerPane.tsx
git commit -m "feat(elmer): context meter counter-mode when window is unknown

ContextMeter renders a bare token count (no bar, no %) when numCtx is null, so
the gauge stays present for compat endpoints that advertise no context window.
Event payload numCtx becomes number | null; pressure warning guards null.

Agent: falcon-oriole-canyon
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Parse the compat `usage` object and emit ContextUsage (windowless)

Reads `usage.prompt_tokens` / `usage.completion_tokens` from the compat response (non-stream JSON and streamed final chunk) and emits `ContextUsage` with `num_ctx: None`. The window arrives in Task 4.

**Files:**
- Modify: `src-tauri/tuxlink-agent-frontend/src/provider.rs` (`SseAccumulator`, `OpenAiProvider::turn`, new pure `parse_usage`)

**Interfaces:**
- Produces: `fn parse_usage(value: &Value) -> Option<(u32, u32)>` returning `(prompt_tokens, eval_tokens)`; `SseAccumulator::usage() -> Option<(u32, u32)>`; a `ContextUsage { num_ctx: None, .. }` emit from `turn()` on both the stream and non-stream branches.

- [ ] **Step 1: Write failing pure-function tests for `parse_usage`**

In `provider.rs` `#[cfg(test)] mod tests`, add:
```rust
    #[test]
    fn parse_usage_reads_prompt_and_completion() {
        let v = json!({ "usage": { "prompt_tokens": 1500, "completion_tokens": 40, "total_tokens": 1540 } });
        assert_eq!(parse_usage(&v), Some((1500, 40)));
    }

    #[test]
    fn parse_usage_absent_or_partial_is_none() {
        assert_eq!(parse_usage(&json!({})), None);
        // completion_tokens missing → treat as 0 (some servers omit it on the final chunk).
        assert_eq!(parse_usage(&json!({ "usage": { "prompt_tokens": 10 } })), Some((10, 0)));
        // prompt_tokens missing → None (no usable numerator).
        assert_eq!(parse_usage(&json!({ "usage": { "completion_tokens": 5 } })), None);
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p tuxlink-agent-frontend parse_usage 2>&1 | tail -20`
Expected: FAIL — `cannot find function parse_usage`.

- [ ] **Step 3: Implement `parse_usage`**

Add near `parse_completion` in `provider.rs`:
```rust
/// Extract `(prompt_tokens, eval_tokens)` from an OpenAI-compat `usage` object.
/// `prompt_tokens` is required (it is the meter numerator); `completion_tokens`
/// defaults to 0 when a server omits it on the final streamed chunk. Returns
/// `None` when there is no usable `prompt_tokens`. Pure — no IO.
pub(crate) fn parse_usage(value: &Value) -> Option<(u32, u32)> {
    let usage = value.get("usage")?;
    let prompt = usage.get("prompt_tokens").and_then(Value::as_u64)? as u32;
    let eval = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Some((prompt, eval))
}
```

- [ ] **Step 4: Run — verify pass**

Run: `cargo test -p tuxlink-agent-frontend parse_usage 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Write failing test for streamed usage capture**

Add a test that feeds a choices-empty usage frame to `SseAccumulator` and asserts `usage()`:
```rust
    #[test]
    fn sse_accumulator_captures_trailing_usage_frame() {
        let mut acc = SseAccumulator::new();
        let sink = |_e: RunEvent| {};
        // A content frame, then the usage-only final frame vLLM/OpenAI send when
        // stream_options.include_usage=true (choices empty), then [DONE].
        let bytes = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":3}}\n\n",
            "data: [DONE]\n\n",
        );
        acc.feed(bytes.as_bytes(), &sink).unwrap();
        assert_eq!(acc.usage(), Some((200, 3)));
    }
```

- [ ] **Step 6: Run — verify fail**

Run: `cargo test -p tuxlink-agent-frontend sse_accumulator_captures_trailing_usage 2>&1 | tail -20`
Expected: FAIL — no `usage()` method / usage not captured.

- [ ] **Step 7: Capture usage in `SseAccumulator`**

Add a field `usage: Option<(u32, u32)>,` to `SseAccumulator` (init `None` in `new()`). In `apply_chunk`, BEFORE the early-return-on-missing-delta, capture usage:
```rust
        // A usage-only final chunk (choices empty) carries token counts when the
        // request set stream_options.include_usage. Capture it regardless of the
        // delta branch below.
        if let Some(u) = parse_usage(chunk) {
            self.usage = Some(u);
        }
```
Add the accessor:
```rust
    /// Token usage captured from the streamed final chunk, if the server sent one.
    fn usage(&self) -> Option<(u32, u32)> {
        self.usage
    }
```

- [ ] **Step 8: Run — verify pass**

Run: `cargo test -p tuxlink-agent-frontend sse_accumulator_captures_trailing_usage 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 9: Request usage on the stream + emit on both branches**

In `OpenAiProvider::turn`, after `body["stream"] = json!(true);` add:
```rust
        // Ask the server to append a final usage chunk to the stream (vLLM /
        // OpenAI / OpenRouter honor this; without it, streamed usage is omitted).
        body["stream_options"] = json!({ "include_usage": true });
```
In the non-stream branch, before `return parse_completion(&value)...`, emit:
```rust
            if let Some((prompt_tokens, eval_tokens)) = parse_usage(&value) {
                on_event(RunEvent::ContextUsage { prompt_tokens, eval_tokens, num_ctx: None });
            }
```
In the streaming branch, after `acc.finish(on_event)?;` and before `Ok(acc.into_turn())`, capture usage first (borrow before the `into_turn` move):
```rust
        let usage = acc.usage();
        if let Some((prompt_tokens, eval_tokens)) = usage {
            on_event(RunEvent::ContextUsage { prompt_tokens, eval_tokens, num_ctx: None });
        }
        Ok(acc.into_turn())
```

- [ ] **Step 10: Full crate test + clippy**

Run: `cargo test -p tuxlink-agent-frontend 2>&1 | tail -15 && cargo clippy -p tuxlink-agent-frontend --all-targets 2>&1 | tail -15`
Expected: all pass, no warnings.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/tuxlink-agent-frontend/src/provider.rs
git commit -m "feat(elmer): emit ContextUsage from the compat path (windowless)

Parse the OpenAI-compat usage object (prompt_tokens/completion_tokens) on both
the non-stream JSON and the streamed final chunk (stream_options.include_usage),
and emit ContextUsage with num_ctx=None. The meter now shows a token counter for
vLLM/cloud; the window denominator arrives next task.

Agent: falcon-oriole-canyon
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Probe `/v1/models` for the window; drive meter + trim; drop compat operator num_ctx

Adds the memoized, best-effort `/v1/models` probe, uses its result as the meter denominator AND the trim budget, and removes the operator `num_ctx` threading from the compat path (single task so no dead-code intermediate).

**Files:**
- Modify: `src-tauri/tuxlink-agent-frontend/src/provider.rs` (probe parser, `OpenAiProvider` field, `turn` wiring, remove `num_ctx`/`with_num_ctx`)
- Modify: `src-tauri/src/elmer/provider.rs` (drop `.with_num_ctx(num_ctx)` at the 3 compat sites, ~299, ~316, ~329)

**Interfaces:**
- Consumes: `parse_usage` (Task 3).
- Produces: `fn parse_model_context_window(models_json: &Value, model: &str) -> Option<u32>`; `OpenAiProvider` resolves a window once and emits `ContextUsage { num_ctx: <window> }` and trims to it.

- [ ] **Step 1: Failing pure-function tests for the probe parser**

In `provider.rs` tests:
```rust
    #[test]
    fn probe_reads_vllm_max_model_len() {
        let v = json!({ "data": [
            { "id": "meta-llama/Llama-3.1-8B-Instruct", "max_model_len": 32768 }
        ]});
        assert_eq!(parse_model_context_window(&v, "meta-llama/Llama-3.1-8B-Instruct"), Some(32768));
    }

    #[test]
    fn probe_reads_openrouter_context_length() {
        let v = json!({ "data": [
            { "id": "anthropic/claude-3.5-sonnet", "context_length": 200000 }
        ]});
        assert_eq!(parse_model_context_window(&v, "anthropic/claude-3.5-sonnet"), Some(200000));
    }

    #[test]
    fn probe_model_not_found_or_no_field_is_none() {
        let v = json!({ "data": [ { "id": "other", "max_model_len": 8192 } ]});
        assert_eq!(parse_model_context_window(&v, "missing"), None);
        let no_field = json!({ "data": [ { "id": "m" } ]});
        assert_eq!(parse_model_context_window(&no_field, "m"), None);
        assert_eq!(parse_model_context_window(&json!({}), "m"), None);
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p tuxlink-agent-frontend parse_model_context_window 2>&1 | tail -20`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement the probe parser**

```rust
/// Find `model`'s context window in an OpenAI-compat `/v1/models` response.
/// Reads `max_model_len` (vLLM) or `context_length` (OpenRouter); first present
/// wins. Returns `None` when the model is not listed or advertises neither
/// field (bare llama.cpp, OpenAI). Pure — no IO. Exact-id match only; never
/// guesses from a partial name.
pub(crate) fn parse_model_context_window(models_json: &Value, model: &str) -> Option<u32> {
    let entry = models_json
        .get("data")
        .and_then(Value::as_array)?
        .iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(model))?;
    entry
        .get("max_model_len")
        .and_then(Value::as_u64)
        .or_else(|| entry.get("context_length").and_then(Value::as_u64))
        .map(|n| n as u32)
}
```

- [ ] **Step 4: Run — verify pass**

Run: `cargo test -p tuxlink-agent-frontend parse_model_context_window 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Derive the `/v1/models` URL (pure helper + test)**

Add a failing test then implement:
```rust
    #[test]
    fn models_url_derives_from_chat_completions() {
        let u = Url::parse("https://host:8000/v1/chat/completions").unwrap();
        assert_eq!(models_url(&u).unwrap().as_str(), "https://host:8000/v1/models");
    }
    #[test]
    fn models_url_none_when_path_not_chat_completions() {
        let u = Url::parse("https://host/custom/path").unwrap();
        assert!(models_url(&u).is_none());
    }
```
```rust
/// Derive the `/v1/models` URL from a `…/chat/completions` endpoint by swapping
/// the trailing segment. `None` when the endpoint does not end in
/// `/chat/completions` (we then skip the probe → counter-mode). Pure.
pub(crate) fn models_url(endpoint: &Url) -> Option<Url> {
    let path = endpoint.path();
    let base = path.strip_suffix("/chat/completions")?;
    let mut u = endpoint.clone();
    u.set_path(&format!("{base}/models"));
    u.set_query(None);
    Some(u)
}
```

- [ ] **Step 6: Replace the `num_ctx` field with a memoized window + probe**

In `provider.rs`, on `OpenAiProvider`:
- Remove the `num_ctx: Option<u32>` field and the `with_num_ctx` method.
- Add `context_window: tokio::sync::OnceCell<Option<u32>>,` (init `OnceCell::new()` in `new()`).

Add a private async resolver on `OpenAiProvider`:
```rust
    /// Resolve (once) this endpoint's context window via `GET /v1/models`.
    /// Best-effort: any failure (no models URL, network error, non-2xx,
    /// unparseable, model not listed, no context field) resolves to `None`
    /// (counter-mode + no trim). Credential-safe; never fails the turn.
    async fn resolve_window(&self) -> Option<u32> {
        *self
            .context_window
            .get_or_init(|| async {
                let url = models_url(&self.endpoint)?;
                let mut req = self.client.get(url);
                if let Some(key) = &self.api_key {
                    req = req.bearer_auth(key.expose());
                }
                let resp = req.send().await.ok()?;
                if !resp.status().is_success() {
                    return None;
                }
                let value: Value = resp.json().await.ok()?;
                parse_model_context_window(&value, &self.model)
            })
            .await
    }
```

- [ ] **Step 7: Wire the window into trim + emit in `turn`**

In `turn`, replace the trim line that read `self.num_ctx`:
```rust
        let window = self.resolve_window().await;
        let trimmed = transcript_budget(window, system_prompt, tools).and_then(|budget| {
```
(the rest of the trim block is unchanged — `window: Option<u32>` slots into `transcript_budget`'s existing `Option<u32>` parameter).

Change BOTH `ContextUsage` emits added in Task 3 from `num_ctx: None` to `num_ctx: window` (the non-stream branch and the streaming branch both have `window` in scope after the line above).

- [ ] **Step 8: Drop compat `num_ctx` threading in the factory**

In `src-tauri/src/elmer/provider.rs`, at the 3 compat construction sites (~299, ~316, ~329), change:
```rust
                    Box::new(OpenAiProvider::new(client, url, model, temperature, system_prompt, api_key)
                    .with_num_ctx(num_ctx)),
```
to:
```rust
                    Box::new(OpenAiProvider::new(client, url, model, temperature, system_prompt, api_key)),
```
The `num_ctx` local is still consumed by the `OllamaProvider::new(..)` site, so it does not become unused.

- [ ] **Step 9: Fix any `with_num_ctx`-based unit tests**

The pure trim tests (`trim_messages_to_budget`, `transcript_budget`) take an explicit budget and are unaffected. If any `provider.rs` test constructs `OpenAiProvider::...with_num_ctx(..)`, delete the `.with_num_ctx(..)` call (trim is now probe-driven; those tests should assert the pure functions instead). Run the crate tests and fix compile errors surfaced by the removed method.

Run: `cargo test -p tuxlink-agent-frontend 2>&1 | tail -25`
Expected: PASS after fixes.

- [ ] **Step 10: Confirm Cargo.lock unchanged + clippy both crates**

Run: `git diff --stat Cargo.lock; cargo clippy -p tuxlink-agent-frontend -p tuxlink --all-targets 2>&1 | tail -20`
Expected: `Cargo.lock` shows NO changes (no new deps); clippy clean. (`tokio::sync::OnceCell` and `reqwest` are already dependencies.)

- [ ] **Step 11: Commit**

```bash
git add src-tauri/tuxlink-agent-frontend/src/provider.rs src-tauri/src/elmer/provider.rs
git commit -m "feat(elmer): probe /v1/models for the compat context window

Best-effort memoized GET /v1/models reads max_model_len (vLLM) / context_length
(OpenRouter) as the meter denominator AND the client-side trim budget. Removes
the operator num_ctx from the compat path (server owns the window); num_ctx is
now Ollama-only. Probe failure => counter-mode + no trim (non-fatal). No new deps.

Agent: falcon-oriole-canyon
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Gate the operator num_ctx control on native Ollama (frontend cleanup)

Stops showing/sending the operator `num_ctx` control on non-Ollama tiles (it does nothing on compat after Task 4). Uses the preset kind, not the `isLoopback` string.

**Files:**
- Modify: `src/elmer/ElmerPane.tsx:781` (save-payload gate)
- Modify: `src/elmer/GetKeyCard.tsx:~200` (`isLocalTile` display gate)
- Modify/verify tests: `src/elmer/elmerModelConfig.test.ts` covers `inferPreset('…11434…') === 'localOllama'`.

**Interfaces:**
- Consumes: `inferPreset(endpoint) → 'localOllama' | 'openai' | 'openrouter' | 'anthropic' | 'custom'` (existing).

- [ ] **Step 1: Change the save-payload gate**

In `ElmerPane.tsx:781`, change:
```tsx
    const numCtxArg: number | null = isLoopback(endpoint) ? parsedNumCtx : null;
```
to:
```tsx
    // num_ctx is causal ONLY on native Ollama (it allocates the KV cache). On
    // every compat/cloud tile the server owns a fixed window (read via
    // /v1/models), so we never send an operator num_ctx there — gate on the
    // Ollama preset, not the loopback string (a loopback llama.cpp is compat).
    const numCtxArg: number | null = inferPreset(endpoint) === 'localOllama' ? parsedNumCtx : null;
```

- [ ] **Step 2: Change the tile display gate**

In `GetKeyCard.tsx`, change:
```tsx
  const isLocalTile = isLoopback(preset.endpoint);
```
to:
```tsx
  // Show the num_ctx control only on the native Ollama tile (causal there);
  // compat/cloud tiles read the window from the server, so hide it.
  const isLocalTile = inferPreset(preset.endpoint) === 'localOllama';
```
Ensure `inferPreset` is imported in `GetKeyCard.tsx` (add to the existing `elmerModelConfig` import if absent).

- [ ] **Step 3: Add a regression test for the gate signal**

In `elmerModelConfig.test.ts` (or the nearest suitable existing suite), confirm the discriminator:
```ts
  it('gate: a loopback llama.cpp on a non-ollama port is NOT localOllama', () => {
    expect(inferPreset('http://127.0.0.1:8080/v1/chat/completions')).toBe('custom');
  });
```
(The `…11434… → 'localOllama'` cases already exist at lines 282–287.)

- [ ] **Step 4: Typecheck + full elmer vitest**

Run: `pnpm tsc --noEmit 2>&1 | tail -20 && pnpm vitest run src/elmer 2>&1 | tail -25`
Expected: no type errors; all pass.

- [ ] **Step 5: Commit**

```bash
git add src/elmer/ElmerPane.tsx src/elmer/GetKeyCard.tsx src/elmer/elmerModelConfig.test.ts
git commit -m "fix(elmer): gate operator num_ctx control on the Ollama preset

num_ctx is causal only on native Ollama; on compat/cloud the server owns the
window (read via /v1/models). Gate the control on inferPreset==='localOllama'
instead of the isLoopback string, which wrongly matched loopback llama.cpp
(BUG2/BUG3).

Agent: falcon-oriole-canyon
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- D1 (parse usage + `stream_options`) → Task 3. ✓
- D2 (probe `/v1/models`, memoized, best-effort, drives trim + denominator) → Task 4. ✓
- D3 (optional denominator end to end + counter-mode) → Task 1 (Rust) + Task 2 (UI). ✓
- D4 (`num_ctx` Ollama-only + isLoopback→provider-kind gate) → Task 4 (backend threading) + Task 5 (frontend gate). ✓
- Edge cases (probe fails/model-not-found/no-field → counter-mode + no trim; non-stream fallback; usage absent → no emit) → covered by Task 3 (non-stream emit, absent-usage None) + Task 4 (probe None paths) tests. ✓
- Watched risk (`stream_options` 400) → accepted; noted in the Task 3 commit body; no capability detection built (YAGNI). ✓

**Placeholder scan:** No TBD/TODO; every code step shows complete code or an exact old→new edit with a file:line anchor; test steps show full assertions and exact commands with expected outcomes.

**Type consistency:** `num_ctx: Option<u32>` (Rust) ↔ `numCtx: number | null` (TS) consistent across Tasks 1/2; `parse_usage → Option<(u32,u32)>` produced in Task 3 and consumed in Task 4; `parse_model_context_window`/`models_url`/`resolve_window` names consistent within Task 4; `inferPreset(...) === 'localOllama'` consistent across Task 5's two sites.

## Notes for execution

- Rust builds are heavy on this Pi. Prefer scoped `cargo test -p <crate>` / `cargo clippy -p <crate>` per step as written; the final both-arch gate is CI on the PR, not local (per project convention: cheap gates local, heavy → CI).
- Task 5 is frontend-only and independent of Tasks 3/4; it may be deferred to a follow-up if execution runs long, but it prevents an operator seeing a dead num_ctx control on compat, so it belongs in this PR.
- Anthropic tile (`AnthropicProvider`) is out of scope — its meter stays hidden (no `usage` emit), consistent with the counter/hidden semantics.
