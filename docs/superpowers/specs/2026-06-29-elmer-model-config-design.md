# Elmer Model Configuration — In-App "Connect an AI Agent" (Design Spec)

**bd:** tuxlink-1wi5w · **Date:** 2026-06-29 · **Author agent:** redwood-falcon-bluff
**Status:** approved design (mock signed off); pending Codex adversarial review → plan → build.

## Goal

Let a non-technical operator connect Elmer to a model — local or cloud — entirely
from the UI, so nobody ever edits `~/.config/tuxlink/config.json`. Elmer exists to
*assist* operators with tasks like this; requiring a hidden JSON file (with a strict
loopback rule and an all-or-nothing block that fails silently on a typo) is
self-defeating. Replace the placeholder "Endpoint / model" disclosure in the Elmer
drawer with a real form, reachable from **Tools → "Connect an AI Agent…"**.

## Context (verified against shipped code)

- `ElmerConfig` (`src-tauri/src/config.rs`): `{ agent_endpoint: String, agent_model: String }`,
  persisted to `~/.config/tuxlink/config.json` under the `elmer` key
  (`#[serde(default, skip_serializing_if = "ElmerConfig::is_default")]`). Default:
  `http://127.0.0.1:11434/v1/chat/completions` + `llama3`.
- `agent_endpoint` is **loopback-only today**, validated by `LoopbackEndpoint::parse`
  (`src-tauri/tuxlink-agent-frontend/src/endpoint.rs`): accepts `127.0.0.0/8`, `::1`,
  literal `localhost`; default-deny on names (no DNS resolution). A non-loopback
  endpoint is rejected and the runtime falls back to the default loopback
  (`src-tauri/src/lib.rs:1576`).
- Config is read **once at Elmer setup** (app startup, `lib.rs:1567`). There is **no
  `config_set` command for elmer** and **no API-key field** anywhere today.
- The provider (`OpenAiProvider`, `tuxlink-agent-frontend`) speaks the OpenAI
  chat-completions wire format. Ollama and the major frontier providers all expose
  an OpenAI-compatible endpoint, so one provider abstraction covers local + cloud.
- The drawer's "Endpoint / model" disclosure is a **placeholder** ("configured in
  Settings → Elmer") — a stub that should not have shipped.
- The arm/taint **send-to-radio** gate (`EgressGuard`, `quarantine_and_rearm`, the
  2ouqf model) is independent of the model endpoint and is **out of scope here** — it
  stays exactly as-is.

## Operator decisions (locked during brainstorming)

1. **Local + cloud are equal peers. No framing, no disclosure modal, no "are you
   sure."** It is the operator's choice; preferences (especially while local models
   and local hardware are still maturing) are respected. Mirrors the project's
   no-added-safeguards stance.
2. **Model picker = free-text field + a `Detect` button** (probes `/v1/models`) — not
   an always-on auto-probe.
3. Two silent, non-framing engineering defaults are kept because they protect the
   *credential*, not the choice: the **API key lives in the OS keyring** (never
   `config.json`, never logs) and is **redacted** from the session-log window.
4. Home: **Tools → "Connect an AI Agent…"** opens the Elmer drawer with the Model
   config expanded. **Tools → "Elmer (AI assistant)…"** (existing) opens the same
   drawer focused on chat. Two plainly-named doors into one drawer; no second dialog,
   no pop-up.

## Design

### The form (drawer "Model" section, expanded by the new menu item)

Fields, top to bottom:

1. **Provider** — preset `<select>`: `Local Ollama · OpenAI · OpenRouter · Custom…`.
   Selecting a preset fills the Endpoint with that provider's chat-completions URL.
   `Custom…` leaves Endpoint free. On open, the preset is *inferred* from the saved
   endpoint (exact match against the known preset URLs → that preset; else `Custom…`).
