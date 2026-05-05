# Handoff — 2026-04-22 → Tomorrow's Session

**From agent:** lichen
**Session context:** Project kickoff, office-hours, build-robust-features, writing-plans, self-review.
**Status:** Plan committed. Execution paused pending overnight review.

## Tomorrow's starting prompt (paste into a fresh Claude Code session)

> I'm resuming the tuxlink project. Read these in order:
>
> 1. `dev/handoffs/2026-04-22-plan-approved-pending-execution.md` — this file.
> 2. `CLAUDE.md` — ethos, git safety rails, agent-moniker discipline.
> 3. `docs/ux-anti-patterns.md` — forbidden UI patterns for every UI task.
> 4. `docs/pitfalls/implementation-pitfalls.md` + `docs/pitfalls/testing-pitfalls.md`.
> 5. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — the 19-task implementation plan.
>
> The v0.0.1 plan is approved and ready to execute. Pick a new agent moniker for this session
> (lichen persists in prior commits; tomorrow's session is a new agent). Invoke
> `superpowers:subagent-driven-development` and start executing Task 1. Review at the gates
> after Tasks 6, 11, 16, 19.

## State at pause

### What landed

- Repo initialized on `main`, two commits:
  - `168f4c8` — initial framing (CLAUDE.md, AGENTS.md, README.md, .gitignore)
  - `0565381` — v0.0.1 plan + ux-anti-patterns + pitfalls docs
- Branch: `main`. Not pushed to origin (user's call).
- Working tree: clean.

### What's in the plan (at a glance)

- **Scope:** telnet-only Linux desktop Winlink client, single-crate Rust + Tauri + React,
  Pat v1.0.0 bundled as managed child, AppImage distribution, local fake CMS for tests.
- **Tasks:** 19 total. TDD-structured. Each task has complete in-line code (no placeholders).
- **Review gates:** after Tasks 6, 11, 16, 19 (min 3 rounds each, per build-robust-features).
- **Tech stack locked:** Rust 1.75+, Tauri 2, React 18, TypeScript 5, Vite, TanStack Query
  5, Radix UI, react-virtuoso, `reqwest` blocking, `nix` for signals, `chrono` for
  timestamps, `mockito` for HTTP mock tests, `tempfile` for test fixtures.
- **Deferred to v0.1+:** workspace split, protocol-native crate, VARA (via WINE for v0.1,
  native for v0.5+), hamlib, AX.25, P2P, RMS Relay, forms rendering (ICS-213 etc.),
  attachments, ARM64 / RaspPi image, bundled WebKitGTK, real session-state tracking.

### Locked premises (from office-hours)

- **P1.** VARA-on-Linux-native achievable in 12-24 months (clean-room RE + AI-assisted DSP).
- **P2.** Clean-room interoperability posture, never clone VARA.
- **P3.** UX wedge ships first (Mail.app over Pat), VARA-native is v1.0 capstone.
- **P4 (revised).** Full Winlink Express parity is v1.0 bar, NOT v0.1.
- **P5 (revised).** Zero genuine user barriers on first-run; training OK for advanced features.

### Locked UX commitments

- Tauri native menu bar (File / Message / Session / Mailbox / View / Tools / Help).
- System tray; window-close hides to tray; only File→Quit / tray→Quit / Ctrl+Q exit.
- Keyboard shortcuts (Ctrl+N, Ctrl+R, F5, F6, etc.) per docs/ux-anti-patterns.md.
- Familiar Winlink Express terminology (Mailbox, Outbox, Posted, Templates, Session).
- Draft persistence NEVER lost on window-hide, app-background, or dialog-close.
- Forms render EMBEDDED in the app (NOT in an external browser window).
- No hamburger menus, no slide-out drawers, no mobile-first layouts, no SPA-page-swaps.

### Architecture commitments (for subagents)

- **Single-crate Rust binary** for v0.0.1. No workspace split until v0.1.
- **No `tuxlink-protocol-native` stub** in v0.0.1; define the protocol trait inside the main
  binary. Extract to a separate crate at v0.5 when the native backend lands.
- **Pat owns the mailbox.** Tuxlink has NO SQLite in v0.0.1. Pat's HTTP API is the
  authoritative mailbox source. (v0.1 may add a SQLite index/cache; invariant TBD.)
- **Forms component has a clean IPC boundary** (`htmlTemplate + xmlData` in,
  `formSubmission` out) so v1.0+ can swap to GTK+WebKitGTK without rewriting the mail
  UX.
- **Pandora integration preserved structurally**, not implemented. Crate boundaries let
  future Pandora services mount the protocol code; no v0.0.1 dependency closes that door.

### Adversarial review findings worth remembering mid-execution

Codex round 1 (from office-hours Phase 3.5):
- Proposed weekend-demo scope that became Appendix A.
- Challenged P4 (v0.1 vs v1.0 sequencing — accepted and revised).
- Surfaced the "communications console" / geographica integration vision as a v1.0+ north
  star under the Pandora Project framing.

Round 2 (Architecture):
- Pat dual-ownership of mailbox + tuxlink SQLite is a divergence problem. FIX: no SQLite
  in v0.0.1, Pat owns.
- Stubbed `tuxlink-protocol-native` creates a trait shape you'll regret. FIX: no native
  crate stub in v0.0.1.
- Tauri + long-running radio session: window close must NOT quit app. FIX: Task 8's tray
  behavior.

Round 3 (Scope):
- Cut ICS-213 renderer + SQLite + session-log translator from v0.0.1.
- Add Winlink account onboarding + Pat binary bundling + config-file writing + test-send
  button + AppImage release. All added to plan.

Round 4 (Testing):
- Use local fake CMS, never real Winlink CMS in CI. Task 4 + Task 6 implement this.
- `PatBackend` trait-behind for tuxlink-core tests. (Simplified to direct `PatClient` use
  in v0.0.1; trait introduction deferred to v0.1 when second backend appears.)
- Pristine test output assertion in CI. Task 19's `scripts/assert-pristine-tests.sh`.

Round 5 (Subagent sabotage):
- 5 predicted drift scenarios, 7 guardrail phrases. All 7 live in the plan's "Subagent
  Guardrails" section at the top.
- Critical: every commit must have `Agent: <moniker>` trailer, no worktrees, no
  destructive git, no dependency additions not listed in the plan.

### Loose ends for tomorrow to decide

1. **Push `main` to origin?** Commits are local only. Cameron's call.
2. **Moniker for tomorrow's session.** `lichen` is attached to yesterday's framing
   commits. Pick a new moniker for execution work. Keep it in the same aesthetic (plant
   / animal / geographic, ctrl+F-friendly, not a human first name). Suggestions:
   `cedar`, `sparrow`, `flint`, `hemlock`, `basalt`, `quartz`, `marsh`, `birch`.
3. **Branch?** CLAUDE.md suggests `feat/v0.0.1` off main. Plan's "Branch Setup" step
   creates it before Task 1. Honor that.
4. **Execution cadence.** Cameron's call: nonstop through to Task 19 over a weekend, or
   paced 2-4 tasks per session with thinking time between review gates? The plan assumes
   4-8 session-hours spread over 2-3 focused days.

### Open questions from the design doc (not blocking execution)

- UI framework locked to React (vs SvelteKit/Solid).
- Hamlib integration: `rust-hamlib` FFI vs `rigctld` subprocess (v0.1 decision).
- Form rendering strategy (v0.1).
- Governance/foundation/CLA (v1.0 decision).
- Trademark check on "tuxlink" name (before v0.1 marketing push).

### File inventory

Committed:
```
.gitignore
AGENTS.md
CLAUDE.md
README.md
docs/pitfalls/implementation-pitfalls.md
docs/pitfalls/testing-pitfalls.md
docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md
docs/ux-anti-patterns.md
```

Uncommitted (this handoff):
```
dev/handoffs/2026-04-22-plan-approved-pending-execution.md
```

External (not in repo — session artifacts):
```
~/.gstack/projects/cameronzucker-tuxlink/cameronzucker-main-design-20260422-200809.md
```

---

**If the plan looks wrong tomorrow:** the plan is on `main` at `0565381`. Revise via
amendment in a new commit (CLAUDE.md bans `--amend` on pushed commits; since nothing is
pushed, `--amend` would be OK, but still prefer a new `docs(plan): revise <area>` commit
for audit clarity). The design doc is a session artifact — update it if the revision is
substantive.

**If the plan looks right:** start a fresh Claude Code session, paste the starting
prompt above, and go.
