# Handoff — Elmer model-access onboarding: design + plan + foundation + picker shipped to branch

**Agent:** basil-cedar-fern · **Date:** 2026-06-30 · **bd:** `tuxlink-wpqwy` (in_progress)
**Branch:** `bd-tuxlink-wpqwy/elmer-model-access-onboarding` (pushed; 8 commits ahead of origin/main)
**Worktree:** `worktrees/bd-tuxlink-wpqwy-elmer-model-access-onboarding`

## One-line

The keyless model-access feature was brainstormed (office-hours, approved), put through the full
adversarial pipeline (5-round design adrev incl. Codex + 3-round plan review), and the **frontend
foundation + the ModelTilePicker component are built, TDD-green (136 vitest), typecheck-clean, and
pushed.** Remaining: wire the picker into ElmerPane (8b), the guided get-a-key flow (9),
rate-limit/framing/layout (10), regression port (11), the three Rust backend additions (3/4/5), and
the browser-smoke + wire-walk gate (12) — all fully specified in the plan.

## Canonical artifacts

- **Plan (the continuation spec, execution-ready, revision 2):**
  `docs/plans/2026-06-30-elmer-model-access-onboarding-plan.md` — subagent-proof, all 8 adversarial-
  review rounds folded in. **Read this first; execute 8b → 9 → 10 → 11 → 3 → 4 → 5 → 12 in order.**
- **Design doc (approved):** `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260630-123333-elmer-model-access.md`
- **Consolidated adversarial findings (file:line grounded):** `dev/scratch/elmer-adrev-consolidated.md`
- **Codex design adrev transcript:** `dev/adversarial/2026-06-30-elmer-model-access-design-codex.md` (gitignored)
- **High-fidelity mock:** `dev/scratch/elmer-mock-board-v2.png` (tactical dark; tiers Free/Pay-as-you-go/Local/Other)

## The decision (locked, by operator)

Cloud-first onboarding (local gpt-oss too weak for the Pi audience). Providers organized by **price
tier**: Free (Gemini *recommended* / Groq) · Pay-as-you-go (Anthropic / OpenAI) · Local (Ollama) ·
Other (OpenRouter / custom). The **tiered tile picker is Elmer's canonical model surface** (first-run
AND settings), full-guided. Honest per-tier framing. Defaults: Gemini `gemini-2.5-flash`, Groq
`llama-3.3-70b-versatile`, Anthropic `claude-haiku-4-5` (Sonnet step-up), OpenAI `gpt-4o-mini`.

## Done + green + pushed (8 commits)

