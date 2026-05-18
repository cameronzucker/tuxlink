# Task 9 — Wizard Screen 1 (Welcome / Connection-Type) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the boilerplate Tauri+React scaffold landing page with a first-run wizard whose Screen 1 asks the operator **"Will this installation connect to the Winlink CMS?"** and routes them either to the CMS-credentials path (handled by Task 10) or to the offline-deployment path (handled by Task 11.5) — establishing the shared wizard scaffold (`WizardLayout`, `wizardState`, `App.tsx` routing) that Tasks 10 / 11 / 11.5 will extend.

**Architecture:** Wizard is React + TypeScript on the frontend, with two Tauri commands on the Rust side: `config_exists()` (already partly implied by existing `config_path()`) and `wizard_complete_offline()` (Task 11.5's deliverable — out of scope here; this plan defines only the contract Step1 expects). The wizard is gated on the absence of `~/.config/tuxlink/config.json` (or `$XDG_CONFIG_HOME/tuxlink/config.json`). State lives in a single `WizardState` object held by `App.tsx`; each step is a pure presentational component that receives `state` + callbacks via props. Step1 collects ONE piece of state (`connectToCms: boolean | null`) and renders two large choice cards. **No config write happens in Task 9** — Step1 only updates in-memory `wizardState`; persistence is the responsibility of Task 10 (CMS path) or Task 11.5 (offline path).

**Tech Stack:** Tauri 2, React 18, TypeScript 5, Vitest + @testing-library/react + jsdom (added in Task 9 as part of the wizard scaffold; reused by Tasks 10/11/11.5). Rust side uses existing `serde` + `thiserror` already in `src-tauri/Cargo.toml`.

## Terminology

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in BCP 14 [RFC 2119] [RFC 8174] when, and only when, they appear in all capitals, as shown here.

## Phase-dependency graph

```
Phase 1 (scaffold) ─────┬───→ Phase 2 (Step1Welcome — imports WizardLayout)
                        │
                        └───→ Phase 4 (App.tsx — imports wizardState + Step1Welcome)
Phase 3 (Rust command) ─────→ Phase 4 (App.tsx — invokes config_exists)
```

Phases 1 and 3 are independent and MAY execute in any order or in parallel within the same session (different file trees: `src/` vs `src-tauri/`). Phase 2 requires Phase 1. Phase 4 requires Phases 1 + 2 + 3. Sequential execution (1 → 2 → 3 → 4) is the recommended default; parallel-1+3 then sequential 2 → 4 is acceptable for a time-pressed implementer.

---

## Living Document Contract

This plan is a living document. Every executing agent MUST update it as
execution progresses, not only at completion.

- **On phase claim:** the executor MUST flip the banner to 🚧 IN PROGRESS
  with a claim timestamp (ISO 8601 UTC) and the active branch name. The
  banner MUST NOT include an expected-completion estimate — agents cannot
  reliably estimate their own wall-clock, and a fabricated duration
  becomes a stale anchor that misleads future readers. Followers
  encountering a 🚧 banner determine liveness by observable signals (PR
  existence, recent branch commits), not by arithmetic on expected times.
  See Step 5's stale-claim reclaim protocol.
- **On phase ship:** the executor MUST update that phase's **Execution
  Status** banner with the shipped commit SHA(s) and date. If a PR is
  open, the PR number and URL MUST appear in the top-of-plan Execution
  Status table.
- **On phase defer:** the executor MUST update the banner with ⏸ status
  AND a prose description of the unblock condition + a link to the
  likely-unblocker artifact (plan page, task, or PR whose own Execution
  Status banner will signal completion). Prose + link is durable across
  paraphrases and scope edits; exact-string coordination between agents
  is not.
- **On PR merge:** the executor MUST record the merge SHA in the banner
  + the top-of-plan Execution Status table.
- **On deviation from the written plan** (scope edits, structural
  refactors, dropped tasks, reordered phases): the executor MUST
  inline-document the deviation in the affected task AND summarize it
  in the top-of-plan Execution Status as a "Deviations" subsection.
  Deviation state MUST NOT live only in PR notes or status reports.
- **On discovery** (pre-existing drift surfaced during execution, new
  bugs found, architectural issues noted): the executor MUST add a
  "Discoveries" subsection at the top of the plan with pointers to the
  files/lines affected. Follow-up dispatches read this subsection to
  avoid duplicate discovery work.

The plan SHOULD reflect reality at the end of every session that touches
it. Anything worth putting in a status report to the user is worth
putting in the plan.

Rationale: `/writing-plans-enhanced` Step 5. Writing at ship time is
cheap; reconstruction by downstream readers is expensive, compounds
across dispatches, and fails silently when state is split across PR
notes and commit messages.

---

## Execution Status

**Overall:** Not started. 0/4 phases shipped.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — Wizard scaffold (state machine + layout, test-first) | ⬜ Not started | — | foundation reused by Tasks 10/11/11.5 |
| 2 — Step1Welcome component + tests | ⬜ Not started | — | the per-task UI |
| 3 — Rust `config_exists` Tauri command + test | ⬜ Not started | — | small backend change |
| 4 — `App.tsx` integration + manual browser smoke | ⬜ Not started | — | replaces the scaffold landing page |

### Deviations

_(none yet — populate at execution time per Living Document Contract)_

### Discoveries

_(none yet — populate at execution time per Living Document Contract)_

---

## Spec — what AMD-2 actually changes vs. the original plan

The implementing agent MUST read this section BEFORE starting Phase 1. The original Task 9 plan text in [`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`](2026-04-22-tuxlink-v0.0.1-plan.md) lines 2155-2415 was written pre-amendment and CARRIES STALE CODE SNIPPETS that this plan overrides. Specifically:

| Original plan (stale) | AMD-2 amended (canonical) |
|---|---|
| File: `src/wizard/Step1Account.tsx` | File: **`src/wizard/Step1Welcome.tsx`** |
| Screen question: "Do you have a Winlink account?" | **"Will this installation connect to the Winlink CMS?"** |
| `wizardState.hasAccount: boolean \| null` | **`wizardState.connectToCms: boolean \| null`** |
| `WizardStep` union: `'account' \| 'credentials' \| 'test_send' \| 'complete'` | **`'welcome' \| 'credentials' \| 'test_send' \| 'offline_identity' \| 'complete'`** (the offline branch is new per AMD-5) |
| Step1 has an in-page "Register at winlink.org" button that opens system browser via `shell.open` | **No Register button in Step1.** The Register link moves to Task 10's header (AMD-3). |
| Step indicator: hardcoded "Step 1 of 3" | **Step indicator parameter** (CMS path stays "Step 1 of 3"; offline path will collapse to "Step 1 of 2" per AMD-5 — Task 11.5's responsibility, but `WizardLayout` MUST accept `totalSteps` as a prop now so Task 11.5 doesn't have to refactor it) |
| `advanceWizard` transitions `account → credentials` | **`advanceWizard` transitions `welcome → credentials` when `connectToCms === true`, `welcome → offline_identity` when `connectToCms === false`** |
| Original `WizardState` included `mboAddress: string` field | **`mboAddress` field dropped from `WizardState`.** Rationale: under AMD-1 schema, `pat_mbo_address` is COMPUTED at config-write time from `callsign + "@winlink.org"`, not stored in wizard state. It's a downstream-of-callsign derived value. Tasks 10/11.5 compute it when they write the config; the wizard state doesn't carry it. |

The canonical source for these decisions is [`docs/design/v0.0.1-ux-mockups.md`](../design/v0.0.1-ux-mockups.md) §5.1. The plan's AMD-2 callout at line 2157 explicitly states the implementation snippets in the original plan's Steps 1-7 "have NOT been updated and the implementing agent must consult §5.1 of the design doc + the new wizardState shape." This plan does that consultation for the implementer.

### Design-doc-vs-mockup divergences (resolved in this plan; flagged for review)

The plan-writer (`beaver-dune-poplar`, 2026-05-18) inspected `docs/design/mockups/images/wizard-a-welcome.png` and found it diverges from the design doc §5.1 prose on several details. The mockup PNG landed PR #29 (2026-05-17) and was visually accepted by Cameron; the §5.1 prose landed later. Where they diverge, **this plan defers to the mockup as the more concrete source** and notes the divergences below so Cameron can issue an explicit ruling if needed.

| Aspect | Design doc §5.1 prose | Mockup PNG | This plan implements |
|---|---|---|---|
| CMS card title | "Yes, connect to the Winlink CMS" | "Yes — connect to Winlink CMS" | **mockup** (em-dash form) |
| Offline card title | "No, this is an offline / radio-only deployment" | "No — offline / lab / training" | **mockup** (em-dash form) |
| CMS card body | "Standard setup. You'll enter..." (implied) | "For sending and receiving Winlink mail to/from real Winlink stations. Requires a Winlink account." | **mockup** |
| Offline card body | "For Winlink Hybrid Network, ARES drills, EOC tabletops, or lab work." | "No internet required. For dummy-load testing, training exercises, field deployments without connectivity, or developing without a Winlink account." + an Examples line in mono font | **mockup** |
| Default selection | "(default selection)" — ambiguous | CMS card pre-selected with orange border + checkmark icon | **mockup** — Phase 1's `initialWizardState()` sets `connectToCms: true` (NOT `null`) |
| Continue button visibility | (not specified; my initial Interpretation A gated it) | always visible at bottom-right | **mockup** — Continue is always rendered; clicking advances per current state |
| Header style | implied "Step 1 of 3" text indicator | branded "T" icon, "Tuxlink Setup — first run" subtitle, horizontal step indicator with labels "WELCOME / IDENTITY / VERIFY", "Step 3 is optional" annotation | **mockup-compatible** — `WizardLayout` accepts a `steps` array (label + state) instead of just `step` + `totalSteps` numbers (small expansion; see Phase 1 Step 10's amended code) |
| Footer text | (not specified) | "You can switch between modes anytime in Settings → Connection." | **mockup** — rendered as a small muted text left of the Continue button |
| Card 2 body Examples line | (not in §5.1) | `bench test on BAOFENG-FM-01 · ARES training drill · EOC tabletop · radio-only emcomm deployment when internet is down (Winlink Hybrid Network) · radio loopback validation` | **mockup** |

**Open decision for Cameron (NOT a Wave-2 blocker):** confirm the mockup-aligned copy is canonical and the design doc §5.1 prose should be amended to match. If Cameron prefers the design-doc copy over the mockup copy, Wave-2 patches Phase 2's `Step1Welcome.tsx` text to match — small one-commit amendment. The plan-writer chose mockup-alignment because (a) the mockup is more concrete + already-visually-accepted, (b) the §5.1 prose explicitly says "see `wizard-a-welcome.png`" for the visual reference, and (c) the offline-card examples line (mono-font) is operator-meaningful detail that the prose lost.

**Pre-selection semantics:** with `initialWizardState().connectToCms = true`, an operator who immediately clicks Continue without reading either card commits to the CMS path. The "click-through without reading" failure mode IS real but is mitigated by (a) clear visual presence of both cards, (b) the chosen card is visibly highlighted, (c) the offline-mode operator pool is small and self-aware. The mockup's design accepts this tradeoff explicitly. Phase 1's test #5 ("refuses to advance without a connectToCms decision") becomes redundant under the new default — DROP that test in Phase 1 Step 4 (we keep the test of throw-on-null at the `advanceWizard` level for defense-in-depth, since callers MAY still pass null via direct manipulation; the test changes to a `manually-null` test rather than a "initial state" test).

### What Step1 does NOT do

To prevent subagent over-engineering (a real failure mode — see "Anti-patterns" in `superpowers:writing-plans`):

- Step1 does **NOT** write the config file. (Task 10 writes for the CMS path; Task 11.5 writes for the offline path.)
- Step1 does **NOT** call any Tauri command. (No keychain writes, no `wizard_complete_*` invocations.)
- Step1 does **NOT** render the credentials form. (Task 10.)
- Step1 does **NOT** render the offline identity form. (Task 11.5.)
- Step1 does **NOT** include the Register-at-winlink.org link. (Per AMD-2; that link lives in Task 10's header per AMD-3.)
- Step1 does **NOT** include a Skip / Back button. (No prior step exists; the wizard has no in-flow exit on Step1 by design — operator closes the window to exit, which the OS handles.)

### What the wizard scaffold (Phase 1) MUST do

The scaffold is shared infrastructure that Tasks 10 / 11 / 11.5 reuse. Implementer MUST design Phase 1's deliverables to absorb these future requirements WITHOUT requiring refactor:

- `WizardLayout` accepts `totalSteps: number` (not hardcoded 3). Task 11.5 passes 2.
- `WizardState` includes ALL future fields stubbed as empty strings / `null` (`callsign: ''`, `password: ''`, `gridSquare: ''`, `identifier: ''`, `testSendStatus: 'idle'`). Subsequent tasks fill them in; this plan only ADDS them so the type contract is stable across the wizard cluster.
- `WizardStep` union enumerates all five step variants (`'welcome' | 'credentials' | 'test_send' | 'offline_identity' | 'complete'`). Subsequent tasks render the new variants; this plan only ADDS them.
- `advanceWizard` handles the `welcome` transitions (both branches based on `connectToCms`). Subsequent tasks add their own transitions. The function throws on any unhandled / invalid transition — this is the safety net that catches "you forgot to handle Step N" bugs in later tasks.

This is the **wizard pattern** Tasks 10 / 11 / 11.5 reuse. Document it explicitly in JSDoc comments on `wizardState.ts` (Phase 1, Step 3) so the pattern is discoverable from the code, not just from this plan.

---

## Cross-task ordering & dependencies

### Hard prerequisites (must be shipped before this plan is pickable)

- **Task 1 (Tauri+React scaffold)** — ✅ shipped (PR #2/3, see plan §Task 1 status).
- **AMD-1 / Task 2 (config schema amendment)** — current code in `src-tauri/src/config.rs` is the PRE-AMD-1 flat shape (`callsign: String`, `grid_square: String`, etc.). AMD-1 introduces `ConnectConfig`, `IdentityConfig`, `PrivacyConfig`. **However**, Task 9 itself does NOT depend on AMD-1 being shipped, because Task 9 does not WRITE the config — it only checks for its existence (a path-existence test that works with either schema shape). The `config_exists()` Tauri command added in Phase 3 only calls `config_path().exists()`. The dependency on AMD-1 is OWNED by Task 10 (which writes `connect.connect_to_cms = true`) and Task 11.5 (which writes `connect.connect_to_cms = false`).

  **Implementer action:** confirm at start of Phase 3 that `src-tauri/src/config.rs` still exports `config_path()` as a public function. If AMD-1 has shipped between plan-writing time and Phase-3 execution, `config_path()` is unchanged by AMD-1 (verify by reading the post-AMD-1 file). If AMD-1 has NOT shipped, the existing `config_path()` is fine to call.

### Soft coupling (this plan establishes the contract; downstream tasks consume)

- **Task 10 (Wizard Step 2 — credentials)** consumes: `WizardLayout`, `WizardState`, `advanceWizard`, the `'credentials'` step variant. Task 10 fills the `callsign` / `password` / `gridSquare` fields and transitions `credentials → test_send`.
- **Task 11 (Wizard Step 3 — test send)** consumes: `WizardLayout`, the `'test_send'` step variant, the `testSendStatus` field.
- **Task 11.5 (offline path)** consumes: `WizardLayout` (with `totalSteps={2}`), `WizardState`, `advanceWizard`, the `'offline_identity'` step variant. Task 11.5 fills the `identifier` field and transitions `offline_identity → complete`.

### What this plan touches that other concurrent plans MIGHT also touch

- `src/App.tsx` — Tasks 10, 11, 11.5 will each add a route. Task 9 establishes the routing pattern (a `switch (wizardState.step)` block); subsequent tasks add cases. If Task 9 and Task 10 land in parallel branches off `feat/v0.0.1`, the second-to-merge will need to rebase the `App.tsx` switch. **Mitigation:** Task 9 lands its `App.tsx` with a `default:` case that says "step not implemented" so the next merger only needs to add a `case`. No file-level conflict if everyone uses the switch pattern.
- `src/wizard/wizardState.ts` — Tasks 10, 11, 11.5 may extend the state shape or add transitions. Task 9 establishes ALL fields (per "Cross-task ordering" above), so subsequent tasks should NOT need to extend the shape — they only flip already-declared fields. If a downstream task does need a new field, it's a Task 9 plan amendment, not a free-edit on the file.
- `src-tauri/src/lib.rs` — Task 9 adds `pub mod wizard_commands;` (Phase 3) IF AMD-1's `wizard_commands.rs` module doesn't already exist. Task 11.5 will also add `pub mod wizard_commands;` if Task 9 didn't. **First-to-land wins.** Mitigation: Phase 3's commit message explicitly notes the module declaration so reviewers catch a duplicate.
- `src-tauri/src/main.rs` — Task 9 registers `config_exists` in `invoke_handler!`. Tasks 10 / 11 / 11.5 register their own commands. **Sequential merge resolves conflicts naturally** (each task adds one line to the handler list).

**Wave-2 implementer guidance:** if you discover a real file-level conflict at integration time (e.g., another Wave-2 implementer beat you to `App.tsx`), the resolution is "rebase your branch onto the merged base, re-apply your additions, push." This is NOT a force-push case — your branch can take new commits. If the rebase is non-trivial, escalate via a Discoveries entry rather than improvising.

---

## Safety-stack reminders (read once, internalize)

These cross-cutting rules from CLAUDE.md + pitfalls apply throughout this plan:

- **You are working in a worktree provided by the Wave-2 dispatch.** Per ADR 0008 + HOOK-1, the worktree is the correct place for all write operations on this task. If `block-main-checkout-race.sh` denies a write op, you are accidentally in the main checkout — go back to your worktree directory and retry. **Do NOT** consult `get_tuxlink_sessions.py` or attempt to take the main-checkout lease (the HOOK-1 anti-pattern).
- **Task 9 does NOT transmit anything.** No live-CMS calls, no Pat HTTP invocations to a transmit-capable endpoint, no test-send code. The test-send substate is Task 11's deliverable; Task 9's `wizardState.testSendStatus` field is declared as 'idle' and not advanced. RADIO-1 (Section 0 of pitfalls) does not apply because there's no transmit path; if it ever feels like it does, STOP — Task 9 has gone off-spec.
- **Task 9 does NOT decide encryption.** Transport selection (CmsSsl vs Telnet) is Task 10's surface, governed by AMD-3. Task 9's `connectToCms` boolean does NOT influence transport. RADIO-2 does not apply.
- **Commit messages MUST be passed via heredoc** (already shown in each Step's commit block) because `block-destructive-git.sh` substring-matches against commit-message text via the heredoc body and the discipline hook needs the `Agent:` trailer in the bash command stream. Do NOT switch to `-F file` — that bypasses the discipline hook's Agent-trailer check.
- **No force-push, no `--amend` of pushed commits, no `git reset --hard`** — destructive-git hook denies all of these. If you genuinely need to revise history, open a NEW branch/PR per the "open new PR, close old with link" pattern documented in CLAUDE.md.

## Mandatory pre-execution reads

Before claiming Phase 1, the implementing agent MUST read these in order:

1. **`docs/design/v0.0.1-ux-mockups.md` §5.1** (canonical UX spec for Task 9 — the "two choice cards" decision and the rationale).
2. **`docs/ux-anti-patterns.md`** — at minimum the sections on external-navigation, full-view-swap, and "Anti-Patterns Observed in Winlink Express." Task 9 has NO external-navigation surface itself (the Register link moved to Task 10), but `Step1Welcome.tsx` must not introduce one inadvertently.
3. **`docs/pitfalls/implementation-pitfalls.md` SCOPE-1** (Task 9's framing reinforces tuxlink-is-client-not-gateway by treating "offline / radio-only deployment" as the operator's choice for their CLIENT, not as a "host a gateway" option).
4. **`docs/pitfalls/testing-pitfalls.md` §1 (Test Output Pristine), §3 (Error Path Coverage), §6 (Boundary & Configuration Validation), §7 (Test Infrastructure Hygiene)** — relevant to vitest hygiene (jsdom isolation, no shared state) and to the `advanceWizard` error-branch tests.
5. **`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Task 9 (lines 2155-2415)** — the spec source; note that the original code snippets (`Step1Account.tsx`, `hasAccount`) are STALE per the AMD-2 callout at line 2157. Use this PLAN's code, not the original plan's code.
6. **The image referenced by §5.1: `docs/design/mockups/images/wizard-a-welcome.png`** — visual reference for the two choice cards. **This plan's copy is mockup-aligned, NOT prose-aligned** (see plan §"Design-doc-vs-mockup divergences" for the resolved divergences). The implementer should match the mockup's layout (two large cards stacked vertically, CMS card pre-selected with orange border + "✓" checkmark, mockup-canonical card titles and body copy, mockup-canonical Examples line in mono font on the offline card, Continue button always visible).

---

## Pre-flight verification

Before claiming Phase 1, run these commands from the worktree root. **All MUST succeed.** Any failure is an environmental issue — STOP and escalate rather than working around.

```bash
# 0. Confirm package manager is pnpm (not npm / yarn).
ls /home/administrator/Code/tuxlink/worktrees/<your-worktree>/pnpm-lock.yaml
# Expected: file exists. If you see "No such file" but `package-lock.json` or
# `yarn.lock` exists instead, STOP — the package manager assumed by this plan
# (pnpm) has been changed and the `pnpm add -D ...` commands will install into
# the wrong lockfile.

# 1. Verify Tauri scaffold builds in dev mode
cd /home/administrator/Code/tuxlink/worktrees/<your-worktree>
pnpm install
pnpm tauri dev &
PID=$!
sleep 15  # Tauri dev startup is slow on Pi 5; if your machine is faster, this can be shorter
kill -INT $PID
wait $PID 2>/dev/null
# Expected: window opened showing the "Welcome to Tauri + React" page; no compile errors

# 2. Verify TypeScript compiles
pnpm tsc --noEmit
# Expected: no output (zero errors)

# 3. Verify Rust compiles
cd src-tauri && cargo check && cd ..
# Expected: "Finished `dev` profile"

# 4. Verify config_path() is present in src-tauri/src/config.rs
grep -n "pub fn config_path" src-tauri/src/config.rs
# Expected: one line match. If zero matches, STOP — config.rs structure changed.

# 5. Verify lib.rs declares the config module (Phase 3 assumes this).
grep -n "pub mod config" src-tauri/src/lib.rs
# Expected: one line match like "pub mod config;". If zero matches, STOP and
# investigate — Phase 3's `config::config_path()` call requires the module to
# be public from lib.rs.

# 6. Determine your session moniker (used in all commit-message Agent: trailers).
python3 .claude/scripts/get_agent_moniker.py
# Records to STDOUT. Persist it for the session — substitute into every commit
# message's `Agent: <SESSION-MONIKER>` line. The Wave-2 implementer's moniker,
# not the plan-writer's, is what lands in implementation commits.

# 7. Verify Vitest matchers expected by this plan are available.
# This plan uses toHaveBeenCalledExactlyOnceWith (Vitest 1.4+).
# If your Vitest is older, replace each .toHaveBeenCalledExactlyOnceWith(args)
# with .toHaveBeenCalledOnce() followed by .toHaveBeenCalledWith(args) on the
# previous line. (The semantics are equivalent for our purposes.)
# We'll know which Vitest version after Phase 1 Step 1 installs it.
```

If pre-flight passes, claim Phase 1 per the Living Document Contract.

---

## Phase 1 — Wizard scaffold (state machine + layout, test-first)

**Execution Status:** ⬜ NOT STARTED

**Goal:** Create the shared wizard infrastructure (`wizardState.ts` + `WizardLayout.tsx`) that all four wizard tasks (9, 10, 11, 11.5) will reuse. Test-first via Vitest. No UI integration yet — that's Phase 4.

**Files:**
- Create: `src/wizard/wizardState.ts`
- Create: `src/wizard/wizardState.test.ts`
- Create: `src/wizard/WizardLayout.tsx`
- Create: `src/wizard/WizardLayout.test.tsx`
- Modify: `package.json` (add devDependencies)
- Modify: `vite.config.ts` (add `test:` config block)
- Create: `src/setupTests.ts` (vitest setup for jest-dom matchers)

**WARNING on stale files from prior partial attempts:** the pre-AMD-2 plan referenced `src/wizard/Step1Account.tsx` (with the `hasAccount` field). If you find that file in the working tree from a prior abandoned implementation, DELETE it explicitly with `git rm src/wizard/Step1Account.tsx` before starting — it is NOT renamed to `Step1Welcome.tsx` (renames lose history in some downstream tooling). If the file is untracked (never committed), use `rm src/wizard/Step1Account.tsx`. Same for `src/wizard/wizardState.ts` if it contains a `hasAccount` field — overwrite per Step 6 below.

**BEFORE starting work:**

1. Invoke `superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` §1 (Test Output Pristine), §3 (Error Path Coverage), §7 (Test Infrastructure Hygiene).
3. Read `docs/pitfalls/implementation-pitfalls.md` SCOPE-1.
4. Follow TDD: write failing test → implement → verify green. No exceptions.

---

- [ ] **Step 1: Install Vitest + React Testing Library devDependencies**

Run from the worktree root. Install ALL devDeps needed across Phases 1, 2, 4 here in one shot (avoids a duplicate install in Phase 2):

```bash
pnpm add -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom @vitest/ui
```

Expected: `package.json` `devDependencies` gains six entries; `pnpm-lock.yaml` updated. No errors.

Verify:

```bash
pnpm list --depth=0 | grep -E "vitest|@testing-library|jsdom"
```

Expected: 5 lines printed (vitest, @testing-library/react, @testing-library/jest-dom, @testing-library/user-event, jsdom). `@vitest/ui` is optional dev-time convenience for `pnpm vitest --ui`; not asserted on.

Then confirm the Vitest version (needed for the `toHaveBeenCalledExactlyOnceWith` matcher used in Phase 2):

```bash
pnpm list vitest --depth=0
```

Expected: vitest version >=1.4.0. If older, see Pre-flight Step 7 — replace `toHaveBeenCalledExactlyOnceWith(args)` calls in Phase 2's test with the equivalent `toHaveBeenCalledOnce()` + `toHaveBeenCalledWith(args)` pair.

- [ ] **Step 2: Add Vitest config block to `vite.config.ts`**

Read the current `vite.config.ts` first to preserve the existing Tauri-specific config. Then add a `test:` block. The final file should look like:

```ts
/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/setupTests.ts"],
    css: false,
  },
}));
```

**Do NOT** change the existing Tauri-specific blocks (port 1420, HMR config, src-tauri ignore). If the current file diverges from the snippet above, preserve the existing Tauri config and ADD the `test:` block + the triple-slash reference at the top.

- [ ] **Step 3: Create `src/setupTests.ts`**

```ts
import "@testing-library/jest-dom";
```

One line; pulls in custom matchers like `toBeInTheDocument()`.

- [ ] **Step 4: Write failing test for `wizardState.ts`**

Create `src/wizard/wizardState.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import {
  initialWizardState,
  advanceWizard,
  type WizardState,
  type WizardStep,
} from "./wizardState";

describe("wizardState", () => {
  it("starts at the welcome step with CMS pre-selected per mockup", () => {
    // Mockup wizard-a-welcome.png shows the CMS card pre-selected at load
    // (orange border + checkmark). See plan §"Design-doc-vs-mockup divergences".
    const s = initialWizardState();
    expect(s.step).toBe<WizardStep>("welcome");
    expect(s.connectToCms).toBe(true);
  });

  it("initializes all future fields to safe empty values", () => {
    // Cross-task contract: Tasks 10/11/11.5 read these fields. They MUST exist
    // and MUST be empty-string / null / 'idle' so the type contract is stable
    // across the wizard cluster (see plan §"What the wizard scaffold MUST do").
    const s = initialWizardState();
    expect(s.callsign).toBe("");
    expect(s.password).toBe("");
    expect(s.gridSquare).toBe("");
    expect(s.identifier).toBe("");
    expect(s.testSendStatus).toBe("idle");
  });

  it("advances welcome → credentials when connectToCms is true", () => {
    const s = advanceWizard({ ...initialWizardState(), connectToCms: true });
    expect(s.step).toBe<WizardStep>("credentials");
    // Sanity: connectToCms is preserved through the transition.
    expect(s.connectToCms).toBe(true);
  });

  it("advances welcome → offline_identity when connectToCms is false", () => {
    const s = advanceWizard({ ...initialWizardState(), connectToCms: false });
    expect(s.step).toBe<WizardStep>("offline_identity");
    expect(s.connectToCms).toBe(false);
  });

  it("throws if advanceWizard is called from welcome with connectToCms === null (defense-in-depth)", () => {
    // The initial state has connectToCms = true per the mockup, but callers
    // MAY still construct a state with null via direct manipulation. The
    // safety net stays in place: throw rather than advance to an undefined
    // branch.
    const s: WizardState = { ...initialWizardState(), connectToCms: null };
    expect(() => advanceWizard(s)).toThrow(/connectToCms.*null|decision/i);
  });

  it("throws when called on a step with no Task-9 transition rule (forward-compat)", () => {
    // Downstream tasks (10/11/11.5) will add transitions for their own steps.
    // Task 9 only defines the welcome-step transitions. Calling advanceWizard
    // from a non-welcome step before downstream tasks land MUST throw — this
    // is the safety net that catches "forgot to handle Step N" bugs.
    const s: WizardState = { ...initialWizardState(), step: "credentials" };
    expect(() => advanceWizard(s)).toThrow(/credentials.*not implemented/i);
  });
});
```

- [ ] **Step 5: Run the test — confirm RED**

```bash
pnpm vitest run src/wizard/wizardState.test.ts
```

Expected: "Cannot find module './wizardState'" or "module not found" error. Test count: 0 passed, file errored. This is the red-stage failure we want.

If the test runs and passes, something is wrong — STOP, check whether `wizardState.ts` already exists, and investigate.

- [ ] **Step 6: Implement `src/wizard/wizardState.ts` to satisfy the tests**

Create `src/wizard/wizardState.ts`:

```ts
/**
 * Wizard state machine for tuxlink's first-run setup wizard.
 *
 * Established by Task 9 (the "welcome / connection-type" screen); extended
 * by Tasks 10 (credentials), 11 (test send), and 11.5 (offline-path identity).
 *
 * Pattern documented in docs/plans/2026-05-18-task-9-wizard-winlink-account-plan.md
 * §"What the wizard scaffold MUST do".
 *
 * Design intent:
 * - One state shape across all wizard screens. Per-step props derive from
 *   the shared state; no per-step state silos.
 * - `advanceWizard()` is the ONLY transition function. Tasks 10/11/11.5
 *   extend it by adding cases for their step variants; Task 9 establishes
 *   the welcome → credentials | offline_identity branch.
 * - Invalid transitions throw, never silently no-op. This is the safety net
 *   that catches "forgot to handle Step N" bugs across the wizard cluster.
 */

export type WizardStep =
  | "welcome"
  | "credentials"
  | "test_send"
  | "offline_identity"
  | "complete";

export interface WizardState {
  /** Current step the wizard is rendering. */
  step: WizardStep;

  /**
   * Set by Step 1 (Task 9). True = CMS path (Tasks 10 → 11); false = offline
   * path (Task 11.5); null = no decision yet (initial state).
   */
  connectToCms: boolean | null;

  /** Filled by Task 10 (Step 2 — credentials, CMS path). */
  callsign: string;

  /** Filled by Task 10. Never persisted to tuxlink config; written to Pat
   * config or OS keyring per AMD-1 §Pre-amendment shape. */
  password: string;

  /** Filled by Task 10 (CMS path) or Task 11.5 (offline path). */
  gridSquare: string;

  /** Filled by Task 11.5 (offline path) only; e.g., "EOC-1", "BAOFENG-FM-01". */
  identifier: string;

  /** Updated by Task 11 (Step 3 — test send) through its substate machine. */
  testSendStatus:
    | "idle"
    | "connecting"
    | "sending"
    | "waiting_reply"
    | "success"
    | { error: string };
}

export const initialWizardState = (): WizardState => ({
  step: "welcome",
  // CMS path is pre-selected per the mockup (wizard-a-welcome.png — orange
  // border + checkmark on the "Yes — connect to Winlink CMS" card). See
  // plan §"Design-doc-vs-mockup divergences" for the resolution.
  connectToCms: true,
  callsign: "",
  password: "",
  gridSquare: "",
  identifier: "",
  testSendStatus: "idle",
});

/**
 * Advance the wizard to its next step based on current state.
 *
 * Task-9 transitions:
 *   welcome (connectToCms === true)  → credentials
 *   welcome (connectToCms === false) → offline_identity
 *
 * Future-task transitions (Tasks 10/11/11.5 extend this function):
 *   credentials      → test_send       (Task 10)
 *   test_send        → complete        (Task 11)
 *   offline_identity → complete        (Task 11.5)
 *
 * Throws on any unhandled transition — this is intentional. A wizard step
 * lacking an `advanceWizard` case is a bug; throwing surfaces it loudly
 * during dev/test rather than silently no-op'ing.
 */
export function advanceWizard(s: WizardState): WizardState {
  if (s.step === "welcome") {
    if (s.connectToCms === null) {
      // Defense-in-depth: initialWizardState() pre-selects true per the
      // mockup, but callers MAY construct a state with null via direct
      // manipulation. We refuse to advance to an undefined branch.
      throw new Error(
        "Cannot advance from welcome step with connectToCms === null",
      );
    }
    return {
      ...s,
      step: s.connectToCms ? "credentials" : "offline_identity",
    };
  }
  // Tasks 10/11/11.5 will add their cases here. Until they do, advancing
  // from any non-welcome step throws — this is the safety net that catches
  // "forgot to handle Step N" bugs in downstream tasks.
  throw new Error(
    `advanceWizard: transition from step '${s.step}' is not implemented yet ` +
      "(Tasks 10/11/11.5 add the remaining cases)",
  );
}
```

- [ ] **Step 7: Run the test — confirm GREEN**

```bash
pnpm vitest run src/wizard/wizardState.test.ts
```

Expected: 6 tests passed, 0 failed, 0 skipped. Output is pristine: no warnings, no console noise, no stray stderr.

If output is not pristine: per testing-pitfalls §1, either suppress the noise at its source or assert on it. Do NOT mark the phase complete with a noisy suite.

- [ ] **Step 8: Write failing test for `WizardLayout.tsx`**

Create `src/wizard/WizardLayout.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { WizardLayout } from "./WizardLayout";

describe("WizardLayout", () => {
  it("renders the app title, the per-screen title, the step indicator, and children", () => {
    render(
      <WizardLayout title="Welcome to Tuxlink" step={1} totalSteps={3}>
        <p data-testid="content">card content here</p>
      </WizardLayout>,
    );
    expect(screen.getByText("Tuxlink Setup")).toBeInTheDocument();
    expect(screen.getByText("Welcome to Tuxlink")).toBeInTheDocument();
    expect(screen.getByText(/Step 1 of 3/i)).toBeInTheDocument();
    expect(screen.getByTestId("content")).toBeInTheDocument();
  });

  it("renders the optional tagline when provided", () => {
    // Per Phase 2 mockup: an optional muted-text tagline under the title.
    render(
      <WizardLayout
        title="Welcome to Tuxlink"
        tagline="Linux-native Winlink mail. Setup takes 1-3 minutes."
        step={1}
        totalSteps={3}
      >
        <span />
      </WizardLayout>,
    );
    expect(
      screen.getByText(/Linux-native Winlink mail/i),
    ).toBeInTheDocument();
  });

  it("does NOT render a tagline element when tagline prop is omitted", () => {
    render(
      <WizardLayout title="Welcome to Tuxlink" step={1} totalSteps={3}>
        <span />
      </WizardLayout>,
    );
    // No tagline-class element when prop is absent. Use queryAllByText with
    // a permissive regex; expect zero matches.
    expect(
      screen.queryByText(/Linux-native Winlink mail/i),
    ).not.toBeInTheDocument();
  });

  it("supports the offline-path collapsed step count (Task 11.5 will pass totalSteps=2)", () => {
    render(
      <WizardLayout title="Offline identity" step={2} totalSteps={2}>
        <span />
      </WizardLayout>,
    );
    expect(screen.getByText(/Step 2 of 2/i)).toBeInTheDocument();
  });

  it("renders semantic landmarks (header + main) for accessibility", () => {
    render(
      <WizardLayout title="t" step={1} totalSteps={3}>
        <span />
      </WizardLayout>,
    );
    // The layout MUST use <header> and <main> so screen readers can navigate.
    // testing-library's getByRole queries the ARIA role; <header> = banner,
    // <main> = main.
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
  });
});
```

- [ ] **Step 9: Run the test — confirm RED**

```bash
pnpm vitest run src/wizard/WizardLayout.test.tsx
```

Expected: "Cannot find module './WizardLayout'" error. Red-stage failure as designed.

- [ ] **Step 10: Implement `src/wizard/WizardLayout.tsx` to satisfy the tests**

Create `src/wizard/WizardLayout.tsx`:

```tsx
import type { ReactNode } from "react";

/**
 * Shared layout for all wizard screens (Steps 1-3 + offline-path Step 2).
 *
 * Established by Task 9. Tasks 10 / 11 / 11.5 reuse this component without
 * modification. The `totalSteps` prop supports the offline-path collapsed
 * count (Task 11.5 passes 2; CMS path Tasks 9/10/11 pass 3). The optional
 * `tagline` renders a muted-text subtitle under the title (used by Step 1
 * per the mockup).
 */
export interface WizardLayoutProps {
  /** Per-screen title rendered as the <h2>. */
  title: string;
  /** Optional muted-text subtitle under the title. */
  tagline?: string;
  /** 1-indexed current step number. */
  step: number;
  /** Total step count for the active wizard branch (3 for CMS, 2 for offline). */
  totalSteps: number;
  children: ReactNode;
}

export function WizardLayout({
  title,
  tagline,
  step,
  totalSteps,
  children,
}: WizardLayoutProps) {
  return (
    <div className="wizard-layout">
      <header>
        <h1>Tuxlink Setup</h1>
        <div className="wizard-step-indicator">
          Step {step} of {totalSteps}
        </div>
      </header>
      <main>
        <h2>{title}</h2>
        {tagline && <p className="wizard-tagline">{tagline}</p>}
        {children}
      </main>
    </div>
  );
}
```

- [ ] **Step 11: Run the test — confirm GREEN**

```bash
pnpm vitest run src/wizard/WizardLayout.test.tsx
```

Expected: 5 tests passed (default render, tagline-present, tagline-absent, collapsed step count, semantic landmarks). Pristine output.

- [ ] **Step 12: Run the full vitest suite to verify no regression**

```bash
pnpm vitest run
```

Expected: 11 tests passed total (6 wizardState + 5 WizardLayout), 0 failed, 0 skipped. Pristine.

- [ ] **Step 13: TypeScript check**

```bash
pnpm tsc --noEmit
```

Expected: no errors. If errors surface (e.g., `JSX.IntrinsicElements` issues from missing React types), they're environmental — investigate the `tsconfig.json` includes before changing the implementation.

**BEFORE marking this phase complete:**

1. Review tests against `docs/pitfalls/testing-pitfalls.md` §1 (output pristine), §3 (error path coverage — confirmed: `advanceWizard` error cases are tested with `.toThrow(/regex/)` matching the message, not just `.toThrow()`), §7 (no shared mutable state — confirmed: each test calls `initialWizardState()` fresh).
2. Verify test coverage:
   - Error paths? ✓ (the "refuses to advance without decision" + "transition not implemented yet" cases)
   - Edge cases? ✓ (offline-path step count, semantic landmarks for a11y)
   - Initialization correctness? ✓ (all-fields-initialized test as cross-task contract)
3. Run tests one more time and confirm green: `pnpm vitest run`.

- [ ] **Step 14: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/<your-worktree>
git add src/wizard/wizardState.ts src/wizard/wizardState.test.ts \
        src/wizard/WizardLayout.tsx src/wizard/WizardLayout.test.tsx \
        src/setupTests.ts vite.config.ts package.json pnpm-lock.yaml
git status  # verify ONLY the above are staged
git commit -m "$(cat <<'EOF'
feat(wizard): scaffold WizardState + WizardLayout (Task 9 Phase 1)

Establish the shared wizard infrastructure reused by Tasks 9/10/11/11.5:
- `WizardState` includes all future-task fields (callsign, password,
  gridSquare, identifier, testSendStatus) so the type contract is stable
  across the wizard cluster.
- `WizardStep` union enumerates all five variants (welcome, credentials,
  test_send, offline_identity, complete).
- `advanceWizard` handles welcome → credentials | offline_identity branch
  per AMD-2; downstream tasks extend with their cases. Throws on any
  unhandled transition (the safety net for forgotten cases).
- `WizardLayout` accepts `totalSteps` (CMS path = 3, offline path = 2 per
  AMD-5) and optional `tagline` (used by Step 1 per the mockup).
- Vitest + @testing-library/react wired in; 11 tests pass.

Per docs/plans/2026-05-18-task-9-wizard-winlink-account-plan.md Phase 1.
Per docs/design/v0.0.1-ux-mockups.md §5.1.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Replace `<SESSION-MONIKER>` with the moniker from your `python3 .claude/scripts/get_agent_moniker.py` run.

Verify: `git log --oneline -1` shows the commit with the Agent trailer.

**On phase ship:** flip the Phase 1 banner above to ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>` per the Living Document Contract.

---

## Phase 2 — Step1Welcome component + tests

**Execution Status:** ⬜ NOT STARTED

**Goal:** Implement the welcome-screen UI component (`Step1Welcome.tsx`) with two large choice cards. Test-first via React Testing Library.

**Files:**
- Create: `src/wizard/Step1Welcome.tsx`
- Create: `src/wizard/Step1Welcome.test.tsx`
- Modify: `src/App.css` (add wizard-scoped CSS — choice card layout)

**BEFORE starting work:**

1. Invoke `superpowers:test-driven-development`.
2. Re-read `docs/design/v0.0.1-ux-mockups.md` §5.1 — the two choice cards, the default-selection language ("(default selection)"), and the operational-modes language. The mockup wizard-a-welcome.png is the canonical visual reference; this plan §"Design-doc-vs-mockup divergences" aligns to the mockup copy where it diverges from §5.1.
3. Examine `docs/design/mockups/images/wizard-a-welcome.png` for visual reference. Match the layout (two large vertical cards) and the copy.
4. Re-read `docs/pitfalls/implementation-pitfalls.md` SCOPE-1. The offline-deployment language MUST frame the operator's choice as a CLIENT-side decision (the operator is configuring their own client for offline operation), NEVER as "host a gateway" or "be a server." If you find yourself writing words like "gateway," "host," "listen for incoming," STOP.
5. Read `docs/pitfalls/testing-pitfalls.md` §3 (error path coverage) — the click handlers must be tested for both paths.

---

- [ ] **Step 1: Write failing test for `Step1Welcome.tsx`**

Create `src/wizard/Step1Welcome.test.tsx`. Note: copy text matches the mockup (`docs/design/mockups/images/wizard-a-welcome.png`), not the design-doc §5.1 prose — see plan §"Design-doc-vs-mockup divergences".

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Step1Welcome } from "./Step1Welcome";
import { initialWizardState } from "./wizardState";

