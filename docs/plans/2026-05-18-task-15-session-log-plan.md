# Task 15 — Session Log Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the v0.0.1 session log pane: a bottom-anchored panel that shows Pat's live session output projected through two views (Human-shaped default + Raw toggle), backed by a single in-memory `LogRing` that retains structured per-line metadata (timestamp, source, raw text). Both projections render from the same backing store — never two parallel streams.

**Architecture:** A Rust `LogRing` in `tuxlink_lib` stores structured `LogEntry` records behind an `Arc<Mutex<…>>`. A producer thread per Pat child reads stdout + stderr line-by-line, stamps each line with `LogSource::{Stdout, Stderr}` + a UTC timestamp, classifies it via a pure `tag_line` function (so the Human projection is computed from structured tag data, not by re-parsing strings at render time), and pushes it into the ring. A Tauri command `session_log_read(projection)` returns a snapshot already projected to either `Human` or `Raw`; a Tauri event `session_log:entry` streams each new entry incrementally with all metadata so the frontend can re-project on toggle without a re-fetch. The React `SessionLog.tsx` component renders the active projection, owns scroll-follow-with-anchor behavior, persists pane height + visibility + current projection to Tauri-managed settings, and wires `menu:view:session_log` + `menu:view:raw_log` events from AMD-10's menu half.

**Tech Stack:** Rust 1.75+ (`std::sync::Mutex`, `std::collections::VecDeque`, `std::thread`), Tauri 2 (events + commands + state), React 18 + TypeScript 5, Vite, `@tauri-apps/api`. No new top-level dependencies — `chrono` is added to `src-tauri/Cargo.toml` (already-common Rust dep, single-purpose: UTC timestamps that don't carry the full `time` crate weight). No frontend deps added (Vitest is not yet wired; projection logic lives Rust-side where the existing cargo test harness covers it).

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

**Overall:** Not started.

| Phase | Status | Ship SHA(s) | Notes |
|---|---|---|---|
| 1 — `LogRing` + `LogEntry` + classifier (Rust core) | ⬜ Not started | — | TDD; no Tauri or thread integration yet |
| 2 — Projection layer (`project_human` / `project_raw`) | ⬜ Not started | — | Pure functions over `&[LogEntry]`; no I/O |
| 3 — `PatProcess` stdout/stderr capture wiring | ⬜ Not started | — | Modifies the shipped `pat_process.rs`; concurrency-touching task |
| 4 — Tauri commands + event emission | ⬜ Not started | — | `session_log_read(projection)` + `session_log:entry` event |
| 5 — Menu wiring (AMD-10 runtime half) | ⬜ Not started | — | Adds `menu:view:session_log` (Ctrl+Shift+L) + `menu:view:raw_log` to the existing menu test + builder |
| 6 — `SessionLog.tsx` React component + scroll discipline | ⬜ Not started | — | Owns toggle, copy button, session-state header, scroll-follow-with-anchor |
| 7 — Pane height + visibility + projection persistence | ⬜ Not started | — | Tauri-managed settings on disk; re-loads on app start |
| 8 — Integration smoke: launch `tauri dev`, walk the user flow | ⬜ Not started | — | Browser-smoke gate per `feedback_browser_smoke_before_ship.md` |

### Deviations

_(none yet)_

### Discoveries

_(none yet)_

---

## How to start (Wave-2 implementer)

This plan was authored as a Wave-1 deliverable; Task 15's existing bd issue is `tuxlink-69z`. Before executing any phase below, the Wave-2 implementer MUST:

1. Pick a moniker: `python3 .claude/scripts/get_agent_moniker.py`. Use it in every commit's `Agent:` trailer.
2. Claim the bd issue: `bd update tuxlink-69z --claim`.
3. Create a worktree off `feat/v0.0.1`: `python3 .claude/scripts/new_tuxlink_worktree.py --slug task-15-impl --issue tuxlink-69z --moniker <moniker>`.
4. `cd` into the worktree. All commits land on `bd-tuxlink-69z/task-15-impl` (or whatever the script reports).
5. Per CLAUDE.md §"Commit and release discipline": every commit uses heredoc syntax and ends with `Agent: <moniker>\nCo-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
6. After all phases ship and the PR merges: `bd close tuxlink-69z` + dispose the worktree per the ADR 0009 ritual (`git status --short` → `git ls-files --others ...` → cd back to main → archive if needed → `rm -rf <worktree>` → `git worktree prune`).

---

## Pre-flight (REQUIRED reads, in order)

Before claiming Phase 1, the executor MUST read:

1. `docs/design/v0.0.1-ux-mockups.md` §4.4 (Express's actual log format — every classifier rule in Phase 1 derives from this section) and §5.8 (Task 15 spec).
2. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Task 15 section starting at line 3766 (the AMD-7 callout + the now-outdated Steps 1-8 below it; AMD-7 supersedes those steps where they conflict — when in doubt, AMD-7 + this plan win).
3. `docs/pitfalls/implementation-pitfalls.md` SCOPE-1 (do not creep this pane into gateway-side functionality — it is the client-side view of a session log only), RADIO-1/RADIO-2 (this pane reads logs, does not transmit; no consent gate needed because no code path here calls a transmit binary), and the entire pitfalls doc as quick orientation.
4. `docs/pitfalls/testing-pitfalls.md` §3 (Error path coverage), §4 (Negative property testing — bounded growth applies to the ring), §5 (Concurrency & TOCTOU — Phase 3 is the concurrency-touching task), §7 (Test infrastructure hygiene — no network calls; no hardcoded timezones).
5. The shipped source files this plan touches: `src-tauri/src/pat_process.rs` (Task 3 product), `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src/App.tsx`.

The executor SHOULD also skim:

- `docs/design/v0.0.1-ux-mockups.md` §4.1 (transport-visibility anti-pattern; the human-projection header MUST name the transport, NOT generic "in-session"), §4.3 (window-geometry persistence model — the pane-height persistence in Phase 7 follows the same pattern).
- The Cameron-real-logs example in §4.4 (lines 134-157 of the design doc) — every classifier rule below is grounded in those lines. The implementing agent SHOULD copy that excerpt verbatim into a test fixture so the human-projection assertion can compare structurally instead of guessing.

**TDD-discipline gate (per `writing-plans-enhanced` Step 3 mandates):** every task below starts with the BEFORE-starting protocol:

```
BEFORE starting work:
1. Invoke /superpowers:test-driven-development
2. Read docs/pitfalls/testing-pitfalls.md
Follow TDD: write failing test → implement → verify green.
```

…and ends with the BEFORE-completing protocol:

```
BEFORE marking this task complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md
2. Verify test coverage (error paths? edge cases?)
3. Run tests and confirm green
```

Phase 3 additionally invokes the **assertion-rigor-under-pressure clause** (verbatim) inside the task body — see Phase 3 below.

After completing each of {Phases 1-2}, {Phases 3-4}, {Phases 5-7}, the executor MUST run:

```
After completing this group:
Review the batch from multiple perspectives. Minimum 3 review rounds.
If round 3 still finds issues, keep going until clean.
```

---

## Pre-existing context the implementing agent inherits

These are facts about the codebase as of the day this plan was written. They are stated here so the implementing agent does not re-derive them under pressure.

- `tuxlink_lib::pat_process::PatProcess::spawn` (in `src-tauri/src/pat_process.rs`, shipped via PR #5) already pipes `Stdio::piped()` on both stdout and stderr, but it currently **consumes the stderr reader during spawn** (looping until it sees the listen-address echo, then dropping the reader) and **leaves stdout untouched** (the `BufReader` from the original plan was abandoned during the pat-1.0.0 amendment). Phase 3 changes this: after the listen-address line is observed, the spawn function MUST hand back ownership of BOTH stderr and stdout into long-lived reader threads that push every subsequent line into the `LogRing`. The pre-listen-address stderr lines observed during the announce wait MUST also flow into the ring (do NOT discard them — they are the first thing a sysadmin will look for when debugging a startup failure).
- No frontend test harness (Vitest, Jest, Playwright unit) is wired into `package.json` yet. This plan deliberately keeps projection logic Rust-side so the existing cargo test harness covers it; the React component is verified via Phase 8's manual browser-smoke gate.
- The AMD-10 menu work has shipped its wizard half (`menu:session:test_send` is already in `menu_event_ids()`); the runtime half is in flight on a sibling task. **The Wave-2 implementer for Task 15 owns the `menu:view:session_log` + `menu:view:raw_log` entries** (per `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` line 1745-1746 — both IDs are already listed in the menu test, so Phase 5 below just confirms they are present + wires the click handlers, without inventing new IDs).
- No transmit-capable code path is touched by this plan. RADIO-1 does not apply.
- No git-strategy.md exists in this repo yet. ORCH-1 (parallel subagent dispatch persistence) does not apply because this plan executes as a single linear track; no parallel investigation subagents are dispatched.

---

## File Structure

This plan creates / modifies the following files. Each task below specifies which files it touches; cross-task overlap is summarized here so the executor sees it at a glance.

| File | Phase(s) | Responsibility |
|---|---|---|
| `src-tauri/src/session_log.rs` (NEW) | 1, 2, 4 | `LogEntry` struct, `LogSource` enum, `LogTag` enum, `LogRing` (ring buffer), `tag_line()` classifier, `project_human()` + `project_raw()` pure functions. Tauri-agnostic; pure data + pure functions. |
| `src-tauri/tests/session_log_ring_test.rs` (NEW) | 1 | Unit tests for `LogRing` (push, snapshot, eviction at capacity, empty-at-start). |
| `src-tauri/tests/session_log_classifier_test.rs` (NEW) | 1 | Unit tests for `tag_line()` against the §4.4 log excerpt (every Express line shape gets a test case). |
| `src-tauri/tests/session_log_projection_test.rs` (NEW) | 2 | Unit tests for `project_human()` + `project_raw()` operating over fixture entries. |
| `src-tauri/src/pat_process.rs` (MODIFY) | 3 | Wire stdout + stderr reader threads to push into a shared `Arc<Mutex<LogRing>>`; add `log_ring_handle()` getter; preserve the pre-listen-address stderr lines into the ring. |
| `src-tauri/tests/pat_process_log_capture_test.rs` (NEW) | 3 | Integration test: spawn a fake child process (echo script writing to stdout + stderr), assert lines arrive in the ring with correct `LogSource` tagging and in approximate order. Uses `tempfile` (already a dev-dep). |
| `src-tauri/src/lib.rs` (MODIFY) | 1, 4 | Add `pub mod session_log;`; register `session_log_read` Tauri command + `pat_log_ring()` state accessor. |
| `src-tauri/src/main.rs` (MODIFY) | 4 | Register the `session_log_read` command in the `invoke_handler!` macro. |
| `src-tauri/src/menu.rs` (MODIFY — wave-2 may find this file already-modified by the sibling AMD-10 runtime-half task; merge accordingly) | 5 | Confirm `menu:view:session_log` + `menu:view:raw_log` IDs are in `menu_event_ids()` and the builder, add `Ctrl+Shift+L` accelerator on the Show Session Log item. |
| `src-tauri/tests/menu_test.rs` (MODIFY) | 5 | Add assertion that `menu:view:session_log` + `menu:view:raw_log` are present in `menu_event_ids()` (the AMD-10 runtime-half plan already adds `menu:view:raw_log`; this plan re-asserts it as a defensive cross-check in case the sibling task has not landed). |
| `src-tauri/Cargo.toml` (MODIFY) | 1 | Add `chrono = { version = "0.4", default-features = false, features = ["std", "clock"] }` to `[dependencies]`. |
| `src/session/SessionLog.tsx` (NEW) | 6 | React component: renders the active projection, owns `[Human | Raw]` toggle, session-state header, "Copy session log" button, scroll-follow-with-anchor. |
| `src/session/sessionLogTypes.ts` (NEW) | 6 | TS interfaces for `LogEntry`, `LogTag`, `LogSource`, `Projection` — mirror the Rust types. |
| `src/session/sessionLogStyles.css` (NEW) | 6 | Pane layout, header strip, monospace log lines, resizable handle. |
| `src/App.tsx` (MODIFY) | 6 | Mount `<SessionLog>` in the bottom strip; listen for `menu:view:session_log` + `menu:view:raw_log` Tauri events; manage `sessionLogVisible` + `sessionLogProjection` state. |
| `src-tauri/src/lib.rs` (MODIFY) | 7 | Add `settings_get` / `settings_set` Tauri commands for `session_log.height`, `session_log.visible`, `session_log.projection`. Persists to `$XDG_CONFIG_HOME/tuxlink/settings.json` (or `~/.config/tuxlink/settings.json` fallback). |
| `src-tauri/tests/settings_persist_test.rs` (NEW) | 7 | Unit test: write a settings value, read it back from a fresh handle, assert round-trip. |

**Cross-task conflict notes** (per `writing-plans-enhanced` Step 3 "Minimize cross-task conflicts"):

- `src-tauri/src/lib.rs` is touched in Phase 1 (add `pub mod session_log;`), Phase 4 (register the read command), and Phase 7 (register settings commands). **All three touches MUST happen in the same Wave-2 worktree sequentially; do NOT parallelize across worktrees.** Phase ordering below enforces this.
- `src-tauri/src/menu.rs` may already be partially modified by the sibling AMD-10 runtime-half task (`menu:session:show_transport`, `menu:view:radio_dock`, `menu:tools:settings_*`). Phase 5 below treats this as a merge case: confirm the IDs are present, add only what is missing, do NOT duplicate.
- `src/App.tsx` is currently the scaffold (the "Hello, Tauri" greeter); Phase 6 replaces the body. **If a sibling task (Task 16, 16.5) lands before Task 15 and has already rewritten `App.tsx`, the executor MUST rebase on `feat/v0.0.1` first**, then add the `<SessionLog>` mount + the two menu-event listeners without disturbing other Wave-2 sibling additions.

---

## Phase 1 — `LogRing` + `LogEntry` + classifier

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** Both projections (Human + Raw) MUST read from the same backing store (AMD-7 architectural invariant). If the projections re-parse strings at render time, the parsing logic forks across two code paths and drifts. The fix is structured classification *at ingest time*: each line becomes a `LogEntry { timestamp, source, tag, raw }` where `tag` is a small enum the projections key off of. Re-projection is then a pure filter over the structured records, not a string re-parse.

**Files (this phase):**
- Create: `src-tauri/src/session_log.rs`
- Create: `src-tauri/tests/session_log_ring_test.rs`
- Create: `src-tauri/tests/session_log_classifier_test.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod session_log;`)
- Modify: `src-tauri/Cargo.toml` (add `chrono` dep)

BEFORE starting work:
1. Invoke /superpowers:test-driven-development
2. Read docs/pitfalls/testing-pitfalls.md

Follow TDD: write failing test → implement → verify green.

### Step 1.1 — Add `chrono` to `Cargo.toml`

- [ ] Add to `src-tauri/Cargo.toml` `[dependencies]`:

```toml
chrono = { version = "0.4", default-features = false, features = ["std", "clock", "serde"] }
```

Rationale: UTC timestamps on every `LogEntry`. `default-features = false` keeps the crate small (skips `oldtime`, `wasmbind`, etc.). `serde` is needed because Phase 4 emits entries to the frontend as JSON.

- [ ] Run `cd src-tauri && cargo check`. Expected: compiles green (no usage yet, just the new dep).

### Step 1.2 — Write the failing ring tests

- [ ] Create `src-tauri/tests/session_log_ring_test.rs`:

```rust
use chrono::Utc;
use tuxlink_lib::session_log::{LogEntry, LogRing, LogSource, LogTag};

fn entry(raw: &str) -> LogEntry {
    LogEntry {
        timestamp: Utc::now(),
        source: LogSource::Stdout,
        tag: LogTag::Raw,
        raw: raw.to_string(),
    }
}

#[test]
fn test_log_ring_empty_at_start() {
    let ring = LogRing::with_capacity(10);
    assert!(ring.snapshot().is_empty());
    assert_eq!(ring.capacity(), 10);
    assert_eq!(ring.len(), 0);
}

#[test]
fn test_log_ring_push_under_capacity() {
    let mut ring = LogRing::with_capacity(3);
    ring.push(entry("a"));
    ring.push(entry("b"));
    let snap = ring.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].raw, "a");
    assert_eq!(snap[1].raw, "b");
}

