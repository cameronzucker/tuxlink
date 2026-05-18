# Task 12 Implementation Plan — Inbox / Sent tabbed view

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the first persistent main-window UI for tuxlink — a tabbed Inbox / Sent mailbox view that lists messages from Pat via a Tauri command, virtualizes rows with `react-virtuoso`, auto-refetches every 10 seconds, and lays the foundation Task 13 (reading pane) will wire into. Scope follows the plan body's "minimum viable mailbox" shape with the design doc's tractable §5.5 deltas folded in (unread-count tab badge, column model expansion, no-full-view-swap discipline).

**Architecture:** Frontend = three new React modules (`Mailbox.tsx`, `MessageList.tsx`, `useMailbox.ts`) wrapped in a TanStack-Query provider mounted from `App.tsx`; tabs use Radix `Tabs` primitive; rows render via `react-virtuoso`. Backend = one new Tauri command (`mailbox_list`) in `lib.rs` delegating to the existing `pat_client::PatClient::list`; a new `session.rs` module holds Pat's HTTP base URL behind a `OnceLock` so commands can reach the running Pat without retrofitting Task 3's process supervisor. The `pat_client::Message` type gains `Serialize` so Tauri can marshal it across the IPC boundary.

**Tech Stack:** React 18 + TypeScript 5.4 (existing); `@tanstack/react-query` ^5.40 (already in `package.json`); `@radix-ui/react-tabs` ^1.1 (already in `package.json`); `react-virtuoso` ^4.7 (already in `package.json`); `vitest` + `@testing-library/react` + `@testing-library/jest-dom` + `jsdom` (NEW devDependencies — none currently installed); Tauri 2.x + `serde` (existing).

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

**Overall:** 0/5 phases shipped, 0 deferred. Plan committed pending implementation pickup.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 0 — Test harness (vitest + RTL bootstrap) | ⬜ Not started | — | Foundational; blocks every following phase |
| 1 — Backend wiring (`mailbox_list` cmd + `session.rs` + `Serialize`) | ⬜ Not started | — | Independent of Phase 2 once Phase 0 ships |
| 2 — Frontend MVP (`MessageList` + `useMailbox` + `Mailbox` + `App.tsx` mount) | ⬜ Not started | — | Depends on Phase 1's Tauri command being callable |
| 3 — §5.5 design deltas (unread-count badge, column model, no-view-swap) | ⬜ Not started | — | Depends on Phase 2's components existing |
| 4 — Review & ship (3-round adversarial impl review, anti-pattern check, PR) | ⬜ Not started | — | Final gate before merge |

### Deviations
_None recorded._

### Discoveries
_None recorded._

---

## Open spec deltas — Cameron decision required BEFORE Phase 3 starts

These items are flagged in `docs/design/v0.0.1-ux-mockups.md` §5.5 but are NOT resolvable by an implementing subagent inside Task 12 scope. The implementer MUST stop at Phase 3 if any of these remain undecided.

1. **Folder sidebar with Outbox / Drafts / Deleted Messages / Templates.** §5.5 says "the folder sidebar shows: Inbox, Outbox, Sent, Drafts, Deleted Messages, Templates (v0.1+ disabled)." For v0.0.1: Templates is explicitly disabled-placeholder. But:
   - **Outbox** — Pat's API exposes `out` (`MailboxFolder::Outbox` already exists in `pat_client.rs`); could ship in v0.0.1.
   - **Drafts** — Pat's HTTP API has no draft endpoint. Draft storage is Task 14's (Compose) responsibility per §5.7. A sidebar entry for "Drafts" before Task 14 lands would be a non-functional disabled item OR would require a v0.0.1 scope-add for draft storage.
   - **Deleted Messages** — Pat's API has `archive` but no "deleted" concept. Either (a) repurpose `archive` as "Deleted Messages" (semantic mismatch, since Express-Deleted ≠ archive), or (b) define a tuxlink-side "Deleted" store, or (c) ship as disabled-placeholder.
   - **Templates** — explicitly v0.1+ per §5.5; ship as disabled-placeholder only.

   **Plan posture:** The MVP shipped in Phases 0-2 is a **two-tab Inbox/Sent layout** matching the plan body (`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` §"Task 12"). Phase 3 adds the §5.5 deltas that ARE tractable (unread-count badge, column model, persistence, view-swap discipline). The folder sidebar is **deferred to a follow-up issue** until Cameron decides which of {Outbox-only, Outbox+disabled-placeholders, full sidebar} v0.0.1 ships. If Cameron picks "full sidebar with disabled placeholders" before Phase 3 starts, fold that into Phase 3's scope; otherwise file a `bd create` issue titled "Task 12 follow-up: folder sidebar (Outbox / Drafts / Deleted / Templates)" with deps on this task and on Task 14 (Drafts unblocker) for Cameron to schedule.

   **Why not relitigate in this plan:** Per CLAUDE.md §"Brainstorming preferences" + memory `feedback_dont_relitigate_settled_architecture.md`, the plan author does not unilaterally re-decide brainstorm-settled design questions. The §5.5 deltas above are real but ambiguous; Cameron's pick decides.

2. **`To` column in the message list.** §5.5 specifies columns "UTC time · From · To · Subject · Attachment indicator · Compressed-size." Pat's `PatMessageDto` (per `src-tauri/src/pat_client.rs`) currently has no `To` field. Inspection of Pat's `/api/mailbox/<folder>` response schema is needed to confirm whether `To` is present and just unparsed, or whether Pat omits it (in which case "no To column for Inbox" is a Pat limitation tuxlink inherits).

   **Plan posture:** Phase 3 adds the `To` column **if a no-cost DTO extension surfaces it from Pat's API**; otherwise the column ships empty (with "—" placeholder) and a `bd create` issue titled "Task 12 follow-up: surface `To` field from Pat /api/mailbox/* response (if available)" is filed. The implementer MUST inspect Pat's actual JSON response (curl against a running Pat instance) in Phase 1 Step 2 and record the finding in this plan's "Discoveries" subsection.

3. **Attachment-indicator column.** §5.5 specifies "Attachment indicator (the `#` column from Express)." Pat's `PatMessageDto` has no attachment-count field. Same investigative posture as #2: inspect Pat's response, surface if present, defer + file a follow-up issue if not.