describe("Step1Welcome", () => {
  it("renders the welcome heading and tagline", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    // Mockup: "Welcome to Tuxlink" + tagline "Linux-native Winlink mail. Setup
    // takes 1-3 minutes. You can change anything here later in Settings."
    expect(screen.getByText(/Welcome to Tuxlink/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Linux-native Winlink mail/i),
    ).toBeInTheDocument();
  });

  it("renders the AMD-2 canonical question, not the pre-amendment account question", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    expect(
      screen.getByText(/Will this installation connect to the Winlink CMS\?/i),
    ).toBeInTheDocument();
    // Anti-regression: the pre-amendment question MUST NOT appear.
    expect(
      screen.queryByText(/Do you have a Winlink account\?/i),
    ).not.toBeInTheDocument();
  });

  it("renders the two mockup-aligned choice cards", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    // Per mockup: "Yes — connect to Winlink CMS" / "No — offline / lab / training"
    expect(
      screen.getByRole("button", { name: /Yes.*connect to Winlink CMS/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /No.*offline.*lab.*training/i }),
    ).toBeInTheDocument();
  });

  it("renders the CMS card body copy from the mockup", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        /sending and receiving Winlink mail to\/from real Winlink stations.*Requires a Winlink account/i,
      ),
    ).toBeInTheDocument();
  });

  it("renders the offline card body copy + examples line from the mockup", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    expect(
      screen.getByText(/No internet required/i),
    ).toBeInTheDocument();
    // Examples line — mono-font block:
    expect(
      screen.getByText(/BAOFENG-FM-01.*ARES training drill.*EOC tabletop/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Winlink Hybrid Network/i),
    ).toBeInTheDocument();
  });

  it("renders the mode-switch footer hint from the mockup", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        /switch between modes anytime in Settings.*Connection/i,
      ),
    ).toBeInTheDocument();
  });

  it("does NOT render any Register-at-winlink.org affordance (moved to Task 10 per AMD-2)", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    // Per AMD-2, the Register link moves to Task 10's header. Step1 has none.
    expect(screen.queryByText(/Register/i)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/winlink\.org\/user/i),
    ).not.toBeInTheDocument();
  });

  it("CMS card is pre-selected at initial render (per mockup)", () => {
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    const cmsCard = screen.getByRole("button", {
      name: /Yes.*connect to Winlink CMS/i,
    });
    const offlineCard = screen.getByRole("button", {
      name: /No.*offline.*lab.*training/i,
    });
    expect(cmsCard).toHaveAttribute("aria-pressed", "true");
    expect(offlineCard).toHaveAttribute("aria-pressed", "false");
  });

  it("offline card becomes selected when state.connectToCms === false", () => {
    render(
      <Step1Welcome
        state={{ ...initialWizardState(), connectToCms: false }}
        onChoice={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    const cmsCard = screen.getByRole("button", {
      name: /Yes.*connect to Winlink CMS/i,
    });
    const offlineCard = screen.getByRole("button", {
      name: /No.*offline.*lab.*training/i,
    });
    expect(cmsCard).toHaveAttribute("aria-pressed", "false");
    expect(offlineCard).toHaveAttribute("aria-pressed", "true");
  });

  it("calls onChoice(true) when the CMS card is clicked", async () => {
    const onChoice = vi.fn();
    render(
      <Step1Welcome
        state={{ ...initialWizardState(), connectToCms: false }}
        onChoice={onChoice}
        onNext={vi.fn()}
      />,
    );
    const user = userEvent.setup();
    await user.click(
      screen.getByRole("button", { name: /Yes.*connect to Winlink CMS/i }),
    );
    expect(onChoice).toHaveBeenCalledExactlyOnceWith(true);
  });

  it("calls onChoice(false) when the offline card is clicked", async () => {
    const onChoice = vi.fn();
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={onChoice}
        onNext={vi.fn()}
      />,
    );
    const user = userEvent.setup();
    await user.click(
      screen.getByRole("button", { name: /No.*offline.*lab.*training/i }),
    );
    expect(onChoice).toHaveBeenCalledExactlyOnceWith(false);
  });

  it("Continue button is always visible and triggers onNext when clicked (per mockup)", async () => {
    const onNext = vi.fn();
    render(
      <Step1Welcome
        state={initialWizardState()}
        onChoice={vi.fn()}
        onNext={onNext}
      />,
    );
    // Per mockup: Continue is ALWAYS rendered (no gate on a card click —
    // the initial state already has CMS pre-selected, so Continue is always
    // semantically valid).
    const cont = screen.getByRole("button", { name: /Continue/i });
    expect(cont).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(cont);
    expect(onNext).toHaveBeenCalledOnce();
  });
});
```

`@testing-library/user-event` was installed in Phase 1 Step 1 as part of the wizard scaffold's devDeps install. Verify it's present:

```bash
pnpm list @testing-library/user-event --depth=0
```

Expected: one line with the installed version. If absent (because someone executed Phase 2 without Phase 1's install), run `pnpm add -D @testing-library/user-event` now.

- [ ] **Step 2: Run the test — confirm RED**

```bash
pnpm vitest run src/wizard/Step1Welcome.test.tsx
```

Expected: "Cannot find module './Step1Welcome'" error. Red-stage.

- [ ] **Step 3: Implement `src/wizard/Step1Welcome.tsx` to satisfy the tests**

Create `src/wizard/Step1Welcome.tsx`. Copy text matches the mockup (see plan §"Design-doc-vs-mockup divergences"):

```tsx
import { WizardLayout } from "./WizardLayout";
import type { WizardState } from "./wizardState";

