# Handoff — 2026-05-05 end-of-day → Next session (UX brainstorm before Task 4 work)

**From agent:** kestrel
**Session arc:** Long single-day session. Worktree ban lifted (ADR 0007). Tasks 1, 2, 3 implemented end-to-end with full per-task-branch wraps (PR + squash-merge + bd close). Plan-spec deviations encountered in Task 3 (pat 1.0.0 CLI mismatch) documented in commit and PR; plan-text amendment deferred. Three real-spawn integration tests pass against pat 1.0.0 installed locally.
**Status:** All work pushed to origin. `feat/v0.0.1` is at `4c64252`. `main` unchanged at `f81b7ad`. bd has 3 closed, 5 ready.

---

## Next session's starting prompt

Paste verbatim into the fresh Claude Code session. **The first action is a UX brainstorm — substantive code work on Tasks 5+ should NOT start until the brainstorm and the frontend-design first-pass have produced a direction Cameron has approved.**

> I'm resuming the tuxlink project. Three tasks landed in the previous session under agent `kestrel`. Read these in order before doing anything else:
>
> 1. `dev/handoffs/2026-05-05-end-of-day-3-tasks-merged.md` — this handoff.
> 2. `docs/design/v0.0.1-ux-principles.md` — **load-bearing**. Cameron's UX guiding principles, captured at end of previous session. The brainstorm session uses this as the design anchor.
> 3. `CLAUDE.md` — pay attention to the `## Tool referee` section overriding three of bd's directives, the lifted worktree ban (`## Git workflow — worktrees are permitted (ADR 0007)`), and the live-radio policy (`## Live radio network operations`).
> 4. `docs/adr/` — seven ADRs in place. 0004 (per-task branch model), 0006 (bd override), 0007 (worktree-ban lift) are the most operationally load-bearing.
> 5. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — the v0.0.1 plan. **Note**: Task 3's CLI invocation in the plan does not match pat 1.0.0; the implementation in `src-tauri/src/pat_process.rs` is correct. A small follow-up commit should amend the plan text but is not blocking.
> 6. `docs/pitfalls/implementation-pitfalls.md` and `docs/pitfalls/testing-pitfalls.md` per CLAUDE.md prerequisites.
>
> Once read:
>
> - Pick a fresh session moniker (NOT `kestrel`, NOT `alder`, NOT `lichen`). Pre-flight against BOTH `grep -rci "<name>" /home/administrator/Code/tuxlink/` AND `git log --all --grep="^Agent: <name>" --since="3 days ago"` per `feedback_moniker_collision_pre_flight.md` in auto-memory.
> - Run `bd ready` to see the available work. Tasks 5, 7, 9, 16 are ready; Task 5 (Pat HTTP client) is the next sequential pick.
> - **DO NOT START IMPLEMENTATION TASKS IMMEDIATELY.** The user has gated all UI tasks (9-16) behind a UX brainstorm. Tasks 5 and 7 do not strictly need the brainstorm but the user prefers brainstorm-first to set a coherent direction.
>
> First action this session: **launch the UX brainstorm.** Specifically:
>
> 1. Invoke `superpowers:brainstorming`. Visual companion default-on per `feedback_visual_companion_default.md` in auto-memory — launch the browser companion immediately, do not ask.
> 2. Anchor the brainstorm on `docs/design/v0.0.1-ux-principles.md`. Cameron is an experienced Winlink operator who has Winlink Express on a local reference host; he can reference it during the brainstorm if needed. Do NOT recommend looking at Pat's web UI for inspiration — Pat's web UI is what tuxlink is *not* trying to be (per design-doc P3 and the principles doc).
> 3. Cover all of Tasks 9-16 in the brainstorm. Goals: agreed wireframes / interaction patterns / dashboard layout for maidenhead-grid + GPS + time + radio connection state; resolved single-pane-vs-multi-window decisions; clarified what each pane shows when.
> 4. After Cameron approves a direction, invoke `frontend-design` to produce a clean first-pass implementation pass. The user explicitly asked that frontend-design be invoked to make the interface as clean as possible on the first attempt.
> 5. Document the brainstorm output in `docs/design/v0.0.1-ux-mockups.md` (or similar), committed via the per-task-branch wrap, before opening any of Tasks 9-16 in bd.
>
> Take absolute sweet time on this. Cameron has explicitly framed it as a hard review gate before Task 9. Quality over speed; this is the leverage point that makes Tasks 9-16 successful or makes them feel like Express-with-a-coat-of-paint.

---

## What landed in this session (kestrel)

Three PRs merged into `feat/v0.0.1`, in order:

```
4c64252 feat(pat): child-process lifecycle for the bundled Pat daemon (#5)   ← HEAD
b85da90 feat(config): typed tuxlink Config with schema version and validation (#4)
52d4181 chore: scaffold Tauri 2 + React + TypeScript project (#3)
c8bbcae docs: lift worktree ban via ADR 0007 (#2)
f81b7ad chore: pre-execution scaffolding — governance, hooks, ADRs, plan patches
```