2. **Endpoint** — text input (monospace), auto-filled by the preset, editable.
3. **API key** — text input (`type=password`), shown when the endpoint is **not**
   loopback (loopback Ollama needs none). Badged "🔒 keyring". Placeholder shows a
   masked existing key (`••••`) when one is already stored; leaving it untouched keeps
   the stored key, clearing it removes the key.
4. **Model** — text input (monospace) + a **`Detect`** button. `Detect` calls the
   backend to `GET <endpoint-origin>/v1/models`; on success it renders the returned
   ids as a small dropdown/typeahead the operator can pick from, and a
   "✓ N models detected" line. On failure it shows a quiet inline reason (e.g. "No
   server responded at <host>" / "401 — check the API key") and the field stays
   free-text.
5. **Save & use** — primary button. Persists endpoint + model to config, key to
   keyring, and **applies to the next message — no app restart**. A hint states this.

Empty state: when no model is configured/reachable, the chat area shows a quiet
"Connect a model to start" message instead of the input.

### Menu integration

- Add menu id `menu:tools:connect-agent`, label **"Connect an AI Agent…"**, under
  Tools, adjacent to the existing `menu:tools:elmer`.
- It dispatches an action that (a) opens the Elmer drawer (`elmerOpen = true`) and
  (b) sets the drawer to expand the Model section.
- **Required:** add `menu:tools:connect-agent` to `menuModel.ts` AND to the exhaustive
  `EXPECTED_IDS` vocabulary assertion in `menuModel.test.ts`, or that test fails.

### Backend

1. **Endpoint validation — relax loopback-only.** Replace the loopback-only contract
   with a general validator that accepts any well-formed `http(s)://host[:port]/path`
   URL. Keep an `is_loopback()` predicate on the parsed result (drives the "API key
   field hidden for loopback" UX and the "no key needed" affordance). Presets use
   `https` for cloud; a custom `http://` remote is **permitted** (operator's choice —
   no framing) but see Security below. The rename: `LoopbackEndpoint` →
   `AgentEndpoint` (or add a permissive constructor; keep the loopback predicate).
2. **Config schema is unchanged** — still `{ agent_endpoint, agent_model }`. The API
   key is **not** a config field; it lives in the keyring.
3. **Keyring** — store/read/delete the API key under a fixed service+account
   (e.g. service `tuxlink`, account `elmer-agent-api-key`), via the existing keyring
   dependency. No key on disk, ever.
4. **New Tauri commands:**
   - `elmer_config_read() -> { agent_endpoint, agent_model, has_key: bool }` — note
     `has_key` is a boolean only; the key value is never returned to the frontend.
   - `elmer_config_set({ agent_endpoint, agent_model, api_key: Option<SetKey> })` —
     validates the endpoint, writes `agent_endpoint`/`agent_model` to config, and
     applies the key action to the keyring. `SetKey` is a three-state: `Keep`
     (untouched), `Set(secret)`, `Clear`. Returns `Ok(())` or a typed validation
     error surfaced inline.
   - `elmer_detect_models({ agent_endpoint, api_key: Option<…> }) -> Vec<String>` —
     issues `GET <origin>/v1/models` with the bearer key if present; maps transport
     and HTTP errors to a small typed reason for the inline message.
5. **Live-apply (no restart):** build the `OpenAiProvider` **per Elmer session/turn**
   from the current config + keyring key, rather than once at startup. The startup
   read becomes a "warm default"; the authoritative source for each run is the live
   config + keyring. This is what lets the operator switch models repeatedly (the
   primary workflow — comparing gpt-oss / Claude / Gemini / etc.) without restarting.
6. **Log redaction:** the API key must be redacted everywhere the session-log /
   diagnostics surface request metadata. Add the bearer token to the redaction sink
   set; never log the raw `Authorization` header.

### Data flow

```
operator → form → elmer_config_set(endpoint, model, key) → validate endpoint
   → write config.json (endpoint, model) + keyring (key) → Ok
operator sends a turn → Elmer builds OpenAiProvider from {config, keyring} → request
Detect → elmer_detect_models(endpoint, key) → GET /v1/models → ids → dropdown
```

### Security posture (honest, for the adversarial review to attack)

- **Loopback relaxation is the headline risk.** It was a deliberate appsec control
  (model stays local; no remote exfil/DNS-rebind). Relaxing it is an **operator
  decision** made on purpose: cloud frontier models require it, and the operator's
  stance is that this is the operator's choice. The conversation — including any
  message content Elmer read via MCP tools — is sent to the chosen provider. That is
  inherent to using a cloud model; we do not block or nag it.
- **The transmit (Part 97 / CMS send) gate is unaffected.** A cloud model still has
  **no** send-to-radio authority; that path remains arm/taint-gated (2ouqf). A
  prompt-injected cloud model cannot transmit without the operator arming send.
- **Credential handling is the part we *do* harden** (it is the project's top bar —
  cred-theft): key in OS keyring only, never returned to the frontend (only
  `has_key`), never written to config or logs, redacted in the session-log window.
- **Cleartext-key caveat:** a custom `http://` remote endpoint would send the bearer
  key without TLS. We permit it (no framing) but presets are all `https`. *Open
  question for the adversarial review: should a non-loopback `http://` endpoint at
  least surface a one-line inline note (not a block), or is even that too much
  framing?* Provisional answer: no note — the operator chose the endpoint.

## Error handling

- Invalid/malformed endpoint → inline field error from `elmer_config_set`; not saved.
- `Detect` with server down / wrong port → "No server responded at <host:port>".
- `Detect` 401/403 → "Check the API key for <provider>." 4xx/5xx → status + reason.
- Cloud endpoint saved with no key → allowed (some gateways are keyless); the first
  turn surfaces the provider's auth error via the existing offline/error outcome.
- Keyring unavailable (headless/locked) → typed error surfaced inline on save; config
  endpoint/model still persist so the operator isn't stuck.

## Testing

- **Rust:** endpoint validator (loopback + remote http/https accepted; junk rejected;
  `is_loopback` correct); `elmer_config_set` three-state key action (Keep/Set/Clear)
  against a keyring fake; `elmer_config_read` never leaks the key (returns `has_key`);
  `elmer_detect_models` maps transport/HTTP errors to reasons (mock HTTP); redaction
  sink includes the bearer token. Provider live-rebuild reads current config + key.
- **Frontend (vitest):** form renders; preset fills endpoint; key field shown iff
  non-loopback; `Detect` populates the dropdown from a mocked command and shows the
  failure reason on error; `Save & use` calls `elmer_config_set` with the right
  payload incl. the three-state key; empty-state "Connect a model to start" shows when
  unconfigured; the `menu:tools:connect-agent` id is in `EXPECTED_IDS`.
- **App-level:** Tools → "Connect an AI Agent…" opens the drawer with Model expanded.

## Out of scope

- The arm/taint **send** model and `quarantine_and_rearm` (untouched).
- Native (non-OpenAI-compatible) provider SDKs — everything goes through the
  OpenAI-compatible wire format (OpenRouter covers the frontier set with one key).
- Per-identity or multiple saved model profiles (single active config for now; YAGNI).
- Streaming token display changes (unrelated).

## File touch-list (for the plan)

- `src-tauri/tuxlink-agent-frontend/src/endpoint.rs` — relax to `AgentEndpoint` +
  `is_loopback()`.
- `src-tauri/src/config.rs` — (no schema change) helpers if needed.
- `src-tauri/src/elmer/` + `src-tauri/src/lib.rs` — keyring helper, the three new
  commands, per-session provider build, redaction-sink addition.
- `src/elmer/ElmerPane.tsx` + `ElmerPane.css` — the Model form replacing the
  placeholder; empty state.
- `src/shell/chrome/menuModel.ts` + `menuModel.test.ts` — the new menu id.
- `src/shell/AppShell.tsx` — menu action → open drawer + expand Model.