/**
 * Wizard Step 1 — welcome / connection-type chooser.
 *
 * Per AMD-2 (docs/design/v0.0.1-ux-mockups.md §5.1) + mockup
 * docs/design/mockups/images/wizard-a-welcome.png:
 *   Question: "Will this installation connect to the Winlink CMS?"
 *   Choice A (pre-selected per mockup): CMS path → Task 10 (credentials).
 *   Choice B:                           Offline path → Task 11.5.
 *
 * NOT a Register-at-winlink.org affordance. The Register link moved to
 * Task 10's header per AMD-3.
 */
export interface Step1WelcomeProps {
  state: WizardState;
  /** Operator picked one of the two cards. Updates `connectToCms` in state. */
  onChoice: (connectToCms: boolean) => void;
  /** Operator clicked Continue to advance the wizard. */
  onNext: () => void;
}

export function Step1Welcome({ state, onChoice, onNext }: Step1WelcomeProps) {
  const cmsSelected = state.connectToCms === true;
  const offlineSelected = state.connectToCms === false;

  return (
    <WizardLayout
      title="Welcome to Tuxlink"
      tagline="Linux-native Winlink mail. Setup takes 1-3 minutes. You can change anything here later in Settings."
      step={1}
      totalSteps={3}
    >
      <p className="wizard-question">
        Will this installation connect to the Winlink CMS?
      </p>

      <div className="wizard-choice-cards">
        <button
          type="button"
          className={`wizard-choice-card ${cmsSelected ? "selected" : ""}`}
          aria-pressed={cmsSelected}
          onClick={() => onChoice(true)}
        >
          <span className="wizard-choice-card-title">
            <span aria-hidden="true" className="wizard-choice-check">
              {cmsSelected ? "✓" : "○"}
            </span>{" "}
            Yes — connect to Winlink CMS
          </span>
          <span className="wizard-choice-card-body">
            For sending and receiving Winlink mail to/from real Winlink
            stations. Requires a Winlink account.
          </span>
        </button>

        <button
          type="button"
          className={`wizard-choice-card ${offlineSelected ? "selected" : ""}`}
          aria-pressed={offlineSelected}
          onClick={() => onChoice(false)}
        >
          <span className="wizard-choice-card-title">
            <span aria-hidden="true" className="wizard-choice-check">
              {offlineSelected ? "✓" : "○"}
            </span>{" "}
            No — offline / lab / training
          </span>
          <span className="wizard-choice-card-body">
            No internet required. For dummy-load testing, training exercises,
            field deployments without connectivity, or developing without a
            Winlink account.
          </span>
          <span className="wizard-choice-card-examples">
            Examples: bench test on BAOFENG-FM-01 · ARES training drill · EOC
            tabletop · <strong>radio-only emcomm deployment when internet is
            down (Winlink Hybrid Network)</strong> · radio loopback validation
          </span>
        </button>
      </div>

      <div className="wizard-footer">
        <span className="wizard-footer-hint">
          You can switch between modes anytime in Settings → Connection.
        </span>
        <button type="button" className="wizard-primary" onClick={onNext}>
          Continue →
        </button>
      </div>
    </WizardLayout>
  );
}
```

**Note on `WizardLayout` API change.** The mockup shows a richer header (branded "T" icon, "first run" subtitle, horizontal step indicator with labels WELCOME / IDENTITY / VERIFY, and "Step 3 is optional" annotation in the upper-right). To accommodate this without rewriting `WizardLayout` per task, Phase 1 Step 10's `WizardLayout` MUST be amended to accept:

- `title: string` (was already there — used as the prominent `<h2>` heading; here "Welcome to Tuxlink")
- `tagline?: string` (NEW — optional muted-text subtitle below the heading)
- `step: number` (was already there — current 1-indexed step number)
- `totalSteps: number` (was already there — total steps in this branch)
- The header's horizontal step indicator with labels is implementation detail of WizardLayout; Phase 1 Step 10 SHOULD render labeled pills (Welcome / Identity / Verify for the CMS path; Welcome / Identity for the offline path) but for the v0.0.1 MVP a simple "Step 1 of 3 — Welcome" text is acceptable. The mockup's full pill-indicator can be a Phase 1 polish iteration if time allows; if not, file a bd issue to polish in a follow-up PR. **Test coverage is the gate; visual polish is not.**
- The mockup's "Step 3 is optional" annotation is a Task-11 (verify-step) concern — Task 11 amends WizardLayout to show it when on Step 1 or 2 of the CMS path. NOT a Task-9 deliverable.

**Update Phase 1 Step 10's WizardLayout code** (back-reference): the `WizardLayoutProps` interface adds `tagline?: string` and the body renders the tagline as a `<p className="wizard-tagline">` immediately under the title `<h2>` if present. Phase 1 Step 8's tests need a corresponding new test:

```tsx
it("renders the optional tagline when provided", () => {
  render(
    <WizardLayout title="X" tagline="hello world" step={1} totalSteps={3}>
      <span />
    </WizardLayout>,
  );
  expect(screen.getByText("hello world")).toBeInTheDocument();
});
```

If you're executing Phase 1 BEFORE Phase 2 (recommended), add the tagline prop and the test in Phase 1 directly. If you're going back to Phase 1 after partial Phase 2 work, amend in place.

- [ ] **Step 4: Add wizard CSS to `src/App.css`**

The current `src/App.css` contains the scaffold styles. Read it first, then APPEND the wizard CSS at the bottom (do not modify existing rules). Accent color `#f5a524` (orange) matches the mockup's selected-card highlight + Continue button:

