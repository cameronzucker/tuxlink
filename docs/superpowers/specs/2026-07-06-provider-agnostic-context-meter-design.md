# Provider-agnostic context meter — design

- **Issue:** tuxlink-xnenf
- **Date:** 2026-07-06
- **Agent:** falcon-oriole-canyon
- **Status:** design, pending operator review

## Intent

The Elmer context meter is a **fuel gauge: always present, always honest, the
same component whether the model runs on local Ollama, remote vLLM, or a cloud
endpoint.** "Provider-agnostic" is the whole point of the re-scope — a gauge that
silently vanishes for some backends is failing its own name.

Everything below derives from that one sentence. The meter degrades in
**fidelity**, never in **presence**: it always shows tokens consumed, and it adds
the fill bar and percentage only when it can honestly compute one.

## Why two providers exist (settled — do not relitigate)

The meter has to work across two deliberately different deployment modes, and the
design is shaped by their difference, so the rationale is recorded here:

- **Native Ollama = local, modest/unified-memory hardware, offline.** Required
  targets include Framework / Strix Halo-class laptops (high-end APU, unified
  memory). vLLM's support for unified-memory APUs is immature-to-absent; that
  hardware class runs Ollama/llama.cpp. This is also the off-grid / EmComm
  reality where a remote GPU may be unreachable. Native Ollama is **first-class**,
  not legacy.
- **OpenAI-compat = connected, high-end (vLLM on DGX Spark, cloud) over TLS.**

They are two code paths — not one compat path to all — for exactly one reason:
**`num_ctx`.** Ollama's own `/v1` compat endpoint cannot set the context window,
and Ollama's default window (2–4k) is agent-hostile (Elmer would forget its own
tool results). The native `OllamaProvider` exists solely to send
`options.num_ctx` and *raise* the window. vLLM fixes a large window server-side,
so there is nothing to set — you *read* `max_model_len`.

**That set-the-window vs read-the-window split is the entire shape of this
design.** N305-class boxes are dropped as targets; they do not affect this
feature.

## Scope

Purely **additive to the compat path.** The native Ollama meter already works and
is untouched. The two providers keep **independent** denominator sources and
**share no operator control** — there is no cross-provider "context window" config
to reconcile (an earlier framing that was abandoned).

## Current state (grounded)

- `RunEvent::ContextUsage { prompt_tokens, eval_tokens, num_ctx }`
  (`tuxlink-agent-runner/src/types.rs:224`) → bridged to
  `ElmerEvent::Context { promptTokens, evalTokens, numCtx }`
  (`src-tauri/src/elmer/session.rs:469`, `events.rs`) → rendered by
  `src/elmer/ContextMeter.tsx` as `promptTokens / numCtx` with a bar + %.
- `num_ctx` **is** the denominator. Today it is a **required non-zero `u32`**, and
  `ContextMeter` always draws a bar.
- **Only** the native path emits `ContextUsage`
  (`ollama_provider.rs:255`), using the `num_ctx` it set as the denominator.
- The **compat path (`OpenAiProvider`, `tuxlink-agent-frontend/src/provider.rs`)
  emits nothing.** The `usage` object is present in the non-stream JSON and unread;
  the streamed path never requests it.
- `OpenAiProvider` already carries a `num_ctx: Option<u32>` used **only** for
  client-side transcript trim (`tuxlink-evucv`: `transcript_budget` /
  `trim_messages_to_budget`). Today the frontend sends it only for loopback
  endpoints (`ElmerPane.tsx:781`: `isLoopback(endpoint) ? parsedNumCtx : null`).

## Design

### D1 — Numerator: parse the compat `usage` object

`usage.prompt_tokens` is the numerator; it is authoritative and universal (every
OpenAI-compat response returns it). Map `usage.completion_tokens → eval_tokens`
for parity.

- **Non-stream path** (`parse_completion` branch of `OpenAiProvider::turn`): the
  top-level `usage` object is already in the JSON. Read it and emit `ContextUsage`
  after parsing the completion (mirror `ollama_provider.rs`).