4. **Column-width persistence.** §5.5 "Acceptance criteria delta from plan: match Express column shape; persist column widths to settings." Settings storage in v0.0.1 is `$XDG_CONFIG_HOME/tuxlink/config.json` (per Task 2's `config.rs`). Per-pane column widths are NOT in the current config schema (per `src-tauri/src/config.rs`). Adding them requires either (a) extending `config.rs` (low cost, additive), or (b) introducing a separate UI-state file (more architecture).

   **Plan posture:** Phase 3 adds column widths to the existing `config.rs` as an additive field with a sane default (matching the Express `Column widths=20;21;100;100;45;60;100;100;900` ratios per §5.5). If `config.rs`'s structure makes this non-trivial (e.g., it would require a schema-version bump), defer to a follow-up issue and ship Phase 3 without persistence — the columns still render with default widths.

**Cameron, the implementer needs your call on #1 BEFORE Phase 3 starts.** Items #2-4 are investigative-then-decide and the implementer can proceed without a synchronous answer. Item #1 (folder sidebar) blocks Phase 3 scope.

---

## Mandatory Per-Task Preamble

**Every task below starts with this work, implicitly. Do it even though it's not repeated verbatim per task:**

1. Read `CLAUDE.md` (full), `docs/pitfalls/implementation-pitfalls.md` (SCOPE-1, ORCH-1, HOOK-1, BD-1 at minimum), `docs/pitfalls/testing-pitfalls.md`, `docs/ux-anti-patterns.md`, and `docs/design/v0.0.1-ux-mockups.md` §5.5 (Task 12 spec).
2. Invoke the `superpowers:test-driven-development` skill.
3. Follow TDD: write the failing test first, run to confirm it fails, implement the minimal code to pass, run to confirm green. **No implementation code before a failing test.**
4. Note the moniker the dispatching session passed in and use it verbatim in `Agent:` commit trailers.

## Mandatory Per-Task Completion Check

**Before marking any phase complete:**

1. Re-read `docs/pitfalls/testing-pitfalls.md` with your just-written tests in mind. Specifically check: test output pristine, no skipped-is-passing, error paths covered, boundary values validated, no shared-state test flakes.
2. Run the phase's test command(s) and confirm green.
3. If any test assertion races, flakes, or fails nondeterministically, the fix is deterministic synchronization (e.g., `vi.waitFor`, `findBy*` queries, explicit promise resolution) — NOT assertion removal or weakening. If synchronization cannot make the assertion pass reliably, STOP and surface to the dispatching agent. Do not ship a weaker test. Weakened assertions rationalized as "test stabilization" are the exact pattern this rule prevents. Prefer mechanism assertions (e.g., "MessageList received `messages` prop with N entries") over symptom assertions (e.g., "scroll position is N pixels") where feasible.
4. Commit using the exact format specified in the phase's commit step, including the `Agent: <SESSION-MONIKER>` trailer.
5. Update the Execution Status banner of that phase (per Living Document Contract).

## Subagent Guardrails

- **Do NOT add dependencies not listed in this plan.** Phase 0 lists the exact new devDependencies (`vitest`, `@testing-library/react`, `@testing-library/jest-dom`, `@testing-library/user-event`, `jsdom`); no other JS deps added in this task. Phase 1 adds no Rust deps. If you think you need a different dep, STOP and surface it.
- **Do NOT create additional crates or top-level frontend modules** beyond `src/mailbox/`. The shape is one frontend module folder + one new Rust module (`session.rs`).
- **Worktree:** the implementing session creates its own worktree per ADR 0008 + `new_tuxlink_worktree.py`. Branch is `bd-tuxlink-zsm/task-12-inbox-sent` (matching the existing Task-12 bd issue `tuxlink-zsm`). Before creating the worktree, the implementing session MUST claim the bd issue:
  ```bash
  bd update tuxlink-zsm --claim
  python3 .claude/scripts/new_tuxlink_worktree.py --slug task-12-inbox-sent --issue tuxlink-zsm --moniker <your-moniker>
  cd worktrees/bd-tuxlink-zsm-task-12-inbox-sent
  ```
  The `bd-tuxlink-zsm/` branch prefix is mandatory per ADR 0008 worktree ownership.
- **Do NOT run destructive git commands.** No `git reset --hard`, `git push --force` / `-f` / `--force-with-lease`, `git checkout -- .`, `git restore .`, `git clean -f`, `git branch -D`, `git commit --amend` on pushed commits, `git rebase -i`, `git worktree remove`, `--no-verify`. The destructive-git hook enforces this; the right response to a denial is the non-destructive alternative, never a workaround.
- **Every commit MUST end with `Agent: <SESSION-MONIKER>` on its own line above the `Co-Authored-By:` trailer.** The literal token `<SESSION-MONIKER>` in commit heredocs is a placeholder — substitute the implementing session's actual moniker (generated via `python3 .claude/scripts/get_agent_moniker.py` at session start) before running each `git commit` block.
- **Commit cadence:** one commit per phase (at the end of each phase). No intra-phase WIP commits.
- **Branch model:** per ADR 0010 (no-squash-merge) + ADR 0004 (per-task branches). Task branch off `feat/v0.0.1` → commits land on the branch → PR against `feat/v0.0.1` → `gh pr merge --merge --delete-branch` (NOT `--squash`).
- **Live amateur radio network operations are licensee-only.** Nothing in Task 12 transmits — it only reads Pat's local HTTP API. If a subagent reasoning chain leads you to suggest "let's test against the real Pat connected to the real CMS to verify mailbox refresh," STOP. The verification is against a curl-able Pat instance the licensee starts, or against a mocked Pat HTTP server in unit tests. See `docs/live-cms-testing-policy.md` + RADIO-1.
- **If a hook denies a write op, route to a worktree per the deny message's QUICK FIX.** Do NOT take the lease, delete lease files, propose hook enhancements, or consult `get_tuxlink_sessions.py` to second-guess the deny. See pitfall HOOK-1.
- **Heredoc commit syntax mandatory.** The destructive-git hook substring-matches on commit text; the `-F file` route bypasses the discipline hook's `Agent:` trailer regex. Use heredoc (`git commit -m "$(cat <<'EOF' ... EOF)"`).
- **If the task description and this guardrail list disagree, the guardrail wins.** Surface the conflict before proceeding.

---

## Phase 0 — Test harness bootstrap (vitest + React Testing Library)

**Execution Status:** ⬜ NOT STARTED

**Why this phase exists:** The plan body's Task 12 test code (lines 3104-3128 of `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md`) imports `vitest` and `@testing-library/react`. Neither is in `package.json` as of the plan-writing audit (2026-05-18, confirmed via `grep -n "vitest\|testing-library" package.json`). The plan body silently assumes they're installed. Phase 0 makes that assumption real before any test gets written.

**Files:**
- Modify: `package.json` (add 5 devDependencies + `test` script)
- Create: `vitest.config.ts`
- Create: `src/setupTests.ts`
- Modify: `tsconfig.json` (add `vitest/globals` + `@testing-library/jest-dom` to `types`)

- [ ] **Step 0.1: Read the prerequisites**

Per Mandatory Per-Task Preamble above.

- [ ] **Step 0.2: Write the smoke test that proves the harness works**

Create `src/setupTests.ts`:

```ts
import '@testing-library/jest-dom/vitest';
```

Create `vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/setupTests.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    css: false,
  },
});
```

Create `src/__smoke__/harness.test.tsx` (this file is deleted at the end of Phase 0 — it exists only to prove the harness works):

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

describe('test harness smoke', () => {
  it('renders a React element and queries it via testing-library', () => {
    render(<button>hi</button>);
    expect(screen.getByRole('button', { name: 'hi' })).toBeInTheDocument();
  });
});
```

- [ ] **Step 0.3: Run to confirm failure (harness not installed)**

```bash
pnpm vitest run 2>&1 | head -20
```

Expected: command not found, OR module not found for `vitest`, OR `@testing-library/react` not found. **This is the failing state we want before installing.** Record the actual error in your TodoWrite list so Step 0.5 can confirm it changes.

- [ ] **Step 0.4: Add devDependencies + test script + tsconfig types**

Modify `package.json` `devDependencies`:

```json
"devDependencies": {
  "@tauri-apps/cli": "^2",
  "@testing-library/jest-dom": "^6.4.0",
  "@testing-library/react": "^16.0.0",
  "@testing-library/user-event": "^14.5.0",
  "@types/react": "^18.3.0",
  "@types/react-dom": "^18.3.0",
  "@vitejs/plugin-react": "^4.3.0",
  "jsdom": "^25.0.0",
  "typescript": "^5.4.0",
  "vite": "^5.3.0",
  "vitest": "^2.1.0"
}
```

Add to `package.json` `scripts`:

```json
"scripts": {
  "dev": "vite",
  "build": "tsc && vite build",
  "preview": "vite preview",
  "tauri": "tauri",
  "test": "vitest run",
  "test:watch": "vitest"
}
```

Modify `tsconfig.json`'s `compilerOptions` block. The current file (as of this plan's writing) has no `types` field — add it. The full block becomes:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

The `"vitest/globals"` entry pairs with `globals: true` in `vitest.config.ts` so `describe` / `it` / `expect` / `vi` are globally typed; explicit imports in test files still work and are NOT a conflict. The `@testing-library/jest-dom` entry pulls in the matcher augmentations (`.toBeInTheDocument()`, etc.) for type-checking.

Run install:

```bash
pnpm install
```

- [ ] **Step 0.5: Run the smoke test, confirm green**

```bash
pnpm vitest run src/__smoke__
```

Expected: 1 test passes; clean output (no jsdom deprecation warnings escaping; if warnings appear, fix the config — testing-pitfalls.md §1 forbids stray warnings).

- [ ] **Step 0.6: Remove the smoke test**

```bash
rm -r src/__smoke__
```

The smoke test served its purpose; Phase 2 will add real tests for the actual mailbox modules.

- [ ] **Step 0.7: Verify `pnpm vitest run` exits 0 with "no test files found" + tsc clean**

```bash
pnpm vitest run
pnpm tsc --noEmit
```

Expected: vitest exits 0 with a "No test files found" or equivalent (acceptable transient state until Phase 2 writes the first real test). If vitest exits non-zero on empty test discovery, configure `passWithNoTests: true` in `vitest.config.ts`. `pnpm tsc --noEmit` MUST be clean (no type errors anywhere — the `types` array addition to tsconfig is load-bearing for Phase 2's tests; if a stray `src/__smoke__/` reference lingers, tsc will catch it).

- [ ] **Step 0.8: Commit**

```bash
git add package.json pnpm-lock.yaml vitest.config.ts src/setupTests.ts tsconfig.json
git commit -m "$(cat <<'EOF'
chore(test): bootstrap vitest + React Testing Library harness

Adds vitest, @testing-library/react, @testing-library/jest-dom,
@testing-library/user-event, and jsdom as devDependencies. Adds
`pnpm test` (single-run) and `pnpm test:watch` scripts. Phase 0
of Task 12 — the plan body's test code at
docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md:3104-3128 assumes
these are installed; this commit makes that real.

No production code or schemas changed. The smoke test that
verified the harness was removed in the same phase; Phase 2 will
add the first real mailbox tests.

ANTI-PATTERN REVIEW: none — this commit only touches test
infrastructure; no UI surface introduced.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 0.9: Update Phase 0's Execution Status banner**

Flip the Phase 0 banner above to `✅ SHIPPED at <SHA> on <YYYY-MM-DD>` and update the Execution Status table at top of plan. Per Living Document Contract.

---

## Phase 1 — Backend wiring (`mailbox_list` command + `session.rs` + `Serialize`)

**Execution Status:** ⬜ NOT STARTED

**Why this phase exists:** The plan body's `mailbox_list` Tauri command (lines 3209-3224) cannot work as written because (a) `pat_client::Message` does NOT implement `Serialize`, so Tauri's `invoke_handler` cannot marshal `Result<Vec<Message>, String>` across IPC; (b) `crate::session::pat_http_url` does not exist yet; (c) the command must be registered in `invoke_handler!`. Phase 1 makes the backend callable BEFORE Phase 2 writes a frontend that depends on it.

**Files:**
- Modify: `src-tauri/src/pat_client.rs` (add `Serialize` derive to `Message`; add `unit` integration test for the round-trip JSON shape)
- Create: `src-tauri/src/session.rs` (Pat HTTP URL `OnceLock`)
- Modify: `src-tauri/src/lib.rs` (add `pub mod session`; add `#[tauri::command] pub async fn mailbox_list`; register in `invoke_handler!`)
- Modify: `src-tauri/tests/mailbox_list_integration.rs` (NEW — exercises the command via the underlying client with a mock Pat HTTP server)

- [ ] **Step 1.1: Inspect Pat's actual `/api/mailbox/<folder>` JSON response (5-minute investigation, NOT optional)**

Per "Open spec deltas" items #2 and #3 above, the implementer must confirm what fields Pat actually returns. Methodology:

```bash
# In one shell, with a Pat config already set up (Cameron's dev install OR
# a tuxlink Phase 0+ wizard-completed config), start Pat:
pat --listen 127.0.0.1:8088 http &

# In another shell, drop a known-shape test message into Pat's mailbox
# (Cameron or the operator does this manually, OR use the wizard's test-send
# from a prior session). Then:
curl -sS http://127.0.0.1:8088/api/mailbox/in | jq '.[0] | keys'
curl -sS http://127.0.0.1:8088/api/mailbox/in | jq '.[0]'
```

Record findings in this plan's "Discoveries" subsection at the top AND in `bd remember` so future tasks (especially Task 13's reading pane) can consume the same observed-Pat-shape without re-running the probe:

```bash
bd remember tuxlink-zsm "Pat /api/mailbox/<folder> response shape (probed YYYY-MM-DD): fields=[MID, Subject, From{Addr}, Date, Unread, BodySize, <add any new fields observed>]. To field present? <yes/no>. Attachment field present? <yes/no/N — present as <fieldname>>. Sample response in dev/scratch/pat-mailbox-probe-YYYY-MM-DD.json (gitignored)."
```