```css
/* === Wizard scaffold (Task 9 Phases 1-2) === */

.wizard-layout {
  max-width: 760px;
  margin: 0 auto;
  padding: 2rem 1.5rem;
  font-family: system-ui, -apple-system, sans-serif;
}

.wizard-layout header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  margin-bottom: 1.5rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}

.wizard-layout header h1 {
  font-size: 1.25rem;
  margin: 0;
}

.wizard-step-indicator {
  font-size: 0.875rem;
  opacity: 0.6;
}

.wizard-layout main h2 {
  font-size: 1.75rem;
  margin: 0 0 0.5rem 0;
  font-weight: 600;
}

.wizard-tagline {
  font-size: 0.95rem;
  opacity: 0.7;
  margin: 0 0 2rem 0;
  line-height: 1.5;
}

.wizard-question {
  font-size: 1rem;
  margin: 0 0 1rem 0;
}

.wizard-choice-cards {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
}

.wizard-choice-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: left;
  padding: 1rem 1.25rem;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.02);
  color: inherit;
  font-family: inherit;
  cursor: pointer;
  transition: border-color 120ms, background 120ms;
}

.wizard-choice-card:hover {
  border-color: rgba(255, 255, 255, 0.24);
  background: rgba(255, 255, 255, 0.04);
}

.wizard-choice-card.selected {
  border-color: #f5a524;
  background: rgba(245, 165, 36, 0.06);
}

.wizard-choice-card-title {
  font-size: 1rem;
  font-weight: 600;
  margin-bottom: 0.375rem;
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
}

.wizard-choice-check {
  color: #f5a524;
  font-size: 1.1rem;
}

.wizard-choice-card-body {
  font-size: 0.875rem;
  opacity: 0.8;
  line-height: 1.45;
}

.wizard-choice-card-examples {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.78rem;
  opacity: 0.6;
  margin-top: 0.5rem;
  line-height: 1.6;
}

.wizard-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 1.5rem;
}

.wizard-footer-hint {
  font-size: 0.85rem;
  opacity: 0.6;
}

.wizard-primary {
  padding: 0.5rem 1.25rem;
  border-radius: 6px;
  border: 1px solid #f5a524;
  background: #f5a524;
  color: #1a1a1a;
  font-family: inherit;
  font-size: 1rem;
  font-weight: 500;
  cursor: pointer;
}

.wizard-primary:hover {
  background: #e09618;
  border-color: #e09618;
}
```