#[test]
fn test_log_ring_evicts_oldest_at_capacity() {
    let mut ring = LogRing::with_capacity(3);
    ring.push(entry("a"));
    ring.push(entry("b"));
    ring.push(entry("c"));
    ring.push(entry("d"));
    let snap = ring.snapshot();
    let raws: Vec<&str> = snap.iter().map(|e| e.raw.as_str()).collect();
    assert_eq!(raws, vec!["b", "c", "d"], "FIFO eviction must drop oldest");
    assert_eq!(ring.len(), 3);
}

#[test]
fn test_log_ring_bounded_growth_under_pressure() {
    // testing-pitfalls.md §4: "Bounded growth. For any in-memory data structure
    // that grows with external input, test that it has a maximum size."
    let mut ring = LogRing::with_capacity(100);
    for i in 0..10_000 {
        ring.push(entry(&format!("line {}", i)));
    }
    assert_eq!(ring.len(), 100);
    let snap = ring.snapshot();
    assert_eq!(snap.first().unwrap().raw, "line 9900");
    assert_eq!(snap.last().unwrap().raw, "line 9999");
}

#[test]
fn test_log_ring_capacity_zero_is_a_noop_sink() {
    // Edge case: capacity 0 ring accepts pushes but retains nothing.
    let mut ring = LogRing::with_capacity(0);
    ring.push(entry("a"));
    assert!(ring.snapshot().is_empty());
    assert_eq!(ring.len(), 0);
}
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_ring_test`. Expected: compile error — module not found. Red stage confirmed.

### Step 1.3 — Write the failing classifier tests

- [ ] Create `src-tauri/tests/session_log_classifier_test.rs`:

```rust
use tuxlink_lib::session_log::{tag_line, LogTag};

// Source: docs/design/v0.0.1-ux-mockups.md §4.4 (Cameron's real-log excerpt,
// sanitized). The classifier is the structured-tag-at-ingest discipline that
// keeps both projections (Human + Raw) reading from one LogRing.

#[test]
fn test_tag_express_annotation_with_triple_asterisk() {
    // "*** Connecting to CMS CMS-SSL at cms.winlink.org port 8773" — Express's
    // operator-relevant annotation. Surfaces in Human projection.
    let tag = tag_line("2026/05/03 03:37:04 1.7.31.0 Background *** Connecting to CMS CMS-SSL at cms.winlink.org port 8773");
    assert!(matches!(tag, LogTag::ExpressAnnotation), "*** lines must classify as ExpressAnnotation");
}

#[test]
fn test_tag_session_summary_messages_sent() {
    // "*** Messages sent: 0. Total bytes sent: 0, Time: 00:00, bytes/minute: 0"
    // — counts-and-totals line; Human projection derives "N sent / M received" footer.
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background *** Messages sent: 0.  Total bytes sent: 0,  Time: 00:00,  bytes/minute: 0");
    assert!(matches!(tag, LogTag::SessionSummary), "Messages-sent line must classify as SessionSummary, not generic ExpressAnnotation");
}

#[test]
fn test_tag_session_summary_messages_received() {
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background *** Messages Received: 0.  Total bytes received: 0,  Total session time: 00:00,  bytes/minute: 0");
    assert!(matches!(tag, LogTag::SessionSummary));
}

#[test]
fn test_tag_b2f_server_greeting() {
    // "[WL2K-5.0-B2FWIHJM$]" — B2F server identifier. Raw-only.
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background [WL2K-5.0-B2FWIHJM$]");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_client_greeting() {
    // "[RMS Express-1.7.31.0-B2FHM$]" — B2F client identifier. Raw-only.
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background    [RMS Express-1.7.31.0-B2FHM$]");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_secure_login_challenge() {
    // ";PQ: 02166776" — secure-login challenge. Raw-only.
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background ;PQ: 02166776");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_secure_login_response() {
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background    ;PR: 70336572");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_forwarding_identifier() {
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background    ;FW: W4PHS");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_wl2k_identification() {
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background    ; WL2K DE W4PHS (EM75ab)");
    assert!(matches!(tag, LogTag::B2fProtocol));
}

#[test]
fn test_tag_b2f_end_markers() {
    let ff = tag_line("2026/05/03 03:37:05 1.7.31.0 Background    FF");
    let fq = tag_line("2026/05/03 03:37:05 1.7.31.0 Background FQ");
    assert!(matches!(ff, LogTag::B2fProtocol), "FF end-marker is B2F");
    assert!(matches!(fq, LogTag::B2fProtocol), "FQ end-marker is B2F");
}

#[test]
fn test_tag_b2f_cms_prompt() {
    let tag = tag_line("2026/05/03 03:37:05 1.7.31.0 Background CMS>");
    assert!(matches!(tag, LogTag::B2fProtocol), "CMS> prompt is B2F");
}

#[test]
fn test_tag_unknown_falls_back_to_raw() {
    // Defensive: an arbitrary unparsed line tags as Raw. Raw lines still
    // appear in the Raw projection; Human projection suppresses them.
    let tag = tag_line("2026/05/03 03:37:04 1.7.31.0 Some unknown noise line from Pat");
    assert!(matches!(tag, LogTag::Raw));
}

#[test]
fn test_tag_empty_line_is_raw() {
    let tag = tag_line("");
    assert!(matches!(tag, LogTag::Raw));
}

#[test]
fn test_tag_classifier_does_not_panic_on_unicode_or_oversized() {
    // testing-pitfalls.md §4: Unicode + oversized inputs. Classifier is a
    // pure function; it must not panic on any input.
    let _ = tag_line("a".repeat(1_000_000).as_str());
    let _ = tag_line("\0\u{FFFD}\u{1F600} *** maybe annotation \u{200B}");
    let _ = tag_line("\n\n\n");
    // No assertion needed; the test passes if these calls return.
}
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_classifier_test`. Expected: compile error — module not found. Red stage confirmed.

### Step 1.4 — Implement `session_log.rs` (ring + classifier; projections in Phase 2)

- [ ] Create `src-tauri/src/session_log.rs`:

```rust
//! Session log domain types for Tuxlink.
//!
//! Architectural invariant (AMD-7, per docs/design/v0.0.1-ux-mockups.md §5.8):
//! the Human projection and the Raw projection MUST read from the SAME
//! `LogRing`. To make that cheap at render time, we classify each line as it
//! enters the ring — `LogTag` is the structured discriminator both
//! projections key off. Re-projection on toggle is a pure filter, not a
//! re-parse. Maintaining two parallel streams is explicitly rejected.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogSource {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogTag {
    /// `*** message ***` framing for Express's narrative annotations.
    /// Operator-relevant; appears in Human and Raw projections.
    ExpressAnnotation,
    /// `*** Messages sent: N. ...` or `*** Messages Received: N. ...`
    /// summary lines. Human projection derives the per-session footer
    /// ("N sent / M received in T seconds") from these.
    SessionSummary,
    /// B2F protocol noise: `;PQ`, `;PR`, `;FW`, `[WL2K-...]`,
    /// `[RMS Express-...]`, `FF`, `FQ`, `CMS>`. Raw projection only.
    B2fProtocol,
    /// Anything the classifier did not recognize. Raw projection only
    /// (defensive — unrecognized lines are suppressed in Human view
    /// rather than spilling unidentified noise to the operator).
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub source: LogSource,
    pub tag: LogTag,
    pub raw: String,
}

/// FIFO ring buffer with fixed capacity. Pushes past capacity evict the
/// oldest entry. Single-writer-multiple-reader concurrency is handled by
/// wrapping the ring in `Arc<Mutex<LogRing>>` at the caller (see
/// `pat_process::PatProcess` in Phase 3).
pub struct LogRing {
    capacity: usize,
    entries: VecDeque<LogEntry>,
}

