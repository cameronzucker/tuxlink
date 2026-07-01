# Plan: Native Ollama `/api/chat` local provider path (tuxlink-65qhn)

**Branch:** `bd-tuxlink-65qhn/native-ollama-provider` (off `origin/main`)
**Agent:** willow-heron-badger
**Design source of truth:** `bd show tuxlink-65qhn` (DESIGN + NOTES) + `dev/scratch/elmer-native-ollama-mock-v2.png` (operator-approved 2026-07-01).
**Reviewed by:** `/plan-eng-review` 2026-07-01.

Subsumes: tuxlink-7mb3r (eager unload), tuxlink-e0w3x (num_ctx truncation), tuxlink-31tbw (advanced settings incl. editable system prompt).

---

## Problem

The local/loopback Elmer path runs Ollama through its **OpenAI-compat** shim
(`/v1/chat/completions`). That shim cannot express three things the local edge
needs, all of which live on Ollama's **native** `/api/chat`:

1. **`num_ctx`** — the OpenAI-compat path leaves the context window at Ollama's
   default (often 2048/4096), silently truncating agentic prompts and dropping
   tool definitions (tuxlink-e0w3x). Lower-tier local models then "can't see"
   the tool surface.
2. **`keep_alive: 0`** — no way to eagerly unload the previous model on switch;
   memory pressure on edge/R2 hosts (tuxlink-7mb3r).
3. **Context-usage meter** — native `/api/chat` returns `prompt_eval_count`
   (full prompt tokens) + `eval_count` (generated tokens) each turn. Against
   the `num_ctx` we set, that drives the fullness meter the operator asked for.

Cloud + remote endpoints stay exactly as they are (OpenAI-compat / Anthropic).
This is a **loopback-only** addition.

---

## Architecture

### Selection seam

```
ElmerProvider::new(LoopbackEndpoint, model, api_key)     ← loopback path (THIS epic)
  currently: always OpenAiProvider
  becomes:   probe endpoint → OllamaProvider (native) | OpenAiProvider (compat fallback)

ElmerProvider::new_vetted(AgentEndpoint, model, api_key) ← remote path (UNCHANGED)
  is_anthropic_endpoint → AnthropicProvider | OpenAiProvider
```

The native provider is a THIRD `Box<dyn Provider>` implementation, mirroring
`AnthropicProvider`. No `Provider` trait change. It plugs into the **loopback**
constructor only.

```
                  ┌─────────────────────────────────────────────┐
   loopback URL → │ ElmerProvider::new                            │
                  │   probe /api/tags (200 = Ollama)              │
                  │     ├── Ok  → OllamaProvider (native /api/chat)│
                  │     └── else→ OpenAiProvider (/v1 compat)      │
                  └─────────────────────────────────────────────┘
```

**Loopback discrimination (DECISION — probe-with-fallback).** On loopback we
cannot assume Ollama: a user may run llama.cpp's server (OpenAI-compat only,
404s on `/api/*`). At construction we probe `GET {origin}/api/tags` with a
short timeout:
- **200 + parseable** → Ollama present → `OllamaProvider` (native).
- **404 / connect-refused / unparseable** → `OpenAiProvider` (compat), current
  behavior preserved.

Rationale: the bd issue states "loopback → OllamaProvider," but a blind switch
would break llama.cpp loopback configs. The probe is additive, reversible, and
costs one cheap local round-trip at session start. `/api/tags` is also what the
"Detect" button already needs for model listing, so the code is shared.

### Meter plumbing (additive, mirrors the Delta channel — tuxlink-e2vw7)

```
OllamaProvider::turn
  parses final /api/chat msg → prompt_eval_count, eval_count, (num_ctx known locally)
  on_event(RunEvent::ContextUsage { prompt_tokens, eval_tokens, num_ctx })   [runner crate]
        │
        ▼
session.rs bridge  RunEvent::ContextUsage ⇒ ElmerEvent::Context { ... }       [emit sink]
        │  (new arm; do NOT rely on the `_ => return` catch-all)
        ▼
events.rs  ElmerEvent::Context (serde tag="kind" → {kind:"context", ...})     [+ EV_CONTEXT const]
        │
        ▼
elmerEvents.ts  EV_CONTEXT + ElmerContextPayload                              [frontend types]
        │
        ▼
useElmer.ts  store latest {promptTokens, numCtx} in state                     [hook]
        │
        ▼
ElmerPane.tsx  <ContextMeter> above composer (hidden until first ContextUsage)[UI]
```

