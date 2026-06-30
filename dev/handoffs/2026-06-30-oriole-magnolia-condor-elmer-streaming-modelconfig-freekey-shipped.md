# Handoff — Elmer: streaming + model-config fixes + free-key presets shipped

**Agent:** oriole-magnolia-condor · **Date:** 2026-06-30 · **Branch state:** all merged to `main` (HEAD `aab35c20`), every merge verified green by SHA.

## Shipped to main tonight (all CI-green on the merge commit, by SHA)
- **#974** conversational chat (stream assistant turns + tool chips) + operator-location (system prompt + `position_status` desc)
- **#975** egress prompt: agent learns the stage→Arm-to-send model, no faked sends/fabrication
- **#976** radio-verb thinking indicator + markdown-rendered assistant turns (sanitized)
- **#977** pane-close: closing mid-run keeps the pane mounted (no orphaned inference) — `tuxlink-9uat6`
- **#978** double-output: `useElmer` listener async-race fixed (cancelled-guard cleanup) — `tuxlink-hn5k6`
- **#979** live token/reasoning streaming (e2vw7): SSE provider, RunEvent deltas, session bridge, streaming render w/ collapsible reasoning. Codex-reviewed; SSE hardened (byte-safe UTF-8, bounded accumulation, cancel-safe).
- **#980** Cargo.lock futures-util edge **+ serde camelCase rename** of Outcome/Delta `_kind` fields — the latter was the real **"raw box after every run"** bug (enum `rename_all` doesn't reach variant fields → `outcomeKind` was undefined → `'error'` phase). Earlier mis-attributed to HMR.
- **#981** model-config: inter-provider **key-drop → 401** (`buildSetKey` used stale `keyStatus` not `effectiveKeyStatus`); **silent `config_set` failures** now surfaced (`.elmer-save-error`/`.elmer-save-ok`); **"Custom…"** provider now selectable (clears+focuses endpoint) — `tuxlink-xutqy`
- **#982** **free-key cloud presets: Google Gemini + Groq** (OpenAI-compat endpoints, free AI Studio/Groq keys, no billing card)

## Incident (resolved) — read before trusting any green
Early on, the CI-watch logic (`gh run watch` / `gh run list --limit 1`) latched **stale runs** and reported **false-green**; #979 was merged while its CI had actually failed (stale Cargo.lock → `--locked`), which masked the serde test failures. Root-caused + fixed (#980). Discipline now: **verify CI by matching the commit SHA + explicit conclusion before merging** (memory `feedback_verify_ci_by_commit_sha`; also `project_rust_dep_requires_cargo_lock_update`, `reference_serde_rename_all_enum_fields`). The "scoped vitest misses contract tests" trap also bit once (#982's `elmerModelConfig.test.ts` count) — run the whole `src/elmer/` dir, not one file.

## Open / pending decision
1. **FIRST MORNING GATE: model-access strategy brainstorm** (deferred at operator's request, ~5 AM). Topic: keyless onboarding for the non-developer audience — which free-tier providers to feature, default models, a "get a free key" flow, honest framing of the bring-your-own-key reality. **Grounding constraint:** OAuth/subscription auth (ChatGPT Plus / Claude Pro) is **NOT a sanctioned third-party path** — consumer subs are locked to first-party clients (Codex/Claude Code); reverse-engineering is ToS-risky/fragile. Free-tier API keys (Gemini/Groq) are the realistic answer. Use `office-hours` + the visual companion.
2. **e2vw7 streaming**: CI- and Codex-verified but **NOT on-air/live-validated** — operator's live drive on r2-poe is the real test. Wire-walk was skipped per operator.
3. **Anthropic (Claude) preset**: offered, not added — pending the brainstorm's provider decisions. Endpoint verified: `https://api.anthropic.com/v1/chat/completions` (bearer + Anthropic key).

## Runtime context
- Operator runs on **r2-poe (N305, tailnet)** via `pnpm tauri dev` from `.local/converge-build-worktree` (NOT the converged prod build — dev mode = StrictMode/HMR, which amplified the #978 listener race). For a representative test, prefer `pnpm dev:converged`.
- gpt-oss:20b (local) is **too weak** for useful agentic output (operator confirmed). Cloud models (Gemini Flash / Haiku / Sonnet) are the real test.

## To test Gemini in the morning (free, ~2 min)
1. `pnpm dev:converged` on r2-poe.
2. Elmer → Endpoint/model → provider **"Google Gemini (free key)"**.
3. Free key at **aistudio.google.com** → "Get API key" → paste.
4. Model field: `gemini-2.5-flash` → **Save** (you'll get a "Saved — Elmer will use…" confirmation, or the real error now).

## Working tree
Worktree `bd-tuxlink-1wi5w-elmer-model-config` is on `docs/handoff-2026-06-30-elmer` (this doc). All feature branches merged + auto-deleted. No stashes. bd issues 9uat6/hn5k6/xutqy/e2vw7 closed (e2vw7 shipped; live-validate separately).