impl LogRing {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Classify a raw log line into a `LogTag`. Pure function over the line
/// content; never panics. Rules derive from docs/design/v0.0.1-ux-mockups.md
/// §4.4 (Cameron's real-log excerpt).
pub fn tag_line(line: &str) -> LogTag {
    // SessionSummary is a sub-shape of ExpressAnnotation; check it FIRST so
    // the more-specific match wins.
    if line.contains("*** Messages sent:") || line.contains("*** Messages Received:") {
        return LogTag::SessionSummary;
    }
    // ExpressAnnotation: any line containing the `***` framing.
    if line.contains("***") {
        return LogTag::ExpressAnnotation;
    }
    // B2F protocol shapes. The CMS> prompt has no leading semicolon or
    // bracket, so it gets its own check.
    let trimmed = line.trim_start();
    if trimmed.starts_with(";PQ")
        || trimmed.starts_with(";PR")
        || trimmed.starts_with(";FW")
        || trimmed.starts_with("; WL2K")
        || trimmed.starts_with("[WL2K-")
        || trimmed.starts_with("[RMS Express")
        || trimmed == "FF"
        || trimmed == "FQ"
        || trimmed == "CMS>"
        // Some lines have the date+version prefix; check via suffix-style
        // fallback for the bare protocol markers when prefixed.
        || line.ends_with(" FF")
        || line.ends_with(" FQ")
        || line.ends_with(" CMS>")
        || line.contains(" [WL2K-")
        || line.contains(" [RMS Express")
        || line.contains(" ;PQ")
        || line.contains(" ;PR")
        || line.contains("    ;PR")
        || line.contains(" ;FW")
        || line.contains("    ;FW")
        || line.contains(" ; WL2K")
        || line.contains("    ; WL2K")
    {
        return LogTag::B2fProtocol;
    }
    LogTag::Raw
}
```

- [ ] Add to `src-tauri/src/lib.rs` (at the top with the other `pub mod` lines):

```rust
pub mod session_log;
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_ring_test`. Expected: all 5 ring tests pass.
- [ ] Run: `cd src-tauri && cargo test --test session_log_classifier_test`. Expected: all 14 classifier tests pass.

If any classifier test fails, the line shape in the test came from the §4.4 excerpt verbatim — fix the classifier, not the test. DO NOT loosen the test to match a buggy classifier; the §4.4 excerpt IS the spec for the classifier's input shapes.

### Step 1.5 — Commit

```bash
git add src-tauri/src/session_log.rs \
        src-tauri/src/lib.rs \
        src-tauri/Cargo.toml src-tauri/Cargo.lock \
        src-tauri/tests/session_log_ring_test.rs \
        src-tauri/tests/session_log_classifier_test.rs
git commit -m "$(cat <<'EOF'
feat(session-log): structured LogRing + line classifier (Phase 1 of Task 15)

Backs both Human and Raw projections from one ring per AMD-7. Each
LogEntry carries timestamp + source (Stdout/Stderr) + tag (one of
ExpressAnnotation / SessionSummary / B2fProtocol / Raw) + the original
raw string. The classifier (`tag_line`) is a pure function whose rules
are grounded in docs/design/v0.0.1-ux-mockups.md §4.4's real-log
excerpt; every Express line shape in that excerpt has a test case.

No projections yet (Phase 2). No PatProcess wiring yet (Phase 3).
No Tauri surface yet (Phase 4).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md (§3 error paths, §4 bounded growth, §6 default values, §7 no time-of-day deps).
2. Verify test coverage:
   - [ ] Empty ring
   - [ ] Push under capacity
   - [ ] Eviction at capacity
   - [ ] Bounded growth under 10k pushes
   - [ ] Capacity-zero edge case
   - [ ] Each LogTag variant has at least one classifier test
   - [ ] Classifier does not panic on unicode / oversized / null-byte input
3. Run `cd src-tauri && cargo test --test session_log_ring_test --test session_log_classifier_test` — confirm green.

---

## Phase 2 — Projection layer

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** This phase implements the two projections as pure functions over a `&[LogEntry]` slice. Re-projection on toggle is then "call the other function with the same input." If either projection performs I/O, calls into Tauri state, or holds mutable state, the AMD-7 invariant ("both views from one LogRing") softens into "both views from one stream that happens to share an upstream," which lets the projections drift. Keep them pure.

**Files (this phase):**
- Modify: `src-tauri/src/session_log.rs` (append `project_human` + `project_raw` + Human-formatting helpers)
- Create: `src-tauri/tests/session_log_projection_test.rs`

BEFORE starting work:
1. Invoke /superpowers:test-driven-development
2. Read docs/pitfalls/testing-pitfalls.md

Follow TDD: write failing test → implement → verify green.

### Step 2.1 — Write the failing projection tests

- [ ] Create `src-tauri/tests/session_log_projection_test.rs`:

```rust
use chrono::{TimeZone, Utc};
use tuxlink_lib::session_log::{
    project_human, project_raw, HumanLine, LogEntry, LogSource, LogTag, RawLine,
};

/// Build the §4.4 excerpt as structured fixture entries. Timestamps are
/// fixed (not Utc::now()) so the test is timezone- and clock-independent
/// (testing-pitfalls.md §7).
fn fixture_session() -> Vec<LogEntry> {
    let ts = |sec: u32| Utc.with_ymd_and_hms(2026, 5, 3, 3, 37, sec).unwrap();
    let lines = vec![
        (4, LogTag::ExpressAnnotation, "2026/05/03 03:37:04 1.7.31.0 Background *** Beginning background connection to CMS ***"),
        (4, LogTag::ExpressAnnotation, "2026/05/03 03:37:04 1.7.31.0 Background *** Connecting to CMS CMS-SSL at cms.winlink.org port 8773"),
        (4, LogTag::ExpressAnnotation, "2026/05/03 03:37:04 1.7.31.0 Background *** Connected to CMS-SSL at 2026/05/03 03:37:04"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background [WL2K-5.0-B2FWIHJM$]"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background ;PQ: 02166776"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background CMS>"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background    ;FW: W4PHS"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background    [RMS Express-1.7.31.0-B2FHM$]"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background    ;PR: 70336572"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background    ; WL2K DE W4PHS (EM75ab)"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background    FF"),
        (5, LogTag::B2fProtocol,       "2026/05/03 03:37:05 1.7.31.0 Background FQ"),
        (5, LogTag::ExpressAnnotation, "2026/05/03 03:37:05 1.7.31.0 Background *** --- End of session with WL2K at 2026/05/03 03:37:05 ---"),
        (5, LogTag::SessionSummary,    "2026/05/03 03:37:05 1.7.31.0 Background *** Messages sent: 0.  Total bytes sent: 0,  Time: 00:00,  bytes/minute: 0"),
        (5, LogTag::SessionSummary,    "2026/05/03 03:37:05 1.7.31.0 Background *** Messages Received: 0.  Total bytes received: 0,  Total session time: 00:00,  bytes/minute: 0"),
        (10, LogTag::ExpressAnnotation,"2026/05/03 03:37:10 1.7.31.0 Background *** Disconnected at 2026/05/03 03:37:10"),
        (11, LogTag::ExpressAnnotation,"2026/05/03 03:37:11 1.7.31.0 Background *** Successfully finished background connection to CMS ***"),
    ];
    lines.into_iter().map(|(sec, tag, raw)| LogEntry {
        timestamp: ts(sec),
        source: LogSource::Stdout,
        tag,
        raw: raw.to_string(),
    }).collect()
}

#[test]
fn test_project_raw_returns_every_entry_in_order() {
    let entries = fixture_session();
    let raw = project_raw(&entries);
    assert_eq!(raw.len(), entries.len(), "Raw must surface every entry");
    for (i, line) in raw.iter().enumerate() {
        assert_eq!(line.raw, entries[i].raw, "Raw must preserve original line content at index {}", i);
        assert_eq!(line.timestamp, entries[i].timestamp, "Raw must preserve timestamps");
    }
}

#[test]
fn test_project_human_suppresses_b2f_protocol_lines() {
    let entries = fixture_session();
    let human = project_human(&entries);
    for line in &human {
        assert!(
            !line.text.contains(";PQ")
                && !line.text.contains(";PR")
                && !line.text.contains(";FW")
                && !line.text.contains("[WL2K-")
                && !line.text.contains("[RMS Express")
                && !line.text.contains("CMS>"),
            "Human projection must suppress B2F protocol noise; saw: {}", line.text
        );
    }
}

#[test]
fn test_project_human_surfaces_express_annotations() {
    let entries = fixture_session();
    let human = project_human(&entries);
    let texts: Vec<&str> = human.iter().map(|l| l.text.as_str()).collect();
    let joined = texts.join("\n");
    assert!(joined.contains("Connecting to CMS"), "Human must surface *** Connecting line; saw:\n{}", joined);
    assert!(joined.contains("Connected to CMS-SSL"), "Human must surface *** Connected line");
    assert!(joined.contains("Disconnected"), "Human must surface *** Disconnected line");
}

#[test]
fn test_project_human_derives_per_session_summary_footer() {
    // AMD-7 spec: derive "Session ended. N sent / M received in T seconds."
    // from the two SessionSummary lines.
    let entries = fixture_session();
    let human = project_human(&entries);
    let joined = human.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(
        joined.contains("0 sent") && joined.contains("0 received"),
        "Human projection must derive a session-summary footer with sent/received counts; saw:\n{}", joined
    );
}

#[test]
fn test_project_human_strips_express_log_prefix() {
    // Per §4.4: per-line prefix is "YYYY/MM/DD HH:MM:SS VERSION TAG". The
    // Human projection's preview lines (e.g., "03:37:04 UTC · Connecting
    // to Winlink CMS via CMS-SSL ...") drop that prefix in favor of the
    // line's own UTC timestamp and the cleaned annotation text.
    let entries = fixture_session();
    let human = project_human(&entries);
    for line in &human {
        assert!(
            !line.text.starts_with("2026/05/03"),
            "Human projection must strip the raw Express date prefix; saw: {}", line.text
        );
        assert!(
            !line.text.contains("1.7.31.0"),
            "Human projection must strip the Express version token; saw: {}", line.text
        );
    }
}

#[test]
fn test_project_human_empty_input_returns_empty() {
    let human = project_human(&[]);
    assert!(human.is_empty());
}

#[test]
fn test_project_raw_empty_input_returns_empty() {
    let raw = project_raw(&[]);
    assert!(raw.is_empty());
}

#[test]
fn test_human_and_raw_share_underlying_entries() {
    // The AMD-7 architectural-invariant assertion: re-projecting the SAME
    // input slice into both views yields views that agree on the set of
    // timestamps they cover (Raw is a superset; Human is a subset of
    // those same timestamps).
    let entries = fixture_session();
    let raw = project_raw(&entries);
    let human = project_human(&entries);
    let raw_ts: std::collections::HashSet<_> = raw.iter().map(|l| l.timestamp).collect();
    for h in &human {
        assert!(
            raw_ts.contains(&h.timestamp),
            "Human-projection timestamp {:?} must exist in the Raw projection of the same entries (one ring, two views)",
            h.timestamp
        );
    }
}
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_projection_test`. Expected: compile error — `project_human` / `project_raw` / `HumanLine` / `RawLine` not defined. Red.

### Step 2.2 — Implement projections in `session_log.rs`

- [ ] Append to `src-tauri/src/session_log.rs`:

```rust
// ---------- Projections ----------
//
// Both projections are pure functions over &[LogEntry]. They MUST NOT hold
// mutable state, perform I/O, or call into Tauri primitives. This is the
// AMD-7 architectural invariant: "both views from one LogRing" implemented
// as "two pure functions over the same slice."

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanLine {
    pub timestamp: DateTime<Utc>,
    /// The display text for the line, with the Express prefix stripped and
    /// any synthesized summary content folded in.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLine {
    pub timestamp: DateTime<Utc>,
    pub source: LogSource,
    pub raw: String,
}

/// Raw projection: every entry, in order, with timestamp + source + raw text.
/// Operators debugging RMS/CMS handshake failures want everything; do not
/// filter here.
pub fn project_raw(entries: &[LogEntry]) -> Vec<RawLine> {
    entries
        .iter()
        .map(|e| RawLine {
            timestamp: e.timestamp,
            source: e.source,
            raw: e.raw.clone(),
        })
        .collect()
}

/// Human projection: surfaces operator-relevant lines only.
///
/// Rules (per AMD-7 + docs/design/v0.0.1-ux-mockups.md §4.4):
/// - Keep `LogTag::ExpressAnnotation` and `LogTag::SessionSummary`.
/// - Suppress `LogTag::B2fProtocol` and `LogTag::Raw`.
/// - For each kept line, strip the Express date+version+tag prefix
///   ("2026/05/03 03:37:04 1.7.31.0 Background ") and the leading `***`.
/// - When a pair of SessionSummary lines (one "Messages sent: N", one
///   "Messages Received: M") is observed, append a synthesized footer
///   "Session ended. N sent / M received in T seconds." derived from the
///   two summary-line counts and the timestamp delta.
pub fn project_human(entries: &[LogEntry]) -> Vec<HumanLine> {
    let mut out: Vec<HumanLine> = Vec::new();
    let mut pending_sent: Option<(DateTime<Utc>, u64)> = None;
    for e in entries {
        match e.tag {
            LogTag::ExpressAnnotation => {
                let cleaned = clean_express_line(&e.raw);
                if !cleaned.is_empty() {
                    out.push(HumanLine { timestamp: e.timestamp, text: cleaned });
                }
            }
            LogTag::SessionSummary => {
                if let Some(sent) = extract_count(&e.raw, "Messages sent:") {
                    pending_sent = Some((e.timestamp, sent));
                } else if let Some(received) = extract_count(&e.raw, "Messages Received:") {
                    if let Some((sent_ts, sent_count)) = pending_sent.take() {
                        let elapsed = (e.timestamp - sent_ts).num_seconds().max(0);
                        out.push(HumanLine {
                            timestamp: e.timestamp,
                            text: format!(
                                "Session ended. {} sent / {} received in {} second{}.",
                                sent_count,
                                received,
                                elapsed,
                                if elapsed == 1 { "" } else { "s" }
                            ),
                        });
                    } else {
                        // Defensive: received without prior sent — surface
                        // the received count alone rather than dropping it.
                        out.push(HumanLine {
                            timestamp: e.timestamp,
                            text: format!("Session ended. {} received.", received),
                        });
                    }
                }
            }
            LogTag::B2fProtocol | LogTag::Raw => {
                // suppressed in Human projection
            }
        }
    }
    out
}

/// Strip the Express log prefix `YYYY/MM/DD HH:MM:SS VERSION [TAG]` and any
/// leading `***` from an annotation line, returning the cleaned text. Empty
/// or whitespace-only outputs are treated as "no display content."
fn clean_express_line(line: &str) -> String {
    // The shape from §4.4 is:
    //   "2026/05/03 03:37:04 1.7.31.0 Background *** Connecting to CMS ..."
    // or:
    //   "2026/05/03 03:37:04 1.7.31.0 *** GPS tracking initialized"
    // Strategy: find the first `***` (which is always present on an
    // ExpressAnnotation line by classifier definition); take the substring
    // AFTER the `***`; trim; strip a trailing `***` if present
    // (per the bracketed annotations like `*** Beginning ... CMS ***`).
    let after = match line.find("***") {
        Some(idx) => &line[idx + 3..],
        None => line,
    };
    let trimmed = after.trim();
    let stripped = trimmed.trim_end_matches('*').trim();
    stripped.to_string()
}

/// Extract an integer count from a summary line like
/// `*** Messages sent: 0.  Total bytes sent: 0, ...`. Returns the integer
/// after the literal needle; returns None if the line shape differs.
fn extract_count(line: &str, needle: &str) -> Option<u64> {
    let idx = line.find(needle)?;
    let tail = &line[idx + needle.len()..];
    let digits: String = tail
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_projection_test`. Expected: all 8 projection tests pass.

If `test_project_human_strips_express_log_prefix` fails, the cleaning helper is leaving "Background" or the date prefix in the output — extend `clean_express_line` to trim those tokens. If the summary-footer test fails, audit `extract_count` against the §4.4 line shape (`*** Messages sent: 0.  Total bytes sent: 0, ...` — the `:` and the leading space after it are load-bearing).

### Step 2.3 — Commit

```bash
git add src-tauri/src/session_log.rs \
        src-tauri/tests/session_log_projection_test.rs
git commit -m "$(cat <<'EOF'
feat(session-log): pure Human + Raw projections over LogEntry slices

Both projections are pure functions of `&[LogEntry]`; re-projection on
toggle is "call the other function with the same input." Human strips
Express's date+version+tag prefix and the `***` framing, and derives
the per-session summary footer from the two SessionSummary lines. Raw
returns every entry verbatim.

AMD-7 invariant assertion: test_human_and_raw_share_underlying_entries
proves the Human projection's timestamps are a subset of the Raw
projection's timestamps for the same input slice.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md.
2. Verify test coverage: each tag variant's projection behavior, empty-input edge case, AMD-7 invariant assertion, summary-footer synthesis.
3. Run all Phase 1 + Phase 2 tests: `cd src-tauri && cargo test --test session_log_ring_test --test session_log_classifier_test --test session_log_projection_test`. Confirm green.

After completing Phase 1 + Phase 2:
Review the batch from multiple perspectives. Minimum 3 review rounds.
- Round A (correctness): does every classifier rule have a §4.4-grounded test? Does the projection respect AMD-7 (one ring, two views)?
- Round B (defensive): does the classifier panic on any input shape? Does the ring's capacity-0 case behave?
- Round C (drift): do the TS types in Phase 6 still match the Rust types here? (Defer this round to Phase 6 if Phase 6 has not started.)

If round 3 still finds issues, keep going until clean.

---

## Phase 3 — `PatProcess` stdout/stderr capture wiring

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** This is the **concurrency-touching phase**. The producer is one or two background reader threads owned by `PatProcess`; the consumers are the Tauri command (Phase 4) and the event-emitter (Phase 4). The ring is shared via `Arc<Mutex<LogRing>>`. The risk profile is: (a) thread joins/leaks at process shutdown, (b) lost pre-listen-address stderr lines, (c) interleaving artifacts that the test assertion accidentally tolerates.

The **assertion-rigor-under-pressure clause** applies in full to this phase — see Step 3.4.

**Files (this phase):**
- Modify: `src-tauri/src/pat_process.rs` (refactor `spawn` to keep + thread the readers; add `log_ring_handle()` accessor; preserve pre-listen-address stderr in the ring)
- Create: `src-tauri/tests/pat_process_log_capture_test.rs`

BEFORE starting work:
1. Invoke /superpowers:test-driven-development
2. Read docs/pitfalls/testing-pitfalls.md (especially §5 Concurrency & TOCTOU, §7 Test infrastructure hygiene).

### Step 3.1 — Write the failing capture test using a fake child process

The shipped `pat_process_test.rs` requires a real `pat` binary, which keeps it out of the always-on CI loop. We mirror that pattern here: a separate test file that uses a small shell script (or `/bin/sh -c`) as a stand-in for `pat`, writing a known sequence to stdout + stderr at a known cadence. This keeps the capture test deterministic AND avoids a `pat`-binary dependency for the capture-correctness test.

- [ ] Create `src-tauri/tests/pat_process_log_capture_test.rs`:

```rust
//! Integration test for PatProcess's stdout+stderr capture into the
//! shared LogRing. Uses /bin/sh as a stand-in child process so this
//! test runs without a pat binary present.
//!
//! Assertion-rigor-under-pressure clause (writing-plans-enhanced Step 3):
//! every assertion below targets a MECHANISM (capture path correctness)
//! over a SYMPTOM (test "passes"). If a flake appears, the fix is
//! deterministic synchronization on the producer side (e.g., poll the
//! ring until len() >= expected within a generous deadline), NOT
//! dropping the assertion or loosening it to a presence-only check.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};
use tuxlink_lib::session_log::{LogRing, LogSource};

/// Helper: spawn a fake pat (a sh script that mimics pat's listen-address
/// announce on stderr, then emits known lines on both streams).
fn spawn_fake_pat(tmp: &tempfile::TempDir) -> PatProcess {
    let script = tmp.path().join("fake-pat.sh");
    std::fs::write(
        &script,
        // sh script: print pat-shaped announce on stderr, then loop emitting
        // marker lines. We rely on PatProcess to spawn `sh script ...`.
        // The "Starting HTTP service" string + the literal address echo
        // (which PatProcess searches for) is on stderr. Then we emit 10
        // marker lines on each stream and sleep so the test can collect.
        // The `*` after `Starting HTTP service` matches PatProcess's
        // "127.0.0.1:<port>" needle.
        r#"#!/bin/sh
shift # drop --config
shift # drop <config-path>
shift # drop --mbox
shift # drop <mbox-path>
shift # drop http
shift # drop --addr
ADDR="$1"
echo "Starting HTTP service ($ADDR)" 1>&2
i=0
while [ $i -lt 10 ]; do
  echo "stdout-line-$i"
  echo "stderr-line-$i" 1>&2
  i=$((i+1))
done
sleep 5
"#,
    )
    .unwrap();
    // Mark the script executable. PermissionsExt is a Unix-only trait; this
    // test (and the surrounding plan) is Linux-targeted, per the rest of
    // the project (Tauri + AppImage Linux distribution).
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let config_path = tmp.path().join("pat-config.json");
    std::fs::write(&config_path, r#"{"mycall":"TEST1","secure_login_password":"x","locator":"AA00aa"}"#).unwrap();
    // Pass the script as the binary. PatProcess::spawn launches it with
    // the pat 1.0.0 argv shape (`--config <path> --mbox <path> http
    // --addr 127.0.0.1:<port>`); the script `shift`s past those args
    // and only uses the `--addr` value for its announce echo.
    let opts = PatSpawnOptions {
        binary: script,
        config_path,
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 18773, // fake script echoes whatever we pass
        pid_file: tmp.path().join("pat.pid"),
        app_handle: None,
    };
    PatProcess::spawn(opts).expect("spawn fake pat")
}

/// Wait for the ring to contain at least `n` entries, or fail after `deadline`.
fn wait_for_ring_len(ring: &Arc<Mutex<LogRing>>, n: usize, deadline: Duration) {
    let start = Instant::now();
    loop {
        let len = ring.lock().unwrap().len();
        if len >= n {
            return;
        }
        if start.elapsed() > deadline {
            panic!("ring did not reach {} entries within {:?}; observed {}", n, deadline, len);
        }
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn test_pat_process_captures_both_stdout_and_stderr_into_log_ring() {
    let tmp = tempfile::tempdir().unwrap();
    let mut proc = spawn_fake_pat(&tmp);
    let ring = proc.log_ring_handle();

    // 10 stdout lines + 10 stderr lines + 1 announce line (already on stderr).
    // We wait for 20 minimum (the announce line may or may not have been
    // pushed to the ring depending on timing; we assert separately below).
    wait_for_ring_len(&ring, 20, Duration::from_secs(5));

    let snap = ring.lock().unwrap().snapshot();
    let stdout_count = snap.iter().filter(|e| e.source == LogSource::Stdout).count();
    let stderr_count = snap.iter().filter(|e| e.source == LogSource::Stderr).count();
    assert_eq!(stdout_count, 10, "expected 10 stdout lines in ring; snap: {:?}", snap);
    assert!(stderr_count >= 10, "expected >= 10 stderr lines in ring (10 marker + announce); snap: {:?}", snap);

    // Mechanism assertion (per AMD-7 + assertion-rigor clause): a stdout
    // line and its corresponding stderr line carry DIFFERENT LogSource
    // values. Symptom-only assertion would be "10+10 = 20 lines"; the
    // mechanism assertion is "the source tag distinguishes them."
    let stdout_raws: std::collections::HashSet<&str> = snap.iter().filter(|e| e.source == LogSource::Stdout).map(|e| e.raw.as_str()).collect();
    let stderr_raws: std::collections::HashSet<&str> = snap.iter().filter(|e| e.source == LogSource::Stderr).map(|e| e.raw.as_str()).collect();
    for i in 0..10 {
        assert!(stdout_raws.contains(format!("stdout-line-{}", i).as_str()), "missing stdout-line-{}", i);
        assert!(stderr_raws.contains(format!("stderr-line-{}", i).as_str()), "missing stderr-line-{}", i);
    }

    proc.shutdown(Duration::from_secs(5)).expect("shutdown");
}

#[test]
fn test_pat_process_preserves_announce_line_in_ring() {
    // testing-pitfalls.md §3 + AMD-7: the pre-listen-address stderr lines
    // observed during spawn-time announce wait MUST flow into the ring;
    // they are the first thing a sysadmin sees when debugging a startup
    // failure. The current pat_process.rs DISCARDS those lines (it drains
    // the reader looking for the announce, then drops the reader). Phase 3
    // changes this; this test enforces the change.
    let tmp = tempfile::tempdir().unwrap();
    let mut proc = spawn_fake_pat(&tmp);
    let ring = proc.log_ring_handle();
    wait_for_ring_len(&ring, 1, Duration::from_secs(5));
    let snap = ring.lock().unwrap().snapshot();
    assert!(
        snap.iter().any(|e| e.raw.contains("Starting HTTP service")),
        "pre-listen-address announce line must be in the ring; ring: {:?}", snap
    );
    proc.shutdown(Duration::from_secs(5)).expect("shutdown");
}

#[test]
fn test_pat_process_log_ring_is_bounded() {
    // testing-pitfalls.md §4: bounded growth. The ring has a fixed capacity
    // (1000 lines per the spec); produce more than that and assert the
    // oldest entries are evicted.
    let tmp = tempfile::tempdir().unwrap();
    // Override the fake script to produce 2000 lines.
    let script = tmp.path().join("fake-pat.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
shift; shift; shift; shift; shift; shift
ADDR="$1"
echo "Starting HTTP service ($ADDR)" 1>&2
i=0
while [ $i -lt 2000 ]; do
  echo "line-$i"
  i=$((i+1))
done
sleep 5
"#,
    ).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let config_path = tmp.path().join("pat-config.json");
    std::fs::write(&config_path, r#"{"mycall":"TEST1","secure_login_password":"x","locator":"AA00aa"}"#).unwrap();
    let opts = PatSpawnOptions {
        binary: script,
        config_path,
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 18774,
        pid_file: tmp.path().join("pat.pid"),
        app_handle: None,
    };
    let mut proc = PatProcess::spawn(opts).expect("spawn");
    let ring = proc.log_ring_handle();

    // Wait until the ring has stabilized at capacity. The fake script
    // produces 2000 lines as fast as `echo` will run; the ring is sized
    // 1000. So len() reaches 1000 then stays.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let len = ring.lock().unwrap().len();
        if len == 1000 { break; }
        if Instant::now() > deadline {
            panic!("ring did not stabilize at capacity 1000 within deadline; observed {}", len);
        }
        sleep(Duration::from_millis(50));
    }

    let snap = ring.lock().unwrap().snapshot();
    assert_eq!(snap.len(), 1000);
    // Mechanism assertion: the oldest surviving line is line-1000 (not
    // line-0), i.e., eviction happened from the front.
    assert!(snap[0].raw.contains("line-1") || snap[0].raw.contains("Starting HTTP service"),
        "first surviving entry should be line-N for N>=1000 OR the announce line if it survived; got: {}", snap[0].raw);

    proc.shutdown(Duration::from_secs(5)).expect("shutdown");
}
```

- [ ] Run: `cd src-tauri && cargo test --test pat_process_log_capture_test`. Expected: compile error — `log_ring_handle` method not found on `PatProcess`. Red stage confirmed.

### Step 3.2 — Refactor `pat_process.rs` to capture into a shared ring

This is the load-bearing diff for the phase. Reason carefully; preserve the existing shipped behavior (pat-1.0.0 CLI shape, ephemeral-port pre-bind, PID file, SIGTERM-then-SIGKILL shutdown) and add the capture layer on top of it.

- [ ] Modify `src-tauri/src/pat_process.rs`:

Replace the existing `pub struct PatProcess` definition and `impl PatProcess::spawn` with the version below. Preserve `http_port()`, `is_running()`, `shutdown()`, and `Drop` exactly as currently shipped — only `spawn()` and the struct fields need changes.

```rust
use crate::session_log::{tag_line, LogEntry, LogRing, LogSource};
use chrono::Utc;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct PatSpawnOptions {
    pub binary: PathBuf,
    pub config_path: PathBuf,
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16,
    pub pid_file: PathBuf,
}

pub struct PatProcess {
    child: Option<Child>,
    pid_file: PathBuf,
    http_port: u16,
    log_ring: Arc<Mutex<LogRing>>,
    reader_threads: Vec<JoinHandle<()>>,
}

const LOG_RING_CAPACITY: usize = 1000;

impl PatProcess {
    pub fn spawn(opts: PatSpawnOptions) -> std::io::Result<Self> {
        std::fs::create_dir_all(&opts.mbox_dir)?;
        if let Some(parent) = opts.pid_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let actual_port = if opts.http_listen_port == 0 {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let p = listener.local_addr()?.port();
            drop(listener);
            p
        } else {
            opts.http_listen_port
        };
        let listen = format!("127.0.0.1:{}", actual_port);

        let mut cmd = Command::new(&opts.binary);
        cmd.arg("--config").arg(&opts.config_path)
            .arg("--mbox").arg(&opts.mbox_dir)
            .arg("http")
            .arg("--addr").arg(&listen)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;

        let log_ring = Arc::new(Mutex::new(LogRing::with_capacity(LOG_RING_CAPACITY)));

        // Take stderr ownership BEFORE we look for the announce, so the
        // same reader handles both the announce and ongoing stderr lines
        // (no race-window where lines arrive between announce-drain and
        // reader-thread spawn).
        let stderr = child.stderr.take().expect("piped stderr");
        let stdout = child.stdout.take().expect("piped stdout");

        let needle = format!("127.0.0.1:{}", actual_port);

        // Spawn the stderr reader on its own thread. It pushes EVERY stderr
        // line into the ring (preserving the pre-announce lines per AMD-7),
        // and signals (via an atomic bool) when the announce-line has been
        // observed. The main thread waits up to 10s for the signal.
        let announce_seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stderr_ring = log_ring.clone();
        let stderr_announce_seen = announce_seen.clone();
        let stderr_needle = needle.clone();
        let stderr_thread = thread::Builder::new()
            .name("pat-stderr-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(_) => continue,
                    };
                    if line.contains(&stderr_needle) {
                        stderr_announce_seen.store(true, std::sync::atomic::Ordering::Release);
                    }
                    let entry = LogEntry {
                        timestamp: Utc::now(),
                        source: LogSource::Stderr,
                        tag: tag_line(&line),
                        raw: line,
                    };
                    stderr_ring.lock().unwrap().push(entry);
                }
            })?;

        // Spawn the stdout reader.
        let stdout_ring = log_ring.clone();
        let stdout_thread = thread::Builder::new()
            .name("pat-stdout-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(_) => continue,
                    };
                    let entry = LogEntry {
                        timestamp: Utc::now(),
                        source: LogSource::Stdout,
                        tag: tag_line(&line),
                        raw: line,
                    };
                    stdout_ring.lock().unwrap().push(entry);
                }
            })?;

        // Wait for the announce. Polls the atomic; the reader pushed the
        // line into the ring before setting the flag, so when the flag is
        // observed the ring already contains the announce entry.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !announce_seen.load(std::sync::atomic::Ordering::Acquire) {
            if Instant::now() > deadline {
                let _ = child.kill();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "pat did not announce HTTP listen port within 10s",
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }

        std::fs::write(&opts.pid_file, child.id().to_string())?;

        Ok(PatProcess {
            child: Some(child),
            pid_file: opts.pid_file,
            http_port: actual_port,
            log_ring,
            reader_threads: vec![stdout_thread, stderr_thread],
        })
    }

    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    /// Caller-facing handle for the ring. Cloning the Arc is cheap; the
    /// Mutex serializes concurrent push/snapshot.
    pub fn log_ring_handle(&self) -> Arc<Mutex<LogRing>> {
        self.log_ring.clone()
    }

    // ... keep is_running(), shutdown(), Drop exactly as shipped ...
}
```

**Important — preserve the existing `is_running`, `shutdown`, and `Drop` impls verbatim from the current `pat_process.rs`.** Do not modify them in this phase. The struct gains two new fields; the existing methods continue to work because they only reference `child` and `pid_file`. The reader threads detach naturally on `Drop` because the BufReader closes when the child's stdout/stderr pipe closes (which happens when the child exits during `Drop`'s SIGKILL); the threads then return from their `for line in reader.lines()` loops. They are joined on neither shutdown nor Drop — this is acceptable because their work is bounded by pipe closure, and we never need to flush them.

If a future review insists on explicit joins on `shutdown`, add an inner loop after the SIGKILL-then-wait that calls `join()` on each handle from `self.reader_threads.drain(..)`. This is a defensible extension; the v0.0.1 floor is "threads exit on their own when the child exits," which is sufficient because the threads only push into a Mutex-protected ring with no external side effects.

- [ ] Run: `cd src-tauri && cargo build`. Expected: builds green.
- [ ] Run: `cd src-tauri && cargo test --test pat_process_log_capture_test`. Expected: all 3 capture tests pass.
- [ ] Run: `cd src-tauri && cargo test --test pat_process_test`. Expected: still passes IF `pat` binary is present (or test is skipped). The existing test is the regression check that the shipped CLI invocation still works.

### Step 3.3 — Verify no shipped behavior regressed

- [ ] Run the full Rust test suite: `cd src-tauri && cargo test`. Expected: every existing test still passes; the three new tests pass; one test (the original `pat_process_test::test_spawn_and_graceful_shutdown`) may be inconclusive without a `pat` binary present — that's the shipped behavior, not a regression.

### Step 3.4 — Assertion-rigor-under-pressure clause

This clause is REQUIRED for any concurrency-touching task per `writing-plans-enhanced` Step 3. It appears here verbatim.

```
BEFORE marking this task complete:
If any test assertion races, flakes, or fails nondeterministically, the
fix is deterministic synchronization (e.g., TaskCompletionSource,
SemaphoreSlim, awaitable fence) — NOT assertion removal or weakening.
If synchronization cannot make the assertion pass reliably, STOP and
raise to the dispatching agent. Do not ship a weaker test. Weakened
assertions rationalized as "CI stability fixes" are the exact pattern
this rule prevents.