- Does Pat surface a `To` field? If yes: extend `PatMessageDto` in Phase 3 to deserialize it. If no: §5.5's `To` column ships empty + a follow-up issue is filed per "Open spec deltas" #2.
- Does Pat surface an attachment count or attachment list? If yes: extend `PatMessageDto`. If no: ship without the attachment-indicator column + follow-up issue per #3.

**If you cannot reach a running Pat instance** (no operator-set-up config exists yet because Tasks 9-11 haven't shipped at this stage of v0.0.1): SKIP the live probe, write the DTO to the current shape (no `To`, no attachments), and add a TODO note in this plan's "Discoveries" subsection: "Pat HTTP API field probe deferred to Phase 3 — no running Pat available at Phase 1 time."

This step does NOT transmit on RF. It only hits Pat's localhost HTTP API. RADIO-1 does not apply.

- [ ] **Step 1.2: Write the failing test for `mailbox_list` end-to-end (Rust integration)**

**Per-test-binary process isolation note.** Cargo runs each `tests/<name>.rs` file as its OWN binary (separate process). This matters because `session::PAT_URL` is a `OnceLock` (process-wide; cannot be reset). We split OnceLock-touching tests into their own files so each gets a fresh process:

- `src-tauri/tests/mailbox_list_integration.rs` — contract + happy-path + http-500 tests (NO session::set_pat_http_url calls)
- `src-tauri/tests/session_round_trip.rs` — ONLY session round-trip
- `src-tauri/tests/mailbox_list_unknown_folder.rs` — Step 1.9b's `mailbox_list_rejects_unknown_folder` test (touches OnceLock via the command path)
- `src-tauri/tests/mailbox_list_pat_not_running.rs` — Step 1.9b's `mailbox_list_surfaces_pat_not_running_when_url_unset` test (deliberately DOES NOT touch OnceLock)

Create `src-tauri/tests/mailbox_list_integration.rs`:

```rust
//! Integration test for the mailbox_list path via the underlying
//! PatClient. We do not invoke the Tauri command itself in tests
//! (Tauri commands need a tauri::App runtime); instead we exercise
//! the same logic the command delegates to: session::set_pat_http_url
//! + PatClient::list. This guarantees:
//!   * Message can be JSON-serialized (Tauri requires Serialize on
//!     command return values; the round-trip catches missing derives).
//!   * The session module's URL plumbing works as a OnceLock.
//!   * PatClient::list honors the folder param.

use mockito;
use serde_json;
use tuxlink_lib::pat_client::{MailboxFolder, Message, PatClient};
use tuxlink_lib::session;

#[test]
fn message_serializes_to_camelcase_json_for_tauri_ipc() {
    // Tauri's tauri::generate_handler marshals command return values
    // via serde_json. If Message doesn't derive Serialize, this test
    // fails to compile — that compile failure IS the kill-shot for
    // "the Tauri command would silently fail at runtime."
    let m = Message {
        mid: "ABC123".into(),
        subject: "Hello".into(),
        from: "K0SWE@winlink.org".into(),
        date: "2026-05-18T14:32:00Z".into(),
        unread: true,
        body_size: 120,
    };
    let json = serde_json::to_string(&m).expect("Message must serialize");
    // Frontend MessageList expects these field names (camelCase OR snake_case
    // depending on serde rename; the assertion below is the contract).
    // Pick a stable shape: snake_case to match Rust naming + the existing
    // PatMessageDto deserialization pattern. The frontend bridge in
    // useMailbox.ts maps body_size -> bodySize.
    assert!(json.contains("\"mid\":\"ABC123\""));
    assert!(json.contains("\"subject\":\"Hello\""));
    assert!(json.contains("\"from\":\"K0SWE@winlink.org\""));
    assert!(json.contains("\"body_size\":120"));
    assert!(json.contains("\"unread\":true"));
}

// NOTE: session::PAT_URL round-trip test lives in its OWN test binary
// at tests/session_round_trip.rs (see Step 1.2's per-test-binary note).
// Do NOT add a session::set_pat_http_url call to THIS file.

#[test]
fn pat_client_list_inbox_via_mock_returns_messages() {
    let mut server = mockito::Server::new();
    let body = r#"[
        {"MID":"M1","Subject":"Test 1","From":{"Addr":"K0SWE@winlink.org"},"Date":"2026-05-18T14:32:00Z","Unread":true,"BodySize":120},
        {"MID":"M2","Subject":"Test 2","From":{"Addr":"SERVICE@winlink.org"},"Date":"2026-05-18T15:00:00Z","Unread":false,"BodySize":400}
    ]"#;
    let _mock = server.mock("GET", "/api/mailbox/in")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();
    let client = PatClient::new(server.url());
    let msgs = client.list(MailboxFolder::Inbox).expect("list inbox");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].mid, "M1");
    assert_eq!(msgs[0].subject, "Test 1");
    assert_eq!(msgs[0].from, "K0SWE@winlink.org");
    assert!(msgs[0].unread);
    assert_eq!(msgs[0].body_size, 120);
    assert_eq!(msgs[1].mid, "M2");
    assert!(!msgs[1].unread);
}

#[test]
fn pat_client_list_propagates_http_500() {
    let mut server = mockito::Server::new();
    let _mock = server.mock("GET", "/api/mailbox/in")
        .with_status(500)
        .create();
    let client = PatClient::new(server.url());
    let res = client.list(MailboxFolder::Inbox);
    assert!(res.is_err(), "500 from Pat must surface as Err");
    match res.unwrap_err() {
        tuxlink_lib::pat_client::PatClientError::Status(500) => {}
        other => panic!("expected Status(500), got {other:?}"),
    }
}
```

- [ ] **Step 1.2b: Write the session round-trip test in its OWN test binary**

Create `src-tauri/tests/session_round_trip.rs` (single test in a dedicated binary; the comment block in `mailbox_list_integration.rs` references this file):

```rust
//! Isolated test binary: session::PAT_URL is a process-wide OnceLock
//! that cannot be reset. Co-locating other tests with this one would
//! make their assertions order-dependent. Keep this file single-test.

use tuxlink_lib::session;

#[test]
fn session_pat_http_url_round_trip() {
    let url = "http://127.0.0.1:18888".to_string();
    session::set_pat_http_url(url.clone());
    assert_eq!(session::pat_http_url().as_deref(), Some(url.as_str()));
}
```

- [ ] **Step 1.3: Run to confirm failure (Serialize missing; session module missing)**

```bash
cd src-tauri
cargo test --test mailbox_list_integration 2>&1 | head -40
```

Expected: compile error citing one or both of (a) `Message` does not implement `Serialize`, (b) `session` is not a recognized module. **If compile passes immediately, you skipped Phase 0 or the existing `Message` type already has `Serialize` — re-read `src-tauri/src/pat_client.rs` before continuing.**

- [ ] **Step 1.4: Add `Serialize` derive + create `session.rs`**

Modify `src-tauri/src/pat_client.rs` (only the `Message` struct's derive line — preserve all other code verbatim):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub mid: String,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
}
```

Create `src-tauri/src/session.rs`:

```rust
//! Session-scoped runtime state for tuxlink. v0.0.1: holds Pat's
//! HTTP base URL behind a OnceLock so Tauri commands can locate
//! Pat without retrofitting Task 3's process supervisor.
//!
//! v0.1+ replaces this with a full session manager that tracks
//! connection state, active callsign, and Pat lifecycle events.

use std::sync::OnceLock;

static PAT_URL: OnceLock<String> = OnceLock::new();

/// Set Pat's HTTP base URL. The first caller wins (OnceLock
/// semantics); subsequent calls are silently ignored. In v0.0.1
/// this is called from the wizard's test-send path
/// (Task 11 / wizard_commands.rs) and from main.rs's `.setup()`
/// when an existing wizard-completed config is loaded at app
/// start.
pub fn set_pat_http_url(url: String) {
    let _ = PAT_URL.set(url);
}

/// Get Pat's HTTP base URL, if it has been set this process
/// lifetime. Returns None before the first call to
/// set_pat_http_url; the mailbox_list command treats this as
/// "pat not running" and surfaces an error to the frontend.
pub fn pat_http_url() -> Option<String> {
    PAT_URL.get().cloned()
}
```

- [ ] **Step 1.5: Add the Tauri command in `lib.rs`**

Modify `src-tauri/src/lib.rs`:

```rust
pub mod config;
pub mod pat_client;
pub mod pat_process;
pub mod session;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn mailbox_list(folder: String) -> Result<Vec<crate::pat_client::Message>, String> {
    let url = crate::session::pat_http_url().ok_or_else(|| "pat not running".to_string())?;
    let client = crate::pat_client::PatClient::new(url);
    let pat_folder = match folder.as_str() {
        "inbox" => crate::pat_client::MailboxFolder::Inbox,
        "sent"  => crate::pat_client::MailboxFolder::Sent,
        other   => return Err(format!("unknown folder: {}", other)),
    };
    client.list(pat_folder).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, mailbox_list])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 1.6: Add `mockito` to `[dev-dependencies]` in `src-tauri/Cargo.toml` IF NOT ALREADY PRESENT**

Inspect `src-tauri/Cargo.toml` first:

```bash
grep -n 'mockito\|serde_json' src-tauri/Cargo.toml
```

If `mockito` is absent under `[dev-dependencies]`, add it:

```toml
[dev-dependencies]
mockito = "1.4"
serde_json = "1.0"  # only if also absent — likely already present transitively
```

Per the Subagent Guardrails, do NOT add any other deps.

- [ ] **Step 1.7: Run the tests, confirm green**

```bash
cd src-tauri
cargo test --test mailbox_list_integration --test session_round_trip 2>&1
```

Expected: 4 tests pass (3 from `mailbox_list_integration` + 1 from `session_round_trip`). Clean output (no compiler warnings in the new files — `cargo check` should be silent on the new code). Step 1.9b's two additional test binaries land later in this phase; Step 1.10 runs them all together.

