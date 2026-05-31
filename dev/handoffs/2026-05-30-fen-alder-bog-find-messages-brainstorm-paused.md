# Handoff: 2026-05-30 — find-messages brainstorm paused at Q4 (and a 4-thread session summary)

**Agent:** fen-alder-bog
**Branch:** `task-amd-main-ui` (NOTE: behind `origin/main` — see below)
**Working-tree state:** mostly clean. Untracked: `dev/scratch/ham-knowledge-store/` (full Hamexandria project, has its own git repo inside); `.superpowers/brainstorm/.../` (visual-companion content). Tracked-modified: `.beads/issues.jsonl` (auto-managed bd export). Plus this handoff.

---

## TL;DR for the next agent

You are mid-brainstorm on **Tuxlink v0.1 capability 1.15: find-messages**. Q1–Q3 are settled; **Q4 (filter chip set) is the open question to re-pose first.** The full Hamexandria RAG project shipped earlier in this session and is parked-with-pending-operator-items. Read §"Current state of find-messages brainstorm" first; everything else is context.

---

## What happened this session (chronological)

### Thread 1 — Hamexandria (DONE, dogfooded)

YouTube ham-radio transcript RAG pipeline. From design → TDD build → Codex hardening → uv migration → full-corpus index → live use.

- **Final index:** 56,972 chunks across 3,890 unique videos (HRCC + Tech Minds + OH8STN + W6LG + ModernHam + The Tech Prepper) in `dev/scratch/ham-knowledge-store/hamexandria.db` (255 MB).
- **Built robustly via `setsid`** after an earlier session-bound watcher died mid-index — finished cleanly at 2026-05-30T10:21Z.
- **`git init` performed** inside `dev/scratch/ham-knowledge-store/` per operator request — Hamexandria is now its own git repo (master branch), separate from tuxlink. Any commits to Hamexandria need a session rooted *inside* that dir per `sibling_repo_needs_own_session_root` memory.
- **CLI verified:** `./bin/ham-search -k 5 --json "<query>"` works via the uv-managed `.venv`.
- **Dogfood test 1 — MPPT/solar:** clean win. Corpus answered "Genasun GV-10L (RF-quiet, OH8STN's tested pick) and AVOID Victron MPPTs on HF (on-air RF-hash test)" — definitively better than my training-data answer which would've over-indexed Victron from general off-grid content.
- **Dogfood test 2 — FT-857D failure modes (blind test):** corpus AND my training data both missed. Operator's hint was "moisture contamination"; grep-level inspection of TTP transcripts showed his moisture complaints are about HTs and Raspberry Pis, NOT the 857D specifically (TTP *trusts* the 857D and wants Yaesu to bring it back). Operator concluded the recall was probably a morning hiccup; moved on.

**Calibration insight emerged:** Hamexandria adds value when the answer is *empirical and niche-expert-on-YouTube*; training data often beats it for *forum-thread / repair-shop / gripe-knowledge* topics — the harvested channels don't deeply cover that genre.

**Parked Hamexandria items (operator action):**
- **Task 9 §4 eval-ID fill** — drop real "correct video_id" into `dev/scratch/ham-knowledge-store/eval/ham_queries.tsv` (currently placeholder `REPLACE_WITH_REAL_VIDEO_ID` rows) and run `.venv/bin/python eval/precision_at_k.py eval/ham_queries.tsv hamexandria.db 5` for a precision@5 baseline.
- **PLAN.md Task 0 step** still shows `python3 -m venv .venv` + `pip install -e`, superseded mid-session by uv (`uv sync --extra dev`). Low-priority doc hygiene.

### Thread 2 — Tuxlink capability inventory absorbed

Operator pointed me at PR #151 for the discrepancy list; **the actual capability-comparison inventory is PR #150 — merged** to origin/main at `docs/design/2026-05-29-winlink-express-feature-inventory.md` (by yew-cypress-oak as bd-tuxlink-95z).

