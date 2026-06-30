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

---

# Revision 2 — Adversarial-review hardening (2026-06-29)

Incorporates the 5-lens adversarial review (Codex + SSRF + credential + live-apply + UX;
raw + dispositions in `dev/adversarial/2026-06-29-elmer-model-config-*.md`). All findings are
ADOPTED except the cleartext-`http` note (operator decision: **no note** — see §F). The arm/taint
**transmit** gate is untouched throughout. These sections supersede the corresponding parts above.

## R2.1 — Shared `AgentEndpoint` egress policy (replaces §Backend.1)

This is **socket-layer hygiene, not provider-choice framing** — the distinction drawn by the
project's own `docs/pitfalls/implementation-pitfalls.md` **SSRF-1** ("no-added-safeguards governs
UX; it does NOT override socket-layer egress hygiene"). The operator still reaches any real host
they type; only attacker-controlled redirects/rebinds/metadata-literals are refused.

`AgentEndpoint::parse(&str) -> Result<AgentEndpoint, EndpointError>`:
- **`http`/`https` only** (preserve the existing `UnsupportedScheme` rejection + test).
- **Reject URL userinfo** (`http://user:pass@host` → error) — prevents creds in logs/error strings.
- Expose `is_loopback()` (drives the "no key field for loopback" UX) and `origin()` (for the
  origin-keyed keyring, R2.2).