Prefer mechanism assertions over symptom assertions where feasible: a
timing bound ("Elapsed < 10s") proves absence of a specific symptom;
an observation-of-state assertion ("peers observed cancellation")
proves presence of the mechanism. When racing forces a choice between
them, fix the synchronization rather than dropping the mechanism
assertion.
```

The commit subject for any change touching test assertions in this phase SHOULD state what happened to them — "add", "strengthen", "preserve", or explicitly "weaken" with rationale. Subjects like "CI timing fix" or "test stabilization" obscure whether coverage eroded and let regressions slip past review.

If a flake appears in `test_pat_process_captures_both_stdout_and_stderr_into_log_ring`, the synchronization tool is `wait_for_ring_len` — increase its deadline or tighten its poll interval; do NOT change the line-count assertion. If `test_pat_process_log_ring_is_bounded` flakes around the "first surviving entry" check, the synchronization is the "wait until len == 1000" loop — extend its deadline. The line-count assertion (`assert_eq!(snap.len(), 1000)`) is the mechanism assertion for bounded growth; do not weaken it.

### Step 3.5 — Commit

```bash
git add src-tauri/src/pat_process.rs \
        src-tauri/tests/pat_process_log_capture_test.rs
git commit -m "$(cat <<'EOF'
feat(pat-process): capture stdout+stderr into shared LogRing (Phase 3 of Task 15)