**The per-test-binary split (Step 1.2's note) eliminates the OnceLock cross-test race that would otherwise occur** if all `session::*` and `mailbox_list::*` tests lived in one binary. If the implementer ever consolidates them back into one file "for simplicity," the OnceLock pollution returns and tests become order-dependent — DO NOT consolidate.

- [ ] **Step 1.8: Wire `session::set_pat_http_url` into the app-start path (light touch — investigate first)**

The goal: somewhere in the running app, after Pat is spawned, the new `session::set_pat_http_url(url)` call must fire so that `mailbox_list`'s `session::pat_http_url()` returns `Some(url)` instead of `None`. The exact wiring location depends on the current shape of `main.rs` + `wizard_commands.rs`, which may have evolved since this plan was written. **Investigate first, then add the minimum-viable call.**

Investigation:

```bash
grep -rn "pat_process::PatProcess::spawn\|PatSpawnOptions\|http_port" src-tauri/src/
```

Three cases:

- **Case A — `main.rs` already spawns Pat at startup** (Task 3 + a wizard-already-completed config-load shipped): add `session::set_pat_http_url(format!("http://127.0.0.1:{}", pat.http_port()));` immediately after the spawn site.
- **Case B — only `wizard_commands.rs` spawns Pat (during the wizard's test-send)**: add the same call in `wizard_commands.rs`'s spawn path. `main.rs` requires no change in this phase — the `Mailbox` will show `"pat not running"` until the operator completes the wizard, which is acceptable v0.0.1 behavior (wizard-completed is a precondition for the mailbox per Phase 2 Step 2.7's `wizardCompleted` gate).
- **Case C — neither spawns Pat at startup yet** (Tasks 3 + 11 partially landed; Pat is only spawned by the live-CMS smoke binary): the `set_pat_http_url` wiring has nowhere to live yet. RECORD this as a discovery in the plan's "Discoveries" subsection + surface to the dispatching agent. Phase 1 still ships (the command + session module are still useful skeletons); Phase 2 will surface a "pat not running" error in the frontend, which Phase 4 Step 4.1 will catch and trigger an escalation.

**Reference for `pat_process` shape (as of plan writing):** `pat_process::PatProcess::spawn(opts: PatSpawnOptions) -> std::io::Result<PatProcess>`; `PatProcess::http_port(&self) -> u16`. If the API has changed, adapt.

This step has NO unit test of its own (it's plumbing; the integration test in Step 1.2 covers `session` round-trip semantics; runtime wiring is exercised manually in Phase 4's smoke test).

- [ ] **Step 1.9: Confirm `cargo check` is clean + run ALL test binaries**

```bash
cd src-tauri
cargo check --all-targets 2>&1
cargo test --all-targets 2>&1
```

Expected: no warnings, no errors from `cargo check`. `cargo test --all-targets` runs ALL test binaries (mailbox_list_integration + session_round_trip + mailbox_list_unknown_folder + mailbox_list_pat_not_running + any others already in the project). Warnings count as failures here — testing-pitfalls.md §1 forbids stray warnings in test output, and the same discipline applies to compile output.

- [ ] **Step 1.9b: Add unit coverage for the `mailbox_list` command's error branches (TWO new test binaries)**

The `mailbox_list` Tauri command function is `pub async fn` in `lib.rs`. The `#[tauri::command]` macro WRAPS the function (it generates an additional `__cmd__mailbox_list` shim) but the underlying `pub async fn` remains directly callable from integration tests. Verify by `grep -n "pub async fn mailbox_list" src-tauri/src/lib.rs`.

Per the per-test-binary discipline established in Step 1.2's note, create TWO new test files (NOT one — they have incompatible OnceLock requirements):

Create `src-tauri/tests/mailbox_list_unknown_folder.rs`:

```rust
//! Tests the command's "unknown folder" error path. This file's process
//! sets session::PAT_URL early; that's fine because no other test in
//! this binary depends on PAT_URL being unset.

#[tokio::test]
async fn mailbox_list_rejects_unknown_folder() {
    tuxlink_lib::session::set_pat_http_url("http://127.0.0.1:19999".into());
    let err = tuxlink_lib::mailbox_list("drafts".into()).await.unwrap_err();
    assert!(err.contains("unknown folder"), "got: {err}");
    // Specifically: the error includes the rejected folder name for
    // operator debuggability.
    assert!(err.contains("drafts"), "got: {err}");
}
```

Create `src-tauri/tests/mailbox_list_pat_not_running.rs`:

```rust
//! Tests the command's "pat not running" error path. This file's
//! process MUST NOT call session::set_pat_http_url before the test —
//! the test relies on PAT_URL being None.

#[tokio::test]
async fn mailbox_list_surfaces_pat_not_running_when_url_unset() {
    // Sanity: PAT_URL should be None at the start of this binary's
    // process. If it isn't, something in the dependency tree is
    // pre-populating it — fail loudly so the implementer can
    // investigate, rather than vacuously asserting.
    assert!(tuxlink_lib::session::pat_http_url().is_none(),
        "PAT_URL pre-populated; this test binary needs PAT_URL=None at startup. Investigate: who is setting it?");
    let err = tuxlink_lib::mailbox_list("inbox".into()).await.unwrap_err();
    assert_eq!(err, "pat not running");
}
```

Add `tokio = { version = "1", features = ["macros", "rt"] }` to `[dev-dependencies]` in `src-tauri/Cargo.toml` IF NOT ALREADY PRESENT. Per the guardrails, do NOT add to `[dependencies]`.

**If `mailbox_list` is not directly callable from a test** (Tauri's macro made it opaque — unlikely but possible): mark this step as deferred + record in "Discoveries"; the integration test in Step 1.2 still covers the command's underlying logic via `PatClient::list`. A pure-Rust extract-the-logic-into-a-helper refactor is acceptable IF small (e.g., `fn folder_from_str(s: &str) -> Result<MailboxFolder, String>` becomes the testable surface; `mailbox_list` calls it).

- [ ] **Step 1.10: Commit**

```bash
# git add list is conditional based on Step 1.8 case:
#   Case A or B: include main.rs and/or wizard_commands.rs
#   Case C:      omit both (no wiring change this phase)
# The implementer trims the add list to match what actually changed.
# `git status --short` first to confirm — never `git add -A` per CLAUDE.md.
git add src-tauri/src/pat_client.rs src-tauri/src/session.rs src-tauri/src/lib.rs \
  src-tauri/tests/mailbox_list_integration.rs \
  src-tauri/tests/session_round_trip.rs \
  src-tauri/tests/mailbox_list_unknown_folder.rs \
  src-tauri/tests/mailbox_list_pat_not_running.rs \
  src-tauri/Cargo.toml src-tauri/Cargo.lock
# Conditionally (per Step 1.8's case determination):
# git add src-tauri/src/main.rs              # only if Case A modified it
# git add src-tauri/src/wizard_commands.rs   # only if Case B modified it
git commit -m "$(cat <<'EOF'
feat(mailbox): mailbox_list Tauri command + session URL plumbing

Adds Serialize to pat_client::Message so the Tauri IPC layer can
marshal it across the WebView boundary. Introduces session.rs
holding Pat's HTTP base URL behind a OnceLock; this is the
minimum viable session manager v0.0.1 needs (v0.1+ extends to
full lifecycle tracking). Registers mailbox_list in
invoke_handler! as the backend half of Task 12.

Tests: 6 integration tests split across 4 binaries (process-isolation
required by session::PAT_URL's OnceLock): (1) Message JSON shape Tauri
contract, (2) PatClient::list happy path vs mockito-backed Pat, (3)
HTTP 500 propagation, (4) session OnceLock round-trip [its own binary],
(5) mailbox_list rejects unknown folder [its own binary, PAT_URL set],
(6) mailbox_list surfaces "pat not running" when PAT_URL unset [its
own binary, PAT_URL deliberately untouched].

ANTI-PATTERN REVIEW: none — backend-only commit; no UI introduced
yet. The frontend that consumes mailbox_list ships in Phase 2.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 1.11: Update Phase 1's Execution Status banner** per Living Document Contract.

---

## Phase 2 — Frontend MVP (`MessageList` + `useMailbox` + `Mailbox` + `App.tsx` mount)

**Execution Status:** ⬜ NOT STARTED

**Why this phase exists:** This is the "Inbox/Sent tabbed view" the plan body, task name, and bd issue all describe. Phase 2 ships the minimal frontend that consumes Phase 1's `mailbox_list`, renders the two tabs, virtualizes the list, and mounts into `App.tsx` post-wizard. §5.5 deltas (unread-count badge, column model, persistence) land in Phase 3 — Phase 2 ships a usable mailbox first to keep the commit small and reviewable.

**Files:**
- Create: `src/mailbox/Mailbox.tsx`
- Create: `src/mailbox/MessageList.tsx`
- Create: `src/mailbox/useMailbox.ts`
- Create: `src/mailbox/mailbox.test.tsx`
- Modify: `src/App.tsx` (mount Mailbox after wizard completes; wrap in `QueryClientProvider`)
- Modify: `src/App.css` (add `.mailbox-tabs`, `.mailbox-pane`, `.mailbox-row`, `.mailbox-empty`, `.mailbox-row.unread`, `.mailbox-row.selected` CSS — match `mock-b-principles-faithful.png` palette)

- [ ] **Step 2.1: Read the prerequisites** per Mandatory Per-Task Preamble.

- [ ] **Step 2.2: Write the failing tests for `MessageList`**

Create `src/mailbox/mailbox.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MessageList, type MessageSummary } from './MessageList';

const fixtures: MessageSummary[] = [
  { mid: 'A', subject: 'Hello', from: 'K0SWE@winlink.org',     date: '2026-05-18T14:32:00Z', unread: true,  bodySize: 120 },
  { mid: 'B', subject: 'Welfare check', from: 'SERVICE@winlink.org', date: '2026-05-18T15:00:00Z', unread: false, bodySize: 4096 },
];

describe('MessageList', () => {
  it('renders each message row with subject, from, and human-formatted size', () => {
    render(<MessageList messages={fixtures} selectedMid={null} onSelect={() => {}} />);
    expect(screen.getByText('Hello')).toBeInTheDocument();
    expect(screen.getByText('Welfare check')).toBeInTheDocument();
    expect(screen.getByText('K0SWE@winlink.org')).toBeInTheDocument();
    expect(screen.getByText('SERVICE@winlink.org')).toBeInTheDocument();
    expect(screen.getByText('120 B')).toBeInTheDocument();
    expect(screen.getByText('4.0 KB')).toBeInTheDocument();
  });

  it('renders empty state with the discoverable next-action hint when messages is empty', () => {
    render(<MessageList messages={[]} selectedMid={null} onSelect={() => {}} />);
    // Per docs/ux-anti-patterns.md "discoverable next actions" — the empty
    // state must point the operator at HOW to populate the inbox.
    expect(screen.getByText(/No messages yet/i)).toBeInTheDocument();
    expect(screen.getByText(/F5|Session.*Connect/i)).toBeInTheDocument();
  });

  it('marks the selected row with aria-selected=true and onSelect is called on click', async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(<MessageList messages={fixtures} selectedMid="A" onSelect={onSelect} />);
    const rows = screen.getAllByRole('row');
    // The "A" row is selected per props; the "B" row is not.
    const selectedRow = rows.find(r => r.getAttribute('aria-selected') === 'true');
    expect(selectedRow).toBeTruthy();
    expect(selectedRow).toHaveTextContent('Hello');

    // Click the "B" row's container — fires onSelect('B').
    await user.click(rows.find(r => r.textContent?.includes('Welfare check'))!);
    expect(onSelect).toHaveBeenCalledWith('B');
  });

  it('formats sizes with the documented thresholds (boundary check)', () => {
    const sizeCases: MessageSummary[] = [
      { mid: 's1', subject: '0 B',          from: 'x', date: 'd', unread: false, bodySize: 0 },
      { mid: 's2', subject: '1023 B',       from: 'x', date: 'd', unread: false, bodySize: 1023 },
      { mid: 's3', subject: 'exactly 1 KB', from: 'x', date: 'd', unread: false, bodySize: 1024 },
      { mid: 's4', subject: '1 MB',         from: 'x', date: 'd', unread: false, bodySize: 1024 * 1024 },
    ];
    render(<MessageList messages={sizeCases} selectedMid={null} onSelect={() => {}} />);
    expect(screen.getByText('0 B')).toBeInTheDocument();
    expect(screen.getByText('1023 B')).toBeInTheDocument();
    expect(screen.getByText('1.0 KB')).toBeInTheDocument();
    expect(screen.getByText('1.0 MB')).toBeInTheDocument();
  });
});
```

The Phase 2 tests above cover MessageList only. Mailbox-level tests (which exercise `useMailbox` + the Tauri-invoke boundary) need their own `vi.mock` setup that Phase 3 introduces. To add the Phase-2 Mailbox tests **now** (preferred — keeps the error-path coverage in the same phase), include the file-top `vi.mock('@tauri-apps/api/core')` block and a second `describe('Mailbox MVP')` block in the same file:

```tsx
import { invoke } from '@tauri-apps/api/core';
import { Mailbox } from './Mailbox';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Mock } from 'vitest';

// Top-level: vitest hoists this. Each test sets invoke's behavior.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('Mailbox MVP', () => {
  beforeEach(() => { (invoke as unknown as Mock).mockReset(); });

  it('renders an error state when mailbox_list rejects', async () => {
    (invoke as unknown as Mock).mockImplementation(async (_cmd: string, args: { folder: string }) => {
      if (args.folder === 'inbox') throw new Error('pat not running');
      return [];
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={qc}><Mailbox onMessageSelect={() => {}} /></QueryClientProvider>);
    // The Inbox pane's role="alert" appears once the query rejects.
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(/pat not running/);
  });
});
```
```