### PR #2 — Worktree ban lifted (ADR 0007)

The 2026-04-22 worktree ban was a behavioral bandaid for the geographica subagent-drift incidents. By 2026-05-05 the structural mitigations were in place (per-task-branch model + Beads + destructive-git/commit-discipline hooks); the ban was redundant. Lifted via ADR 0007. CLAUDE.md, AGENTS.md, the v0.0.1 plan callouts, and the ADR README index were updated.

Worktrees are now **permitted but not required**. Default workflow remains `git checkout` in the main repo; worktrees are an option when concurrent agents need filesystem isolation (e.g., eventual auto-claude lease model).

### PR #3 — Tauri scaffold (Task 1)

`pnpm create tauri-app` scaffold pinned to React 18 / Tauri 2 / TypeScript 5 per the plan. End-to-end build verified on aarch64 in 7m 43s; three bundles produced (AppImage 94MB, deb 3.1MB, rpm 3.1MB). Plan deviations documented in commit body:

- Scaffolded to /tmp scratch dir + merged into project root (create-tauri-app refuses non-empty target dirs)
- Removed `tauri_plugin_opener` from `lib.rs` and `capabilities/default.json` (scaffold default; plan's pinned Cargo.toml omits it)
- Added `pnpm.onlyBuiltDependencies: ["esbuild"]` to package.json (pnpm 10 default-denies postinstalls; Vite needs esbuild's platform-specific binary)
- Added `.claude/scheduled_tasks.lock` to `.gitignore`

### PR #4 — Config schema (Task 2)

Typed Rust `Config` struct with serde Serialize/Deserialize, schema_version validation, non-empty validation on required fields, `config_path()` resolver honoring `XDG_CONFIG_HOME`. Plain TDD: 3-test red, 3 pass green, 4th XDG test added per Step 5, final 4/4 passing in 9.45s. Output pristine. No deviations from plan.

### PR #5 — Pat lifecycle (Task 3)

`PatProcess::spawn()` / `shutdown()` / `is_running()` / `http_port()` against pat 1.0.0 installed via la5nta/pat releases v1.0.0 .deb (SHA-256 verified at install). 2/2 tests passing (real spawn-and-shutdown, real stale-pid-cleanup) in 9.36s incremental. **Four plan-spec deviations** encountered and worked around:

| Plan said | pat 1.0.0 reality |
|---|---|
| `--listen 127.0.0.1:PORT` | `--addr` (or `-a`); `--listen` is the radio-mode selector |
| `--config` / `--mbox` after `http` | These are global flags, must precede the subcommand |
| Pat logs to stdout | Pat logs to stderr (and `--log` file) |
| Pat echoes resolved port | Pat echoes the literal input (`:0` stays `:0`); workaround: pre-bind a `TcpListener` in Rust to learn an unused port, drop, pass that fixed port to pat |

Implementation is correct against pat 1.0.0. Plan amendment deferred.

---

## Tasks 4-19 — auto-claude framing

Auto-claude is **not yet installed**. Adoption is its own work item (probably 1-2 hours of install + first-run validation). The per-task-branch model and Beads are auto-claude-ready when we want it. Recommended sequencing:

1. **Next session:** UX brainstorm + frontend-design first pass (this handoff).
2. **Session after:** auto-claude install + validate using Task 5 (Pat HTTP client) as the first run. Task 5 is the right "first auto-claude run" by every criterion: small, deterministic, no UI, no design judgment, well-isolated tests. Blast radius is one branch.
3. **Then:** Tasks 12-15 (UI tasks) once the design is locked, with auto-claude doing implementation + tests overnight, leaving "BROWSER-SMOKE-PENDING" markers for human review in the morning per `feedback_browser_smoke_before_ship.md`.

Task fit table for auto-claude (from end-of-session conversation):

| # | Task | Auto-claude fit |
|---|---|---|
| 5 | Pat HTTP client | **Excellent** — fully spec'd Rust, deterministic tests, no UI |
| 6 | Live-CMS smoke binary | **No** — REVIEW GATE; touches transmission risk; licensee-only (RADIO-1) |
| 7 | Native OS menu bar | Marginal — code mechanical, verification needs visual check |
| 8 | System tray | Marginal — same as 7 |
| 9-11 | Wizard | **No** — UX decisions get made here; brainstorm-driven, human-in-the-loop |
| 12-16 | Inbox / reading / compose / session log / status bar | **No directly**, but suitable post-brainstorm with browser-smoke pending |
| 17 | AppImage packaging | Decent — mostly tauri.conf.json edits |
| 18 | README + install docs | Decent for draft, no for voice |
| 19 | CI + release | Mixed — YAML scaffolding mechanical, validation needs eyes; FINAL REVIEW GATE |

---

## State at pause

### What's pushed to origin

```
main          f81b7ad  (unchanged this session)
feat/v0.0.1   4c64252  (Task 3 squash-merged)
```

Branch protection on main is active. `feat/v0.0.1` ff-merges into main only at the v0.0.1 release tag (Task 19).

### What's NOT pushed

- bd's dolt issue database (solo-use deferral; cross-machine sync deferred)
- Auto-memory updates (live in `~/.claude/projects/-home-administrator-Code-tuxlink/memory/`, harness-managed)

### Working tree state at pause

Clean except for the kestrel handoff (this file) + `docs/design/v0.0.1-ux-principles.md` + alder's pre-execution-prep handoff (untracked all session). All three commit on this session's `docs-session-handoff` branch and squash-merge to feat/v0.0.1 at end-of-session per ADR 0004.

### Toolchain installed this session

- Tauri Linux deps via apt: `libwebkit2gtk-4.1-dev`, `libxdo-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf` (build-essential and libssl-dev were already present)
- Rust 1.95.0 stable via rustup (`~/.cargo`, `~/.rustup`); minimal profile
- pnpm 10.33.3 via corepack (no global npm install)
- pat 1.0.0 via the official .deb at `/usr/bin/pat`, sha256 verified

---

## Plan amendments queued

- **`docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Task 3 step 3** — pat 1.0.0 CLI invocation is wrong in the plan. Correction: `--config` / `--mbox` are global flags before `http`; the http port flag is `--addr` (not `--listen`); pat logs to stderr; pat doesn't echo the resolved port for `:0`. The implementation in `src-tauri/src/pat_process.rs` is correct; only the plan text needs updating.
  - Recommended action: small docs commit on a `docs-amend-plan-task3-pat-cli` branch, squash-merged to feat/v0.0.1.
  - Not blocking: downstream tasks don't re-derive pat CLI from the plan; they call the `PatProcess` API.

---

## bd state

```
Total: 18  |  Open: 15  |  Closed: 3  |  Blocked: 10  |  Ready: 5
```

Closed: tuxlink-wkz (1), tuxlink-6on (2), tuxlink-b9d (3).

Ready: tuxlink-hvv (16 status bar), tuxlink-ko0 (9 wizard 1), tuxlink-6vi (7 native menu), tuxlink-c0w (5 Pat HTTP client), and one more from the post-Task-3 unblock.

Blocked tasks unblock as their deps close. The full graph is auditable via `bd dep tree tuxlink-n65` (Task 19).

---

## Reminders for the next agent

- **First action is brainstorming, not coding.** The UX brainstorm is gated above all Tasks 5+ work by user direction. Default-on the visual companion.
- **bd directives in `<!-- BEGIN BEADS INTEGRATION -->` are overridden** by `## Tool referee` in CLAUDE.md (per ADR 0006). Use TodoWrite for in-turn working memory; auto-memory at `~/.claude/projects/...` for cross-session knowledge; operator owns push timing.
- **Worktrees are now permitted** (ADR 0007). Default solo-agent workflow remains `git checkout` in the main repo; only opt into worktrees when there's a real isolation benefit.
- **Per-task-branch wrap:** branch off `feat/v0.0.1` → commit → push → PR (`gh pr create --base feat/v0.0.1`) → squash-merge (`gh pr merge --squash --delete-branch`) → `git pull --ff-only origin feat/v0.0.1` → `git branch -d` → `bd close`.
- **Hooks:** `block-destructive-git.sh` rejects 13 banned patterns; `check-commit-discipline.sh` rejects unsubstituted `<SESSION-MONIKER>` placeholders, missing `Agent:` trailers, and direct commits to `main` / `feat/v0.0.1`. The latter has an `ALLOW_INTEGRATION_COMMIT=1` env-var carve-out for legitimate squash-merge step (we have not needed to use this; gh-driven server-side squash-merges bypass the local hook).
- **`set -o pipefail`** in any cargo-test pipeline that ends in `tail` — without it, the pipeline's exit code is `tail`'s, not cargo's, masking test failures. Lesson learned in Task 2.
- **Live amateur radio operations are licensee-only.** Tasks 5 and 6 touch Pat which can transmit; Task 5 (HTTP client) does not transmit on its own (Pat's HTTP API serves the local mailbox). Task 6 is the live-CMS smoke binary that explicitly transmits — operator-only by construction.

---

## Open decisions for the next agent or Cameron

1. **UX brainstorm output format:** wireframes (PNG / Figma export / mermaid?), interaction patterns (text + diagrams?), single doc or multiple? The brainstorm session will arrive at this naturally; flag if a tooling choice needs Cameron's input.
2. **Auto-claude install timing:** before Task 5 (so Task 5 is the validation run) or after Task 5 (so we have hand-validated baseline before automating)? Recommend before — the validation IS the value.
3. **Plan amendment for Task 3 pat CLI:** roll into the docs-session-handoff PR, do separately, or defer until a "plan housekeeping" sweep before the v0.0.1 release? Current handoff suggests a separate small branch; happy to fold differently.

---

**If something in this handoff looks wrong tomorrow:** the previous-session agent (kestrel) wasn't perfect; flag it before acting on it. The UX-principles document and the design-doc P3/P5 framing are the load-bearing inputs to the brainstorm; if they conflict with something in this handoff, principles + design doc win.