**Do NOT** delete or modify existing scaffold styles in `App.css` — Tasks 10/11/11.5 may also add wizard styles, and the scaffold styles stay until Task 16 (dashboard ribbon) replaces them.

- [ ] **Step 5: Run the test — confirm GREEN**

```bash
pnpm vitest run src/wizard/Step1Welcome.test.tsx
```

Expected: 12 tests passed (welcome heading, AMD-2 question, two cards, CMS body, offline body + examples, footer hint, no-Register, CMS pre-selected, offline-selected-on-state, click CMS, click offline, Continue always-visible). Pristine output.

If any test fails:
- "Cannot find module" → check the import paths in the test file.
- "Element not found by role 'button'" → verify the implementation uses `<button>` elements (not `<div onClick>`); the accessibility-by-default rule is intentional, not negotiable.
- "Expected aria-pressed 'true' but got 'false'" → check the boolean expression on `cmsSelected` / `offlineSelected`.
- "Element not found with text /BAOFENG-FM-01/" → the offline-card examples line was not rendered; check that `wizard-choice-card-examples` span is in the JSX.

- [ ] **Step 6: Run the full vitest suite**

```bash
pnpm vitest run
```

Expected: 23 tests passed (6 wizardState + 5 WizardLayout + 12 Step1Welcome), 0 failed.

- [ ] **Step 7: TypeScript check**

```bash
pnpm tsc --noEmit
```

Expected: 0 errors.

**BEFORE marking this phase complete:**

1. Review tests against `docs/pitfalls/testing-pitfalls.md`:
   - §1 (output pristine): suite output is clean. ✓
   - §3 (error path coverage): both click branches tested via `toHaveBeenCalledExactlyOnceWith`, plus the "Continue hidden until choice" path. ✓
   - §7 (no shared state): each test uses `initialWizardState()` fresh and a fresh `vi.fn()` per assertion. ✓
2. Review against `docs/pitfalls/implementation-pitfalls.md` SCOPE-1: confirm the offline-card body copy does NOT include "gateway," "host," "listen for incoming," "server," or "be a Winlink." Search the file: `grep -niE 'gateway|host|listen.*incoming|be a winlink|server' src/wizard/Step1Welcome.tsx` — expected zero hits.
3. Run tests one more time: `pnpm vitest run`. Confirm green.

- [ ] **Step 8: Commit**

```bash
git add src/wizard/Step1Welcome.tsx src/wizard/Step1Welcome.test.tsx src/App.css package.json pnpm-lock.yaml
git status  # verify ONLY the above are staged
git commit -m "$(cat <<'EOF'
feat(wizard): Step1Welcome component — connection-type chooser (Task 9 Phase 2)

Per AMD-2 (docs/design/v0.0.1-ux-mockups.md §5.1) + mockup
docs/design/mockups/images/wizard-a-welcome.png:
- Question: "Will this installation connect to the Winlink CMS?"
- Two large choice cards, copy aligned to the mockup:
  - "Yes — connect to Winlink CMS" (pre-selected at load per mockup)
  - "No — offline / lab / training"
- Offline card body cites legitimate operational modes (Winlink Hybrid
  Network, ARES training drill, EOC tabletop, BAOFENG-FM-01 bench test,
  radio loopback validation) per the mockup Examples line.
- Continue button is always visible (initial state has CMS pre-selected;
  the button is semantically valid from the start).
- No Register-at-winlink.org affordance (moved to Task 10 header per AMD-3).
- 12 component tests pass; total wizard suite 23 tests green.

CSS appended to App.css; existing scaffold styles preserved (Task 16
dashboard ribbon will replace them).

The mockup-vs-design-doc divergences are catalogued in the plan §"Design-
doc-vs-mockup divergences"; flagged for Cameron's review (PR body).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**On phase ship:** flip the Phase 2 banner to ✅ SHIPPED at `<SHA>` on `<YYYY-MM-DD>`.

---

## Phase 3 — Rust `config_exists` Tauri command + test

**Execution Status:** ⬜ NOT STARTED

**Goal:** Add a Rust-side `config_exists()` Tauri command so the frontend can ask "has the wizard been completed previously?" The command is a thin wrapper over `config_path().exists()`.

**Files:**
- Modify: `src-tauri/src/lib.rs` (add `#[tauri::command] config_exists()`)
- Create: `src-tauri/tests/config_exists_test.rs`
- Modify: `src-tauri/src/main.rs` (register the command in `invoke_handler!`)