- [ ] **Step 2.3: Run to confirm failure (module not found)**

```bash
pnpm vitest run src/mailbox
```

Expected: 4 tests, all FAIL because `./MessageList` does not exist yet. Confirm the failure mode is "Cannot find module" — if it's anything else, the harness from Phase 0 has a problem.

- [ ] **Step 2.4: Implement `MessageList.tsx`**

Create `src/mailbox/MessageList.tsx`:

```tsx
import { Virtuoso } from 'react-virtuoso';

export interface MessageSummary {
  mid: string;
  subject: string;
  from: string;
  date: string;       // ISO-8601 UTC from Pat
  unread: boolean;
  bodySize: number;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function MessageList({
  messages, selectedMid, onSelect,
}: {
  messages: MessageSummary[];
  selectedMid: string | null;
  onSelect: (mid: string) => void;
}) {
  if (messages.length === 0) {
    return (
      <div className="mailbox-empty" role="status">
        No messages yet. Press F5 or click Session → Connect to check for new mail.
      </div>
    );
  }
  return (
    <Virtuoso
      style={{ height: '100%' }}
      data={messages}
      itemContent={(_index, msg) => (
        <div
          role="row"
          aria-selected={msg.mid === selectedMid}
          className={[
            'mailbox-row',
            msg.unread ? 'unread' : '',
            msg.mid === selectedMid ? 'selected' : '',
          ].filter(Boolean).join(' ')}
          onClick={() => onSelect(msg.mid)}
        >
          <span className="mailbox-subject">{msg.subject}</span>
          <span className="mailbox-from">{msg.from}</span>
          <span className="mailbox-date">{msg.date}</span>
          <span className="mailbox-size">{formatSize(msg.bodySize)}</span>
        </div>
      )}
    />
  );
}
```

- [ ] **Step 2.5: Implement `useMailbox.ts`**

Create `src/mailbox/useMailbox.ts`:

```ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { MessageSummary } from './MessageList';

/**
 * Pat returns Message with snake_case `body_size`; the Tauri command
 * marshals it as-is (Phase 1's mailbox_list_integration test pins the
 * shape). This hook adapts to the frontend's camelCase MessageSummary.
 */
interface RustMessage {
  mid: string;
  subject: string;
  from: string;
  date: string;
  unread: boolean;
  body_size: number;
}

function adapt(m: RustMessage): MessageSummary {
  return {
    mid: m.mid, subject: m.subject, from: m.from,
    date: m.date, unread: m.unread, bodySize: m.body_size,
  };
}

export function useMailbox(folder: 'inbox' | 'sent') {
  return useQuery<MessageSummary[]>({
    queryKey: ['mailbox', folder],
    queryFn: async () => {
      const raw = await invoke<RustMessage[]>('mailbox_list', { folder });
      return raw.map(adapt);
    },
    refetchInterval: 10_000,
  });
}
```

- [ ] **Step 2.6: Implement `Mailbox.tsx`**

Create `src/mailbox/Mailbox.tsx`:

```tsx
import * as Tabs from '@radix-ui/react-tabs';
import { useState } from 'react';
import { useMailbox } from './useMailbox';
import { MessageList } from './MessageList';

/**
 * Inbox / Sent tabbed view. Phase 2 ships the two-tab MVP per the
 * plan body. Phase 3 adds the §5.5 design deltas (unread-count
 * badge, column model).
 *
 * Anti-pattern note (docs/ux-anti-patterns.md): selecting a row
 * does NOT swap the entire view — it calls onMessageSelect, which
 * Task 13 will wire to a reading pane sibling. Tabs change the
 * mailbox-list content but the surrounding chrome stays put.
 */
export function Mailbox({ onMessageSelect }: { onMessageSelect: (mid: string) => void }) {
  const [selectedMid, setSelectedMid] = useState<string | null>(null);
  const inbox = useMailbox('inbox');
  const sent = useMailbox('sent');

  return (
    <Tabs.Root defaultValue="inbox" className="mailbox-tabs">
      <Tabs.List aria-label="Mailbox folders">
        <Tabs.Trigger value="inbox">
          Inbox {inbox.data ? `(${inbox.data.length})` : ''}
        </Tabs.Trigger>
        <Tabs.Trigger value="sent">
          Sent {sent.data ? `(${sent.data.length})` : ''}
        </Tabs.Trigger>
      </Tabs.List>

      <Tabs.Content value="inbox" className="mailbox-pane">
        {inbox.isLoading && <div role="status">Loading…</div>}
        {inbox.isError && <div role="alert">Error: {String(inbox.error)}</div>}
        {inbox.data && (
          <MessageList
            messages={inbox.data}
            selectedMid={selectedMid}
            onSelect={(mid) => { setSelectedMid(mid); onMessageSelect(mid); }}
          />
        )}
      </Tabs.Content>

      <Tabs.Content value="sent" className="mailbox-pane">
        {sent.isLoading && <div role="status">Loading…</div>}
        {sent.isError && <div role="alert">Error: {String(sent.error)}</div>}
        {sent.data && (
          <MessageList
            messages={sent.data}
            selectedMid={selectedMid}
            onSelect={(mid) => { setSelectedMid(mid); onMessageSelect(mid); }}
          />
        )}
      </Tabs.Content>
    </Tabs.Root>
  );
}
```

- [ ] **Step 2.7: Mount `Mailbox` in `App.tsx` — INSPECT FIRST**

Modify `src/App.tsx`. **The current `App.tsx` is the Tauri scaffold placeholder as of the plan-writing audit (2026-05-18, see `src/App.tsx` shipping the "Welcome to Tauri + React" greet form). Inspect it FIRST.** Two cases:

```bash
grep -n "Wizard\|wizard\|QueryClientProvider\|config_wizard_completed" src/App.tsx
```

**Case A — Tasks 9-11 already shipped (wizard mount + `config_wizard_completed` command exist):** Task 12's diff to `App.tsx` is ADDITIVE only — wrap the existing wizard/main branch in `<QueryClientProvider>` (if not already wrapped) + add the `<Mailbox>` mount in the wizard-completed branch. Do NOT alter the wizard mount or its routing.