- **Stream path** (`SseAccumulator`): OpenAI-compat streams omit `usage` unless the
  request sets `stream_options: { include_usage: true }`. Add that to the streamed
  request body. The server then sends a final chunk with `choices: []` and a
  populated `usage`; `SseAccumulator::apply_chunk` currently early-returns when
  `choices[0].delta` is absent, so extend it to capture `usage` from a
  choices-empty chunk. Emit `ContextUsage` once, after the stream completes
  (alongside `into_turn`).
- **Fire-and-forget**, exactly like the Ollama emit: it never changes the returned
  `ModelTurn`. If `usage` is absent (a non-conformant server), emit nothing — the
  meter simply does not appear for that endpoint (the one legitimate never-shows
  case).

**Watched risk:** a strict/exotic proxy could reject the unknown `stream_options`
field with an HTTP 400, which would fail the whole turn, not just the meter.
`stream_options.include_usage` is standard OpenAI and supported by vLLM /
OpenRouter / OpenAI / recent llama.cpp, so the risk is low. If a specific endpoint
is found to reject it, make the field conditional in a follow-up; do **not**
pre-emptively add per-endpoint capability detection now (YAGNI).

### D2 — Denominator: probe `/v1/models`, else counter-mode

The compat denominator comes from **whoever authoritatively owns the window: the
server.** Probe it; never ask the operator (they cannot enlarge a server-fixed
window and would not shrink an agentic one).

- On the compat path, GET `{base}/models` (derive from the configured
  `…/chat/completions` endpoint by replacing the trailing `/chat/completions` with
  `/models`). Find the entry whose `id` matches the configured model and read its
  context length: **`max_model_len`** (vLLM) **or** **`context_length`**
  (OpenRouter). First present wins.
- Run the probe **once** per provider, memoized (e.g. `tokio::sync::OnceCell`), and
  resolve it **before** the trim step in `turn()` so both the meter and the trim
  budget use it. First turn pays one extra round-trip; subsequent turns reuse the
  cached value.
- **Credential-safe and best-effort:** reuse the same bearer auth, `redacted_url`,
  and `scrub_key` machinery. Any failure — network error, non-2xx, unparseable
  body, model not in the list, or no context field — resolves to **no window**.
  A probe failure **never** fails the turn.

**Denominator resolution (compat):** `probed window` when known, else **none**.
There is no operator override on the compat path.

### D3 — Event + UI: optional denominator, two fidelities

Make the denominator optional end to end so the gauge can render without one:

- `RunEvent::ContextUsage.num_ctx`: `u32` → **`Option<u32>`**
  (`tuxlink-agent-runner`). Update the crate's unit tests.
- `ElmerEvent::Context.num_ctx`: `Option<u32>` → serializes as
  `numCtx: number | null`. `session.rs` bridge passes it through unchanged.
- `ContextMeter` prop `numCtx: number | null`, rendering two fidelities of the
  **same** component:
  - **Windowed** (`numCtx` present): unchanged — `Context 12k / 32k`, right label
    `38% · room for tools + history`, fill bar, amber ≥75% / red ≥90%.
  - **Counter** (`numCtx` null): `Context 12k` only — no `/ X`, no percentage, no
    fill bar (hide the track). `aria-label` becomes e.g. `Context usage: 12k
    tokens (window unknown)`.
- The native Ollama path **always** passes `Some(num_ctx)` → always windowed.
  Nothing about the Ollama rendering changes.

### D4 — `num_ctx` stays Ollama-only; fix the provider-kind gate

Per the handoff's item (c). `num_ctx` as a **control the operator turns** belongs
only to native Ollama, where turning it is causal. On compat the number is read,
not set.

- The frontend `num_ctx` input moves from an `isLoopback(endpoint)` gate to a
  **provider-kind** gate (shown only for the native Ollama provider), fixing
  BUG2/BUG3 (`ElmerPane.tsx:781`, `GetKeyCard.tsx:200`). Post-qe6ie a loopback
  endpoint can be a compat llama.cpp shim, so `isLoopback` no longer implies
  "native Ollama." Determining true provider kind in the frontend is an
  implementation detail for the plan (the tile/preset selection already
  distinguishes the native Ollama tile from compat tiles).
