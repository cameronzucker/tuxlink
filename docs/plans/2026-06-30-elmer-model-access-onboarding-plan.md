# Elmer Model-Access Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Revision 2** — incorporates 5-round design adrev + 3-round plan review (Codex + Claude). Source findings: `dev/scratch/elmer-adrev-consolidated.md`.

**Goal:** Give the keyless, non-developer operator a cloud-first, price-tiered model picker that is Elmer's canonical model surface (first-run AND settings), with a guided "get a free key" flow and honest per-tier framing.

**Architecture:** A new `<ModelTilePicker>` becomes the primary model surface, consuming `PRESETS` + per-preset metadata from `elmerModelConfig.ts`. The existing `ModelForm` (in the 1133-line `ElmerPane.tsx`) is EXPORTED and REUSED VERBATIM as the "Other / custom endpoint" branch — not rewritten. Three backend additions (onboarding sentinel, per-origin key-status, typed 429) support honest first-run + per-tile state. The #981 origin/key-reset logic is preserved; all picker work is additive.

**Tech Stack:** Tauri 2.x (Rust, `src-tauri/`), React 18 + TypeScript (Vite, `src/`), WebKitGTK 4.1, vitest, `@tauri-apps/plugin-shell`.

## Global Constraints

- **MSRV 1.75** — no API stabilized in 1.76+ (clippy `incompatible_msrv` denied). **CI bar for Rust tasks:** `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` (test code is linted too) + `cargo test --locked`, BOTH arches. Verify CI by matching headSha + conclusion (NOT bare `gh run list --limit 1`, which latches stale=false-green). No new crate → no Cargo.lock regen; confirm none is added.
- **Pi cannot cold-compile Rust** — Rust tasks (3,4,5) are CI-verified on the PR, not locally.
- **DECOUPLING (the load-bearing sequencing decision):** the **TypeScript contract** for the new backend fields lands as pure TS in **Task 1** (`onboarded` on `ConfigReadDto`; the `KeyStatusByOrigin` return type; the `rateLimited` `ElmerPhase` + `outcomeKindToPhase` case). Frontend tests mock the Tauri `invoke` boundary, so **every frontend task is `pnpm vitest run src/elmer/`-green locally with zero Rust compiled.** The Rust producers (Tasks 3/4/5) land in the same PR; the **serde wire-shape test in each Rust task is the cross-language guarantee** that Rust emits exactly the literals the TS matches. No frontend task waits on a Rust merge.
- **Frontend gate:** `pnpm vitest run src/elmer/` — the WHOLE dir (scoped single-file runs miss contract tests / silently drop counts) — plus `pnpm typecheck`, green before any frontend task is done. RED-phase single-test runs (`-t`) are fine; GREEN must be whole-dir. Assert the total test count did not drop wherever a task rewrites existing tests (Tasks 8b, 11).
- **TDD mandatory** — failing test → minimal impl → green, every task.
- **Reuse `ModelForm` verbatim** (export it; Task 0). Do NOT rewrite it. **Keep the #981 fix untouched:** the reset `useEffect` (`ElmerPane.tsx:450-460`), `effectiveKeyStatus` (`:477-480`), `buildSetKey` (`:525-550`). Changes ADDITIVE only.
- **No parallel second model-selection path** — the picker and the custom-branch `ModelForm` share `PRESETS` + the metadata map + the `nextModelForPreset` helper (Task 7).
- **Secrets:** keyring only; never echo a key value in any tile label/badge/log; "open key page" URLs are hardcoded constants mapped from a preset-id enum, NEVER config/endpoint-derived. **No renderer-side `fetch`/XHR to any provider endpoint — all egress goes through `elmer_config_set`/`elmer_detect_models` → `ElmerProvider::new_vetted` (SSRF-1, implementation-pitfalls §8; Codex #10).**
- **serde raw-box trap** (`reference_serde_rename_all_enum_fields`): `rename_all` tags enum variants but NOT their fields, and casing must match the TS matcher exactly. Every new DTO field / enum variant (`onboarded`, `KeyStatus` variants, the 429 reason) gets a serde test asserting the EXACT on-wire literal the TS side matches.
- **Keyring tests** run in an isolated `HOME`/`XDG_*` with `assert_keyring_isolated()` (testing-pitfalls §7, tuxlink-cnd) — NEVER against the real login keyring.
- **vitest invoke-mock teardown** (`feedback_vitest_invoke_mock_cleanup_call`): the invoke mock is called with no args at teardown — write call-arg assertions to account for it; clean mocks between tests (no shared mutable state, testing-pitfalls §7).
- **LAYOUT-1** (implementation-pitfalls §4): if any new element becomes a direct child of `.panes`, give it an explicit `grid-column` mirroring `.contacts-panel`/`.reading-pane`. The picker should live INSIDE `.elmer-messages` (Elmer's own flex region, NOT a `.panes` child) — state that explicitly so the reviewer can confirm.
- **No added safeguards** beyond WLE parity (no airtime/usage caps; `feedback_no_tuxlink_added_safeguards`). RADIO-1 N/A (inference, not transmit).
- **Commits:** conventional type + `Agent: basil-cedar-fern` + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`. Branch `bd-tuxlink-wpqwy/elmer-model-access-onboarding`. No destructive git; no `--no-verify`.
- **Rust line numbers are HINTS — locate by symbol** (`ElmerConfig`, `ConfigReadDto`, `config_set_inner`, `config_read`, the `invoke_handler!`/`generate_handler!` macro). The Pi can't compile Rust, so a wrong line ref won't fail fast.

---

## File Structure

- `src/elmer/elmerModelConfig.ts` — `ProviderPreset` (tier/keyPageUrl/defaultModel); `anthropic`; `DEFAULT_MODEL_BY_PRESET`; `onboarded` on `ConfigReadDto`; `KeyStatusByOrigin` type. (Tasks 1)
- `src/elmer/elmerModelConfig.test.ts` — contract test + new cases. (Task 1)
- `src/elmer/useElmer.ts` — `onboarded` passthrough; `keyStatusForOrigins` wrapper; `rateLimited` phase mapping. (Tasks 1, 4-fe, 10)
- `src/elmer/ElmerPane.tsx` — export `ModelForm` (Task 0); `buildKeySource` fix (6); `nextModelForPreset` + `handlePresetChange` (7); wire picker + first-frame gate + reopen (8b); rate-limit phase + framing (10).
- `src/elmer/ModelTilePicker.tsx` (NEW) + `.test.tsx` — picker (8a) + guided get-a-key flow (9).
- `src/elmer/ElmerPane.css` — tiles/tiers/guided-card/single-column layout. (8a, 9, 10)
- `src-tauri/capabilities/default.json` — `shell:allow-open` URLs. (Task 2)
- `src-tauri/src/config.rs` — `ElmerConfig.onboarded`. (Task 3)
- `src-tauri/src/elmer/config_commands.rs` — `onboarded` DTO+set; `elmer_key_status_for_origins`; typed 429. (3,4,5)
- `src-tauri/src/lib.rs` — register key-status command. (Task 4)
- `src-tauri/tuxlink-agent-frontend/src/provider.rs` + the outcome/EV path — typed 429. (Task 5)

**Execution order:** 0 → 1 → 2 → 6 → 7 → 8a → 8b → 9 → 10 → 11 (all frontend, locally vitest-green) → 3 → 4 → 5 (Rust, CI-verified) → 12 (browser-smoke + wire-walk). Frontend-first front-loads verifiable progress; Rust lands before the PR's CI gates.

---

## Task 0: Export ModelForm (precursor — unblocks 8a/9/11)

**Files:** Modify `src/elmer/ElmerPane.tsx` (`function ModelForm` ~407).

- [ ] **Step 1:** Change `function ModelForm(` to `export function ModelForm(` (one-line additive change; do NOT move or alter its body).
- [ ] **Step 2:** `pnpm typecheck` + `pnpm vitest run src/elmer/` — expect PASS (no behavior change).
- [ ] **Step 3: Commit** `refactor(elmer): export ModelForm so the tile picker can reuse it verbatim`.

**Do NOT:** change ModelForm's logic, props, or testid (`elmer-model-form` at `:629`).

---

## Task 1: Preset metadata + Anthropic + default-model map + TS DTO contract + contract test

**Files:** Modify `src/elmer/elmerModelConfig.ts` (PRESETS ~79-115; `ProviderPreset` ~62-70; `ConfigReadDto` ~27-33; header ~5) and `src/elmer/useElmer.ts` (ElmerPhase union + `outcomeKindToPhase` ~103-128). Test `src/elmer/elmerModelConfig.test.ts` (contract ~32-42, `:93-99`, header `:5`/title `:33`, inferPreset block ~150).

**Interfaces produced:**
- `interface ProviderPreset { id; label; endpoint; tier:'free'|'paygo'|'local'|'other'; defaultModel?:string; keyPageUrl?:string }`
- `PRESETS` gains `{id:'anthropic', label:'Anthropic — Claude', endpoint:'https://api.anthropic.com/v1/chat/completions', tier:'paygo', defaultModel:'claude-haiku-4-5', keyPageUrl:'https://console.anthropic.com/settings/keys'}` immediately AFTER `openai`. Final id order: `['localOllama','openai','anthropic','openrouter','gemini','groq','custom']`.
- `export const DEFAULT_MODEL_BY_PRESET: Record<string,string>` = `{localOllama:'', openai:'gpt-4o-mini', anthropic:'claude-haiku-4-5', openrouter:'', gemini:'gemini-2.5-flash', groq:'llama-3.3-70b-versatile', custom:''}`.
- keyPageUrl per preset (hardcoded const): gemini `https://aistudio.google.com/apikey`, groq `https://console.groq.com/keys`, anthropic `https://console.anthropic.com/settings/keys`, openai `https://platform.openai.com/api-keys`. local/openrouter/custom omit.
- tier per preset: localOllama→local, openai→paygo, anthropic→paygo, openrouter→other, gemini→free, groq→free, custom→other.
- **TS contract additions (decoupling):** `ConfigReadDto` gains `onboarded: boolean`. New `export type KeyStatusByOrigin = Record<string, KeyStatus>`. `ElmerPhase` (useElmer.ts) gains `'rateLimited'`; `outcomeKindToPhase` maps the literal `'rate_limited'` → `'rateLimited'`.

- [ ] **Step 1: Update the contract test FIRST.** Set the exact-order `toEqual` to the 7-id array above; update the `:33` title string + the `:93-99` `toContain` block + the stale "four providers" header comment (`:5`). Add:
```ts
it('infers the anthropic preset from its origin', () =>
  expect(inferPreset('https://api.anthropic.com/v1/chat/completions')).toBe('anthropic'));
it('maps every preset id to a default-model entry', () => {
  for (const p of PRESETS) expect(DEFAULT_MODEL_BY_PRESET).toHaveProperty(p.id);
});
it('free/paygo presets carry a https keyPageUrl and a valid tier', () => {
  for (const p of PRESETS) {
    expect(['free','paygo','local','other']).toContain(p.tier);
    if (p.tier === 'free' || p.tier === 'paygo') expect(p.keyPageUrl).toMatch(/^https:\/\//);
  }
});
```
Add (in a useElmer or elmerModelConfig test as appropriate) a case asserting `outcomeKindToPhase('rate_limited') === 'rateLimited'` and that `ConfigReadDto` type accepts `onboarded`.
- [ ] **Step 2: Run `pnpm vitest run src/elmer/` — expect FAIL.**
- [ ] **Step 3: Implement** in `elmerModelConfig.ts` (extend interface, insert anthropic after openai, add tier/keyPageUrl/defaultModel to all entries, export `DEFAULT_MODEL_BY_PRESET` + `KeyStatusByOrigin`, add `onboarded` to `ConfigReadDto`, fix header comment) and `useElmer.ts` (`rateLimited` in the phase union + the `'rate_limited'`→`'rateLimited'` mapping). Do NOT change `originOf`/`inferPreset`/`isLoopback`.
- [ ] **Step 4: `pnpm vitest run src/elmer/` + `pnpm typecheck` — expect PASS.**
- [ ] **Step 5: Commit** `feat(elmer): Anthropic preset, per-preset metadata, default-model map, and TS DTO contract for onboarded/key-status/rate-limit`.

**Do NOT:** reorder presets beyond inserting anthropic after openai; add OpenRouter to free; add Rust here.

---

## Task 2: shell:allow-open allowlist for provider key pages (C1 — BLOCKER)

**Files:** Modify `src-tauri/capabilities/default.json` (`shell:allow-open`, ~16-23; object form `{ "url": "https://…/**" }`).

- [ ] **Step 1:** Add four path-tight entries to the `shell:allow-open` `allow` array, copying the existing entry's exact object shape: `https://aistudio.google.com/**`, `https://console.groq.com/**`, `https://console.anthropic.com/**`, `https://platform.openai.com/**`.
- [ ] **Step 2:** `python3 -c "import json;json.load(open('src-tauri/capabilities/default.json'))"` exits 0.
- [ ] **Step 3: Commit** `feat(elmer): allowlist provider key-page URLs for the get-a-key flow`.

**Do NOT:** broaden beyond path-tight; touch `wizard.json` (Elmer is in `main`); allow non-https. (vitest mocks plugin-shell — this is verified only by the Task 12 browser-smoke.)

---

## Task 6: Fix buildKeySource Detect bug on inter-provider switch (F3 — BLOCKER)

**Files:** Modify `src/elmer/ElmerPane.tsx` (`buildKeySource` ~565-589; the bug is the raw `keyStatus === 'absent'` at `:577`). Test `src/elmer/ElmerPane.test.tsx` (mirror the Detect-inline describe at `:1323-1402`).

**Context:** post-switch, `effectiveKeyStatus`→'absent' renders the absent input; operator types `absentKeyValue`; but raw `keyStatus`='present' → `:577` false → `:585` `originMatchesLoaded` false → returns `{source:'none'}`. Detect/Test sends no key. This is the Detect-path analog of the #981 `buildSetKey` fix.

- [ ] **Step 1: Write the failing test** — render `<ElmerPane>` (NOT ModelForm directly — it is reachable but tests drive the full pane); mock `elmer_config_read` → OpenAI endpoint + `keyStatus:'present'`; use the existing `renderAndOpen()` helper to open the model section; change `elmer-endpoint-input` to the Gemini origin; type into the absent-key input (`elmer-key-input` / the absent-key testid used in the `:1323` test); click `elmer-detect-btn`; assert the `elmer_detect_models` invoke arg `keySource === { source:'inline', value:<typed> }`. Copy structure from the `:1323-1402` `detect_uses_inline_key_when_typed_not_saved` describe. Account for the invoke-mock teardown no-arg call.
- [ ] **Step 2: Run `pnpm vitest run src/elmer/ElmerPane.test.tsx -t '<name>'` — expect FAIL** (`source:'none'`).
- [ ] **Step 3: Implement** — in `buildKeySource`, make the inline-key term fire when origin diverged: replace `(keyStatus === 'absent' && absentKeyValue)` with one that also fires when `originOf(endpoint) !== keyAffordanceOrigin`. Keep the loopback short-circuit and `useStored` branch unchanged.
- [ ] **Step 4: `pnpm vitest run src/elmer/` (whole dir) + `pnpm typecheck` — expect PASS** (all #981 tests still green).
- [ ] **Step 5: Commit** `fix(elmer): Detect/Test sends the pasted key after an inter-provider switch (buildKeySource origin-aware)`.

**Do NOT:** modify `buildSetKey`, the reset `useEffect`, or `effectiveKeyStatus`.

---

## Task 7: nextModelForPreset helper + handlePresetChange model repopulation (F2 — BLOCKER)

**Files:** Modify `src/elmer/ElmerPane.tsx` (`handlePresetChange` ~488-522, deps `[endpoint]` at `:522`). Test `src/elmer/ElmerPane.test.tsx`.

**Context:** switching providers leaves the old model (404s). Repopulate to the new default ONLY when the current model is untouched (== the OUTGOING preset's default). The real confirm guard (`:505-519`) fires on ENDPOINT-dirty only — it does NOT relate to a hand-edited model; do not couple the two.

**Interface produced:** `export function nextModelForPreset(currentEndpoint: string, currentModel: string, targetPresetId: string): string | null` — returns the target preset's `defaultModel` when `currentModel === DEFAULT_MODEL_BY_PRESET[inferPreset(currentEndpoint)]` (untouched) and the target has a non-empty default; otherwise `null` (preserve current). Consumed by both `handlePresetChange` and the picker (Task 8a) — single source of truth, no duplication.

- [ ] **Step 1: Write failing tests** — (a) `nextModelForPreset('https://api.openai.com/v1/chat/completions','gpt-4o-mini','anthropic') === 'claude-haiku-4-5'` (untouched → new default); (b) `nextModelForPreset('https://api.openai.com/...','gpt-4o','anthropic') === null` (hand-edited → preserve); (c) component test: switch from OpenAI tile (model = its default) to Anthropic → model field shows `claude-haiku-4-5`; (d) component test: hand-edited model + untouched endpoint → switch provider → model is PRESERVED and NO `window.confirm` fires (the endpoint-dirty confirm is a separate concern).
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** — add the exported `nextModelForPreset` helper; in `handlePresetChange`, after the existing `setEndpoint(...)`, add `const m = nextModelForPreset(endpoint, model, presetId); if (m !== null) setModel(m);`; **add `model` to the `useCallback` deps array → `[endpoint, model]`**. Leave the endpoint-dirty `window.confirm` exactly as-is; leave the `custom` branch (clear endpoint, focus) as-is.
- [ ] **Step 4: `pnpm vitest run src/elmer/` + `pnpm typecheck` — PASS.**
- [ ] **Step 5: Commit** `feat(elmer): tile selection pre-fills the provider default model, preserving a hand-edited one`.

**Do NOT:** touch key state; add a second confirm; couple model-overwrite to the endpoint-dirty guard.

---

## Task 8a: ModelTilePicker component (F1-consume, F4)

**Files:** Create `src/elmer/ModelTilePicker.tsx` + `src/elmer/ModelTilePicker.test.tsx`. Modify `src/elmer/ElmerPane.css`.

**Interface produced:** `<ModelTilePicker onSave onDetect detectState keyStatusByOrigin initialEndpoint initialModel initialKeyStatus initialTurnTimeoutSecs />`. Renders four tier groups (Free/Pay-as-you-go/Local/Other) from `PRESETS` grouped by `tier`; RECOMMENDED badge on `gemini`; per-tile "key saved ✓" badge from the `keyStatusByOrigin` prop (statuses only, never values); the tile matching `inferPreset(initialEndpoint)` pre-selected showing `initialModel` (NOT the tile default); selecting a different tile uses `nextModelForPreset`; the Other/custom tier renders the EXPORTED `ModelForm` verbatim. `keyStatusByOrigin` is a PROP (testable), not a hook call.

- [ ] **Step 1: Failing tests** — (a) four tier headers + a tile per preset; (b) Gemini has RECOMMENDED; (c) `keyStatusByOrigin={{'https://api.openai.com':'present'}}` → OpenAI tile shows key-saved badge, no value rendered; (d) `initialEndpoint`=Anthropic → Anthropic tile pre-selected, shows `initialModel`; (e) selecting the Other tier renders `data-testid="elmer-model-form"`. Account for invoke-mock teardown if any test wires invoke.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** the component (single-column; tiles are real `<button>`/`role="radio"`, keyboard-reachable; import `ModelForm` from `./ElmerPane`; use `nextModelForPreset` on tile switch). CSS: tiles/tiers/guided-card; the picker is rendered INSIDE `.elmer-messages` flex region (LAYOUT-1: NOT a `.panes` child — state this in a CSS comment).
- [ ] **Step 4: `pnpm vitest run src/elmer/` + `pnpm typecheck` — PASS.**
- [ ] **Step 5: Commit** `feat(elmer): ModelTilePicker component (tiered tiles, key-saved badges, custom-branch reuse)`.

**Do NOT:** call `keyStatusForOrigins` hook inside the component (take the map as a prop); rewrite ModelForm; create a second model-selection path.

---

## Task 8b: Wire the picker as canonical surface — first-frame gate + reopen (F5, F6)

**Files:** Modify `src/elmer/ElmerPane.tsx` (gate `hasNoModelConfigured` ~937/1022; `elmer-advanced` disclosure ~1084-1124; `expandModel`/`advancedOpen` ~906; consume `onboarded` from `modelConfig`; call `keyStatusForOrigins` Task 4-fe wrapper). Modify `src/elmer/useElmer.ts` (expose `onboarded`). Test `src/elmer/ElmerPane.test.tsx`.

- [ ] **Step 1: Failing tests** — (a) `elmer_config_read` mock returns `{...,onboarded:false}` → the picker renders in place of the message list and the chat input is disabled with a hint (not the old "Connect a model" button); (b) `onboarded:true` → chat renders, picker not shown by default; (c) opening the model section after Elmer has already mounted (simulate a second open via the gear / `expandModel` change) re-renders the picker (F6 reopen — effect/open-counter, not initial-state-only). Assert total test count did not drop.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** — replace the `hasNoModelConfigured` derivation with `modelConfigState==='loaded' && modelConfig && !modelConfig.onboarded`; when not onboarded, render `<ModelTilePicker>` in place of the message list AND disable the chat input (with a one-line hint) until onboarded — drop the `items.length===0` coupling (state in a comment that the picker replaces the list). Replace the `advancedOpen` initial-state-only open with an effect keyed off an open-counter / `expandModel` change so the gear reopens the picker after mount (`:906` is initial-state only today). Pass `keyStatusByOrigin` (from the Task 4-fe wrapper, called once on open) to the picker.
- [ ] **Step 4: `pnpm vitest run src/elmer/` + `pnpm typecheck` — PASS; confirm count didn't drop.**
- [ ] **Step 5: Commit** `feat(elmer): tile picker is the first-run + settings surface; chat gated until onboarded; gear reopens after mount`.

**Do NOT:** remove the arm-strip/footer; leave a second open path; key the gate off empty `agentModel`.

---

## Task 9: Guided "get a free key" flow + masked key field (F7, F12)

**Files:** Modify `src/elmer/ModelTilePicker.tsx` (or new `GetKeyCard.tsx`) + test; `src/elmer/ElmerPane.css`.

- [ ] **Step 1: Failing tests** — (a) selecting Gemini shows a guided card with an "Open key page" button; click calls the mocked plugin-shell `open()` with EXACTLY `https://aistudio.google.com/apikey` (the Task 1 constant); (b) paste field is `type="password"` with a reveal toggle → `type="text"`; (c) pasting `"  AIza...short  "` trims; a paste failing `len>=20 && /^[A-Za-z0-9_\-]+$/` shows "that doesn't look like a complete key" and blocks Save; a valid paste enables Save; (d) a "stuck?" affordance offers the alternate free provider (Groq) / paygo.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** — steps written as OUTCOMES not button labels ("Sign in with any Google account, then create an API key — usually labeled 'Create API key'"); Gemini step notes sign-in/project prerequisite; open page via `preset.keyPageUrl` constant ONLY; trim + sanity-validate (len>=20, charset) before delegating Save to the existing `configSet`/`saveState`; reveal toggle; "stuck?" branch.
- [ ] **Step 4: `pnpm vitest run src/elmer/` + typecheck — PASS.**
- [ ] **Step 5: Commit** `feat(elmer): guided get-a-free-key flow with masked key entry`.

**Do NOT:** pass any non-constant string to `open()`; do renderer-side network validation (SSRF-1); store keys outside the keyring.

---

## Task 10: Test/Save reuse, rate-limit recovery UI, honest framing, layout (F8, F9, F10, F11)

**Files:** Modify `src/elmer/ElmerPane.tsx` (OutcomeCallout ~352; consumes the `rateLimited` phase from Task 1; footer ~1128); `src/elmer/ModelTilePicker.tsx` (framing; reuse detect/save state); `src/elmer/ElmerPane.css`. Test both.

- [ ] **Step 1: Failing tests** — (a) a `rate_limited` outcome → `rateLimited` phase → a distinct callout with provider-keyed copy + a "Switch provider" button that opens the picker at the paygo tier; (b) Test reuses `detectState` (success/auth/network copy) and Save reuses `saveState` (`.elmer-save-error`/`.elmer-save-ok`); (c) chat footer shows the active provider class ("Using Google Gemini (free cloud)") for cloud tiers; (d) Free-tier copy contains the training-on-data sentence + a "what gets sent" note; Local copy is the constructive reframe.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** — render the `rateLimited` callout (the phase/mapping already exist from Task 1; here it's the UI + Switch-provider action); ensure Test routes through `detectState` and Save through `saveState` (no thinner reimpl); persistent provider-class footer indicator; honest copy. CSS: single column at `min(420px,92vw)`; picker scrolls within `.elmer-messages`; input row + footer pinned; header ≤ `--elmer-dock-top:38px` so window controls stay uncovered.
- [ ] **Step 4: `pnpm vitest run src/elmer/` + typecheck — PASS.**
- [ ] **Step 5: Commit** `feat(elmer): rate-limit recovery, honest framing, persistent provider indicator, single-column layout`.

**Do NOT:** add airtime/usage caps; over-redact secure tokens; auto-retry on 429.

---

## Task 11: Behavior regression tests — credential seam + new flows (T2)

**Files:** Modify `src/elmer/ElmerPane.test.tsx`, `src/elmer/ModelTilePicker.test.tsx`.

- [ ] **Step 1:** Port the #981 credential-seam regression tests across all THREE describes (`ElmerPane.test.tsx:1210-1436` — Save-path `:1210`, Detect-inline `:1323`, useStored `:1404`) to the new picker flow — do NOT delete; re-express interactions against the tile UI where the dense form moved. Add an Anthropic-origin regression (switch to Anthropic, stored-key semantics correct). Confirm Task 6/7 cases coexist.
- [ ] **Step 2: `pnpm vitest run src/elmer/` — PASS; explicitly confirm the total test count did NOT drop** (contract-test trap).
- [ ] **Step 3: Commit** `test(elmer): port credential-seam regressions to the tile flow + Anthropic coverage`.

**Do NOT:** weaken/delete #981 assertions; rely on a single-file run.

---

## Task 3: Onboarding sentinel (B1 — BLOCKER, Rust, CI-verified)

**Files:** Modify `src-tauri/src/config.rs` (`ElmerConfig` + its `Default`); `src-tauri/src/elmer/config_commands.rs` (`ConfigReadDto`, `config_read`, `config_set_inner`). Locate by symbol (line hints only).

- [ ] **Step 1: Write the failing Rust test** — fresh `ElmerConfig::default()` has `onboarded == false`; after `config_set_inner` with a valid endpoint+model, `config_read`'s DTO has `onboarded == true`. **serde wire-shape test:** the DTO serializes the field as exactly `"onboarded"` (camelCase) — assert against the literal the TS `ConfigReadDto` reads.
- [ ] **Step 2:** CI-verify note (can't compile cold locally).
- [ ] **Step 3: Implement** — `pub onboarded: bool` on `ElmerConfig` with `#[serde(default)]` (clean default for existing on-disk configs; NOT skipped when true). `onboarded: bool` on `ConfigReadDto` (copy the EXACT serde attribute style the DTO uses for `agentTurnTimeoutSecs` — container `rename_all="camelCase"` vs per-field rename; the raw-box trap applies). Set `config.elmer.onboarded = true` on successful `config_set_inner`. Populate in `config_read`.
- [ ] **Step 4:** CI bar: clippy `--all-targets --locked -D warnings` + `cargo test --locked`, both arches, MSRV 1.75; verify by headSha+conclusion.
- [ ] **Step 5: Commit** `feat(elmer): onboarded sentinel distinguishes never-configured from implicit default`.

**Do NOT:** change the default endpoint/model; break deserialization of existing configs.

---

## Task 4: Per-origin key-status command (B2 — MAJOR, Rust + frontend)

**Files:** Modify `src-tauri/src/elmer/config_commands.rs` (new `elmer_key_status_for_origins`); `src-tauri/src/lib.rs` (register in the `generate_handler!`/`invoke_handler!` macro). Frontend: `src/elmer/useElmer.ts` (wrapper) — the type `KeyStatusByOrigin` already lands in Task 1.

**Interface produced:** `elmer_key_status_for_origins(origins: string[]) -> KeyStatusByOrigin` — keyring status per origin; statuses ONLY, never values; fail-closed (`unreadable`).
Frontend wrapper: `keyStatusForOrigins(origins: string[]): Promise<KeyStatusByOrigin>` in `useElmer.ts` (pure TS, invoke-mocked — independent of Rust merge).

- [ ] **Step 1 (Rust): failing test** in an ISOLATED keyring (`assert_keyring_isolated()`, isolated `HOME`/`XDG_*`, testing-pitfalls §7) — two origins, one with a stored key, one without → correct status each; unreadable keyring → `unreadable`, never a value. **serde wire-shape test:** each `KeyStatus` variant serializes to its exact lowercase literal (`present`/`absent`/`unreadable`) matching the TS `KeyStatus` union (raw-box trap).
- [ ] **Step 2:** CI-verify note.
- [ ] **Step 3 (Rust): Implement** reusing the keyring-read + status logic `config_read` uses for the active origin; iterate the supplied origins; register in `lib.rs`. No new crate.
- [ ] **Step 4 (frontend): Implement** the `keyStatusForOrigins` wrapper in `useElmer.ts`; vitest with the invoke mock returning a map; assert the wrapper passes origins through and returns the map; account for the invoke-mock teardown no-arg call. `pnpm vitest run src/elmer/`.
- [ ] **Step 5: Commit** `feat(elmer): per-origin key-status lookup for per-tile key-saved badges`.

**Do NOT:** return key values; run the keyring test against the real login keyring; call this per-keystroke (once on picker open / on save).

---

## Task 5: Typed 429 / rate-limit classification (B3 — BLOCKER, Rust)

**Files:** Modify `src-tauri/src/elmer/config_commands.rs` (detect error map ~163); `src-tauri/tuxlink-agent-frontend/src/provider.rs` (turn error ~176) + the outcome-kind path.

- [ ] **Step 0 (pre-trace — do before implementing):** locate the outcome/kind enum that carries `outcomeKind` and the `EV_TURN` payload type; record the exact Rust enum + the variant string that must reach the frontend `outcomeKindToPhase` (the TS side already maps the literal `'rate_limited'` → `'rateLimited'` from Task 1). Pin these names in the task notes so no CI round-trip is spent exploring.
- [ ] **Step 1: failing Rust test** — a simulated HTTP 429 upstream produces the typed `rate_limited` reason/kind (the exact literal the frontend matches), NOT a generic transport error; abort/no-runaway unchanged. **serde wire-shape test:** the reason serializes to exactly `"rate_limited"`.
- [ ] **Step 2:** CI-verify note.
- [ ] **Step 3: Implement** — detect path: map 429 to the `rate_limited` reason; turn path: classify 429 in the provider error and surface it through the outcome kind located in Step 0; keep the bounded-body behavior.
- [ ] **Step 4:** CI bar (clippy --all-targets -D, test, both arches, MSRV; verify by SHA).
- [ ] **Step 5: Commit** `feat(elmer): classify HTTP 429 as a typed rate-limit signal`.

**Do NOT:** add retry/backoff; change abort semantics; guess the outcome-kind names (Step 0 pins them).

---

## Task 12: Browser-smoke + wire-walk gate (T3 + reachability)

**Files:** none (verification); evidence → PR body + `dev/implementation-log.md`.

- [ ] **Step 1: Browser-smoke** via the WebKitGTK render harness (`dev/render-harness/`, no Rust build): render the picker; assert no horizontal scroll at 392px AND at a narrow 92vw window; tier headers + tiles visible; window controls visible; **the "Open key page" allowlist actually opens** (the ONLY check that catches a missing Task-2 grant — vitest mocks plugin-shell). Screenshots → `dev/scratch/`.
- [ ] **Step 2: Wire-walk gate** — invoke the `wire-walk` skill. **The operator supplies the key user flows GREENFIELD — do NOT draft them or feed any list; anchoring launders blind spots** (CLAUDE.md wire-walk gate). Trace each operator-supplied flow verbatim to `file:line`. ANY broken primary flow = NOT shipped. (Author's coverage expectation, for self-check only AFTER the operator's independent trace: first-run picker reachable; get-a-key opens browser; key saves→chat works; switch-provider Detect sends key; rate-limit→recovery→picker reopens.)
- [ ] **Step 3:** Record evidence (worktree/branch/SHA, local-vs-CI) in the PR body + `dev/implementation-log.md` top entry.

**Do NOT:** claim done before wire-walk passes; substitute CI-green for reachability; pre-supply the wire-walk flows.

---

## Self-review (revision 2)

- **Spec coverage:** every B/F/C/T finding maps to a task (see file headers). ✓
- **Plan-review fixes folded:** TS-contract-in-Task-1 decoupling (Global Constraints + Task 1); ModelForm export (Task 0); Task 6 renders `<ElmerPane>` not ModelForm + correct `:1323` pointer; Task 7 decoupled from the endpoint-confirm + deps `[endpoint, model]` + shared `nextModelForPreset`; Task 8 split 8a/8b; LAYOUT-1 (8a/8b/10); serde wire-shape tests (3/4/5); keyring isolation (4); count-didn't-drop (8b/11); CI bar + locate-by-symbol (3/4/5); Task 5 Step-0 pre-trace; vitest invoke-mock teardown (Global + 4/6/8a); SSRF-1 DO-NOT (9); wire-walk greenfield (12); 429 line `:163`. ✓
- **Type consistency:** `ProviderPreset.tier/keyPageUrl/defaultModel`, `DEFAULT_MODEL_BY_PRESET`, `KeyStatusByOrigin`, `onboarded`, `nextModelForPreset`, `keyStatusForOrigins`, `rateLimited`/`'rate_limited'` — consistent across tasks. ✓