**Case B — Tasks 9-11 have NOT shipped yet (App.tsx is still the scaffold):** Task 12 STILL ships, but the wizard branch is a placeholder. The Tauri command `config_wizard_completed` may not exist; the implementer either:
  - Adds a minimal stub Tauri command `#[tauri::command] fn config_wizard_completed() -> bool { false }` in `lib.rs` (Tasks 9-11 will replace it), OR
  - Reads the wizard-completed flag directly from the config file via the existing `config::Config::load` path (preferred — uses Task 2's surface as-shipped). The exact call depends on `config.rs`'s current API; investigate via `grep -n "pub fn load\|wizard_completed" src-tauri/src/config.rs`.

The reference shape below assumes Case A. The implementer adapts to Case B if needed and RECORDS the adaptation in the plan's "Discoveries" subsection.

Reference shape:

```tsx
import { useState, useEffect } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { Mailbox } from './mailbox/Mailbox';
// import { Wizard } from './wizard/Wizard';  // <— shipped by Tasks 9-11
import './App.css';

const qc = new QueryClient({
  defaultOptions: {
    queries: {
      // 10s refetch matches useMailbox's interval; retry once on error
      // to absorb transient Pat hiccups during startup.
      retry: 1,
    },
  },
});

function App() {
  const [wizardCompleted, setWizardCompleted] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>('config_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, []);

  if (wizardCompleted === null) {
    return <main className="container"><p>Loading…</p></main>;
  }

  return (
    <QueryClientProvider client={qc}>
      <main className="container">
        {wizardCompleted ? (
          <Mailbox onMessageSelect={(_mid: string) => { /* Task 13 wires reading pane — signature: (mid: string) => void */ }} />
        ) : (
          // <Wizard onComplete={() => setWizardCompleted(true)} />
          <div>Wizard mount goes here — see Tasks 9-11. Task 12 leaves this branch as-is if Tasks 9-11 already shipped.</div>
        )}
      </main>
    </QueryClientProvider>
  );
}

export default App;
```

**Boundary clarification — do NOT alter the wizard mount.** If Tasks 9-11 have already shipped, the current `App.tsx` has its own wizard-vs-main-UI shape. Task 12's diff to `App.tsx` is ADDITIVE only: add the `QueryClientProvider` wrapper + the `<Mailbox>` mount in the wizard-completed branch. If the current `App.tsx` already has a `QueryClientProvider` (e.g., Tasks 9-11 added one for their own queries), reuse it — do NOT nest two providers.

If Tasks 9-11 have NOT shipped at execution time of Task 12, the `config_wizard_completed` Tauri command may not exist. In that case the implementer either (a) stubs the command in `src-tauri/src/lib.rs` returning `Ok(false)` to keep the wizard branch alive (Tasks 9-11 will replace it), or (b) gates Task 12 on Tasks 9-11 (preferred — record as a discovery + escalate).

- [ ] **Step 2.8: Add the mailbox CSS**

Modify `src/App.css`. Append (do NOT replace existing CSS):

```css
/* Task 12 — mailbox styles. Palette per docs/design/mockups/synthesis-* */

.mailbox-tabs {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.mailbox-tabs > [role="tablist"] {
  display: flex;
  gap: 0;
  border-bottom: 1px solid #2a2f3a;
  padding: 0 0.5rem;
}

.mailbox-tabs [role="tab"] {
  padding: 0.5rem 1rem;
  background: transparent;
  border: none;
  color: #cdd6f4;
  cursor: pointer;
  font-size: 0.95rem;
  font-weight: 500;
  border-bottom: 2px solid transparent;
}

.mailbox-tabs [role="tab"][data-state="active"] {
  border-bottom-color: #89b4fa;
  color: #fff;
}

.mailbox-pane {
  flex: 1;
  overflow: hidden;
}

.mailbox-empty {
  padding: 2rem;
  color: #7f849c;
  text-align: center;
}

.mailbox-row {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(0, 2fr) auto 4rem;
  gap: 0.75rem;
  padding: 0.5rem 1rem;
  cursor: pointer;
  border-bottom: 1px solid #1e2030;
  color: #cdd6f4;
}

.mailbox-row.unread {
  font-weight: 600;
  color: #fff;
}

.mailbox-row.selected {
  background: #313244;
}

.mailbox-row:hover:not(.selected) {
  background: #1e2030;
}

.mailbox-subject, .mailbox-from {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mailbox-date, .mailbox-size {
  color: #7f849c;
  font-size: 0.875rem;
  white-space: nowrap;
}
```

**Do NOT add a separate CSS module file** (the existing project uses a single `App.css`; Task 12 follows the established pattern). If `App.css` has grown unwieldy at this point in v0.0.1's life, that's a refactor for a separate PR — not Task 12.

- [ ] **Step 2.9: Run the tests, confirm green + clean output**

```bash
pnpm vitest run src/mailbox
```

Expected: 4 tests pass. Output should be free of React act() warnings, jsdom CSS warnings, or untriggered query warnings. If any warnings appear, fix them — testing-pitfalls.md §1 forbids stray warnings in passing tests.

**If a test races** (e.g., a Virtuoso async render not flushing by assertion time): use `await screen.findByText(...)` instead of `screen.getByText(...)`. Do NOT add `setTimeout` or `vi.advanceTimersByTime` workarounds without a clear race-cause analysis. Per the completion check rule, fix synchronization, not the assertion.

- [ ] **Step 2.10: Commit**

```bash
git add src/mailbox/ src/App.tsx src/App.css
git commit -m "$(cat <<'EOF'
feat(mailbox): inbox/sent tabbed view (MVP)

Two-tab Radix Tabs mailbox view, react-virtuoso row rendering,
TanStack Query 10s auto-refetch, snake_case→camelCase adapter
between Rust IPC and React props. Mounts post-wizard in
App.tsx; preserves the wizard-routing branch verbatim.

Empty state hints at F5 / Session → Connect (discoverable
next actions per docs/ux-anti-patterns.md). Row click fires
onMessageSelect — Task 13 wires the reading pane to this
callback.

Tests: 4 vitest cases covering happy-path render, empty state,
row selection ARIA + click handler, size-formatter boundaries
(0 B, 1023 B, exact 1 KB, exact 1 MB).

ANTI-PATTERN REVIEW: discoverable empty state ✓; no
full-view-swap (selection updates pane content, not the chrome)
✓; familiar Express terminology — Inbox / Sent ✓; no hamburger
menus / drawers introduced ✓.

Phase 3 follow-up: §5.5 design deltas (unread-count badge,
column model expansion, column-width persistence).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 2.11: Update Phase 2's Execution Status banner** per Living Document Contract.

---

## Phase 3 — §5.5 design deltas (unread-count badge, column model, no-view-swap reinforcement)

**Execution Status:** ⬜ NOT STARTED

**Gate before starting:** Re-read the "Open spec deltas" section at the top of this plan. Items #2-4 are investigative-then-decide per Phase 1 Step 1.1; item #1 (folder sidebar) requires Cameron's decision before scope is locked. If item #1 is still open, do NOT add a folder sidebar in this phase — ship Phase 3 with the in-scope deltas (unread badge, columns, persistence) and file the folder-sidebar follow-up issue per the plan's "Open spec deltas" guidance.

**Why this phase exists:** The plan body's Task 12 description silently elides the §5.5 deltas. Per the propagation contract (CLAUDE.md §"Documentation propagation contract"), the design doc is the canonical source for UX-level shape; the plan body's narrower description does not override it. Phase 3 reconciles the implementation with the design doc.

**Files:**
- Modify: `src/mailbox/Mailbox.tsx` (unread-count badge on Inbox tab)
- Modify: `src/mailbox/MessageList.tsx` (column model expansion — add columns conditionally based on what Pat returns; see Phase 1 Step 1.1 findings)
- Modify: `src/mailbox/mailbox.test.tsx` (new tests for unread-count + new columns)
- Modify: `src-tauri/src/config.rs` (add mailbox-column-widths field if Phase 1 Step 1.1's investigation confirmed the column set; defer if config schema bump is non-trivial — record decision in plan)
- Modify: `src-tauri/src/pat_client.rs` (extend `PatMessageDto` with `To` and/or attachment fields IF Phase 1 Step 1.1's curl probe confirmed Pat surfaces them)

- [ ] **Step 3.1: Confirm Phase 1 Step 1.1's investigation findings** before scoping this phase. Read this plan's "Discoveries" subsection. Scope the column expansion to ONLY fields Pat actually surfaces.

- [ ] **Step 3.2: Write the failing test for unread-count badge**

Two important vitest semantics for this step:

1. `vi.mock(modulePath, factory)` is **hoisted by vitest to the top of the file** at transform time — it CANNOT live inside a function or a `beforeEach`. The mock factory itself MUST be self-contained (it cannot close over outer variables, because the hoist happens before any outer-scope code runs).
2. To switch the mocked `invoke`'s behavior between tests, mock the module at the top of the file with a `vi.fn()` placeholder, then `import { invoke } from '@tauri-apps/api/core'` (which now returns the mock) and use `(invoke as Mock).mockImplementation(...)` inside each test.

Append to `src/mailbox/mailbox.test.tsx` (file-level — the `vi.mock` goes at the TOP of the file, not inside the new `describe` block):

At the top of `mailbox.test.tsx`, ADD (next to the existing imports):

```tsx
import { invoke } from '@tauri-apps/api/core';
import { Mailbox } from './Mailbox';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Mock } from 'vitest';

// Top-level mock — vitest hoists this above all imports at transform
// time. Use vi.fn() as a placeholder; each test sets the behavior via
// (invoke as Mock).mockImplementation(...).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
```

Then APPEND the new `describe` block at the end of the file:

```tsx
function setMailboxFixtures(opts: { inbox: MessageSummary[]; sent: MessageSummary[] }) {
  (invoke as unknown as Mock).mockImplementation(async (cmd: string, args: { folder: string }) => {
    if (cmd !== 'mailbox_list') throw new Error(`unexpected cmd: ${cmd}`);
    const src = args.folder === 'inbox' ? opts.inbox : opts.sent;
    return src.map(m => ({
      mid: m.mid, subject: m.subject, from: m.from,
      date: m.date, unread: m.unread, body_size: m.bodySize,
    }));
  });
}

function renderMailbox() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}><Mailbox onMessageSelect={() => {}} /></QueryClientProvider>);
}