All TDD, `pnpm vitest run src/elmer/` = **136 passed**, `pnpm typecheck` clean:
1. `refactor(elmer): export ModelForm…` — T0 (picker reuses it verbatim).
2. `feat(elmer): Anthropic preset, per-preset tier/keyPage/default-model, TS DTO contract` — T1.
   `ProviderPreset` gains tier/defaultModel/keyPageUrl; `anthropic` preset (after openai);
   `DEFAULT_MODEL_BY_PRESET`; **the TS contract for the not-yet-merged Rust fields** —
   `ConfigReadDto.onboarded`, `KeyStatusByOrigin`, `rateLimited` ElmerPhase + `outcomeKindToPhase`
   case. (This is the sequencing decision that lets frontend tasks be locally vitest-green via the
   invoke mock, with no Rust compiled — Rust's serde wire-shape test is the cross-language guarantee.)
3. `feat(elmer): allowlist provider key-page URLs…` — T2. The 4 key-page URLs in
   `src-tauri/capabilities/default.json` `shell:allow-open` (else the get-a-key buttons fail at runtime;
   vitest mocks plugin-shell so only the browser-smoke catches a miss).
4. `fix(elmer): Detect/Test sends the pasted key after an inter-provider switch` — T6. **A real shipped
   bug** Codex caught: `buildKeySource` keyed off raw mount-only `keyStatus`, dropping a pasted key on
   provider switch (the Detect-path analog of #981's buildSetKey fix). Now origin-aware. Regression test added.
5. `feat(elmer): tile/provider switch pre-fills the default model, preserving hand-edits` — T7.
   Shared pure helper `nextModelForPreset` (unit-tested); `handlePresetChange` uses it (deps [endpoint, model]).
6. `feat(elmer): ModelTilePicker component…` — T8a. The picker: tiered tiles (Gemini RECOMMENDED;
   per-tile "key saved" badge from `keyStatusByOrigin` prop, never a value), `role=radio` keyboard-
   reachable, pre-selects `inferPreset(saved endpoint)` showing the SAVED model, tile-switch via
   `nextModelForPreset`, **Other tier reuses exported ModelForm verbatim** (no second model path), no
   renderer fetch (SSRF-1), single-column CSS (LAYOUT-1: renders inside `.elmer-messages`). 7 tests.
   (Plus the plan commit.)

## Remaining (all specified in the plan; do in order)

- **T8b — wire the picker into ElmerPane (the reachability keystone).** When `modelConfig.onboarded ===
  false`, render `<ModelTilePicker>` IN PLACE OF the message list and gate the chat input until onboarded
  (fix the `hasNoModelConfigured && items.length===0` gate at `ElmerPane.tsx:937/1022` — it currently keys
  off empty `agentModel`, which is wrong when a weak local model is seeded). Make the gear/disclosure
  reopen the picker even after Elmer has mounted (the `advancedOpen`/`expandModel` seed at `:906` is
  initial-state only — use an effect/open-counter). useElmer must expose `onboarded` + the
  `keyStatusForOrigins` wrapper (T4-fe; pure TS, invoke-mocked — independent of Rust merge).
- **T9 — guided get-a-key flow + masked key field** (open key page via the hardcoded `preset.keyPageUrl`
  constant through `@tauri-apps/plugin-shell` `open()`; outcome-based steps; trim+sanity-validate paste
  (len≥20, `/^[A-Za-z0-9_\-]+$/`); type="password"+reveal; "stuck?" → alternate provider). The cloud-tile
  editor in 8a is the lightweight placeholder this replaces.
- **T10 — rate-limit recovery UI + honest framing + single-column layout polish** (consumes the
  `rateLimited` phase from T1; Switch-provider action reopens picker at paygo tier; Free copy names
  training-on-data; persistent provider-class footer indicator). Reuse the existing detectState/saveState
  machines for Test/Save (don't reimplement).
- **T11 — port the #981 credential-seam regressions** (`ElmerPane.test.tsx:1210-1436`, all THREE describes)
  to the picker flow; add Anthropic-origin regression. Confirm test count does not drop.
- **T3/T4/T5 — Rust backend** (CI-verified; Pi can't cold-compile): `onboarded` sentinel on ElmerConfig +
  ConfigReadDto (migration: derive onboarded=true for already-customized configs so existing users don't
  see the picker); `elmer_key_status_for_origins` command (keyring isolation in tests!); typed `rateLimited`
  429 (Step-0 pre-trace the outcome-kind enum + EV path). **Each needs a serde wire-shape test** asserting
  the exact on-wire literal the TS matches (raw-box trap): `onboarded` (camelCase), KeyStatus variants
  (lowercase), `rateLimited` (camelCase — matches the existing needsOperator/toolDenied convention; I
  changed the plan's tentative `rate_limited` to `rateLimited` to match — see useElmer.ts outcomeKindToPhase).
- **T12 — browser-smoke (WebKitGTK render harness: allowlist opens, single-column no h-scroll at 392px,
  window controls) + wire-walk gate** (operator supplies flows GREENFIELD — do NOT pre-draft them).

## Then ship

- **Codex diff-adrev** on the full branch diff (`git diff origin/main..HEAD`) per build-robust-features /
  the cross-provider-adrev discipline, before marking ready.
- Open the PR only when the feature is complete AND wire-walk passes (operator dislikes draft parking;
  "features shipped end-to-end"). CI compiles the Rust (both arches) — verify green by headSha.
- **Operator Assignment (r2-poe, before/at ship):** `pnpm dev:converged`, test Gemini (free) + Claude
  Haiku (paid) end-to-end through the real OpenAiProvider; confirm bearer auth resolves on Anthropic
  `/v1/models` for Detect (non-fatal if not — default-model pre-fill makes Detect optional).

## Working-tree / worktree state

- Worktree clean except gitignored dev/scratch + dev/adversarial artifacts (design copy, adrev
  consolidated + Codex transcript, mock PNGs, the http.server still serving the mock on :8731 — kill it).
- No stashes. Branch pushed, tracking origin. bd `tuxlink-wpqwy` in_progress, claims this worktree.
- node_modules installed; githooks active.

## Side tasks (from operator's message — done as instructed)

- `tuxlink-qscz4` CLOSED (verified shipped in #973 on origin/main by SHA).
- `tuxlink-2ouqf` (P1 arm/taint) + `tuxlink-n0irv` (P2 d3zwe vetted-client migration) — left in backlog, untouched.