Key findings from §1 + §13:
- **Rev-2.0.1 correction:** Reply / Reply-All / Forward-with-edit are SHIPPED (🟢). Remaining v0.1 message-action gaps: **1.5 Forward-without-change, 1.8 Acknowledge-receipt, 1.15 Find-messages, 3.3 Position-report-send.** Plus 4.1 inbound-form-render and 4.2 ICS-213-author from §4.
- **3.3 Position-report-send is blocked** for me — operator (Cameron) has 3 in-progress bd issues claiming the GPS/position subsystem (tuxlink-39b GPS privacy, tuxlink-882 locator precision, tuxlink-686 manual grid + gpsd arbiter).
- **4.1 + 4.2 are claimed by parallel agent yew-cypress-oak** (PR #151, design-only, awaiting operator approval).
- **1.5 and 1.8 are unclaimed but touch the compose surface** that PR #151 is restructuring — coordination/merge-conflict risk.
- **1.15 Find-messages is the cleanest unsupervised pick** — purely client-side (no RF), no compose-surface conflict, fully test-coverable.

### Thread 3 — `git init` Hamexandria (DONE)

Operator approved. `git init` inside `dev/scratch/ham-knowledge-store/` succeeded; tuxlink hooks did NOT interfere with `git init` itself (it's not destructive). Tuxlink's git still sees the dir as untracked at the top level — no cross-contamination. Hamexandria-internal commits must run from a session rooted in that dir.

### Thread 4 — Find-messages brainstorm (current, paused at Q4)

THE CURRENT WORK. Operator framed it as: *"Tuxlink is greenfield; don't constrain to legacy Express's search design — at minimum cover the gap, but expand on it."*

`superpowers:brainstorming` skill invoked. Visual companion launched at http://localhost:56876 (almost certainly dead by next session — 30 min inactivity timeout; restart fresh).

---

## Current state of find-messages brainstorm

### Decisions settled (DO NOT RE-LITIGATE)

| # | Question | Decision |
|---|---|---|
| Q1 | Scope ambition | **Floor + structured filters + saved searches.** The meaty greenfield version. Express's grep-headers floor is too low; tagging is too far. |
| Q2 | Index scope | **Headers + body + form-payload fields.** Attachment text (PDFs/CSVs) **deferred to v0.5+** — PDF extraction is a multi-week security surface, not v0.1. |
| Q3 | Storage backend | **sqlite FTS5 in its own `search.db`** alongside the filesystem mailbox. Derived index, regenerable from filesystem. Engine swap-able to Tantivy in v0.5 if fuzzy + faceting earn their keep. |
| Q4 | **Filter chip set** | **OPEN — first thing to ask** |

### Q4 — open question to re-pose

> *What filter chip set should v0.1 ship with? (Folder is implicit / contextual — not a chip.)*
>
> - **A. Lean (4 chips):** From / To / Date-range / Has-form. Plus current-folder-vs-all-folders toggle.
> - **B. Medium (7 chips, recommended):** A + Form-type (ICS-213/309/Position/Bulletin/DamageAssessment) + Read-unread + Transport-used (telnet/packet/VARA/etc.). EmComm net-control sweet spot.
> - **C. Maximum (10+ chips + user tags):** B + Has-attachment (separate from Has-form) + Sent/Received direction + user-defined tag system for incident grouping. Tags are a real subsystem; pushes into stretch territory.

### Questions still to ask after Q4

Per the brainstorming flow (one at a time):
- **Q5 — saved-searches model:** what's actually saved (query string + filter state? snapshot of results?), where in the UI (sidebar list? dropdown?), can searches be renamed/exported?
- **Q6 — UI placement:** search-bar location (top of MessageList? global header? command-K palette?), results layout (split view? full-screen replacement?), filter chip strip below input.
- **Q7 — synchronization / re-index strategy:** how does the FTS5 index stay in sync with filesystem mutations (hooks in native_mailbox.rs `store`/`move`/`delete`? watcher? on-demand `rebuild-index` command?).

These will lead into the "2-3 approaches" proposal step, then design sections, then the spec doc.

---

## Critical architecture facts (don't re-derive these)

1. **The mailbox is filesystem-backed, NOT sqlite.** Source: `git show origin/bd-tuxlink-0ic/native-winlink-client:src-tauri/src/native_mailbox.rs`. Comment in source: *"The on-disk format is deliberately simple (raw message bytes per file)."* Find-messages must build a **derived** sqlite FTS5 index alongside, with filesystem canonical and index regenerable. Do NOT propose migrating the mailbox to sqlite — it'd be unrelated refactoring against the source's explicit "deliberately simple" design choice.

2. **`task-amd-main-ui` is behind `origin/main`.** The mailbox substrate (Rust `native_mailbox.rs`, `winlink/message.rs`, etc.; React `src/mailbox/FolderSidebar.tsx`, `MessageList.tsx`, `MessageView.tsx`) lives on `origin/main`. My current branch has none of it. **The implementation work should branch from `origin/main`**, not from `task-amd-main-ui` — per project_branch_model + ADR 0004: `bd-<id>/<slug>` off the integration branch.

3. **Visual companion server** was launched but is almost certainly dead by next session (30-min idle timeout):
   - URL: `http://localhost:56876` (port assigned this session — will be different next time)
   - screen_dir: `/home/administrator/Code/tuxlink/.superpowers/brainstorm/2562789-1780157790/content`
   - state_dir: `/home/administrator/Code/tuxlink/.superpowers/brainstorm/2562789-1780157790/state`
   - **`.superpowers/` must be added to `.gitignore`** — the brainstorm dir is currently untracked but creating brainstorm session content there is the expected pattern. Visual-companion guide explicitly flags this; I deferred the .gitignore edit to avoid scope creep this session.

4. **Codex quota:** not used this session; available for the adrev round when the design is finalized.

---

## Brainstorming checklist progress

Per `superpowers:brainstorming`:

1. ✅ Explore project context (mailbox shape, inventory §1.15, in-flight branches).
2. ✅ Launch visual companion.
3. 🔄 Clarifying questions (Q1✓ Q2✓ Q3✓; Q4 OPEN; Q5–Q7 pending).
4. ⏳ Propose 2-3 architecture approaches.
5. ⏳ Present design sections (architecture, components, data flow, error handling, testing).
6. ⏳ Write design doc to `docs/design/<YYYY-MM-DD>-find-messages-design.md` (date = day it's committed).
7. ⏳ Spec self-review (placeholders, consistency, scope, ambiguity).
8. ⏳ User reviews spec.
9. ⏳ Transition to `superpowers:writing-plans`.

---

## Resume protocol for the next agent

1. **Read this handoff** (which you'll see surfaced by the session-start hook).
2. **Don't re-litigate Q1–Q3.** Acknowledge them as settled.
3. **Re-pose Q4** (filter set: A/B/C) as the very first clarifying question — operator's pending decision.
4. **Continue clarifying questions** Q5–Q7 one at a time.
5. **Propose 2-3 architecture approaches** bundling backend + filter set + saved-searches model + UI placement.
6. **Present design sections** with operator approval each.
7. **Write spec** to `docs/design/<that-day>-find-messages-design.md` and commit.
8. **Spec self-review, operator review, then invoke `superpowers:writing-plans`.**
9. **Implementation (later session):**
   - `bd create --type=feature --priority=2 --title="v0.1 capability 1.15: find-messages (FTS5 over filesystem mailbox + filters + saved searches)"`.
   - Branch from `origin/main` as `bd-<id>/find-messages`.
   - Worktree at `worktrees/bd-<id>/find-messages` per ADR 0008.
   - Codex `--uncommitted` adrev round before merge.

---

## Files this session created or modified (find-messages thread only)

- **This handoff** (new tracked file).
- `.beads/issues.jsonl` (auto-managed by bd; modification is normal).

The Hamexandria thread also created `dev/scratch/ham-knowledge-store/...` (untracked) and the visual-companion thread created `.superpowers/brainstorm/...` (untracked). Neither is in the tuxlink tracked tree.

---

Agent: fen-alder-bog
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