- **Compat trim** (`evucv`) now consumes the **probed** window instead of an
  operator number. This is a net gain: remote compat (which previously sent
  `null`) finally gets overflow-trim, keyed to the *real* server ceiling. When no
  probe result exists, trim is disabled (unbounded) exactly as remote compat
  behaved before — see Edge cases.

## Data flow

```
compat turn():
  probe /v1/models (once, memoized) ──► window: Option<u32>
        │
        ├─ trim transcript to `window` (evucv) if Some, else no trim
        │
  send request (stream_options.include_usage = true when streaming)
        │
  parse usage.prompt_tokens / completion_tokens
        │
  emit ContextUsage { prompt_tokens, eval_tokens, num_ctx: window }  (fire-and-forget)
        │
  session.rs ──► ElmerEvent::Context { …, numCtx: number | null }
        │
  ContextMeter: numCtx present → bar + % ; numCtx null → bare counter
```

## Components & boundaries

- **`OpenAiProvider` (compat):** owns the probe (memoized), `usage` parsing, and
  the `ContextUsage` emit. Probe + usage parsing live in **pure, unit-testable
  functions** (given JSON in, window/counts out) with no live network, matching the
  file's existing `parse_completion` / `build_request_body` discipline.
- **`SseAccumulator`:** gains a `usage` field captured from a choices-empty final
  chunk; exposed to `turn()` for the post-stream emit.
- **`ContextMeter`:** gains counter-mode; no other consumer changes.
- **Runner event type:** the only cross-crate change (`num_ctx` becomes optional).

## Edge cases

- **Probe returns no context field (bare llama.cpp `/v1`, silent proxy):**
  counter-mode meter (tokens shown, no bar) **and** no client-side trim (unbounded,
  pre-existing remote-compat behavior). Non-fatal: if the transcript overflows, the
  server returns its own 400 which Elmer already surfaces as an error outcome. This
  loopback-compat-without-a-window case is an accepted limitation because the
  documented local path is native Ollama, not raw llama.cpp.
- **Streaming endpoint ignores `stream_options`:** no final usage chunk → no emit →
  meter hidden for that endpoint (same as a server that omits `usage`). Acceptable.
- **Non-stream fallback** (server ignores `stream: true`): `usage` is in the JSON;
  emit from the `parse_completion` branch.
- **Model id mismatch** (configured model not in `/v1/models` list): no window →
  counter-mode. Do not guess from a partial name match.
- **Huge cloud model list** (OpenRouter lists hundreds): a single memoized GET per
  session; acceptable.

## Testing

- **Pure unit tests** (no network), the file's established pattern:
  - `usage` parsing: prompt/completion extraction from recorded non-stream JSON and
    from a streamed choices-empty final chunk; absent `usage` → no emit.
  - probe parsing: `max_model_len` (vLLM sample), `context_length` (OpenRouter
    sample), model-not-found, no-context-field, unparseable → correct
    `Option<u32>`.
  - trim keyed to probed window (extend existing `evucv` tests).
- **`ContextMeter.test.tsx`:** counter-mode (null `numCtx`) renders the bare count,
  no track/percentage; windowed mode unchanged; `formatK` unchanged.
- **Event serialization:** `numCtx: null` round-trips to the frontend (guard the
  `Option` serde shape, echoing the `outcomeKind` / `deltaKind` precedents).
- **Runner:** update `types.rs` `ContextUsage` construction test for `Option`.
- CI runs both arches + full vitest + `clippy --all-targets -D warnings`; a new
  reqwest-dep-free change keeps `Cargo.lock` untouched.

## Out of scope

- Any per-model static context table for clouds that do not advertise a window
  (rots; explicitly rejected).
- An operator context-window control on the compat path (rejected — server owns
  the window).
- Dropping native Ollama / collapsing to a single compat path (closed: Framework /
  Strix Halo require the local Ollama path).
- The separate P0 `tuxlink-jfpj2` (Elmer Stop/OOM).

## Open questions

None blocking. The `stream_options` 400 risk (D1) and the bare-loopback-llama.cpp
trim gap (Edge cases) are documented, accepted, and non-fatal.