`build_vetted_client(&AgentEndpoint) -> reqwest::Client`, used by BOTH `elmer_detect_models` and
the per-turn provider (replace the bare `reqwest::Client::new()` at `provider.rs:47`):
- **`redirect::Policy::none()`** — a `302 → 169.254.169.254` (cloud metadata) or `→ localhost:adminport`
  otherwise carries the conversation; a 3xx becomes a hard error ("endpoint returned a redirect; not
  followed").
- **`.no_proxy()`**, a connect timeout (~10 s; the overall turn timeout stays generous for slow models).
- **Fetch-time resolved-IP gate** (the SSRF-1 `build_vetted_client` pattern already in
  `src-tauri/.../tiles/fetch.rs` — reuse the infra, but with Elmer's permit-set, NOT the tiles
  default-deny-public): resolve the host, then **refuse** if any resolved IP is loopback (unless the
  endpoint was loopback by literal), link-local/metadata (`169.254.0.0/16`, `fe80::/10`, IPv4-mapped
  forms), multicast, or unspecified; **permit** public + RFC1918 (the operator may run a LAN model
  server). **Pin** the connection to the vetted IP so DNS can't rebind between resolve and connect.
- The DNS-rebind IP-pin is the one heavier item: ship it, or if plan reviewers judge it heavy for
  alpha, ship redirect + metadata-literal + scheme + userinfo now and **file the IP-pin as a tracked
  bd follow-up** — do not silently drop (per the original `endpoint.rs:54-59` trigger condition).

Rename `LoopbackEndpoint` → `AgentEndpoint`; update `ElmerProvider::new`'s signature + the
agent-frontend crate seam.

## R2.2 — Credential model (replaces §Backend.3-4 + the form's key field)

- **`ApiKey(String)` newtype** with manual `Debug`/`Display` → `<redacted>` (type-based redaction —
  the only thing that catches a key embedded in a free-form `error`/`message` string, which the
  field-name redaction sink structurally cannot).
- **The key NEVER round-trips to the renderer.** `elmer_config_read()` returns
  `{ agent_endpoint, agent_model, key_status: KeyStatus }` where
  `KeyStatus = Present | Absent | Unreadable` (fail-closed 3-state, mirroring the identity module's
  `activation_secret_status`; a **locked** keyring must read `Unreadable`, never `Absent`).
- `elmer_detect_models({ agent_endpoint, key_source: KeySource })`,
  `KeySource = UseStored | Inline(ApiKey) | None`. `UseStored` → backend reads the keyring itself
  (renderer passes a discriminant, not the secret); `Inline` only for the just-typed-not-yet-saved
  flow. `#[instrument(skip(key_source))]`; **value-scrub** the just-sent key out of any upstream
  error body before it becomes an error string; 401/403 → fixed "check the API key" reason (never
  echo the body).
- `elmer_config_set({ agent_endpoint, agent_model, key: SetKey })`,
  `SetKey = Keep | Set(ApiKey) | Clear`. `Set("")` is a **validation error** (never write an empty
  credential). **Transactional:** on `Set`, write the keyring **first**; if it fails, persist
  **nothing** ("couldn't save the key — nothing was changed"). No half-commit.
- **Origin-keyed keyring account** `elmer-agent-api-key::<origin>` so a provider switch can't reuse a
  foreign key (OpenAI→Ollama→OpenRouter with `Keep` otherwise sends the stale OpenAI key to
  OpenRouter). Switching to a loopback/keyless config does not send a stored key.
- `Clear` is idempotent (`NoEntry → Ok`, matching `service.rs`).
- `#[instrument(skip(...))]` on `elmer_config_set` too; a test asserts the key never appears in a
  captured `LoggedEvent` for either command.

## R2.3 — Live-apply seam (replaces §Backend.5)

"Per session" is meaningless (one `ElmerSession` for process life) — the seam is **per-turn**.
- Build the provider at `send()` entry from **one atomic snapshot** of `{config, key}` read under the
  run loop's lock (or a managed `ElmerModelConfigState` async lock); move the owned `Arc<dyn Provider>`
  into the spawned turn. The build returns `Result` → `RunOutcome::NeedsOperator("…check Connect an
  AI Agent settings")` on failure (mirror the existing startup fallback; **never panic**).
- `elmer_config_set` persists config+keyring **under the same lock** → the config/key pair is atomic
  w.r.t. turns (closes the endpoint-A + key-B torn read that would send a key to the wrong endpoint).
  Changes apply to the **next** turn; an in-flight turn keeps its snapshot.
- Read the keyring only when `!is_loopback`; keep the blocking keyring call in the pre-spawn /
  `spawn_blocking` section.
- **AC-7 provider contract reword** (`provider.rs:11-18` + tests `:316-344`, an unlisted touch-item):
  keep "endpoint never from a **tool result**" (the real SSRF guard); drop "no command supplies an
  endpoint" (now intentionally false — the operator supplies it via `elmer_config_set`).

## R2.4 — MCP boundary (new)

`elmer_config_read` / `elmer_config_set` / `elmer_detect_models` are **Tauri UI commands only,
never MCP tools** — a model-reachable config-mutation tool would let prompt-injection rewrite its own
endpoint = exfil sink. **Document factually** (not as a warning): a cloud endpoint receives Elmer's
conversation + tool outputs; ungated *staging* tools (compose/forms/GRIB) remain reachable but still
cannot **send** (the arm/taint gate). This is existing, accepted behavior.

## R2.5 — Prompt-injection regression suite (new testing requirement)

Borrow the method (an adversarial vector corpus) from MS Agent-Governance-Toolkit's PromptDefense
**17-vector taxonomy** — NOT its dependency (we gate deterministically; we do not lint the system
prompt). Build a corpus of hostile **inbound-message** payloads exercising the vectors that hit our
surface — `indirect-injection`, `encoding-injection` (base64/unicode-smuggled), `least-agency /
goal-hijack`, `data-protection` (system-prompt/key leak) — and assert the **deterministic invariants
hold under injection**:
1. No config mutation: the model tool list contains no `elmer_config_*`; injection cannot change
   `agent_endpoint`/`agent_model`/the key (structurally — R2.4).
2. Withheld egress tools stay unreachable (re-assert `executor.rs:50` after this feature lands).
3. No transmit without an operator arm; staging tools reachable-but-cannot-send.
4. No secret leak: system prompt / API key never echoed back through tool-result or error handling.
This operationalizes the review's MCP-boundary finding and maps to the OWASP Agentic Top 10
categories the toolkit enumerates.

## R2.6 — Operator UX (augments §Design / §Error handling)

- **Empty state is a `"Connect a model"` button** that expands the Model section **in place** (not a
  sentence pointing at a separate menu — chicken-and-egg).
- **Detect failures carry remedies, not labels**, keyed off loopback/preset: loopback refused →
  "the local AI server (Ollama) may not be running — start it, then Detect again"; remote transport →
  "check this device's internet"; 401/403 → "re-enter the key for <provider>".
- **Key affordance: `Key stored 🔒 [Replace] [Remove]`**, not a `••••`-seeded field (touch-and-clear
  silently wipes a key the operator can never re-read). `Replace` reveals an empty input committing as
  `Set` only on non-empty; `Remove` is the explicit `Clear`. Destruction is never inferred from
  emptiness.
- **Preset inference by `origin()`, not exact URL**; selecting a preset must not clobber a
  hand-edited endpoint without confirmation.
- **Per-turn model attribution:** on a mid-conversation model change, drop an inline
  "— now using `<model>` —" marker styled like the ground-truth tool chips.
- **Detect URL derivation:** replace a trailing `/chat/completions` with `/models`, **preserving the
  path prefix** (`/api/v1/chat/completions` → `/api/v1/models`); validate both URLs through R2.1.
- POLISH: "✓ 0 models" on an empty Ollama → a "pull a model" remedy, not a green check; soft
  provider/model-mismatch hint; prefer **one** menu door + the in-context button, or differentiate by
  verb ("Set up Elmer's model…" vs "Open Elmer chat…").

## R2.F — Cleartext `http` note: NO NOTE (operator decision, 2026-06-29)

Held at zero notes. Loopback `http` (the common Ollama case) never had an exposure — the bytes don't
leave the machine. The only residual case is a *non-loopback* `http` endpoint **with** a key, which
is the operator's explicit choice; R2.1's `redirect::Policy::none()` + userinfo rejection already stop
the key reaching an attacker-controlled target, leaving only passive eavesdrop on the operator's own
plaintext route — theirs to accept.

## R2.7 — Touch-list additions

- `src-tauri/tuxlink-agent-frontend/src/endpoint.rs` — `AgentEndpoint` + egress policy (or a new
  `egress.rs`); reuse the `build_vetted_client` IP-gate infra from the tiles fetch path.
- `src-tauri/tuxlink-agent-frontend/src/provider.rs` — `build_vetted_client`, `ApiKey` newtype,
  value-scrub of error bodies, AC-7 doc/tests reword.
- `src-tauri/src/elmer/` + `lib.rs` — `KeySource`/`KeyStatus`/`SetKey`, origin-keyed keyring,
  transactional set, snapshot-at-turn provider build under the lock, `#[instrument(skip)]`, the
  config commands (Tauri-only), the MCP-boundary regression test, the prompt-injection corpus.
- Redaction: `ApiKey` type-redaction + the bearer-header rule (the field-name sink alone is
  insufficient).