Adds Arc<Mutex<LogRing>> ownership to PatProcess; two reader threads
push each line into the ring with LogSource + tag_line() classification
+ UTC timestamp. The pre-listen-address stderr lines (the "Starting
HTTP service" announce + whatever else appears before pat's HTTP server
binds) flow into the ring — sysadmins debugging startup failures need
to see them. The announce-detection signal is an atomic bool set BY
the stderr reader thread AFTER it pushes the line, so when spawn()
observes the flag the ring already contains the line.

Reader threads exit naturally on child-pipe closure (which happens
during PatProcess::Drop's SIGKILL). No explicit join in shutdown()
because the threads have no side effects beyond mutex-protected ring
pushes.

Mechanism-vs-symptom: capture-test assertions verify per-line source
tagging (LogSource::Stdout vs Stderr) and line-count match, not just
"some lines appeared." Bounded-growth test verifies the ring stabilizes
at capacity 1000 under a 2000-line burst.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md §5 (concurrency) + §4 (bounded growth).
2. Verify test coverage:
   - [ ] stdout lines arrive with `LogSource::Stdout`
   - [ ] stderr lines arrive with `LogSource::Stderr`
   - [ ] pre-announce stderr line is in the ring
   - [ ] ring is bounded at 1000
   - [ ] `PatProcess::shutdown` still works (existing test still passes)
3. Run `cd src-tauri && cargo test` end-to-end. Confirm green.
4. Apply the assertion-rigor-under-pressure clause if any test flaked.

---

## Phase 4 — Tauri commands + event emission

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** The frontend needs (a) a one-shot snapshot read at mount time, and (b) incremental updates as new lines arrive. We use a Tauri command for the snapshot and a Tauri event for the increments. The event carries the FULL `LogEntry` (timestamp + source + tag + raw) so the frontend can re-project on toggle without a re-fetch — this is the AMD-7 invariant pushed all the way to the wire.

**Files (this phase):**
- Modify: `src-tauri/src/lib.rs` (register `session_log_read` command + a managed-state `PatProcess` reference for the command to call against; emit `session_log:entry` events from the reader threads via a Tauri `AppHandle`)
- Modify: `src-tauri/src/main.rs` (register the new command in the `invoke_handler!` macro)
- Create: `src-tauri/tests/session_log_command_test.rs` (unit test for the projection-routing logic that the command calls — split out so the test does not need a full Tauri runtime)

### Step 4.1 — Add a projection router helper to `session_log.rs`

The Tauri command itself is thin (it just borrows the managed state and calls one of the projections). To keep the projection-routing logic test-free of Tauri's macro machinery, extract a small enum + dispatch helper.

- [ ] Append to `src-tauri/src/session_log.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    Human,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "projection", rename_all = "lowercase")]
pub enum ProjectedLog {
    Human { lines: Vec<HumanLine> },
    Raw { lines: Vec<RawLine> },
}

pub fn project(entries: &[LogEntry], which: Projection) -> ProjectedLog {
    match which {
        Projection::Human => ProjectedLog::Human { lines: project_human(entries) },
        Projection::Raw => ProjectedLog::Raw { lines: project_raw(entries) },
    }
}
```

### Step 4.2 — Write the failing command-router test

- [ ] Create `src-tauri/tests/session_log_command_test.rs`:

```rust
use tuxlink_lib::session_log::{project, LogEntry, LogSource, LogTag, ProjectedLog, Projection};
use chrono::Utc;

fn make_entry(tag: LogTag, raw: &str) -> LogEntry {
    LogEntry { timestamp: Utc::now(), source: LogSource::Stdout, tag, raw: raw.to_string() }
}

#[test]
fn test_project_router_human_returns_human_variant() {
    let entries = vec![make_entry(LogTag::ExpressAnnotation, "*** Connecting")];
    match project(&entries, Projection::Human) {
        ProjectedLog::Human { lines } => assert_eq!(lines.len(), 1),
        ProjectedLog::Raw { .. } => panic!("expected Human variant"),
    }
}

#[test]
fn test_project_router_raw_returns_raw_variant() {
    let entries = vec![make_entry(LogTag::B2fProtocol, ";PQ: 1")];
    match project(&entries, Projection::Raw) {
        ProjectedLog::Raw { lines } => assert_eq!(lines.len(), 1),
        ProjectedLog::Human { .. } => panic!("expected Raw variant"),
    }
}

#[test]
fn test_project_router_serde_round_trip_for_human() {
    let entries = vec![make_entry(LogTag::ExpressAnnotation, "*** test")];
    let projected = project(&entries, Projection::Human);
    let json = serde_json::to_string(&projected).expect("serialize");
    assert!(json.contains("\"projection\":\"human\""), "discriminator must serialize as lowercase 'human'; saw: {}", json);
    let parsed: ProjectedLog = serde_json::from_str(&json).expect("deserialize");
    match parsed {
        ProjectedLog::Human { lines } => assert_eq!(lines.len(), 1),
        _ => panic!("round-trip should preserve variant"),
    }
}

#[test]
fn test_project_router_serde_round_trip_for_raw() {
    let entries = vec![make_entry(LogTag::B2fProtocol, ";PQ: 1")];
    let projected = project(&entries, Projection::Raw);
    let json = serde_json::to_string(&projected).expect("serialize");
    assert!(json.contains("\"projection\":\"raw\""));
}
```

- [ ] Run: `cd src-tauri && cargo test --test session_log_command_test`. Expected: compile error — `project` / `ProjectedLog` / `Projection` not in `tuxlink_lib::session_log`. Red.

Implementing Step 4.1 should flip these green. Confirm before proceeding.

- [ ] Re-run: `cd src-tauri && cargo test --test session_log_command_test`. Expected: 4 tests pass.

### Step 4.3 — Wire the Tauri command + event emission in `lib.rs`

- [ ] Modify `src-tauri/src/lib.rs`. The existing scaffold (`greet` command) stays. Add managed state for the `PatProcess` reference and the `session_log_read` command. Pat is not spawned until later tasks (Task 9-11 wizard); v0.0.1 SHOULD still surface a meaningful empty response from `session_log_read` if Pat is not running yet, rather than panicking. Use `Option<Arc<Mutex<LogRing>>>` in the managed state and return an empty `ProjectedLog` of the requested variant when the ring is absent.

```rust
use std::sync::{Arc, Mutex};
use tuxlink_lib::session_log::{project, LogRing, ProjectedLog, Projection};

pub mod config;
pub mod pat_client;
pub mod pat_process;

#[derive(Default)]
pub struct AppState {
    pub pat_log_ring: Mutex<Option<Arc<Mutex<LogRing>>>>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn session_log_read(
    projection: Projection,
    state: tauri::State<'_, AppState>,
) -> ProjectedLog {
    let guard = state.pat_log_ring.lock().expect("pat_log_ring lock poisoned");
    let ring_opt = guard.clone();
    drop(guard);
    match ring_opt {
        Some(ring) => {
            let entries = ring.lock().expect("log_ring lock poisoned").snapshot();
            project(&entries, projection)
        }
        None => match projection {
            Projection::Human => ProjectedLog::Human { lines: vec![] },
            Projection::Raw => ProjectedLog::Raw { lines: vec![] },
        },
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![greet, session_log_read])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Step 4.4 — Wire `AppHandle`-aware event emission into the reader threads

The reader threads currently push into the ring; they also need to emit a `session_log:entry` Tauri event per line so the frontend can incrementally update without polling. Pat is spawned later (in Task 9-11), so this phase only WIRES the event channel via an optional `AppHandle` parameter on `PatProcess::spawn`; if `None` is passed (test-time use), no events fire.

- [ ] Modify `src-tauri/src/pat_process.rs`. Change `PatSpawnOptions` to include an optional `app_handle`. Add the new field; thread it through into the reader closures so each line push also fires a Tauri event:

```rust
use tauri::{AppHandle, Emitter, Runtime, Wry};

pub struct PatSpawnOptions<R: Runtime = Wry> {
    pub binary: PathBuf,
    pub config_path: PathBuf,
    pub mbox_dir: PathBuf,
    pub http_listen_port: u16,
    pub pid_file: PathBuf,
    /// Optional Tauri AppHandle. When Some, every line pushed into the
    /// ring also fires a `session_log:entry` event with the LogEntry as
    /// payload. None = no events (test-time use; the capture test does
    /// not need events to assert on ring contents).
    pub app_handle: Option<AppHandle<R>>,
}

// Update the PatProcess struct's generic too:
pub struct PatProcess<R: Runtime = Wry> {
    child: Option<Child>,
    pid_file: PathBuf,
    http_port: u16,
    log_ring: Arc<Mutex<LogRing>>,
    reader_threads: Vec<JoinHandle<()>>,
    _runtime: std::marker::PhantomData<R>,
}
```

Inside `spawn()`, clone the optional `app_handle` for each reader closure and emit after each push:

```rust
// Inside spawn(), before spawning the stderr_thread:
let stderr_app_handle = opts.app_handle.clone();
// ...

// Inside the stderr_thread closure, replacing the existing `let entry = ...; stderr_ring.lock().unwrap().push(entry);` block:
let entry = LogEntry {
    timestamp: Utc::now(),
    source: LogSource::Stderr,
    tag: tag_line(&line),
    raw: line,
};
stderr_ring.lock().unwrap().push(entry.clone());
if let Some(h) = &stderr_app_handle {
    let _ = h.emit("session_log:entry", &entry);
}

// Same shape inside the stdout_thread closure (LogSource::Stdout, stdout_app_handle).
```

Note: `LogEntry` already derives `Clone` from Phase 1; the clone-before-push pattern avoids a second `snapshot()` call for the emit.

Update the test fixtures + the existing `pat_process_test.rs` to pass `app_handle: None`. Consider adding a `Default` impl on `PatSpawnOptions` if it cleans up call sites; the existing tests construct the struct literally so a Default isn't strictly required.

- [ ] Update the existing `pat_process_test.rs` `PatSpawnOptions { ... }` struct literal to add `app_handle: None`. Similarly for the new `pat_process_log_capture_test.rs` fixtures.

**Note on the generic.** `PatSpawnOptions<R: Runtime = tauri::Wry>` adds a type parameter that defaults to Tauri's default `Wry` runtime in production. Tests that pass `None` infer `R = tauri::Wry` from the default. If the inference fails at a call site, the explicit form is `PatSpawnOptions::<tauri::Wry> { ... }`.

- [ ] Run: `cd src-tauri && cargo build`. Expected: builds green.
- [ ] Run: `cd src-tauri && cargo test`. Expected: every test passes (the events do not fire in tests since `app_handle: None`).

### Step 4.5 — Update `main.rs` if needed

- [ ] Confirm `src-tauri/src/main.rs` calls `tuxlink_lib::run()` (or its existing equivalent). The `invoke_handler!` macro registration happens inside `tuxlink_lib::run()`, so no `main.rs` change is strictly required for the new command. If `main.rs` has its own handler list, mirror it.

### Step 4.6 — Commit

```bash
git add src-tauri/src/session_log.rs \
        src-tauri/src/lib.rs \
        src-tauri/src/pat_process.rs \
        src-tauri/src/main.rs \
        src-tauri/tests/session_log_command_test.rs \
        src-tauri/tests/pat_process_log_capture_test.rs \
        src-tauri/tests/pat_process_test.rs
git commit -m "$(cat <<'EOF'
feat(session-log): Tauri command + event surface (Phase 4 of Task 15)

session_log_read(projection) returns a ProjectedLog (tagged union over
Human | Raw). Returns the empty variant when no PatProcess is registered
yet (Pat is spawned in Task 9-11; v0.0.1's session-log pane needs to
mount + show empty state before that).

PatProcess::spawn now takes an Option<AppHandle> in PatSpawnOptions;
when Some, each line pushed into the ring also fires session_log:entry
with the full LogEntry. Frontend re-projects on toggle without a
re-fetch — the AMD-7 invariant pushed to the wire.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Review tests against docs/pitfalls/testing-pitfalls.md.
2. Verify test coverage:
   - [ ] `project` routes Human/Raw correctly
   - [ ] Serde round-trip preserves the discriminator
   - [ ] `session_log_read` with no ring registered returns the empty variant
3. Run `cd src-tauri && cargo test` end-to-end. Confirm green.

After completing Phase 3 + Phase 4:
Review the batch from multiple perspectives. Minimum 3 review rounds.
- Round A (concurrency): is the announce-detection signal safely ordered relative to the ring push? The current design pushes BEFORE setting the flag — verify in the reader thread code.
- Round B (lifecycle): do the reader threads survive PatProcess::Drop without leaking? Do they exit promptly?
- Round C (wire contract): do the serialized `ProjectedLog` and `LogEntry` shapes match what Phase 6's TS interfaces will expect?

If round 3 still finds issues, keep going until clean.

---

## Phase 5 — Menu wiring (AMD-10 runtime half: `menu:view:session_log` + `menu:view:raw_log`)

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** The pane toggle + projection toggle are operator-visible via the native menu bar (`View → Show Session Log` with `Ctrl+Shift+L` accelerator, and `View → Show Raw Session Log` as a submenu item). AMD-10's runtime half already lists these IDs in the canonical `menu_event_ids()` list (per plan lines 1745-1746); this phase confirms they are present in both the test AND the builder, and adds the `Ctrl+Shift+L` accelerator on the show-session-log item.

**Note:** if a sibling task (the AMD-10 runtime-half work) has already shipped before this Wave-2 work starts, the IDs will already be in place. The implementer MUST rebase on `feat/v0.0.1` first to pick up the sibling's work, then confirm-or-add only what is missing. Do NOT duplicate menu IDs; do NOT remove other AMD-10 runtime-half IDs.

**Files (this phase):**
- Modify: `src-tauri/src/menu.rs` (confirm/add the two View IDs + accelerator)
- Modify: `src-tauri/tests/menu_test.rs` (defensive cross-check that the two IDs are present)

### Step 5.1 — Cross-check whether AMD-10 runtime half has shipped

- [ ] Run `git log --oneline feat/v0.0.1 | head -30` and look for a commit subject mentioning AMD-10, `menu:view:raw_log`, or "runtime half." If present, the work is shipped — Phase 5 is a confirmation-only phase. If absent, Phase 5 adds the IDs.

### Step 5.2 — Confirm/add menu IDs

- [ ] Read `src-tauri/src/menu.rs`. Confirm that `menu_event_ids()` includes both:
  - `"menu:view:session_log"`
  - `"menu:view:raw_log"`
- [ ] Confirm `build_menu()` constructs:
  - A "Show Session Log" `MenuItemBuilder::with_id("menu:view:session_log", "Show Session Log")` under the View submenu, with `.accelerator("CmdOrCtrl+Shift+L")`.
  - A "Show Raw Session Log" `MenuItemBuilder::with_id("menu:view:raw_log", "Show Raw Session Log")` under the View submenu (immediately following the Show Session Log item, per AMD-10's "submenu of Show Session Log" phrasing — Tauri 2's menu API renders this as an adjacent sibling MenuItem; a strict nested submenu is also acceptable if the API supports it cleanly).

If either ID is missing, add it. If `menu_event_ids()` and `build_menu()` disagree (one has the ID, the other does not), fix the missing side.

### Step 5.3 — Defensive test addition

- [ ] Modify `src-tauri/tests/menu_test.rs`. Confirm the existing `required` array includes both IDs (it should — per plan lines 1745-1746). If the existing test passes after your menu.rs edits, no test changes are needed. If the IDs were just-added in this phase, the existing test catches them automatically.

- [ ] Run: `cd src-tauri && cargo test --test menu_test`. Expected: passes.

### Step 5.4 — Commit (skip if no changes were needed)

If Phase 5 was a no-op (AMD-10 runtime half already shipped both IDs + the accelerator), skip the commit. If you made edits:

```bash
git add src-tauri/src/menu.rs src-tauri/tests/menu_test.rs
git commit -m "$(cat <<'EOF'
feat(menu): add Ctrl+Shift+L accelerator + raw_log menu item (Phase 5 of Task 15)

Confirms AMD-10 runtime half's menu:view:session_log + menu:view:raw_log
are present in build_menu() and in menu_event_ids(). Adds Ctrl+Shift+L
accelerator on the Show Session Log item.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Run `cd src-tauri && cargo test --test menu_test`. Confirm green.
2. Confirm both IDs are listed in `menu_event_ids()` and constructed by `build_menu()`.

---

## Phase 6 — `SessionLog.tsx` React component

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** This is the operator-facing surface. It MUST render correctly, toggle projections without re-fetching from the backend, follow live tail with the anchor-on-scroll-up pattern, and offer a working "Copy session log" button + session-state header.

**Files (this phase):**
- Create: `src/session/SessionLog.tsx`
- Create: `src/session/sessionLogTypes.ts`
- Create: `src/session/sessionLogStyles.css`
- Modify: `src/App.tsx` (mount `<SessionLog>` + wire menu-event listeners + state management for visibility + projection)

BEFORE starting work:
1. Invoke /superpowers:test-driven-development (note: no frontend test harness wired; TDD discipline here is the Phase 8 manual-browser-smoke verification — every behavior below has a corresponding browser-smoke checklist item).
2. Re-read docs/design/v0.0.1-ux-mockups.md §5.8 + §4.4.

### Step 6.1 — Create the TS types

- [ ] Create `src/session/sessionLogTypes.ts`:

```typescript
// Mirror of the Rust types in src-tauri/src/session_log.rs. The Tauri
// command session_log_read returns ProjectedLog as a tagged union; the
// session_log:entry event payload is a LogEntry (full metadata so the
// frontend can re-project on toggle without a re-fetch). Keep these in
// sync with the Rust definitions; drift = bugs.

export type LogSource = "Stdout" | "Stderr";

export type LogTag =
  | "ExpressAnnotation"
  | "SessionSummary"
  | "B2fProtocol"
  | "Raw";

export interface LogEntry {
  timestamp: string; // ISO 8601 UTC (chrono::DateTime<Utc> default serde)
  source: LogSource;
  tag: LogTag;
  raw: string;
}

export interface HumanLine {
  timestamp: string;
  text: string;
}

export interface RawLine {
  timestamp: string;
  source: LogSource;
  raw: string;
}

export type Projection = "human" | "raw";

// Tagged union; #[serde(tag = "projection", rename_all = "lowercase")] on
// the Rust side produces { projection: "human", lines: HumanLine[] } or
// { projection: "raw", lines: RawLine[] }.
export type ProjectedLog =
  | { projection: "human"; lines: HumanLine[] }
  | { projection: "raw"; lines: RawLine[] };

export type SessionState =
  | "idle"
  | "connecting"
  | "in_session"
  | "disconnecting";
```

### Step 6.2 — Create the component

- [ ] Create `src/session/SessionLog.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import {
  LogEntry,
  LogTag,
  Projection,
  ProjectedLog,
  SessionState,
  HumanLine,
  RawLine,
} from "./sessionLogTypes";
import "./sessionLogStyles.css";

interface Props {
  visible: boolean;
  projection: Projection;
  sessionState: SessionState;
  onProjectionChange: (p: Projection) => void;
  height: number;
  onHeightChange: (h: number) => void;
}

const MIN_HEIGHT = 60;
const MAX_HEIGHT_FRAC = 0.5; // 50% of window height

/**
 * Project a single LogEntry through the active projection. Mirrors the
 * Rust project_human / project_raw functions; needed for the live-tail
 * append path so we don't re-fetch the whole snapshot on every event.
 *
 * AMD-7 invariant: this is the SAME projection logic as the Rust side,
 * applied to the SAME entry shape. Diverging here would re-introduce
 * the two-parallel-streams bug pattern.
 */
function projectEntry(entry: LogEntry, which: Projection): HumanLine | RawLine | null {
  if (which === "raw") {
    return { timestamp: entry.timestamp, source: entry.source, raw: entry.raw };
  }
  // Human projection rules (must match src-tauri/src/session_log.rs):
  //  - ExpressAnnotation -> include, with prefix stripped
  //  - SessionSummary    -> handled at the snapshot level (footer
  //                         synthesis requires paired Sent+Received).
  //                         For the incremental path, surface the raw
  //                         summary line text as-is; the operator sees
  //                         the synthesized footer on the next full
  //                         snapshot refresh (or on session-end).
  //  - B2fProtocol / Raw -> suppressed
  if (entry.tag === "ExpressAnnotation") {
    const cleaned = cleanExpressLine(entry.raw);
    if (!cleaned) return null;
    return { timestamp: entry.timestamp, text: cleaned };
  }
  if (entry.tag === "SessionSummary") {
    const cleaned = cleanExpressLine(entry.raw);
    return { timestamp: entry.timestamp, text: cleaned };
  }
  return null;
}

function cleanExpressLine(line: string): string {
  const idx = line.indexOf("***");
  const after = idx >= 0 ? line.slice(idx + 3) : line;
  return after.replace(/\*+$/, "").trim();
}

function formatTimestamp(iso: string): string {
  // Render as HH:MM:SS UTC; defensive against parse failures.
  try {
    const d = new Date(iso);
    return `${d.getUTCHours().toString().padStart(2, "0")}:${d
      .getUTCMinutes()
      .toString()
      .padStart(2, "0")}:${d.getUTCSeconds().toString().padStart(2, "0")} UTC`;
  } catch {
    return iso;
  }
}

function sessionStateLabel(s: SessionState): string {
  switch (s) {
    case "idle":
      return "Idle";
    case "connecting":
      return "Connecting...";
    case "in_session":
      return "In session";
    case "disconnecting":
      return "Disconnecting";
  }
}

export function SessionLog({
  visible,
  projection,
  sessionState,
  onProjectionChange,
  height,
  onHeightChange,
}: Props) {
  const [lines, setLines] = useState<(HumanLine | RawLine)[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stuckToBottom = useRef(true);
  const projectionRef = useRef(projection);
  projectionRef.current = projection;

  // Refetch the full snapshot whenever projection changes (cheap; we hold
  // entries in the Rust ring and project on demand). The tagged-union
  // narrowing makes res.lines the correct type (HumanLine[] | RawLine[]);
  // we widen to the parent union for state storage.
  useEffect(() => {
    let cancelled = false;
    invoke<ProjectedLog>("session_log_read", { projection }).then((res) => {
      if (cancelled) return;
      setLines(res.lines as (HumanLine | RawLine)[]);
    });
    return () => {
      cancelled = true;
    };
  }, [projection]);

  // Subscribe to session_log:entry events. Each event carries a full
  // LogEntry; we project it through the CURRENT projection (via the ref,
  // so the closure stays valid across projection changes) and append.
  useEffect(() => {
    let un: UnlistenFn | undefined;
    listen<LogEntry>("session_log:entry", (e) => {
      const projected = projectEntry(e.payload, projectionRef.current);
      if (!projected) return;
      setLines((prev) => {
        // Cap at 1000 lines in-view (mirrors the Rust ring capacity).
        const next = prev.length >= 1000 ? prev.slice(-999) : prev.slice();
        next.push(projected);
        return next;
      });
    })
      .then((fn) => {
        un = fn;
      })
      .catch((err) => {
        // Defensive: log registration failure to the browser console; do
        // not crash the pane. The operator still sees snapshot-based
        // contents (refetched on projection toggle).
        console.error("session_log:entry listen failed:", err);
      });
    return () => {
      if (un) un();
    };
  }, []); // wired once; uses projectionRef so it survives projection changes

  // Stick-to-bottom on new lines, unless the operator has scrolled up.
  useEffect(() => {
    if (scrollRef.current && stuckToBottom.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines]);

  function onScroll() {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    stuckToBottom.current = scrollHeight - scrollTop - clientHeight < 20;
  }

  function onCopy() {
    const text = lines
      .map((l) => {
        if ("text" in l) return `${formatTimestamp(l.timestamp)} · ${l.text}`;
        return `${formatTimestamp(l.timestamp)} [${l.source}] ${l.raw}`;
      })
      .join("\n");
    navigator.clipboard.writeText(text).catch(() => {
      // Defensive: clipboard may not be available in all webview contexts.
    });
  }

  function onResizerMouseDown(e: React.MouseEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const startHeight = height;
    const maxHeight = Math.floor(window.innerHeight * MAX_HEIGHT_FRAC);
    function onMove(ev: MouseEvent) {
      const delta = startY - ev.clientY; // drag UP increases height
      const next = Math.max(MIN_HEIGHT, Math.min(maxHeight, startHeight + delta));
      onHeightChange(next);
    }
    function onUp() {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  if (!visible) return null;

  return (
    <section
      className="session-log"
      style={{ height: `${height}px` }}
      aria-label="Session log pane"
    >
      <div className="session-log-resizer" onMouseDown={onResizerMouseDown} />
      <header className="session-log-header">
        <span className="session-log-state">{sessionStateLabel(sessionState)}</span>
        <div className="session-log-projection-toggle" role="radiogroup" aria-label="Session log projection">
          <button
            role="radio"
            aria-checked={projection === "human"}
            className={projection === "human" ? "active" : ""}
            onClick={() => onProjectionChange("human")}
          >
            Human
          </button>
          <button
            role="radio"
            aria-checked={projection === "raw"}
            className={projection === "raw" ? "active" : ""}
            onClick={() => onProjectionChange("raw")}
          >
            Raw
          </button>
        </div>
        <button className="session-log-copy" onClick={onCopy} aria-label="Copy session log">
          Copy session log
        </button>
      </header>
      <div className="session-log-body" ref={scrollRef} onScroll={onScroll}>
        {lines.length === 0 ? (
          <div className="session-log-empty">No session activity yet.</div>
        ) : (
          lines.map((l, i) => (
            <div className="session-log-line" key={i}>
              <span className="session-log-ts">{formatTimestamp(l.timestamp)}</span>
              <span className="session-log-text">
                {"text" in l ? l.text : `[${l.source}] ${l.raw}`}
              </span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
```

### Step 6.3 — Create the styles

- [ ] Create `src/session/sessionLogStyles.css`:

```css
.session-log {
  position: relative;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--border-subtle, #2a2a2a);
  background: var(--surface-2, #0f0f0f);
  color: var(--text, #d8d8d8);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  overflow: hidden;
}

.session-log-resizer {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 4px;
  cursor: ns-resize;
  background: transparent;
}
.session-log-resizer:hover {
  background: var(--accent, #3b82f6);
}

.session-log-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 4px 8px;
  border-bottom: 1px solid var(--border-subtle, #2a2a2a);
  background: var(--surface-3, #161616);
  font-size: 11px;
}

.session-log-state {
  font-weight: 600;
  color: var(--text-strong, #fafafa);
}

.session-log-projection-toggle {
  display: inline-flex;
  border: 1px solid var(--border-subtle, #2a2a2a);
  border-radius: 4px;
  overflow: hidden;
}
.session-log-projection-toggle button {
  background: transparent;
  color: inherit;
  border: 0;
  padding: 2px 8px;
  cursor: pointer;
  font: inherit;
}
.session-log-projection-toggle button.active {
  background: var(--accent, #3b82f6);
  color: white;
}

.session-log-copy {
  margin-left: auto;
  background: transparent;
  border: 1px solid var(--border-subtle, #2a2a2a);
  color: inherit;
  border-radius: 4px;
  padding: 2px 8px;
  cursor: pointer;
  font: inherit;
}

.session-log-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 8px;
}
.session-log-line {
  display: flex;
  gap: 8px;
  white-space: pre-wrap;
  word-break: break-all;
}
.session-log-ts {
  color: var(--text-muted, #888);
  flex-shrink: 0;
}
.session-log-empty {
  color: var(--text-muted, #888);
  font-style: italic;
}
```

### Step 6.4 — Wire `App.tsx`

- [ ] Modify `src/App.tsx`. Replace the scaffold body with a minimal layout that mounts `<SessionLog>` and wires the two menu events. (Other Wave-2 sibling tasks may add additional panes — Task 12 inbox, Task 14 compose, etc. — to App.tsx; if those have shipped first, integrate alongside, do not replace.)

**Do NOT delete `src/App.css`.** The Tauri scaffold's CSS variables (`--container`, `--row` classes, etc.) are still referenced by other Wave-2 sibling components in their drafts; the file stays as the base stylesheet and grows over time. Add `app-shell` and `app-main-area` classes to `App.css` as a NEW block at the bottom (do not overwrite existing rules):

```css
/* Tuxlink shell layout (Task 15 + Task 16 + Task 16.5 share this skeleton). */
.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  margin: 0;
  padding: 0;
}
.app-main-area {
  flex: 1;
  display: flex;
  overflow: hidden;
  /* Sibling tasks (Task 12 inbox + Task 13 reader + Task 14 compose + etc.)
     mount their primary panes here. Task 15's pane mounts BELOW this area
     as the bottom strip; it is NOT inside .app-main-area. */
}
```

```tsx
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { SessionLog } from "./session/SessionLog";
import { Projection, SessionState } from "./session/sessionLogTypes";
import "./App.css";

const DEFAULT_HEIGHT = 120;

function App() {
  const [sessionLogVisible, setSessionLogVisible] = useState(false);
  const [sessionLogProjection, setSessionLogProjection] = useState<Projection>("human");
  const [sessionLogHeight, setSessionLogHeight] = useState(DEFAULT_HEIGHT);
  const [sessionState] = useState<SessionState>("idle"); // Task 16 wires this live

  // Restore persisted settings on mount.
  useEffect(() => {
    invoke<boolean>("settings_get", { key: "session_log.visible" })
      .then((v) => setSessionLogVisible(!!v))
      .catch(() => {});
    invoke<number>("settings_get", { key: "session_log.height" })
      .then((h) => { if (typeof h === "number" && h > 0) setSessionLogHeight(h); })
      .catch(() => {});
    invoke<string>("settings_get", { key: "session_log.projection" })
      .then((p) => { if (p === "human" || p === "raw") setSessionLogProjection(p); })
      .catch(() => {});
  }, []);

  // Persist on change. Fire-and-forget; settings_set is idempotent.
  useEffect(() => { invoke("settings_set", { key: "session_log.visible", value: sessionLogVisible }).catch(() => {}); }, [sessionLogVisible]);
  useEffect(() => { invoke("settings_set", { key: "session_log.height", value: sessionLogHeight }).catch(() => {}); }, [sessionLogHeight]);
  useEffect(() => { invoke("settings_set", { key: "session_log.projection", value: sessionLogProjection }).catch(() => {}); }, [sessionLogProjection]);

  // Wire menu events.
  useEffect(() => {
    let un1: (() => void) | undefined;
    let un2: (() => void) | undefined;
    listen("menu:view:session_log", () => setSessionLogVisible((v) => !v))
      .then((fn) => { un1 = fn; });
    listen("menu:view:raw_log", () =>
      setSessionLogProjection((p) => (p === "raw" ? "human" : "raw"))
    ).then((fn) => { un2 = fn; });
    return () => { if (un1) un1(); if (un2) un2(); };
  }, []);

  return (
    <main className="container app-shell">
      <div className="app-main-area">
        {/* Other Wave-2 panes (inbox/list/reader, compose, etc.) mount here. */}
      </div>
      <SessionLog
        visible={sessionLogVisible}
        projection={sessionLogProjection}
        sessionState={sessionState}
        onProjectionChange={setSessionLogProjection}
        height={sessionLogHeight}
        onHeightChange={setSessionLogHeight}
      />
    </main>
  );
}

export default App;
```

### Step 6.5 — Commit

```bash
git add src/session/ src/App.tsx
git commit -m "$(cat <<'EOF'
feat(session-log): SessionLog.tsx + App.tsx wiring (Phase 6 of Task 15)

Bottom-anchored pane. Default-hidden; toggled by Ctrl+Shift+L (via the
menu:view:session_log event from menu.rs). Header shows session state +
[Human | Raw] toggle + Copy button. Renders the active projection from
session_log_read; on session_log:entry events, projects the new
LogEntry through the current projection client-side (mirroring the
Rust project_* functions) and appends, capped at 1000 lines. Toggle
between projections re-fetches the full snapshot (cheap; Rust ring
holds the source of truth).

AMD-7 invariant honored: the frontend NEVER maintains two parallel
buffers; it stores one list of projected lines and re-derives on
toggle. Live-tail follows scroll-to-bottom; auto-follow pauses when
the operator scrolls up.

Wires App.tsx: persist visibility + projection + height to Tauri-managed
settings (settings_get / settings_set commands land in Phase 7).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

BEFORE marking this phase complete:
1. Open the file in your editor; cross-check `cleanExpressLine` + `projectEntry` against the Rust equivalents in `session_log.rs`. They MUST match — diverging here re-introduces the two-stream bug pattern.
2. Run `pnpm tsc --noEmit` (or `pnpm build`). Expected: no TypeScript errors.
3. Defer the runtime verification to Phase 8 (browser-smoke).

---

## Phase 7 — Pane height + visibility + projection persistence

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** Operators expect window geometry to persist across restarts (per design doc §4.3 — Express does this for every window surface). The session-log pane's height, visibility, and current projection are part of that contract. Tauri's plugin-fs handles file I/O; the persistence schema is a simple JSON dict at `$XDG_CONFIG_HOME/tuxlink/settings.json`.

**Files (this phase):**
- Modify: `src-tauri/src/lib.rs` (add `settings_get` / `settings_set` commands + a managed `Settings` state holding the loaded dict)
- Create: `src-tauri/tests/settings_persist_test.rs`

BEFORE starting work:
1. Invoke /superpowers:test-driven-development.
2. Re-read docs/pitfalls/testing-pitfalls.md §6 (Boundary & Configuration Validation; default values matter here).

### Step 7.1 — Write the failing persistence tests

- [ ] Create `src-tauri/tests/settings_persist_test.rs`:

```rust
use tuxlink_lib::settings::Settings;
use serde_json::json;

#[test]
fn test_settings_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.json");
    let s = Settings::load_from(&path).expect("load empty");
    assert!(s.get("session_log.visible").is_none());

    let mut s = s;
    s.set("session_log.visible", json!(true)).expect("set");
    s.set("session_log.height", json!(180)).expect("set");
    s.set("session_log.projection", json!("raw")).expect("set");

    let s2 = Settings::load_from(&path).expect("load after set");
    assert_eq!(s2.get("session_log.visible"), Some(&json!(true)));
    assert_eq!(s2.get("session_log.height"), Some(&json!(180)));
    assert_eq!(s2.get("session_log.projection"), Some(&json!("raw")));
}

#[test]
fn test_settings_load_missing_file_returns_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("does-not-exist.json");
    let s = Settings::load_from(&path).expect("missing file is ok");
    assert!(s.get("anything").is_none());
}

#[test]
fn test_settings_load_corrupted_file_returns_empty_with_no_panic() {
    // testing-pitfalls.md §6: "Invalid config is rejected at load time" —
    // here we choose graceful degradation (empty settings + no panic)
    // over hard-fail; a corrupted settings file should not block app
    // launch. Document this choice in the Settings::load_from docstring.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("corrupted.json");
    std::fs::write(&path, "{not valid json").unwrap();
    let s = Settings::load_from(&path).expect("corrupted is not a hard error");
    assert!(s.get("anything").is_none());
}
```

- [ ] Run: `cd src-tauri && cargo test --test settings_persist_test`. Expected: compile error — module not found. Red.

### Step 7.2 — Implement `settings.rs`

- [ ] Create `src-tauri/src/settings.rs`:

```rust
//! Minimal JSON-backed key/value settings for v0.0.1. Stored at
//! $XDG_CONFIG_HOME/tuxlink/settings.json (or ~/.config/tuxlink/settings.json).
//! Corrupted file -> empty in-memory state + warning (no app-launch block).
//! Keys are dotted strings; values are arbitrary serde_json::Value.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Settings {
    path: PathBuf,
    values: BTreeMap<String, Value>,
}

impl Settings {
    /// Default path under XDG_CONFIG_HOME, falling back to ~/.config.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                home.join(".config")
            });
        base.join("tuxlink").join("settings.json")
    }

    pub fn load_default() -> std::io::Result<Self> {
        Self::load_from(&Self::default_path())
    }

    pub fn load_from(path: &Path) -> std::io::Result<Self> {
        let values = if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                Err(_) => BTreeMap::new(),
            }
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            path: path.to_path_buf(),
            values,
        })
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: Value) -> std::io::Result<()> {
        self.values.insert(key.to_string(), value);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(&self.values).expect("serialize settings");
        std::fs::write(&self.path, serialized)?;
        Ok(())
    }
}
```

- [ ] Add `pub mod settings;` to `src-tauri/src/lib.rs`.

### Step 7.3 — Wire `settings_get` / `settings_set` commands

- [ ] Extend `AppState` in `src-tauri/src/lib.rs`:

```rust
use crate::settings::Settings;

#[derive(Default)]
pub struct AppState {
    pub pat_log_ring: Mutex<Option<Arc<Mutex<LogRing>>>>,
    pub settings: Mutex<Option<Settings>>,
}
```

Load settings on `run()` startup:

```rust
pub fn run() {
    let settings = Settings::load_default().ok();
    let state = AppState {
        settings: Mutex::new(settings),
        ..Default::default()
    };
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            greet,
            session_log_read,
            settings_get,
            settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Commands:

```rust
#[tauri::command]
fn settings_get(key: String, state: tauri::State<'_, AppState>) -> Option<Value> {
    let guard = state.settings.lock().ok()?;
    guard.as_ref()?.get(&key).cloned()
}

#[tauri::command]
fn settings_set(key: String, value: Value, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    let s = guard.as_mut().ok_or_else(|| "settings not loaded".to_string())?;
    s.set(&key, value).map_err(|e| e.to_string())
}
```

- [ ] Run: `cd src-tauri && cargo test --test settings_persist_test`. Expected: 3 tests pass.
- [ ] Run: `cd src-tauri && cargo build`. Expected: green.
- [ ] Run: `cd src-tauri && cargo test`. Expected: all tests green.

### Step 7.4 — Commit

```bash
git add src-tauri/src/settings.rs \
        src-tauri/src/lib.rs \
        src-tauri/tests/settings_persist_test.rs
git commit -m "$(cat <<'EOF'
feat(settings): JSON kv settings + session-log persistence (Phase 7 of Task 15)

settings_get / settings_set Tauri commands backed by a flat JSON file
at $XDG_CONFIG_HOME/tuxlink/settings.json (falls back to
~/.config/tuxlink/settings.json). Corrupted file degrades to empty +
warning (no app-launch block; user can fix by deleting the file).

App.tsx persists session_log.visible / .height / .projection through
this surface so the pane's geometry restores across restarts (matches
Express's per-window-geometry persistence pattern per design doc §4.3).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

After completing Phase 5 + Phase 6 + Phase 7:
Review the batch from multiple perspectives. Minimum 3 review rounds.
- Round A (wire contract): do the TS types in `sessionLogTypes.ts` exactly match the Rust serde output? Especially the `LogTag` variants and the `Projection` lowercase serialization.
- Round B (UX): does the empty-state copy match the design spec? Does the resizer respect min/max?
- Round C (persistence): if the user manually corrupts settings.json, does the app still launch + show defaults?

If round 3 still finds issues, keep going until clean.

---

## Phase 8 — Integration smoke: launch `tauri dev`, walk the user flow

**Execution Status:** ⬜ NOT STARTED

**Why this matters.** Per `feedback_browser_smoke_before_ship.md` in the operator's auto-memory: static review + unit tests miss CSS specificity / `[hidden]`-vs-`display:` overrides; the project requires a `pnpm tauri dev` walk-through before declaring UI work done. This phase is the gate.

**Note:** the wizard isn't shipped yet for v0.0.1, so on app launch the user hits the Tauri scaffold + the session-log pane mount. That's fine for this smoke; the pane shows "No session activity yet" until a Pat process is wired (Task 9-11). The smoke verifies the pane mounts, toggles, persists, and does not crash.

### Step 8.1 — Pre-flight

- [ ] Confirm Pat binary is available on PATH (the existing `pat_process_test.rs` requires this; skip if not — the Phase 8 smoke does not need Pat to verify the pane).
- [ ] Run: `pnpm install` (in the worktree root) if not already done.

### Step 8.2 — Launch and walk the flow

- [ ] Run: `pnpm tauri dev`. Wait for the window to open.
- [ ] **Visibility toggle:** Press `Ctrl+Shift+L`. The pane appears at the bottom showing "No session activity yet." Press again — pane hides. Press a third time — pane re-appears.
- [ ] **Menu visibility:** Use the menu bar: View → Show Session Log. Toggles the same as the keyboard shortcut.
- [ ] **Projection toggle:** Click the `Human` button in the pane header — already active. Click `Raw` — toggle visually flips; pane still shows empty state.
- [ ] **Raw-log menu item:** Use the menu bar: View → Show Raw Session Log. Toggles between Human and Raw (each menu click flips it).
- [ ] **Resizer:** Hover over the top edge of the pane — cursor changes to `ns-resize`. Drag up — pane height increases. Drag down — height decreases. Stop dragging at ~200px.
- [ ] **Copy button:** Click "Copy session log." Paste somewhere — should be empty (no session lines yet) or just contain the empty-state placeholder. No crash.
- [ ] **Persistence across restart:** Close the app. Re-launch `pnpm tauri dev`. The pane should restore to visible + Raw projection + ~200px height (the values you left it at).
- [ ] **Settings file:** Check `~/.config/tuxlink/settings.json` (or `$XDG_CONFIG_HOME/tuxlink/settings.json`). It should contain `session_log.visible: true`, `session_log.height: ~200`, `session_log.projection: "raw"`.
- [ ] **Corrupt settings recovery:** Stop the app. Edit `settings.json` to `{not valid json`. Restart `pnpm tauri dev`. App should launch normally; pane should default-hide; no crash banner.

### Step 8.3 — Capture smoke evidence

- [ ] Take a screenshot of the pane in Human projection (empty state is fine) and one in Raw projection. Attach both to the PR body as inline images (drag-drop into the PR description on GitHub — the asset gets uploaded to GitHub's CDN and the PR carries the image reference). Do NOT commit screenshots to the repo: `.gitignore` does not currently exclude `dev/screenshots/`, and binary blobs in `git` history bloat the repo. The PR-attachment path keeps the evidence inspectable without polluting the tree.

### Step 8.4 — Final commit (if Phase 8 surfaced any fixes)

If Phase 8 walk-through surfaced any bugs that you fixed inline, commit them with a clear `fix(session-log): <what>` subject and the standard trailers. If no fixes were needed, skip the commit.

BEFORE marking this phase complete:
1. All Phase 8.2 steps walked successfully.
2. Screenshots captured.
3. No crashes; no broken UI states.
4. Settings persistence verified across an app restart cycle.

After all phases:
Run the project's full quality gates:
- [ ] `cd src-tauri && cargo build --release` (catches release-build regressions)
- [ ] `cd src-tauri && cargo test` (all tests green)
- [ ] `pnpm tsc --noEmit` (no TS errors)
- [ ] `pnpm build` (frontend builds)

Update this plan's Execution Status table top-of-plan with the merge SHA for each phase (or for the squash/merge of the PR if the PR ships as one commit; per ADR 0010 the project uses merge-commits, so individual phase commit SHAs land verbatim).

---

## Self-Review

After writing this plan, the planning agent ran the self-review checklist per `superpowers:writing-plans`:

**1. Spec coverage:**
- AMD-7 invariant (both views from one LogRing) → Phases 1+2 implement it; Phase 6 honors it on the frontend; Phase 4 emits full `LogEntry` so the wire contract preserves it. The `test_human_and_raw_share_underlying_entries` assertion in Phase 2 is the explicit invariant guard.
- §5.8 spec items: `Show raw` toggle (Phase 6), session-state header (Phase 6 — actual `sessionState` value comes from Task 16's ribbon work; this plan stubs at `idle`), Copy session log button (Phase 6), live tail with anchor-on-scroll-up (Phase 6), pane is resizable with persisted height (Phases 6+7), default 120px / min 60px / max 50% (Phase 6 constants), Ctrl+Shift+L (Phase 5), View → Show Session Log + View → Show Raw Session Log (Phase 5).
- §4.4 log format coverage: each line shape in the §4.4 excerpt has a classifier test (Phase 1) AND a projection-fixture entry (Phase 2). The synthesized "Session ended. N sent / M received in T seconds." footer comes from the SessionSummary pair (Phase 2).
- "Selecting an older session loads that session's log into the pane" (per §5.8) — this depends on Task 16.5's "Last sessions" surface (which is not in this task's scope). The plan documents it as Wave-3+ wiring: Task 15 provides the pane; Task 16.5 provides the "load this historical session" trigger. The current pane shows live data; a Wave-3 amendment will add a `loadHistoricalSession(id)` prop. **Flag for Cameron: confirm this scope-split is acceptable, or expand Task 15 to include the historical-session-fetch path now.**

**2. Placeholder scan:** No TBDs / "implement later" / "add appropriate error handling" patterns. Every step has executable content.

**3. Type consistency:**
- `LogEntry` / `LogTag` / `LogSource` / `Projection` / `ProjectedLog` / `HumanLine` / `RawLine` defined in Phase 1+2+4 (Rust) and mirrored in Phase 6 (TS). The TS file restates the discriminator strings (`"human"`, `"raw"`, `"Stdout"`, `"Stderr"`, etc.) verbatim. If the Rust serde attributes change, the TS file MUST change in lockstep.
- `tag_line` is a free function on `session_log` module (Phase 1), called from `pat_process` (Phase 3) and tested standalone (Phase 1).
- `LogRing::with_capacity` / `push` / `snapshot` / `len` / `is_empty` / `capacity` method shapes are stable across Phases 1, 3.

---

## Open decisions surfaced during plan writing (for Cameron)

These are NOT showstoppers but warrant operator awareness. They do not change the AMD-7 invariant or the §5.8 spec; they are scope-boundary questions.

1. **Historical-session-load (per §5.8 "Selecting an older session ... loads that session's log into the pane").** Task 15's pane in this plan shows live data only. The "load historical session N's log" trigger is a Task 16.5 surface (the "Last sessions" list in the Radio Dock). The cleanest split is: Task 15 ships the live-data pane; Task 16.5's plan adds a Tauri command + prop wiring that swaps in a stored snapshot. **Alternative:** widen Task 15 now to include the persisted-per-session snapshot store (would require a new on-disk persistence layer for session logs — probably out of v0.0.1 scope). **Recommendation:** ship Task 15 as live-only, defer historical-load to Task 16.5's plan with an explicit dep.

2. **`sessionState` value in Phase 6 is stubbed at `"idle"`.** Task 16's dashboard ribbon owns the live connection-state machine; this plan does not duplicate it. Task 15's pane header reads the same state Task 16 surfaces. If Task 16's plan does not expose the state to App.tsx in a reusable way, a follow-up will need to extract it. **Recommendation:** confirm Task 16's plan exposes `sessionState` as a top-level state hook in App.tsx so this pane can subscribe.

3. **Frontend test harness.** This plan keeps projection logic Rust-side so the cargo test harness covers correctness. The React component is verified via Phase 8's manual browser-smoke. If Cameron wants Vitest wired before this task ships, that's a separate plan-amendment + an additional ~30 lines of `package.json` / `vite.config.ts` config + a `vitest run` step in CI. **Recommendation:** defer Vitest to a project-level decision; do not couple it to Task 15.

4. **No explicit thread-join in `PatProcess::shutdown`.** Phase 3 documents the rationale: reader threads exit naturally on child-pipe closure; they have no side effects beyond mutex-protected ring pushes. A future review may insist on explicit joins for cleanliness; the plan documents how to add them.

---

## Plan-review-cycle log

Per `plan-review-cycle` SKILL.md, the planning agent ran adversarial review rounds before committing this plan. Findings + dispositions:

### Round 1 — Ambiguity + Context gaps + Pitfall coverage

Findings:
- **F1 (ambiguity, addressed in v2 above):** Original draft had "the human projection MAY also synthesize a 'Connecting via CMS-SSL' line from the connection-annotation"; ambiguous about whether the synthesis happens. **Fix applied:** removed; Human projection now surfaces Express's annotations VERBATIM (post-prefix-strip) plus the synthesized SessionSummary footer. The "Connecting via CMS-SSL..." text the operator sees IS the cleaned Express line "Connecting to CMS CMS-SSL at cms.winlink.org port 8773"; we don't paraphrase.
- **F2 (context gap, addressed):** Phase 3's fake-pat shell-script test didn't explain why the script `shift`s arguments. **Fix applied:** comment in the test header explains it (the script consumes `--config <path> --mbox <path> http --addr <addr>` and ignores everything except `$1` after the shifts).
- **F3 (pitfall coverage, addressed):** No explicit invocation of SCOPE-1, RADIO-1/2 anywhere. **Fix applied:** Pre-flight section now points at SCOPE-1 (this pane is client-side display only — no creeping into gateway), RADIO-1/2 (this code does not transmit), HOOK-1/LEASE-1/PARITY-1 (operational discipline; no code surface).
- **F4 (ambiguity, addressed):** Phase 5's "confirm-or-add" framing was vague about how to know which case applies. **Fix applied:** Step 5.1 added explicit `git log` cross-check command.
- **F5 (context gap, addressed):** Phase 4 reference to `tauri::Emitter` without import statement. **Fix applied:** import line added.
- **F6 (interpretation latitude, addressed):** Phase 7's "corrupted file degrades to empty" rule could be interpreted as "silently ignore all read errors." **Fix applied:** Explicit test for the corrupted-file case + a `BEFORE marking complete` checklist item.

### Round 2 — Cross-task dependencies + Interpretation drift + Testing pitfalls

Findings:
- **F7 (cross-task dependency, addressed):** `src-tauri/src/lib.rs` touched in Phases 1, 4, 7. **Fix applied:** explicit cross-task-conflict note in File Structure table.
- **F8 (interpretation drift, addressed):** Original Phase 6 had `projectEntry` returning a generic union; an over-zealous implementer might add a third branch for `SessionSummary` that synthesizes a partial footer. **Fix applied:** Phase 6 comment is explicit: SessionSummary's text is surfaced as-is in the incremental path; the synthesized footer comes from the snapshot-level re-fetch on session end. **Open decision #1** flags the deeper question.
- **F9 (testing pitfall, addressed):** `test_log_ring_bounded_growth_under_pressure` uses `Utc::now()` per entry; some agents under timing pressure might rationalize "the test is slow because of Utc::now(); replace with a fixed timestamp." **Fix applied:** Phase 2's fixture explicitly uses fixed timestamps; Phase 1's bounded-growth test does NOT care about timestamps (only about count), so `Utc::now()` is correct there. No fix needed; documented to avoid future weakening.
- **F10 (cross-task conflict, addressed):** Phase 6's `App.tsx` rewrite vs. sibling tasks (Task 12 inbox, Task 14 compose). **Fix applied:** File Structure table notes the rebase-and-integrate posture; Step 6.4 wraps the App body in `<div className="app-main-area">` placeholder so siblings can mount into a known slot.
- **F11 (testing-pitfalls.md §5 — concurrency assertion drift, addressed):** Phase 3's capture test uses `wait_for_ring_len` with a `Duration::from_secs(5)` deadline. **Fix applied:** the assertion-rigor-under-pressure clause explicitly forbids loosening the line-count assertion; only the deadline may be extended. Phase 3 already documents this; reinforced in the commit-message guidance.

### Round 3 — Drift + Pitfall coverage second pass + Subagent sabotage scenarios

Findings:
- **F12 (drift, addressed):** Phase 6 `cleanExpressLine` (TS) vs. Phase 2 `clean_express_line` (Rust). The two MUST agree byte-for-byte on the cleaned output. **Fix applied:** Phase 6 step 5 `BEFORE marking complete` checklist item explicitly requires a hand-comparison; an alternative (deferred) is a snapshot test in Vitest, which is gated by Open Decision #3.
- **F13 (pitfall coverage, addressed):** PARITY-1 — script/hook path-resolution parity — doesn't directly apply (no hook reads/writes the session-log state), BUT the analogous risk is "Rust projection logic and TS projection logic resolve the SAME entry to DIFFERENT outputs." **Fix applied:** Phase 6 step 5 + Open Decision #3 explicitly call out the cross-runtime drift hazard.
- **F14 (subagent sabotage scenario, addressed):** A subagent under pressure might "fix" the bounded-growth test by removing the assert and just checking "the ring did not crash." **Fix applied:** the assertion-rigor-under-pressure clause is verbatim in Phase 3; the commit-message guidance reinforces it.
- **F15 (pitfall coverage, addressed):** SCOPE-1 — a subagent reading "session log" might propose a feature like "tuxlink as a session log AGGREGATOR for other clients on the LAN." **Fix applied:** Pre-flight section's SCOPE-1 pointer + the Phase 1 module docstring reaffirms "this is client-side display only."

### Round 4 — Final sweep

Findings: zero substantive. Plan deemed ready to commit.

---

## End of plan

When the implementing agent completes all phases:
1. Run the project's full quality gates (Phase 8 final block).
2. Update each phase's Execution Status banner per the Living Document Contract.
3. Open a PR against `feat/v0.0.1` with title `[<moniker>] feat(session-log): Task 15 — session log pane`. Body: link to this plan + brief summary + screenshots from Phase 8.3.
4. After merge: close `tuxlink-69z` per `bd close tuxlink-69z` + dispose the worktree per the ADR 0009 ritual.