describe('Mailbox', () => {
  beforeEach(() => { (invoke as unknown as Mock).mockReset(); });

  it('shows unread count (not total) on the Inbox tab — per design doc §5.5', async () => {
    // 3 messages in inbox, 2 unread (mid C is read).
    setMailboxFixtures({
      inbox: [
        { mid: 'A', subject: 'a', from: 'x', date: 'd', unread: true,  bodySize: 0 },
        { mid: 'B', subject: 'b', from: 'x', date: 'd', unread: true,  bodySize: 0 },
        { mid: 'C', subject: 'c', from: 'x', date: 'd', unread: false, bodySize: 0 },
      ],
      sent: [],
    });
    renderMailbox();
    // Tab label must show "(2)" — the unread count — not "(3)" the total.
    // Design doc §5.5: "Inbox (unread count, not total — per page_10.html)".
    // findByRole waits for the async useMailbox query to resolve.
    expect(await screen.findByRole('tab', { name: /Inbox.*\(2\)/ })).toBeInTheDocument();
  });

  it('suppresses the badge when Inbox has 0 unread', async () => {
    setMailboxFixtures({
      inbox: [{ mid: 'A', subject: 'a', from: 'x', date: 'd', unread: false, bodySize: 0 }],
      sent: [],
    });
    renderMailbox();
    // Wait for the query to resolve (the row renders) — then assert the
    // tab label has no parenthesized count.
    await screen.findByText('a');
    const tab = screen.getByRole('tab', { name: /^Inbox\s*$/ });
    expect(tab).toBeInTheDocument();
  });

  it('suppresses the badge when Sent has 0 messages', async () => {
    setMailboxFixtures({ inbox: [], sent: [] });
    renderMailbox();
    // Radix Tabs.Content only mounts the ACTIVE pane (defaultValue="inbox").
    // We can still assert the Sent TAB label because the tab triggers are
    // always in the DOM (only content is conditionally mounted). Wait for
    // Inbox's empty state to confirm queries have resolved, then assert
    // the Sent tab's text shape.
    await screen.findByText(/No messages yet/i);  // Inbox pane visible + Sent query also resolved (useQuery fires for both regardless of active tab)
    const tab = screen.getByRole('tab', { name: /^Sent\s*$/ });
    expect(tab).toBeInTheDocument();
  });
});
```

Also add `beforeEach` and `Mock` to the file-top imports if not already present.

- [ ] **Step 3.3: Run to confirm failure** (`pnpm vitest run src/mailbox` — expect the unread-count test to fail because Phase 2 ships `inbox.data.length`).

- [ ] **Step 3.4: Implement the unread-count badge**

Modify `src/mailbox/Mailbox.tsx`:

```tsx
const inboxUnreadCount = inbox.data?.filter(m => m.unread).length;
// ...
<Tabs.Trigger value="inbox">
  Inbox {inboxUnreadCount !== undefined && inboxUnreadCount > 0 ? `(${inboxUnreadCount})` : ''}
</Tabs.Trigger>
<Tabs.Trigger value="sent">
  Sent {sent.data && sent.data.length > 0 ? `(${sent.data.length})` : ''}
</Tabs.Trigger>
```

Rationale for the conditional rendering: when Inbox has 0 unread (everything is read) OR Sent has 0 total, suppress the badge — a "(0)" badge is visual noise that fights the "minimal Mail.app style" anti-patterns commitment. Sent still uses total count (not "unread") because Sent doesn't have an unread/read distinction in the operator's mental model.

- [ ] **Step 3.5: Re-run tests, confirm green**

```bash
pnpm vitest run src/mailbox
```

Expected: 5 tests pass (4 Phase 2 + 1 new).

- [ ] **Step 3.6: Column model expansion — conditional on Phase 1 Step 1.1 findings**

**Two paths depending on what Pat surfaces:**

**Path A — Pat exposes `To` and/or attachment-count fields:** extend `PatMessageDto` to deserialize them; add fields to `Message` (Rust) and `RustMessage` / `MessageSummary` (TypeScript); update `MessageList.tsx`'s row template to render the new columns. Write tests for each new column following the same shape as the size-formatter test in Phase 2. **Update Phase 1's `message_serializes_to_camelcase_json_for_tauri_ipc` integration test** to include the new fields in its substring assertions — otherwise Phase 1's contract test still asserts the Phase-1 field set and the contract will silently drift.

**Path B — Pat does NOT expose those fields:** ship the column model in `MessageList.tsx` with empty/"—" placeholders for `To` and attachments; update the row's CSS grid template to reserve the columns. File follow-up bd issues per "Open spec deltas" #2 + #3. Document the choice in this plan's "Discoveries" subsection.

The implementer picks the path based on Phase 1's findings. Either way, the row CSS grid in `App.css` updates:

```css
.mailbox-row {
  display: grid;
  /* Per design doc §5.5: UTC time · From · To · Subject · # · Compressed-size.
     Express ratios from `Column widths=20;21;100;100;45;60;100;100;900`
     reduced to 6 columns for v0.0.1. */
  grid-template-columns: 7rem 12rem 12rem minmax(0, 1fr) 2rem 5rem;
  /* ... rest unchanged ... */
}
```

- [ ] **Step 3.7: Column-width persistence — conditional on `config.rs` extensibility**

Inspect `src-tauri/src/config.rs` first:

```bash
grep -n "pub struct\|pub mod\|UiState\|column" src-tauri/src/config.rs
```

If extending `config.rs` is additive (e.g., a new optional `ui_state: Option<UiStateConfig>` field with `#[serde(default)]`), implement it: add the struct, add a Tauri command (`config_set_mailbox_column_widths`), add a Phase-3 integration test for round-trip. If it requires a schema-version bump, file a `bd create` issue titled "Task 12 follow-up: persist mailbox column widths to config" and SKIP persistence in this phase — Phase 3 ships with column widths in `localStorage` instead (a Phase-3-internal fallback acceptable for v0.0.1):

```ts
// Inside Mailbox.tsx (or a sibling useColumnWidths.ts hook):
const [widths, setWidths] = useState<number[]>(() => {
  const raw = localStorage.getItem('mailbox-column-widths');
  return raw ? JSON.parse(raw) : [112, 192, 192, /* fr-track*/ 0, 32, 80];
});
useEffect(() => { localStorage.setItem('mailbox-column-widths', JSON.stringify(widths)); }, [widths]);
```

**Record the choice** in this plan's "Discoveries" subsection: `config.rs` extension vs `localStorage` fallback, with a one-line reason.

- [ ] **Step 3.8: Reinforce no-full-view-swap discipline (review-only step — no code change unless a regression is found)**

Re-read `docs/ux-anti-patterns.md` line 169-171 ("NO 'single-page app' layouts where the whole UI swaps on a click"). Walk through the Mailbox component manually: does row-click change ONLY the reading-pane state (via `onMessageSelect`), or does anything else in the chrome update? Tab switching is allowed (per Radix Tabs semantics); row-click swapping the entire surface is NOT. If a regression is found, fix it in this step before committing.

- [ ] **Step 3.9: Run all tests, confirm green + clean output**

```bash
pnpm vitest run
cd src-tauri && cargo test --all-targets
```

Expected: all green; no warnings; no shared-state flakes.

- [ ] **Step 3.10: Commit**

```bash
git add src/mailbox/ src/App.css src-tauri/src/pat_client.rs src-tauri/src/config.rs src-tauri/tests/
git commit -m "$(cat <<'EOF'
feat(mailbox): §5.5 design deltas — unread badge, column model

Per docs/design/v0.0.1-ux-mockups.md §5.5:
- Inbox tab badge shows UNREAD count (not total), suppressed at 0
- Sent tab badge shows total count, suppressed at 0
- Column model expanded to UTC · From · To · Subject · # · Size
  (To and # rendered as available from Pat's /api/mailbox/* —
  see "Discoveries" subsection for what Pat actually surfaces)
- Column widths persisted via <localStorage | config.rs> per
  the implementer's recorded choice (Phase 1 Step 1.1 probe)
- No-full-view-swap discipline verified manually per
  ux-anti-patterns.md line 169

Folder sidebar (Outbox / Drafts / Deleted / Templates from §5.5)
deferred to follow-up bd issue per "Open spec deltas" #1 of this
plan, pending Cameron's decision on Drafts and Deleted backing
stores.

ANTI-PATTERN REVIEW: empty count badges suppressed (no "(0)"
visual noise — Mail.app minimal compliance); no view swap on row
click; no in-content toolbar duplicating menu items.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3.11: Update Phase 3's Execution Status banner** per Living Document Contract.

---

## Phase 4 — Review & ship (3-round adversarial impl review, anti-pattern check, PR)

**Execution Status:** ⬜ NOT STARTED

**Why this phase exists:** Per CLAUDE.md §"Brainstorming preferences" + the build-robust-features pattern, "browser smoke before UI ship" is non-negotiable. Phase 4 is the verification gate before PR — and per the Mandatory Per-Task Completion Check, "minimum three review rounds" applies at logical group boundaries.

**Files:** no new code in this phase; review-only + PR opening.

- [ ] **Step 4.1: Manual smoke walk in `pnpm tauri dev`**

```bash
pnpm tauri dev
```

Walk the following user flow (record screenshots in `/tmp/task-12-smoke-<timestamp>/` for the PR body):

1. App launches → wizard mounts (if Tasks 9-11 shipped) OR mailbox placeholder copy renders (if not).
2. (If wizard-shipped) complete the wizard with Cameron-supplied test credentials.
3. Mailbox mounts; Inbox tab is active by default.
4. Inbox tab badge shows the unread count IF any unread messages exist; otherwise no badge.
5. Click a row → `onMessageSelect` fires (verify via React DevTools or a temporary `console.log`).
6. Switch to Sent tab → Sent messages render with the total badge.
7. Switch back to Inbox → tab content swaps but app chrome stays put (no-view-swap discipline check).
8. Empty state for a folder with 0 messages shows the F5 hint.
9. Stop Pat externally (find the Pat PID via `cat $XDG_RUNTIME_DIR/tuxlink/tuxlink-pat.pid` or `pgrep -f 'pat.*http'`; then `kill <pid>`. AVOID `pkill pat` — too broad, may match unrelated processes named "pat"). Wait 10s → Inbox tab shows error state (`role="alert"`).
10. Restart Pat (either by restarting `pnpm tauri dev`, or by re-running the wizard's test-send which spawns Pat) → wait 10s → error clears, data returns.

If any step misbehaves, the corresponding test in `mailbox.test.tsx` is missing — add it, fix the bug, return to Step 4.1.

- [ ] **Step 4.2: Codex adversarial review on the cumulative diff**

Per CLAUDE.md §"Extended capabilities" — Codex CLI is the project's "at least one adversarial round" tool. From the worktree:

```bash
npx --yes @openai/codex review --base feat/v0.0.1 \
  "Review this Task 12 implementation for: (a) Tauri command shape correctness (Serialize, IPC marshaling), (b) React Query stale-state hazards under tab-switch + tab-blur, (c) Virtuoso virtualization correctness when messages array changes during a refetch, (d) accessibility regressions (Radix Tabs ARIA + row aria-selected), (e) test rigor — do the assertions PROVE the behaviors, or are they symptom-shaped? Where assertions are weak, propose stronger ones."