**BEFORE starting work:**

1. Invoke `superpowers:test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` §6 (Boundary & Configuration Validation) and §7 (Test Infrastructure Hygiene — specifically the "no hardcoded time-of-day or timezone assumptions" item; for our case the analog is "no hardcoded XDG_CONFIG_HOME" — use a tempdir).
3. Skim `src-tauri/src/config.rs` to confirm `config_path()` is still exported (per Pre-flight Step 4).

---

- [ ] **Step 1: Write the failing test**

Create `src-tauri/tests/config_exists_test.rs`:

```rust
use std::fs;
use tempfile::tempdir;
use tuxlink_lib::config_exists;

#[test]
fn test_config_exists_returns_false_when_config_absent() {
    let tmp = tempdir().expect("tempdir creation");
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    // No file written; config_path() resolves to <tmp>/tuxlink/config.json
    // which does not exist.
    assert!(
        !config_exists(),
        "config_exists() must return false when the config file is absent"
    );
    std::env::remove_var("XDG_CONFIG_HOME");
}

#[test]
fn test_config_exists_returns_true_when_config_present() {
    let tmp = tempdir().expect("tempdir creation");
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    let cfg_dir = tmp.path().join("tuxlink");
    fs::create_dir_all(&cfg_dir).expect("mkdir -p");
    let cfg_path = cfg_dir.join("config.json");
    fs::write(&cfg_path, "{}").expect("write empty config");
    assert!(
        config_exists(),
        "config_exists() must return true when the config file is present"
    );
    std::env::remove_var("XDG_CONFIG_HOME");
}
```

Check whether `tempfile` is already a dev-dependency (Tasks 2/3/5 may have added it):

```bash
grep -nE '^tempfile\s*=' src-tauri/Cargo.toml
```

If zero matches, add `tempfile = "3"` under `[dev-dependencies]` in `src-tauri/Cargo.toml`. If `[dev-dependencies]` doesn't exist as a section, add it. Final shape:

```toml
[dev-dependencies]
tempfile = "3"
# ... other dev-deps already present
```