`RunEvent` is `#[non_exhaustive]` — adding `ContextUsage` is safe for external
matches. The runner does NOT interpret the counts; it only relays them (same
fire-and-forget contract as the Delta variants).

### Config threading

`ModelConfigSnapshot` grows three OPTIONAL fields (all `None` = current
behavior):

```rust
pub struct ModelConfigSnapshot {
    pub endpoint: String,
    pub model: String,
    pub num_ctx: Option<u32>,            // native Ollama only
    pub temperature: Option<f32>,        // all providers (compat + native accept it)
    pub system_prompt_override: Option<String>, // tuxlink-31tbw; all providers
}
```

`OllamaProvider::new` receives `num_ctx` + `temperature` and puts them in the
`/api/chat` `options` object; it always sends `keep_alive: 0` on the request so
the previous model unloads on switch (tuxlink-7mb3r). `system_prompt_override`
replaces `ELMER_SYSTEM_PROMPT` when present (applies to every provider, so it is
threaded at the `ElmerProvider` layer / request-build layer, not Ollama-only).

### Memory-fit estimate (Advanced disclosure)

Per `project_local_model_ctx_sizing_compute_buffer` memory (the R2 crash saga):
the flat KV formula is a LOOSE UPPER BOUND and over-counts Mamba/sliding-window
architectures; it also ignores the prefill compute buffer. So the estimate is:

- **Geometry from Ollama, not hardcoded**: `POST /api/show {model}` →
  `model_info` (`block_count`, `attention.head_count_kv`, `attention.key_length`
  or `embedding_length/head_count`, `context_length`). Weights size from
  `/api/tags` (`size`).
- **KV bytes/token** = `2 * layers * kv_heads * head_dim * kv_dtype_bytes`
  (`kv_dtype_bytes`: 2 for f16, 1 for q8_0 — assume q8_0, the R2 config default).
- **Total estimate** = weights + KV(num_ctx) + a fixed compute-buffer headroom.
  Flash-attn (the R2 default, `OLLAMA_FLASH_ATTENTION=1`) collapses the prefill
  buffer, so the headroom is a small constant (documented as an assumption in
  the UI copy), NOT the pre-flash worst case.