```

Write the transcript to `dev/adversarial/2026-05-NN-task-12-codex.md` (gitignored per CLAUDE.md). Address findings inline; if any finding requires re-architecting Phase 2 or 3, file a discovery entry + return to that phase rather than papering over in Phase 4. Per memory `feedback_trust_support_engineer_intuition.md`: don't rationalize a Codex finding; do the mutation probe (sed-mutate the production code → re-run the test → confirm it fails → revert) when a finding says "this assertion is vacuous."

- [ ] **Step 4.3: Anti-pattern review checklist (declarative — record PASS/note in PR body)**

Per `docs/ux-anti-patterns.md` §"For Subagents Implementing UI Tasks", the PR body MUST include `ANTI-PATTERN REVIEW: none` or specific notes. Walk the full list (line 56-209) — at minimum these apply to Task 12:

| Anti-pattern | Phase 2/3 status |
|---|---|
| No hamburger menus | n/a — no menus introduced in Task 12 (Task 7's menu bar carries them) |
| No slide-out drawers / auto-hide sidebars | n/a — no sidebar in Phase 2/3 scope; sidebar deferred per "Open spec deltas" #1 |
| No mobile-first / responsive below 1024px | PASS — grid uses fixed/`fr` units sized for desktop |
| No "single-page app" layouts where whole UI swaps on click | PASS — verified manually in Phase 3 Step 3.8 + smoke walk Step 4.1 step 7 |
| No in-content toolbars duplicating menu items | PASS — no toolbar in Mailbox; menu items live in Task 7's menu bar |
| No notification toasts for non-urgent events | PASS — error state is inline (`role="alert"`), not a toast |
| Familiar terminology from Winlink Express | PASS — "Inbox" / "Sent" labels match Express |
| Discoverable next actions (no hamburger) | PASS — empty state hints F5 / Session → Connect |

- [ ] **Step 4.4: Three-round plan review on the IMPLEMENTATION (not the plan doc)**

After Phases 0-3 ship, re-read the diff end-to-end three times with these lenses:

- **Round 1 — type/contract consistency:** Does `Message` (Rust) → `RustMessage` (TS) → `MessageSummary` (TS) round-trip preserve every field? Does the Tauri command's `folder` arg accept only `"inbox" | "sent"`? Does the `body_size` ↔ `bodySize` adapter happen exactly once?
- **Round 2 — error-path coverage:** What happens when Pat is down? When `mailbox_list` returns `Err("pat not running")`? When `mailbox_list` returns malformed JSON? Each path needs either a test or a documented "out of scope for v0.0.1" note.
- **Round 3 — pitfall + anti-pattern cross-check:** Re-walk pitfalls SCOPE-1 (no gateway-shaped surface), ORCH-1 (n/a — no parallel subagent dispatch in Task 12 itself), RADIO-1 (n/a — no transmit path; Pat HTTP API is Part 15 local IPC), HOOK-1 (worktree workflow respected). Re-walk ux-anti-patterns review in Step 4.3.

If any round produces findings, fix in-place + run another round. Continue until a round produces zero findings.

- [ ] **Step 4.5: Push the branch**

```bash
git push -u origin bd-tuxlink-zsm/task-12-inbox-sent
```

- [ ] **Step 4.6: Open the PR**

Substitute `<MONIKER>` with the implementing session's actual moniker (generated via `python3 .claude/scripts/get_agent_moniker.py` at session start) before running. The PR title's bracketed moniker tag is mandatory per CLAUDE.md §"Agent identity".

```bash
gh pr create --base feat/v0.0.1 \
  --title "[<MONIKER>] feat(mailbox): Task 12 — Inbox/Sent tabbed view" \
  --body "$(cat <<'EOF'
## Summary

Ships Task 12 from `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` + the §5.5 deltas from `docs/design/v0.0.1-ux-mockups.md`:

- Backend: `mailbox_list` Tauri command + `session::PAT_URL` OnceLock + `Serialize` on `pat_client::Message`
- Frontend: `Mailbox` (Radix Tabs) + `MessageList` (react-virtuoso) + `useMailbox` (TanStack Query 10s refetch)
- §5.5 deltas: unread-count badge on Inbox tab (suppressed at 0); column model per Express ratios; column-width persistence via `<config.rs | localStorage>` (see commit body for the recorded choice)

Plan: `docs/plans/2026-05-18-task-12-inbox-sent-tabbed-plan.md`

## Deferrals + open follow-ups

- Folder sidebar (Outbox / Drafts / Deleted / Templates) — deferred per the plan's "Open spec deltas" #1; awaiting Cameron's call on Drafts and Deleted backing stores. bd issue: `<issue-id-from-bd-create>`.
- `To` column and attachment-indicator column may render as `—` placeholders if Pat does not surface those fields — Phase 1's curl probe documents what Pat actually returns; follow-ups filed per "Open spec deltas" #2-3.

## Test plan

- [x] `pnpm vitest run` — N tests, all green, clean output
- [x] `cd src-tauri && cargo test --all-targets` — N tests, all green, clean output
- [x] `pnpm tauri dev` smoke walk (steps 1-10 of Phase 4 Step 4.1) — recorded in `/tmp/task-12-smoke-<ts>/`
- [x] Codex adversarial review — transcript at `dev/adversarial/2026-05-NN-task-12-codex.md` (gitignored); findings addressed
- [x] 3-round implementation review (Phase 4 Step 4.4) — N rounds, last clean
- [x] ANTI-PATTERN REVIEW: none (full walk in Step 4.3 of the plan)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4.7: Update the top-of-plan Execution Status table** with the PR number + URL. Per Living Document Contract.

- [ ] **Step 4.8: After merge** — record merge SHA in the Phase 4 banner + the Execution Status table; dispose of the worktree per ADR 0009 ritual; `bd close tuxlink-zsm`.

---

## Recommended execution strategy

**Recommendation: `superpowers:subagent-driven-development` (Option 1) with one fresh subagent per phase.**

Reasoning per `writing-plans-enhanced` Step 2:

- **Context budget per phase:** Phase 0 is purely test-harness ops; Phase 1 is Rust-only; Phase 2 is React MVP; Phase 3 is the §5.5 deltas (touches both stacks); Phase 4 is review-only. Each phase is self-contained and fits comfortably in a fresh subagent's context window (no phase exceeds ~2000 lines of code-affected + reference docs).
- **Quality gate cadence:** the plan's `## Mandatory Per-Task Completion Check` + Phase 4's 3-round review map naturally to subagent-driven-development's "review between tasks" cadence. Per-phase reviews catch regressions before they compound across the next phase.
- **Parallelization is low-value here:** Phase 1 (backend) and Phase 2 (frontend) could in principle run in parallel, but Phase 2's Tauri-command call needs Phase 1's command registered — synchronous dispatch with a fresh agent per phase is simpler and the wall-clock cost is small.
- **Risk concentration:** Phase 0 (test harness) is the highest-risk-of-derailment phase (test setup is famously a yak-shave). A focused fresh subagent for Phase 0 alone — without the cognitive load of also implementing the mailbox — produces a cleaner harness commit.

**Not recommended:**

- Option 2 (parallel-session in-worktree) would batch all 5 phases into one session; the per-phase commit cadence + per-phase review gates would either be skipped under pressure (eroding rigor) or would happen inside one session's context (eroding the "fresh-context" benefit).
- Option 3 (parallel agents) does not apply — there is only one campaign (Task 12), not multiple independent workstreams.

---

## Self-review checklist (run before declaring the plan ready)

- [x] Goal / Architecture / Tech Stack header present
- [x] Living Document Contract block pasted verbatim
- [x] Execution Status table at top + per-phase banners
- [x] Every phase has TDD discipline + completion check
- [x] Every phase commit uses heredoc syntax (avoids destructive-git hook substring match)
- [x] Every phase commit includes `Agent: <SESSION-MONIKER>` trailer
- [x] No "TODO" / "fix as needed" / "appropriate error handling" placeholders
- [x] Every code block is complete (no `...` elisions in shipping code)
- [x] Pitfall references attached where applicable (HOOK-1, ORCH-1, RADIO-1, BD-1, SCOPE-1)
- [x] Anti-pattern review listed in Phase 4 + flagged in commit-message ANTI-PATTERN REVIEW lines
- [x] Cross-task conflict surface minimal (Task 12 touches `src/mailbox/` (new), `src/App.tsx` (additive), `src/App.css` (append-only), `src-tauri/src/lib.rs` (additive), `src-tauri/src/pat_client.rs` (one derive line), new `src-tauri/src/session.rs`, new `src-tauri/tests/mailbox_list_integration.rs` — no shared-file conflicts with Tasks 9-11 or 13)
- [x] Open spec deltas surfaced + Cameron decision flagged on item #1
- [x] Execution-strategy recommendation present per writing-plans-enhanced Step 2

---

## Manual verification tax (inherited from the v0.0.1 plan)

Tests cannot prove that the mailbox actually renders correctly in `tauri dev` against a live Pat instance. The Phase 4 Step 4.1 smoke walk is the manual verification tax — it is non-negotiable before opening the PR. If a subagent cannot run `pnpm tauri dev` (e.g., headless CI), it MUST surface that to the dispatching agent rather than skip the walk.
