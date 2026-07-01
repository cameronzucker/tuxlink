# Handoff — Elmer model-access onboarding executed end-to-end; PR #984 open, CI verifying

**Agent:** bayou-cedar-delta · **Date:** 2026-06-30 · **bd:** `tuxlink-wpqwy` (in_progress)
**Branch:** `bd-tuxlink-wpqwy/elmer-model-access-onboarding` · **PR:** #984 (ready) · **Verified code head:** `57423ee5` (+ this docs commit)

> **CI status:** `verify` (typecheck + full vitest + clippy `--workspace --all-targets` + cargo test) is **GREEN on BOTH arches** at code head `57423ee5` — the Rust (T3/4/5) is CI-confirmed to compile and pass. Two clippy rounds were fixed to get there: `items_after_test_module` (runner.rs test module relocated to EOF) and `field_reassign_with_default` (config.rs onboarded tests → struct-update syntax). This docs commit is code-identical to `57423ee5`; re-run `verify` will be green.
**Worktree:** `worktrees/bd-tuxlink-wpqwy-elmer-model-access-onboarding`

## One-line
The full plan (T8b→T9→T10→T11→Rust T3/T4/T5→T12) was executed via subagent-driven-development, then the **settings-surface fold-in** (spec Approach A — the picker is the model surface for first-run AND settings) was added after the operator flagged it was locked in the spec. Frontend **193 vitest green + 38 shell-mount tests + typecheck clean**; Rust written (CI-verified); WebKitGTK smoke passed; final review + Codex adrev + a dedicated fold-in review all clean. **PR #984 is open and CI is verifying on `e5f05fd3`.**

## What shipped (26 commits on the branch)
- **Frontend:** ModelTilePicker as the canonical model surface for **first-run, 429-recovery, AND settings** (one main-slot mount; gear reopens the same picker; Back-to-chat scoped to onboarded, absent first-run). Guided GetKeyCard (hardcoded-keyPageUrl open, masked entry, sanity validation, per-tile remount, **keep/replace path** so a settings edit doesn't force key re-entry). Typed 429 recovery callout + Switch-provider→paygo returning to chat. Honest per-tier framing + provider footer. #981 credential-seam regressions ported to the tile flow + re-pointed through the Other tier (all assertions intact; seam logic untouched).
- **Rust (CI-verified — Pi can't cold-compile):** `onboarded` sentinel with migration (`onboarded || !is_default()`, `is_default` counts the flag so default-content saves persist); `elmer_key_status_for_origins` (statuses only, MCP-denied); typed 429 → `rateLimited` outcome (**camelCase** — the plan's `rate_limited` was stale vs the shipped FE) across `ProviderError`/`RunOutcome`/`DetectError`, both turn and detect paths. Each new field/variant has a serde wire-shape test.

## Bugs caught & fixed by the review/adrev gates (would NOT have shown as "tests pass")
- **Critical** (per-task review): rate-limit Switch-provider dead-ended — the picker never returned to chat after a switch (stranded operator). Fixed + Back-to-chat.
- **CI-breaker** (T5 review): non-exhaustive `RunOutcome` match in `d3zwe/print.rs` — would fail the d3zwe compile. Added the arm.
- **Stale cross-language literal:** the 429 outcome-kind was `rate_limited` in the plan but the shipped FE matches `rateLimited`; emitting snake_case would make the 429 silently never surface.
- **Codex P2 ×3 (fixed):** detect 429 was only typed in `map_models_response`, not the production `detect_inner` path; the shell allowlist was domain-wide `/**` not path-tight; key-status badges never fetched in the primary (first-run/429) flows.
- **Production-mount crash (CI-caught):** `AppShell.elmer.test.tsx` (outside `src/elmer/`, missed by scoped runs) crashed because `keyStatusForOrigins` could resolve `undefined` into the picker. Fixed fail-closed (map always `{}`; picker defaults the prop).

## Working-tree / worktree state
- Tree clean except gitignored scratch: `dev/scratch/elmer-picker-{392,700}.png` (smoke evidence), `.superpowers/sdd/*` (SDD ledger + briefs + reports + review packages), `dev/adversarial/2026-06-30-elmer-model-access-codex.md` (Codex transcript, ~25k lines). `node_modules` present. No stashes. Branch pushed, tracking origin.
- Render harness gained an `elmer` view (`dev/render-harness/harness.tsx`, committed) for future picker smokes.

## Pending — needs the operator
1. **CI `verify` is GREEN** (both arches, code head `57423ee5`) — already confirmed. Just re-confirm green on whatever head you merge (this docs commit re-runs it, code-identical). The `build-linux`/ECT/Release packaging jobs run longer but are not the merge gate.
2. **Converged R2 build validation** (operator chose this in lieu of the agent wire-walk): trace the key flows; confirm the real `open()` allowlist launches a browser and the OS window min/max/close controls render (both untestable in the harness).
3. **Design divergence to decide:** GetKeyCard and the local tile have **no Test/detect button** — only the Other-tier ModelForm does. The plan deliberately made Detect **optional** (default-model pre-fill), which diverges from the design doc's "test-key per provider + keyless Local Test" completeness note. Decide whether to add a test-key affordance (follow-up) or accept the plan's optional-Detect call.
4. **Merge** is the operator's call (repo has no auto-merge/required checks; use `gh pr merge` after CI green + converged validation; no `--auto`, no `--delete-branch`).

## Not blocking
- release-please/version are operator-owned; agents don't cut releases.