- **Fit indicator** compares total against host RAM. Host RAM comes from a
  Tauri command (`sysinfo` or `/proc/meminfo`); default target is the 32 GB+
  class (R2 reference), per the DESIGN note ("Pis run the cloud thin-client,
  not local agents").
- The estimate is presented as **approximate** and conservative. It never
  drives an automatic num_ctx; it only annotates the operator's manual choice
  (green fits / red exceeds). An UNDER-estimate must never make a too-large
  window look safe — bias the constant upward.

Post-load truth: after a model is resident, `GET /api/ps` reports the actual
`size`. v1 shows the pre-load estimate; surfacing the real `/api/ps` number is
a fast-follow (NOT in scope, see below).

---

## Decisions locked in this review

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Probe-with-fallback** for loopback Ollama-vs-llama.cpp (not a blind switch). | Preserves llama.cpp loopback compat; additive; shares `/api/tags` code with Detect. **Blast radius: this is the one to veto if you disagree.** |
| D2 | **Non-streaming v1** (mirror `AnthropicProvider`), meter counts come from the final `/api/chat` response. Streaming (NDJSON) is a fast-follow. | Bounds the port risk to the freshly-merged Anthropic shape; meter works either way. Local currently streams via compat, so this is the one visible tradeoff — noted below. |
| D3 | **Memory estimate = Ollama-sourced geometry + flash-attn headroom, marked approximate, biased safe.** | Directly encodes the R2 crash-saga lesson; the flat KV formula alone is unsafe. |
| D4 | **`keep_alive: 0` always** on native requests. | Simplest correct unload-on-switch; no separate unload call needed. |
| D5 | **`num_ctx` default is model-aware, 32k baseline** for the 32 GB+ class; NOT Pi-sized. | Matches the locked DESIGN + mock. |
| D6 | Meter is **local/native only** (known `num_ctx`). Cloud shows tokens-used with no denominator (already partially present); no cloud meter in this epic. | The denominator only exists when we set `num_ctx`. |

---

## Task decomposition (subagent-driven build order)

Backend first (each independently testable), then the bridge, then frontend.

- **T1 — `OllamaProvider` adapter** (`tuxlink-agent-frontend/src/ollama_provider.rs`, new).
  Mirror `anthropic_provider.rs`. `build_ollama_request` (native `/api/chat`
  body: `messages`, `tools`, `options.num_ctx`, `options.temperature`,
  `keep_alive:0`, `stream:false`). `parse_ollama_response` → `ModelTurn`
  (native tool-call shape: `message.tool_calls[].function.{name,arguments}` —
  arguments is a JSON **object**, not a string like OpenAI). FIFO synthetic-id
  tool-protocol port (see OpenAI + Anthropic prior art). `is_ollama_endpoint` /
  probe helper. Impl `Provider::turn`. Full unit tests on request shape +
  parse (text, tool-call, multi-tool, missing-args, empty).
- **T2 — `RunEvent::ContextUsage`** (`tuxlink-agent-runner/src/types.rs`).
  New non_exhaustive variant `{ prompt_tokens: u32, eval_tokens: u32, num_ctx: u32 }`.
  Emit from `OllamaProvider::turn` via `on_event` after parsing counts. Runner
  tests: variant relays unchanged; loop behavior unaffected.
- **T3 — Config fields** (`model_config_state.rs`, `config_commands.rs`).
  Add optional `num_ctx`/`temperature`/`system_prompt_override` to
  `ModelConfigSnapshot` + atomic set. Torn-read test extended. Backward-compat:
  missing fields deserialize to `None`.
- **T4 — Provider selection + threading** (`src/elmer/provider.rs`).
  `ElmerProvider::new` probes → picks `OllamaProvider` | `OpenAiProvider`.
  Thread `num_ctx`/`temperature`/`system_prompt_override` from config into the
  chosen provider. `system_prompt_override` applied provider-agnostically.
  Tests: probe-200 → native; probe-404 → compat; deny path unchanged.
- **T5 — Meter bridge** (`src/elmer/events.rs`, `session.rs`).
  `ElmerEvent::Context` (serde tag=kind, camelCase, `EV_CONTEXT`). Bridge arm
  `RunEvent::ContextUsage ⇒ ElmerEvent::Context`. Serialization shape test
  (mirrors the Delta discriminant tests).
- **T6 — Memory estimate command** (`config_commands.rs` or new).
  Tauri command: given model + num_ctx, fetch `/api/show` + `/api/tags`, compute
  weights + KV + headroom + fit vs host RAM. Pure calc fn unit-tested with
  recorded `/api/show` fixtures (gemma4 MoE + a dense model). Host-RAM read
  behind a seam for tests.
- **T7 — Frontend meter** (`elmerEvents.ts`, `useElmer.ts`, `ElmerPane.tsx`).
  `EV_CONTEXT` + `ElmerContextPayload`; store latest in `useElmer`; render
  `<ContextMeter>` above the composer — hidden until first Context event, then
  persistent. Amber ≥75%, red ≥90%. Vitest: hidden→shown transition, color
  thresholds, `k` formatting (12k/32k).
- **T8 — Frontend Advanced disclosure** (`GetKeyCard.tsx` / model editor).
  num_ctx input + live memory estimate line (calls T6 command) + fit badge;
  temperature slider; editable system prompt + Reset (tuxlink-31tbw). Collapsed
  by default. Cloud tiles show the same disclosure minus num_ctx (DESIGN note).
  Vitest: estimate updates on num_ctx change; fit green/red; reset restores
  default prompt.

Dependency order: T1→T2 (T2 emitted by T1); T3→T4; T5 after T2; T6 before T8;
T7 after T5; T8 after T3+T6. Parallel lanes below.

---

## Test coverage diagram

```
BACKEND
=======
[+] ollama_provider.rs (T1)
    ├── build_ollama_request
    │   ├── [PLAN] options.num_ctx + temperature present when set
    │   ├── [PLAN] keep_alive:0 always present
    │   ├── [PLAN] tools serialized in native shape
    │   └── [PLAN] tool-result FIFO pairing (synthetic ids)
    ├── parse_ollama_response
    │   ├── [PLAN] text turn
    │   ├── [PLAN] single tool_call (arguments = JSON object, not string)
    │   ├── [PLAN] multiple tool_calls
    │   ├── [PLAN] tool_call precedence over content
    │   ├── [PLAN] missing arguments → null/empty args
    │   └── [PLAN] empty message → empty text
    └── probe/is_ollama_endpoint
        ├── [PLAN] 200 /api/tags → true
        └── [PLAN] 404 / refused → false
[+] RunEvent::ContextUsage (T2)  ── [PLAN] emitted after parse; relays counts unchanged
[+] ModelConfigSnapshot (T3)     ── [PLAN] round-trips new optional fields; missing→None; torn-read guard
[+] ElmerProvider::new (T4)      ── [PLAN] probe→native | probe-fail→compat | egress deny unchanged
[+] memory-estimate calc (T6)    ── [PLAN] gemma4-MoE fixture; dense fixture; fit green/red boundary; q8 vs f16 KV

BRIDGE
======
[+] ElmerEvent::Context (T5)     ── [PLAN] serde {kind:"context", promptTokens, numCtx,...}; bridge arm maps it

FRONTEND
========
[+] ContextMeter (T7)
    ├── [PLAN] hidden before first context event
    ├── [PLAN] shown + persistent after first
    ├── [PLAN] amber ≥75%, red ≥90%
    └── [PLAN] 12k/32k formatting
[+] Advanced disclosure (T8)
    ├── [PLAN] collapsed by default
    ├── [PLAN] estimate updates on num_ctx change (calls T6)
    ├── [PLAN] fit badge green/red
    ├── [PLAN] temperature slider round-trips
    └── [PLAN] system-prompt edit + Reset (tuxlink-31tbw)

TARGET: 100% of new code paths. No regressions to compat/Anthropic paths
(existing provider tests must stay green; T4 must not alter remote selection).
```

---

## Failure modes (per new codepath)

| Codepath | Realistic prod failure | Test? | Handled? | Silent? |
|----------|------------------------|-------|----------|---------|
| Probe `/api/tags` | Ollama slow to start → probe times out → falls to compat, num_ctx silently ignored | T4 (timeout→compat) | Yes (fallback) | Partially — log the fallback so it's not silent |
| `parse_ollama_response` | Native tool-call `arguments` is an object; OpenAI code expected a string → parse panic | T1 | Yes (native parser) | No |
| `num_ctx` too large for host | Ollama OOM-kills / host crash on load | T6 estimate warns; MemoryMax net catches at runtime | Estimate + red badge | No (red badge + host net) |
| `RunEvent::ContextUsage` counts absent (some models omit) | Meter never appears | T2 | Meter stays hidden (graceful) | Acceptable (hidden = correct) |
| CPU prefill slow (~20 tok/s, 19k prefill timed out at 900s) | Big num_ctx feels frozen | per-turn timeout (Limits) | Existing timeout | **Gap → surface a "large context = slow on CPU" cue in Advanced (folded into T8 copy)** |

Critical gap flagged: the CPU-prefill-speed cue (NOTES point 3). Folded into T8
as UI copy, not a blocking mechanism.

---

## NOT in scope

- **Streaming native path (NDJSON).** v1 is non-streaming (D2). Fast-follow.
- **`/api/ps` real-resident-size readout.** v1 shows the pre-load estimate. Fast-follow.
- **Cloud context meter with denominator.** No `num_ctx` for cloud (D6).
- **Model provisioning (pull / quant / create).** Stays CLI (DESIGN).
- **Benchmark harness (tuxlink-ylmwj).** Separate epic.
- **Agent-send / egress un-cripple (tuxlink-sg5zw).** Parallel agent owns it. If
  both touch the egress path, coordinate via `bd dep add`.

## What already exists (reused, not rebuilt)

- `AnthropicProvider` — the exact adapter template (build/parse/impl Provider).
- Delta channel (tuxlink-e2vw7) — the exact meter-plumbing precedent (RunEvent →
  ElmerEvent tag=kind → EV_ const → useElmer → pane).
- `build_vetted_client` + egress gate (SSRF-1) — native path reuses it (same host).
- `/api/tags` Detect (tuxlink-6614d / #993) — probe + model listing share it.
- `ModelConfigSnapshot` atomic guard — extend, don't replace.
- Editable system prompt intent (tuxlink-31tbw) — realized in T8.

## Parallelization

- **Lane A (backend core):** T1 → T2 (T2 emitted by T1, shared file region).
- **Lane B (config):** T3 → T4 (T4 needs T3 fields; T4 also needs T1's provider — so T4 waits on Lane A too).
- **Lane C (estimate):** T6 (independent backend; recorded fixtures).
- **Lane D (bridge):** T5 (needs T2 from Lane A).
- **Lane E (frontend):** T7 (needs T5), T8 (needs T3 + T6).

Launch A + C in parallel. Then B + D after A. Then E after B/D/C.
Conflict flag: T4 and T5 both touch `src/elmer/` but different files
(`provider.rs` vs `session.rs`/`events.rs`) — safe in parallel.

## Adversarial review gate

Fragile + interop-sensitive (native tool-call wire shape). Before merge: Codex
adrev round on the branch diff, attack angles = tool-call parse divergence from
OpenAI shape, probe-fallback edge cases, KV-estimate under-count safety,
egress-gate reuse on the native path. Per no-carveout rule.