If the grep returned a match, no edit needed — verify the version is `3` or higher; if older, escalate (don't downgrade).

**WARNING about env-mutating tests** (per `docs/pitfalls/testing-pitfalls.md` §5 Concurrency & TOCTOU + §7 Test Infrastructure Hygiene):

`std::env::set_var` mutates process-global state. Cargo runs tests in the SAME binary in parallel threads by default, so these two tests CAN race on `XDG_CONFIG_HOME`. Even though each test sets and removes the var, the interleavings include:

```
Thread A: set XDG_CONFIG_HOME = /tmp/a
Thread B: set XDG_CONFIG_HOME = /tmp/b   (A's "set" is now lost)
Thread A: config_path() reads /tmp/b — assertion fails
```

**Mitigation:** invoke this test file with `--test-threads=1` to serialize the two tests:

```bash
cargo test --test config_exists_test -- --test-threads=1
```

The `cargo test` command in Step 5 below uses this flag. If you forget the flag and see intermittent failures, that's the race; re-run with the flag. The `--test-threads=1` constraint is scoped to THIS test binary only; other tests (`config_test`, `pat_process_test`, etc.) keep their default parallelism because they live in different binaries.

Alternative: add `serial_test = "3"` to dev-deps and annotate each test with `#[serial_test::serial]`. For v0.0.1's two-test footprint this is overkill; `--test-threads=1` is the lighter solution. If a third env-mutating test joins this binary later, promote to `serial_test`.

- [ ] **Step 2: Run the test — confirm RED**

```bash
cd src-tauri && cargo test --test config_exists_test -- --test-threads=1
```

Expected: compile error "cannot find function `config_exists` in crate `tuxlink_lib`". Red-stage.

(The `--test-threads=1` flag is per the env-mutating-tests warning above. Use it for every invocation of this test binary.)

- [ ] **Step 3: Implement `config_exists()` in `src-tauri/src/lib.rs`**

Read `src-tauri/src/lib.rs` first. **Required preconditions** (verify before editing):

1. `pub mod config;` declaration exists at the top. If it does NOT exist, STOP — Phase 3 requires `config::config_path()` to be reachable from `lib.rs`. Adding the `pub mod config;` declaration is a Task-2 concern, not Task 9; escalate.
2. The `tauri` crate is in scope (some scaffolds re-export it via `tauri::generate_handler!` from `main.rs`; if `lib.rs` doesn't `use tauri;`, the `#[tauri::command]` attribute may fail to expand). Test by looking for any existing `#[tauri::command]` attribute in `lib.rs`. If absent, add `use tauri;` at the top of `lib.rs` near the other use statements.

Add the `config_exists` command at the end of `lib.rs` (do NOT modify existing functions):

```rust
/// Tauri command: returns true iff a tuxlink config file exists at the
/// resolved config path (XDG_CONFIG_HOME-aware). Used by the wizard's
/// App.tsx to decide whether to render the wizard or the main UI.
///
/// Added by Task 9 (wizard scaffold). The command does NOT validate the
/// config's contents — only its presence. Validation happens at load time
/// in a later task (config-loading lives outside Task 9 scope; for v0.0.1
/// the main UI is a placeholder per Task 12).
#[tauri::command]
pub fn config_exists() -> bool {
    config::config_path().exists()
}
```

- [ ] **Step 4: Register the command in `src-tauri/src/main.rs`**

Read `src-tauri/src/main.rs` first. Look for the `invoke_handler!` macro call. The current code likely registers some existing commands (or none). Add `tuxlink_lib::config_exists` to the list. Example before/after:

If the current line reads:
```rust
.invoke_handler(tauri::generate_handler![greet])
```

Change to:
```rust
.invoke_handler(tauri::generate_handler![
    greet,
    tuxlink_lib::config_exists,
])
```

If no `invoke_handler` line exists (because the scaffold's default `greet` was removed in a prior task), add:
```rust
.invoke_handler(tauri::generate_handler![tuxlink_lib::config_exists])
```

**Do NOT** remove the `greet` command if it's still registered — that's a separate cleanup task (out of Task 9 scope). Just add `config_exists` alongside whatever is already there.

- [ ] **Step 5: Run the test — confirm GREEN**

```bash
cd src-tauri && cargo test --test config_exists_test -- --test-threads=1
```

Expected: 2 tests pass. Output is pristine — no warnings.

If `cargo test` emits warnings (deprecated APIs, unused imports), fix them per testing-pitfalls §1. Do NOT mark the phase complete with a warning-spammed build.

If you see intermittent failures (assertion mismatch on `XDG_CONFIG_HOME`-derived paths), you forgot `--test-threads=1` — re-read the env-mutating-tests warning.

- [ ] **Step 6: Run the full Rust test suite to verify no regression**

```bash
cd src-tauri && cargo test
```

Note: the full-suite invocation uses default parallelism (no `--test-threads=1`). Cargo runs each `tests/<name>.rs` file as a SEPARATE binary; the `config_exists_test` binary will run its 2 tests in parallel here. The 2 tests CAN race on `XDG_CONFIG_HOME` in this context. **If the full suite is flaky on this binary, that's why** — re-run as `cd src-tauri && cargo test -- --test-threads=1` (slower but deterministic) or run the suspect binary individually with `--test-threads=1` per Step 5. For Phase-3 acceptance, the per-binary invocation in Step 5 is the authoritative pass.

Expected: all tests pass (config_test from Task 2, pat_process_test from Task 3, pat_client_test from Task 5, plus the 2 new config_exists tests). 0 failed.

- [ ] **Step 7: Build the Rust crate to verify nothing broke**

```bash
cd src-tauri && cargo build
```

Expected: "Finished `dev` profile". No warnings.

**BEFORE marking this phase complete:**

1. Review tests against `docs/pitfalls/testing-pitfalls.md`:
   - §1 (output pristine): cargo output is clean. ✓
   - §3 (error path coverage): both branches (present + absent) tested. ✓
   - §6 (default values): the "absent" case is the default state for a first-run install. ✓
   - §7 (no shared mutable state): each test creates a fresh tempdir and cleans up the env var. ✓
2. Verify `cargo build` produces no new warnings vs. the pre-Phase-3 state.

- [ ] **Step 8: Commit**

```bash
cd /home/administrator/Code/tuxlink/worktrees/<your-worktree>
git add src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/tests/config_exists_test.rs src-tauri/Cargo.toml
git status  # verify ONLY the above (Cargo.lock may also appear if tempfile was newly added)
git add src-tauri/Cargo.lock  # if changed
git commit -m "$(cat <<'EOF'
feat(wizard): config_exists Tauri command (Task 9 Phase 3)

Wraps config_path().exists() so the wizard's App.tsx can gate on
first-run state. 2 tempdir-isolated tests cover present/absent cases.

The command does NOT validate config contents — only its presence.
Content validation happens at load time in a later task; for v0.0.1
the main UI is a placeholder per Task 12.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**On phase ship:** flip the Phase 3 banner to ✅ SHIPPED.

---

## Phase 4 — `App.tsx` integration + manual browser smoke

**Execution Status:** ⬜ NOT STARTED

**Goal:** Wire `Step1Welcome` + `WizardLayout` + `wizardState` + the new `config_exists` Tauri command into `App.tsx`, replacing the scaffold's Vite/Tauri/React landing page. Verify end-to-end via `pnpm tauri dev` (per the project memory entry "Browser smoke before UI ship" — static review + unit tests miss CSS specificity / [hidden]-vs-display: overrides).

**Files:**
- Modify: `src/App.tsx` (replace scaffold with wizard router)
- Create: `src/App.test.tsx` (component test for the wizard-vs-main-UI gate)

**BEFORE starting work:**

1. Invoke `superpowers:test-driven-development`.
2. Re-read `docs/pitfalls/testing-pitfalls.md` §6 (Default values — what does the app do when `config_exists` returns true vs false vs the loading state?).
3. Skim `src/App.tsx` (the current scaffold landing page) so you know what's being replaced.

---

- [ ] **Step 1: Write the failing test for `App.tsx`**

Create `src/App.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import App from "./App";

// Mock the @tauri-apps/api/core module so we can control invoke().
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
const mockInvoke = vi.mocked(invoke);

describe("App", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("shows a loading placeholder before config_exists resolves", () => {
    // invoke() returns a never-resolving promise to keep us in the loading state.
    mockInvoke.mockReturnValue(new Promise(() => {}));
    render(<App />);
    expect(screen.getByText(/Loading/i)).toBeInTheDocument();
  });

  it("renders the wizard Step1Welcome when config_exists returns false", async () => {
    mockInvoke.mockResolvedValue(false);
    render(<App />);
    // Wait for the async useEffect to resolve and re-render.
    await waitFor(() => {
      expect(
        screen.getByText(/Will this installation connect to the Winlink CMS\?/i),
      ).toBeInTheDocument();
    });
  });

  it("renders the main-UI placeholder when config_exists returns true", async () => {
    mockInvoke.mockResolvedValue(true);
    render(<App />);
    await waitFor(() => {
      // Per plan: Task 9 lands a placeholder main UI; Task 12 replaces it
      // with the inbox.
      expect(screen.getByText(/Main inbox.*Task 12/i)).toBeInTheDocument();
    });
  });

  it("calls config_exists exactly once on mount", async () => {
    mockInvoke.mockResolvedValue(false);
    render(<App />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("config_exists");
    });
    expect(mockInvoke).toHaveBeenCalledTimes(1);
  });

  it("does NOT render the original Tauri/React scaffold landing page", async () => {
    mockInvoke.mockResolvedValue(false);
    render(<App />);
    await waitFor(() => {
      expect(
        screen.queryByText(/Welcome to Tauri \+ React/i),
      ).not.toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 2: Run the test — confirm RED**

```bash
pnpm vitest run src/App.test.tsx
```

Expected: tests fail with assertion mismatches (the current `App.tsx` renders "Welcome to Tauri + React," which our 5th test asserts is NOT present). Specifically:
- Test 1 may pass coincidentally (no async resolution); OK.
- Tests 2, 3, 4, 5 fail. Red-stage.

If the test fails with "Cannot find module './App'" — something is wrong with the import; investigate before proceeding.

- [ ] **Step 3: Replace `src/App.tsx`**

Read the current `src/App.tsx` first so you can describe in your commit message what's being removed. Then REPLACE its contents entirely. Note: the replacement KEEPS the existing `import "./App.css";` so Phase 2's wizard styles take effect (the existing scaffold's `App.css` is reused, with the wizard CSS rules appended by Phase 2 Step 4). Use:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Step1Welcome } from "./wizard/Step1Welcome";
import {
  initialWizardState,
  advanceWizard,
  type WizardState,
} from "./wizard/wizardState";
import "./App.css";

/**
 * Top-level App router.
 *
 * Established by Task 9. The wizard branch renders Step1Welcome here; Tasks
 * 10 / 11 / 11.5 will add their step cases to the `switch (wizardState.step)`
 * block below — see the plan §"Cross-task ordering" for the conflict-resolution
 * convention.
 *
 * The `configLoaded === null` loading state covers the brief async window
 * between mount and the invoke('config_exists') Promise resolving. We must
 * NOT default to "show wizard" during this window — a flicker of the wizard
 * on every relaunch for users WITH a config would be a UX regression.
 */
export default function App() {
  const [configLoaded, setConfigLoaded] = useState<null | boolean>(null);
  const [wizardState, setWizardState] = useState<WizardState>(
    initialWizardState(),
  );

  useEffect(() => {
    invoke<boolean>("config_exists").then((exists) => {
      setConfigLoaded(exists);
    });
  }, []);

  if (configLoaded === null) {
    return <div className="app-loading">Loading…</div>;
  }

  if (configLoaded === false) {
    switch (wizardState.step) {
      case "welcome":
        return (
          <Step1Welcome
            state={wizardState}
            onChoice={(connectToCms) =>
              setWizardState({ ...wizardState, connectToCms })
            }
            onNext={() => setWizardState(advanceWizard(wizardState))}
          />
        );
      case "credentials":
        // Task 10 deliverable.
        return (
          <div className="wizard-placeholder">
            Wizard step 'credentials' — implemented in Task 10
          </div>
        );
      case "test_send":
        // Task 11 deliverable.
        return (
          <div className="wizard-placeholder">
            Wizard step 'test_send' — implemented in Task 11
          </div>
        );
      case "offline_identity":
        // Task 11.5 deliverable.
        return (
          <div className="wizard-placeholder">
            Wizard step 'offline_identity' — implemented in Task 11.5
          </div>
        );
      case "complete":
        // Tasks 10/11.5 transition here on persist-success; main UI mounts.
        return (
          <div className="wizard-placeholder">
            Wizard complete — main UI mounts in Task 12
          </div>
        );
      default: {
        // TypeScript exhaustiveness check: if `WizardStep` grows a new
        // variant, this assignment fails to compile, forcing the new variant
        // to be handled. (See https://www.typescriptlang.org/docs/handbook/2/narrowing.html#exhaustiveness-checking)
        const _exhaustive: never = wizardState.step;
        return (
          <div className="wizard-placeholder">
            Unknown wizard step: {String(_exhaustive)}
          </div>
        );
      }
    }
  }

  // Config exists → render main UI. For v0.0.1, this is a placeholder;
  // Task 12 (Inbox / Sent tabbed view) replaces it.
  return <div className="app-main-placeholder">Main inbox — implemented in Task 12</div>;
}
```

- [ ] **Step 4: Run the test — confirm GREEN**

```bash
pnpm vitest run src/App.test.tsx
```

Expected: 5 tests passed. Pristine output.

If a test fails with "Element not found":
- The wizard-render test expects the AMD-2 canonical question — verify the test regex matches the implementation copy.
- The main-UI test expects "Main inbox … Task 12" — verify the placeholder copy in `App.tsx` matches.

If a test fails with "invoke called N times, expected 1": React 18 `StrictMode` double-invokes `useEffect` in dev. Vitest's `jsdom` environment does NOT enable StrictMode by default. If your `main.tsx` wraps `<App />` in `<StrictMode>`, the test still sees ONE invocation because RTL renders without StrictMode by default. If you see double-invocation: do NOT change the test. Investigate whether something in `setupTests.ts` is enabling StrictMode (it shouldn't be).

- [ ] **Step 5: Run the full vitest suite**

```bash
pnpm vitest run
```

Expected: 28 tests pass (6 wizardState + 5 WizardLayout + 12 Step1Welcome + 5 App). 0 failed, 0 skipped.

- [ ] **Step 6: TypeScript check**

```bash
pnpm tsc --noEmit
```

Expected: 0 errors. The exhaustiveness-check `_exhaustive: never` assignment proves the switch covers all `WizardStep` variants.

- [ ] **Step 7: Manual browser smoke — `pnpm tauri dev`** (per project memory: "Browser smoke before UI ship")

This step CANNOT be skipped. Static review + unit tests miss CSS specificity / `[hidden]`-vs-`display:` overrides. The project memory entry is explicit: "run `pnpm tauri dev` and walk the user flow before declaring UI work done."

**Before running:** ensure no other `pat` or `tuxlink` process is using port 1420.

```bash
# From the worktree root.
pnpm tauri dev
```

Expected behavior:
1. Vite dev server starts (~5 s).
2. Rust crate compiles (~20-60 s on first run; <5 s on subsequent runs).
3. Tauri window opens.

**Manual checklist** (the operator must perform this walk-through):

```
[ ] Confirm pre-state: no ~/.config/tuxlink/config.json exists.
    Run:  ls -la ~/.config/tuxlink/ 2>&1 | head -5
    If config.json exists: rename it to config.json.bak FOR THIS TEST ONLY
    (and remember to restore at the end). The XDG_CONFIG_HOME override
    approach (XDG_CONFIG_HOME=/tmp/tuxlink-smoke-$$ pnpm tauri dev) MAY
    work, but Tauri's resource bundling occasionally caches the config
    dir at build time on some platforms. The rename-and-restore approach
    is more reliable for the manual smoke; prefer it.

[ ] Window opens. After a brief "Loading…" flash, the wizard renders.
    Header reads "Tuxlink Setup", step indicator reads "Step 1 of 3".
    The page title reads "Welcome to Tuxlink" with the tagline
    "Linux-native Winlink mail. Setup takes 1-3 minutes..." beneath.
    The question reads "Will this installation connect to the Winlink CMS?"

[ ] BOTH choice cards are visible:
    - "Yes — connect to Winlink CMS" (top card, PRE-SELECTED — orange
      border + orange "✓" mark visible)
    - "No — offline / lab / training" (bottom card, NOT selected — gray
      border + open "○" mark)
    Body copy on the offline card mentions BAOFENG-FM-01, ARES training
    drill, EOC tabletop, Winlink Hybrid Network (mono-font Examples line).

[ ] No Register-at-winlink.org link or button is visible. (If you see
    one, AMD-2 was misapplied — STOP and audit Step1Welcome.tsx.)

[ ] The Continue button is visible at the bottom-right of the layout
    from the moment the page renders (no card click required).

[ ] Click the offline card. The CMS card LOSES its selected treatment
    (border returns to gray, checkmark becomes "○"); the offline card
    GAINS it (orange border + "✓"). (Only one card is selected at a time.)

[ ] Click the CMS card again. State swaps back: CMS selected, offline
    not selected. Continue remains visible throughout.

[ ] Click Continue (with CMS selected). The wizard advances to the
    placeholder: "Wizard step 'credentials' — implemented in Task 10".
    (This is the placeholder; Task 10 will replace it with the credentials
    form. The transition itself is what we're verifying.)

[ ] Reload the wizard (close + reopen the window, OR press Ctrl+R if
    the dev server supports it). Click the offline card to select it,
    then click Continue. The wizard now advances to:
    "Wizard step 'offline_identity' — implemented in Task 11.5".
    Verifies the offline branch transition.

[ ] Close the Tauri window. The dev process exits cleanly (no orphan
    Rust process — verify with `pgrep -f tuxlink`).

[ ] Restore the config file if you renamed it:
    mv ~/.config/tuxlink/config.json.bak ~/.config/tuxlink/config.json
```

**If any checklist item fails:** STOP. Do NOT mark Phase 4 complete. The failure is a real bug — diagnose, fix, re-test. Common failure modes:

- "Continue button never appears" → the `choiceMade` derivation in `Step1Welcome.tsx` is wrong, OR React state is not updating (check the `onChoice` callback in `App.tsx`).
- "Both cards look selected" → CSS `.selected` rule isn't scoped to the chosen card; check the className conditional.
- "Wizard re-renders on every click" → fine, this is React-normal.
- "Tauri window shows 'Welcome to Tauri + React'" → the old scaffold persists; check that `App.tsx` was actually replaced (`git diff src/App.tsx`).
- "Loading… flash is multi-second" → expected on first launch (config_exists is a syscall + IPC round-trip); should be sub-100ms on subsequent runs.

- [ ] **Step 8: Confirm one more time — full test suite + TypeScript + Rust build**

```bash
pnpm vitest run && pnpm tsc --noEmit && (cd src-tauri && cargo test && cargo build)
```

Expected: all green. No failures, no warnings.

**BEFORE marking this phase complete:**

1. Review the manual smoke checklist — every item passed?
2. Review tests against `docs/pitfalls/testing-pitfalls.md`:
   - §1 (output pristine): vitest + cargo + tsc all clean. ✓
   - §3 (error path coverage): the three configLoaded states (null / true / false) all tested. ✓
   - §6 (default values): loading state explicitly tested (the "before invoke resolves" case). ✓
   - §7 (no shared mutable state): `mockInvoke.mockReset()` in `beforeEach`. ✓
3. Cross-check against the SCOPE-1 pitfall: the App.tsx and its placeholder copy do NOT reference gateway functionality. Search: `grep -niE 'gateway|RMS Trimode|RMS Relay|host.*incoming' src/App.tsx` — expected zero hits.
4. Cross-check against the HOOK-1 pitfall: you're in a worktree per the task assignment; no main-checkout writes should have been attempted. Confirm via `git rev-parse --show-toplevel` returns the worktree path.

- [ ] **Step 9: Commit**

```bash
git add src/App.tsx src/App.test.tsx
git status  # verify ONLY the above (App.css and others may legitimately have prior commits)
git commit -m "$(cat <<'EOF'
feat(wizard): App.tsx routes to wizard on first run (Task 9 Phase 4)

Replace the Tauri+React scaffold landing page with the wizard router:
- Mounts Step1Welcome when config_exists() returns false.
- Renders a Task-12 main-UI placeholder when config_exists() returns true.
- The `configLoaded === null` loading state covers the brief async
  window between mount and config_exists resolving; we must NOT default
  to "show wizard" during this window (UX regression for users with a
  config).
- The switch (wizardState.step) block enumerates all five WizardStep
  variants with a TypeScript exhaustiveness-check `never` assignment
  in the default arm; Tasks 10/11/11.5 add their cases (see plan
  §"Cross-task ordering" for the conflict-resolution convention).
- 5 component tests cover loading + wizard + main-UI branches plus a
  regression assertion that the original scaffold landing page is gone.
- Manual `pnpm tauri dev` smoke completed; the operator walked the
  click-through and confirmed Continue advances to the offline-path
  placeholder.

Total test count after all 4 phases land: 28 vitest tests (Phase 1: 11, Phase 2: 12, Phase 4: 5) + 2 cargo tests (Phase 3) = 30 tests. All green.

Per docs/plans/2026-05-18-task-9-wizard-winlink-account-plan.md Phase 4.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

**On phase ship:** flip the Phase 4 banner to ✅ SHIPPED. Update the top-of-plan Execution Status table.

---

## After completing all four phases — multi-perspective review

The four-phase implementation is complete. Before opening the PR:

**Review the batch from multiple perspectives. Minimum 3 review rounds. If round 3 still finds issues, keep going until clean.**

Suggested review angles for this plan:

1. **Cross-task contract review.** Open the Phase 1 `wizardState.ts` next to the Tasks 10/11/11.5 plan sections (in the v0.0.1 plan). Do the field names + step variants the downstream tasks will need match exactly what Phase 1 declared? If not, the wizardState shape needs amendment — STOP and amend before the PR. (Reuse the same exact-string strings — `'credentials'`, `'test_send'`, `'offline_identity'`, `'complete'` — that the downstream task plans expect; deviating here cascades.)

2. **Anti-pattern + pitfalls review.**
   - `docs/ux-anti-patterns.md` — full-view-swap forbidden? Check. Embedded-webview-for-external-URL forbidden? Check (we have no external URLs in Step1). Modal-dialog-as-primary-flow forbidden? Check (the wizard is a full window, not a modal).
   - `docs/pitfalls/implementation-pitfalls.md` SCOPE-1 — no gateway language? Confirmed via grep in Step 8 above.
   - `docs/pitfalls/implementation-pitfalls.md` RADIO-1 — no transmission code path introduced by Task 9? Confirmed: Task 9 has no Tauri commands that talk to a network.
   - `docs/pitfalls/implementation-pitfalls.md` RADIO-2 — no encryption decision introduced? Confirmed: Task 9 doesn't touch transport.
   - `docs/pitfalls/implementation-pitfalls.md` ORCH-1 — N/A (Task 9 dispatches no parallel subagents).
   - `docs/pitfalls/testing-pitfalls.md` — all 7 sections checked per per-phase BEFORE-mark-complete blocks above.

3. **Subagent-proofing review.** Re-read this plan top-to-bottom. Could a fresh subagent with zero context execute it? Look for:
   - "the [thing] we discussed" → never (subagent wasn't here).
   - "as appropriate" / "as needed" → never (state the concrete thing).
   - Missing file paths or commands → re-verify.
   - Types/functions referenced in late phases that aren't defined in earlier phases → re-verify.

If a round surfaces issues:
- Per Living Document Contract, add a `## Discoveries` entry at the top.
- Fix the issue in-place in the relevant phase.
- Run that phase's tests again (do not commit-without-test).
- Then continue.

---

## Session-end protocol (Wave-2 implementer)

The implementing agent is either a Wave-2 subagent dispatched by an orchestrator OR an operator running this plan directly in a fresh session — either way, the standard CLAUDE.md §Session Completion protocol applies. Specific reminders for Task 9:

0. **Before any work:** `bd update tuxlink-ko0 --claim` to claim the implementation bd issue. (The plan-writing bd issue `tuxlink-ak2` is a different issue — claimed and closed by the plan-writer; Wave-2 claims `tuxlink-ko0`.)
1. **`git push`** the four phase commits to `origin/<your-branch>`. The branch name follows ADR 0004: prefer `bd-tuxlink-ko0/<slug>` since the bd issue exists.
2. **Open a PR** against `feat/v0.0.1` (NOT `main`; per the v0.0.1 plan's branch model). Title: `[<your-moniker>] feat(wizard): Task 9 — Wizard Screen 1 (welcome / connection-type)`. Body includes:
   - One-paragraph summary
   - Test plan: "30 tests pass (28 vitest + 2 cargo); manual tauri-dev smoke completed per Phase 4 Step 7"
   - Reference to this plan: `docs/plans/2026-05-18-task-9-wizard-winlink-account-plan.md`
   - The AMD-2 amendment callout — cite the design doc by section AND the v0.0.1 plan's AMD-2 callout line: "Per docs/design/v0.0.1-ux-mockups.md §5.1 and docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md §Task 9 AMD-2 callout (line 2157)".
   - **The design-doc-vs-mockup divergences** flagged in the plan §"Design-doc-vs-mockup divergences" — the implementation aligns to the mockup; Cameron should confirm the §5.1 prose should be amended to match the mockup copy in a follow-up PR.
3. **bd close `tuxlink-ko0`** after PR merges (NOT before — closing on PR-open misrepresents completion state).
4. **Write a session-end handoff** to `dev/handoffs/<YYYY-MM-DD>-<your-moniker>.md` enumerating phases shipped, PR opened, worktree state, anything in-flight.
5. **Surface the next-session starting prompt** in your final user-facing message per CLAUDE.md step 7. Mention that Task 10 + Task 11 are now unblocked (they consume Task 9's wizardState/WizardLayout/App.tsx routing pattern); Task 10 should claim `tuxlink-1r5`, Task 11 claims `tuxlink-e4x`. **For Task 11.5:** no bd issue exists yet as of plan-writing date (2026-05-18) — `bd list` shows no entry for "11.5" / "offline". This is a known gap; Cameron may want to file `tuxlink-<new>` for Task 11.5 (referencing v0.0.1 plan §Task 11.5 AMD-5) before dispatching Wave-2 for that task. Flag this in the handoff if you discover the gap is still present.
6. **Worktree disposal** is the orchestrator's call (not Wave-2's). Wave-2 leaves the worktree in place after pushing; the orchestrator runs the ADR 0009 disposal ritual after PR merge.

## Cross-task serialization warning (Wave-2 orchestrator)

The orchestrator dispatching Wave-2 SHOULD NOT dispatch Tasks 10, 11, and 11.5 in parallel WITH Task 9 — those tasks consume artifacts (`wizardState.ts`, `WizardLayout.tsx`, `App.tsx`'s switch block, `setupTests.ts`, the vitest config) that Phase 1 of Task 9 creates. If those tasks dispatch in parallel and their subagents land tests referencing `import { WizardLayout } from './WizardLayout'` before Phase 1 ships, every parallel branch fails compile.

**Correct ordering:** Task 9 ships (PR merged to `feat/v0.0.1`) → THEN Tasks 10 / 11 / 11.5 dispatch in parallel (their files are mostly disjoint; the only shared file is `src/App.tsx`'s switch block, which conflicts cleanly via line-level merge).

If the orchestrator has already dispatched Tasks 10/11/11.5 in parallel with Task 9, the parallel subagents should detect missing imports and ESCALATE rather than try to scaffold `wizardState.ts` themselves (which would conflict with Task 9's branch on merge). Escalation goes to the orchestrator, who pauses the parallel work until Task 9 ships.

---

## Appendix A — Files this plan creates or modifies

| File | Action | Phase |
|---|---|---|
| `src/wizard/wizardState.ts` | Create | 1 |
| `src/wizard/wizardState.test.ts` | Create | 1 |
| `src/wizard/WizardLayout.tsx` | Create | 1 |
| `src/wizard/WizardLayout.test.tsx` | Create | 1 |
| `src/setupTests.ts` | Create | 1 |
| `vite.config.ts` | Modify (add `test:` block + triple-slash ref) | 1 |
| `package.json` | Modify (add 4 devDependencies in Phase 1, 1 in Phase 2) | 1, 2 |
| `pnpm-lock.yaml` | Modify (auto) | 1, 2 |
| `src/wizard/Step1Welcome.tsx` | Create | 2 |
| `src/wizard/Step1Welcome.test.tsx` | Create | 2 |
| `src/App.css` | Modify (append wizard CSS at bottom) | 2 |
| `src-tauri/src/lib.rs` | Modify (add `config_exists` command) | 3 |
| `src-tauri/src/main.rs` | Modify (register command in `invoke_handler!`) | 3 |
| `src-tauri/tests/config_exists_test.rs` | Create | 3 |
| `src-tauri/Cargo.toml` | Modify (add `tempfile` dev-dep if not present) | 3 |
| `src-tauri/Cargo.lock` | Modify (auto) | 3 |
| `src/App.tsx` | Replace (wizard router) | 4 |
| `src/App.test.tsx` | Create | 4 |

Total new files: 9. Modified files: 7. No deleted files (the scaffold's existing `App.tsx` is replaced in place, not deleted).

---

## Appendix B — The wizard pattern (for Tasks 10 / 11 / 11.5)

Wave-2 implementers of Tasks 10 / 11 / 11.5 should reuse Phase 1's scaffold per this contract:

1. **Shared state.** Add your field reads/writes to the existing `WizardState`. Do NOT introduce a per-step state silo. If you need a new field that Phase 1 didn't anticipate, file a bd issue to amend `wizardState.ts` rather than free-editing it.

2. **Layout.** Wrap your screen in `<WizardLayout title="..." step={N} totalSteps={3-or-2}>`. CMS path = `totalSteps={3}`. Offline path (Task 11.5) = `totalSteps={2}`.

3. **Transitions.** Extend `advanceWizard()` with your step's case. Throw on invalid inputs (the existing throw-when-unhandled pattern is the safety net).

4. **App.tsx routing.** REPLACE the placeholder body inside the existing `case '<your-step>':` arm in the `switch (wizardState.step)` block — do NOT delete-and-re-add the case (that's a wider diff that conflicts with parallel work on other cases). The default arm uses `_exhaustive: never` to force compile errors when WizardStep grows; if you ADD a new step variant beyond the five Phase 1 declared, the `never` assignment will catch the missing case at compile time.

5. **Tauri commands.** Wizard-write commands (e.g., `wizard_complete_cms` for Task 10, `wizard_complete_offline` for Task 11.5) SHOULD live in `src-tauri/src/wizard_commands.rs` per the v0.0.1 plan §Task 11.5 spec. Task 11.5 creates this file if Task 9 didn't (Task 9 adds `config_exists` to `lib.rs` directly, not to `wizard_commands.rs`, because `config_exists` is a read-only existence check shared by all wizard branches and isn't a "wizard-completion write"). First-to-land between Tasks 10 and 11.5 creates the file; the second-to-land adds their command alongside. Register every new command in `src-tauri/src/main.rs invoke_handler!`.

6. **Tests.** Mirror Phase 1's structure: a `<thing>.test.ts(x)` per source file. Use `@testing-library/react` + `userEvent` for component tests; use `vi.mock('@tauri-apps/api/core')` to mock Tauri command boundaries (per Phase 4 Step 1).

7. **CSS.** Append to `src/App.css` in a fresh "=== Wizard X (Task N Phase Y) ===" block. Do NOT modify Phase 2's wizard CSS rules. Use the same color palette (`#4f9cf9` for accent, `rgba(255,255,255,0.x)` for neutrals).

8. **Anti-pattern compliance.**
   - No external-browser-via-shell.open unless it's a non-form external URL (Task 10's Register link IS permitted; that's the only one).
   - No modal dialogs.
   - No full-view-swap on click.
   - No silent draft discard.
   - Use familiar Express terminology where it exists (per `docs/ux-anti-patterns.md`).

9. **Config write.** Tasks 10 (CMS path) and 11.5 (offline path) are the wizard tasks that WRITE the tuxlink config (Task 9 only READS via `config_exists`; Task 11 only TRANSMITS via the test-send, no config write). The write sets `wizard_completed: true` AND the branch-specific fields per AMD-1 schema. After write, `App.tsx` re-detects via re-invoking `config_exists` (or setting a local "wizard done" flag — both patterns are acceptable; Task 10/11.5's plans pick one). The wizard state is then garbage-collected and the main UI mounts.

10. **Commit message format.** Match Phase 1-4's style: `feat(wizard): <short subject> (Task N Phase M)` with body that cites the amendment + the per-task plan + the updated suite count (Phase 4 leaves the baseline at 30 tests total: 28 vitest + 2 cargo; each new task adds tests on top of that baseline).

---

**End of plan.** When all 4 phases are SHIPPED and the PR has merged, `bd close tuxlink-ko0`. The wizard scaffold + Step1Welcome is then the foundation Wave-2 Tasks 10 / 11 / 11.5 build on.
