# Tuxlink

Native Linux Winlink client for amateur-radio emergency communications: one
[Tauri](https://tauri.app/) 2.x desktop app with a native Rust Winlink engine and
a React 18 + TypeScript frontend. Alpha. [README.md](README.md) carries the
user-facing overview and the maturity matrix.

## Project structure

Single crate in the `v0.x` series ([ADR 0002](docs/adr/0002-tauri-react-single-crate.md)).

- `src/` — React + TypeScript frontend (Vite), rendered by WebKitGTK 4.1: mailbox,
  compose, the per-mode radio panels, settings, session log.
- `src-tauri/` — Rust backend (`src-tauri/Cargo.toml`; **not** a workspace root). The
  native Winlink B2F engine, CMS connection, mailbox persistence, the AX.25 / ARDOP /
  VARA transports, and the Tauri commands. No external modem daemon or sidecar handles
  CMS.
- `xtask/` — Rust task-runner crate.
- `tests/` — integration fixtures (e.g. `converge_build_fixtures/`).
- `docs/` — `adr/` (architecture decisions; **canonical for project policy** per the
  propagation contract below), `user-guide/` (in-app Help source), `design/`,
  `pitfalls/`.
- `dev/` — `handoffs/` (session-end docs) and `incidents/` are tracked; `scratch/` and
  `adversarial/` are gitignored.
- `scripts/` — operator tooling: `converge-build.sh`, `install-githooks.sh`,
  `new_tuxlink_worktree.py`, `sync-version-sources.ts`.
- `.claude/` — hooks + skills + session scripts; `.githooks/` — commit-msg +
  branch-lifecycle hooks; `.beads/` — the bd issue tracker (Dolt-backed).
- `version.txt` + `package.json` carry the version, kept in sync by
  `scripts/sync-version-sources.ts`; release-please owns the value.

## Commands

Frontend uses `pnpm`. The Rust backend needs an explicit
`--manifest-path src-tauri/Cargo.toml` (there is no workspace-root `Cargo.toml`).

| Task | Command |
|---|---|
| Install deps | `pnpm install --frozen-lockfile` |
| Dev (Vite in a browser) | `pnpm dev` |
| Tauri dev (desktop window) | `pnpm tauri dev` |
| Converged build + launch (operator) | `pnpm dev:converged` — runs `scripts/converge-build.sh`, which builds `origin/main` in a disposable worktree, **not** your branch |
| Type-check | `pnpm typecheck` |
| Frontend tests | `pnpm vitest run` |
| Production web build | `pnpm build` (`tsc && vite build`) |
| Package artifacts | `pnpm tauri build --bundles deb,rpm,appimage` |
| Rust lint | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` |
| Rust tests | `cargo test --manifest-path src-tauri/Cargo.toml --locked` |
| Docs-link lint (pre-push hook) | `pnpm lint:docs` |
| Activate git hooks (first run) | `bash scripts/install-githooks.sh` |
| Issue tracker | `bd ready` / `bd show <id>` / `bd update <id> --claim` / `bd close <id>` |

A fresh worktree has no `node_modules`: run `pnpm install` before the pre-push
`lint:docs` hook (`tsx`) can pass.

## Testing

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) gates every PR on two
jobs, each on amd64 + arm64:

- **verify** — `pnpm typecheck`, `pnpm vitest run`, `pnpm build`, then
  `cargo clippy … --all-targets --locked -- -D warnings` and
  `cargo test … --locked`.
- **build-linux** — `pnpm tauri build` (deb / rpm / appimage) + `pnpm lint:docs`.

**MSRV is 1.75** (`src-tauri/Cargo.toml` `rust-version`); clippy's `incompatible_msrv`
lint is denied, so an API stabilized in 1.76+ (e.g. `Result::inspect_err`) fails the
build — use the pre-1.76 idiom.

**This dev Pi does not finish a cold `cargo` build or test locally** (the target is
contended). Write the Rust and its tests, open the PR (draft is fine), and let CI
compile and run them; `pnpm vitest run` is fast enough to run locally on a single
file. On-air RF validation is operator-only (RADIO-1, [ADR 0018](docs/adr/0018-radio1-gates-operator-execution-not-agent-authorship.md))
— agents validate transmit-path code via mocks / loopback / CI, never a real radio.

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health

## Brainstorming preferences

- Always use the visual companion (browser mockups) during brainstorming — don't ask, just launch it
- Token budget is not a concern during design phases — be thorough

## Extended capabilities available on this dev Pi

### OpenAI Codex CLI — for `build-robust-features`' "at least one adversarial round via Codex" requirement

**Codex IS installed on this Pi at `/usr/local/bin/codex`** (`which codex` does find it; the earlier "not on $PATH" framing was wrong). The `npx --yes @openai/codex ...` form still works and is the conservative invocation in case the local binary version drifts from the npm one.

```bash
# Non-interactive agent call
npx --yes @openai/codex exec "<prompt>"        # alias: codex e

# Stdin-piped prompt to `exec`
cat spec.md | npx --yes @openai/codex exec -

# Optional: pipe alongside argv prompt (argv = primary instructions, stdin = appended <stdin> block)
git diff main..HEAD | npx --yes @openai/codex exec "Review the following diff:"
```

**Adversarial-review pattern (Codex CLI v0.128.0+):** the `review` subcommand requires picking EXACTLY ONE of `--uncommitted` / `--base` / `--commit` / `[PROMPT]` — they are MUTUALLY EXCLUSIVE. Prior CLAUDE.md recipes paired `--base main` with a custom prompt; that combination is now rejected with `error: the argument '--base <BRANCH>' cannot be used with '[PROMPT]'`. Two patterns survive for directed attack-angle reviews:

```bash
# Structured-base mode (no custom prompt; Codex picks its own attack angle):
npx --yes @openai/codex review --base main 2>&1 | tee dev/adversarial/<date>-general-codex.md

# Custom-prompt mode (the one that lets you direct Codex):
#   - The prompt itself must tell Codex to fetch the diff and which files to read.
#   - Codex has read-only sandbox access to the worktree so it can grep/cat/sed source.
cat > /tmp/codex-prompt.txt <<'EOF'
You are doing adversarial code review of the diff against origin/main in this
worktree. Run `git diff origin/main..HEAD` to see the changes. Audit for
<specific attack angle>. Read these files: <list>. Output findings as
markdown at the end.
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/<date>-<topic>-codex.md
```

The custom-prompt pattern is what worked for the `tuxlink-4ek` ARDOP UI adrev (2026-05-30); the prior `--base + prompt` syntax produces a 5-line argparse error stub and the tee'd file contains nothing useful. **If you see the stub, the prompt got rejected — re-run with the stdin pattern.** Verifying: `wc -l dev/adversarial/*-codex.md` — a real review produces ~1500–4000+ lines including the diff + Codex's exec commands + final findings block; a stub is ~5 lines.

- **Authentication:** ChatGPT-mode, cached at `~/.codex/auth.json`. Already authenticated — no setup needed.
- **When to use:** when a workflow (notably `superpowers:build-robust-features`) explicitly calls for "at least one round via Codex." Substitute Claude agents only when this is genuinely unavailable — it isn't unavailable here.
- **MCP-server mode:** `npx --yes @openai/codex mcp-server` — expose Codex as an MCP server if you want the main loop to call it like a tool.

Write adversarial-review output to `dev/adversarial/<date>-<topic>-codex.md`. **This directory is `.gitignore`d** (per the 2026-05-17 "release-ready public repo" cleanliness call): raw codex/adversarial transcripts stay local-only as dev scratch. Summarize findings + dispositions in handoff docs, PR bodies, or pitfalls entries as appropriate; the raw transcripts are reference material, not project artifacts. If a future operator needs to consult an older review trace, they're on the original author's local disk; don't expect them in the public repo.

**Codex's default sandbox blocks writes to `dev/adversarial/` (2026-05-18 wizard-cluster spec R5 incident).** When you tell Codex to write its findings to a file via `apply_patch`, Codex's default `read_only` sandbox-mode rejects the write — the patch attempt is silently swallowed in some Codex CLI versions but writes to stdout in others. The 2026-05-18 R5 round only produced findings because Codex dumped them to stdout after the file-write failed; if you only read the expected output file, you'd think Codex produced nothing. **Workaround:** pipe Codex's stdout to the adrev file alongside the in-process file write so you have a stdout fallback. Example: `npx --yes @openai/codex exec '...' 2>&1 | tee dev/adversarial/<date>-<topic>-codex.md`. Alternative: pass `-c sandbox_permissions='["disk-full-write-access"]'` to Codex to authorize the write (less defensive — gives Codex broader filesystem access than the adrev dir alone). Read process stdout, not just the expected file, when Codex doesn't produce the file you asked for.

### `url-to-markdown` skill — fetch FULL webpages, not summaries

Installed at `/home/administrator/.claude/skills/url-to-markdown/`. Invoke via the `Skill` tool (name: `url-to-markdown`) or directly:

```bash
python3 /home/administrator/.claude/skills/url-to-markdown/scripts/bootstrap.py "https://url" --json --out /tmp
```

**Prefer this over `WebFetch` whenever you need the full content of a page** (product pages, docs, wikis, articles). `WebFetch` runs the page through a summarizer that can drop critical details. `url-to-markdown` downloads the raw content, converts to markdown with YAML frontmatter, and writes to disk so you can read it verbatim.

Returns a JSON envelope; parse the `output_path` and then `Read` the resulting `.md` file. Handles Cloudflare-class bot protection via TLS fingerprint impersonation. Gracefully reports paywalls, SPAs, PDFs, and feeds instead of producing garbage.

## Project ethos

Tuxlink is Cameron's learning sandbox for AI-assisted development techniques —
custom skills, adversarial review, multi-agent teaming, capability mapping —
that he plans to transfer to high-stakes projects at his employer. The
shipped software matters, but **professional-development outcomes are a
first-class goal alongside features.**

Implications:
- Process rigor > raw velocity. Do the right thing, not the fast thing.
- Explain when/what for new workflows so Cameron builds transferable
  skill.
- Prefer patterns that generalize to multi-developer / higher-stakes
  environments.
- Signal professional polish even at A-audience scale — the surface area
  of the repo (commits, CHANGELOG, versioning, CI) teaches Cameron what
  "good" looks like and builds habits that transfer.

## Agent identity — pick a moniker at session start

**At the very start of every session** (after reading CLAUDE.md and the most-recent handoff, before taking any action on the repo), generate a moniker via the script and state it in your first user-facing message:

```bash
python3 .claude/scripts/get_agent_moniker.py
```

This draws 3 words without replacement from a 100-word pool of plant / animal / geographic nouns and hyphen-joins them (e.g. `towhee-wren-aspen`). Combinatorial space ≈ 970,200 trios; collision probability under 1% across project lifetime. The script pre-flights against `git log --all --grep="^Agent: <candidate>"` automatically; if a collision is detected, it retries up to `--max-attempts` times before giving up. This replaces the prior manual `grep -ri <name> .` + `git log --all --grep` dance.

The moniker:

- Is hyphen-joined three-word form (single-word legacy monikers in commit history — `alder`, `cedar`, etc. — remain valid; the new format applies to forward commits).
- Is **ctrl+F-friendly** by construction (the pool excludes common code-identifiers and human first names).
- Persists for the entire session — do not change it mid-session.
- Passes through to every subagent you dispatch: include `"You are agent <moniker>; use this in your commit trailers."` in each Agent tool prompt so subagent-authored commits are grep-discoverable too.

**Include the moniker in every git action as a commit trailer:** `Agent: <moniker>` on its own line in the commit message, alongside the existing `Co-Authored-By:` trailer.

```
<subject>

<body paragraphs>

Agent: juniper
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Also include in:** branch names when creating them (`agent-<moniker>/<topic>` for throwaway branches; regular `feat/` / `fix/` prefixes are fine for shared feature branches but still add the trailer inside commits), and PR titles if you open one (`[juniper] <subject>`).

The repo-local `commit-msg` hook enforces `Agent: <moniker>` for local commits
when `.githooks` is active. GitHub-generated merge/release commits do not pass
through local hooks, so PR title monikers and branch names remain part of the
forensics trail.

**Why:** triage + forensics. When a session goes sideways — a mysterious `git reset --hard`, a stale regression, an unclear commit authorship — Cameron needs to grep the commit graph for "which agent did this" without reconstructing it from timestamps. `git log --grep="^Agent: juniper"` returns the full trail for this session. `git log --all --grep="^Agent:"` enumerates every agent that has ever touched the repo.

**If you forget to set a moniker early in the session:** pick one now and apply it to all forward commits. Do not retroactively amend earlier commits (amending shared/recent commits is banned — see below).

## Git workflow — worktrees mandatory under bd-issue ownership (ADR 0008)

When the `block-main-checkout-race.sh` hook denies a write op citing "another live session is active," create a worktree per the QUICK FIX in the deny message and re-run your op there. The hook's determination is authoritative; agents do not re-decide it via `get_tuxlink_sessions.py` or any other source. Worktrees are MANDATORY for write work when the hook says another live session is active. Read-only ops and `bd` commands stay free regardless.

When the hook does NOT deny — i.e., it has determined there are no other live sessions — main-checkout writes are fine and worktrees are not required for that case.

Rationale and the 2026-05-18 grounding incident: see [`dev/incidents/2026-05-18-main-checkout-race-hook-loop.md`](dev/incidents/2026-05-18-main-checkout-race-hook-loop.md) (write-up) + [`dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md`](dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md) (AzDO-grounded diagnosis from `towhee-wren-aspen`). Prior tuxlink wording included a "solo-session work, worktrees remain optional" carve-out that invited agents to second-guess the hook by consulting `get_tuxlink_sessions.py`; the LFST source-of-truth never had that carve-out. The framing above restores LFST's posture: the hook is the authority.

**Worktree ownership rule.** A worktree is permitted IFF:

1. A **bd issue** is in `in_progress` and claims the worktree (path recorded in the issue body or via `bd remember`). `bd show <id>` is the canonical answer to "what is `worktrees/X` for?"
2. The branch follows the per-task convention ([ADR 0004](docs/adr/0004-per-task-branch-model.md)): `bd-<id>/<slug>` preferred when the bd issue exists; otherwise `agent-<moniker>/<slug>` or `task-NN-<slug>`.
3. The worktree path is `worktrees/<bd-id-or-slug>/` at the repo root (`worktrees/` is `.gitignore`d).
4. The session adheres to all other CLAUDE.md rules (moniker discipline, commit discipline, destructive-git ban, session-end handoff).

A worktree without a bd-issue claim is an anti-pattern. If you encounter one (stale handoff, prior orphan), either (a) retroactively claim it with a bd issue, or (b) inventory + archive + dispose per the disposal ritual (ADR 0009, forthcoming as part of this sprint's D3).

**Pattern A (harness-spawned ephemeral worktrees** — the `Agent` tool's `isolation: "worktree"` parameter) is uncontroversially permitted; the harness manages create + dispose, no per-worktree bd issue required.

**Multi-worktree coordination via bd dep edges.** When two or more worktrees are simultaneously `in_progress`, maintain the dependency graph via `bd dep add <consumer-id> <provider-id>`. `bd ready` reflects unblocked work at any moment.

**Full rationale, alternatives considered, and watched failure modes:** [ADR 0008](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md), which supersedes [ADR 0007](docs/adr/0007-lift-worktree-ban.md)'s "permitted but optional" framing. ADR 0007 remains accepted as the historical record of why the original Geographica-era ban was lifted.

### Worktree disposal ritual ([ADR 0009](docs/adr/0009-worktree-disposal-ritual.md))

`git worktree remove` is banned (destructive-git hook denies it per C1). Disposal uses the 4-step ritual:

```bash
# Step 1 — Inventory (from inside the worktree being disposed)
git status --short                                          # tracked dirty
git ls-files --others --exclude-standard                    # untracked
git ls-files --others --ignored --exclude-standard          # gitignored on disk (critical: .beads/embeddeddolt/ class)
git stash list                                              # worktree-scoped stashes

# Step 2 — Propagate (commit + push) or archive
cd <main-repo-path>                                         # CRITICAL: leave the worktree before archiving (see note below)
#   For propagate: git add ..., git commit -m "...", git push origin <branch>
#   For archive:   tar czf .claude/worktree-archives/<name>-$(date -u +%Y%m%dT%H%M%SZ).tar.gz <worktree-path>

# Step 3 — Physical remove
rm -rf <worktree-path>

# Step 4 — Prune git's registry
git worktree prune
```

The `cd <main-repo-path>` between Step 1 and Step 2 is load-bearing: writing the archive while still cd'd into the doomed worktree resolves the relative `.claude/worktree-archives/...` path INSIDE the worktree, and Step 3's `rm -rf` then deletes the archive along with the worktree. Codex 2026-05-17 D4 review caught this; the fix is to cd back before archiving (or to use an absolute path for the archive destination).

`.claude/worktree-archives/` is `.gitignore`d. The archive directory is per-machine, not pushed to origin. The hook denies `git worktree remove` regardless of how the worktree looks "clean" — `.beads/embeddeddolt/` is the canonical example of gitignored-but-stateful content the git check misses.

**Why no shortcut:** the LFST musing-bhabha incident (May 2026) lost untracked content via `git worktree remove`. The ritual is the replacement; see [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md) for full context and watched failure modes.

## Git workflow — destructive commands are BANNED

The [`.claude/hooks/block-destructive-git.sh`](.claude/hooks/block-destructive-git.sh) hook denies destructive git operations at the harness layer. **The hook is the canonical enforcement; do not work around it.** If a hook denial surprises you, the right move is to find a non-destructive alternative — never `--no-verify`, never an end-run.

**Full banned list and rationale:** see the hook source for the regex-precise list, and [standing-conventions §1](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) for the cross-project rule. Quick reference (not the authoritative list — the hook is):

- `git reset --hard <ref>` — use `git revert <commit>` or restore named files.
- `git push --force` / `-f` / `--force-with-lease` — open a new PR or ask.
- `git checkout -- .` / `git restore .` / `git clean -f` — name files explicitly.
- `git branch -D` / `--delete --force` — use `-d`, which refuses unmerged.
- `git commit --amend` on pushed or other-authored commits — create a new commit.
- `git rebase -i` / `--interactive` — banned outright per C1; use `git rebase <base>` for non-interactive linear replays.
- `git worktree remove` — use the disposal ritual ([ADR 0009](docs/adr/0009-worktree-disposal-ritual.md)).
- `git reflog expire --expire=now` / `git gc --prune=now` — strips the recovery safety net.
- `git filter-branch` / `git filter-repo` — mass history rewrite.
- `--no-verify` / `--no-gpg-sign` / `-c commit.gpgsign=false` — bypasses the project's gates.

**Why hooks, not just prose:** the 2026-04-20 Geographica incident — a subagent ran `git reset --hard feat/noaa-conus` on `dev`, wiping 7 commits including a shipped fix; recovered via reflog only because the regression was caught within the 14-day `git gc` window. Geographica's CLAUDE.md *correctly documented* the rule at the time of the incident. **Prose alone did not prevent it; the hook layer does.**

**If you think you need a banned command:** stop and surface the situation to the user with a proposed non-destructive alternative.

## Git workflow — branch lifecycle state machine (ADR 0017)

A branch's PR state determines whether the branch accepts further commits/pushes. Once a PR is merged or closed-without-merge, the branch is **dead** — `git commit` and `git push` to it are denied by `.githooks/pre-commit` + `.githooks/pre-push`. The discipline replaces the orphan-post-merge anti-pattern (the 2026-06-01 v1p incident) with explicit lifecycle transitions: `active → pr-open → merged-dead`, plus `follow-up` for new branches off a merged predecessor.

**Activate the hooks (first-run step on any clone):**

```bash
bash scripts/install-githooks.sh
```

`scripts/install-githooks.sh` sets `core.hooksPath .githooks` and verifies the hook scripts are executable. Idempotent; safe to re-run after `git pull`.

**Documented escape hatch** (loud + audited at `dev/scratch/branch-lifecycle-overrides.log`):

```bash
TUXLINK_BRANCH_LIFECYCLE_OVERRIDE=I-know-what-Im-doing git commit ...
```

**Full state model + classification heuristics + alternatives considered:** [ADR 0017](docs/adr/0017-branch-state-machine.md).

## Disposable / converged build worktree quarantine

`.local/converge-build-worktree/` is operator tooling state, not an agent task
worktree. Agents must not edit source, stage files, commit, stash, rebase, or
run cleanup commands there. Use bd-bound worktrees under `worktrees/` for agent
code changes.

If a converged-build script refuses to run because the disposable worktree has
dirty or untracked source changes:

1. Inspect only with read-only commands such as
   `git -C .local/converge-build-worktree status --short`.
2. Report the exact paths and whether they are tracked, untracked, or ignored.
3. Do not delete, restore, clean, stash, or overwrite anything there unless the
   operator explicitly authorizes the exact path-level cleanup.

Build-cache directories such as `target/` and `node_modules/` may exist there,
but they must not be treated as permission for agents to work in that tree.

## Live radio network operations

Per [ADR 0018](docs/adr/0018-radio1-gates-operator-execution-not-agent-authorship.md),
RADIO-1 gates the **operator's real-time execution of a transmit-capable binary
against real infrastructure** under the project's callsign — a Part 97
control-operator act. It does **not** gate the agent. The dev shell has no radio
attached and cannot key a transmitter, so the agent **freely claims, writes,
tests (mocks / loopback / fakes), commits, and ships RF-path code** (AX.25,
VARA, ARDOP, transports, modem internals, abort logic) like any other backlog —
no operator green-light to claim, no "rf phobia." The transmission consent gate
is honored by the operator, in the binary, at run time; cached credentials,
stored env vars, and "the user said yes last week" are not operator consent for
that run.

What the agent still does **not** do: run a transmit-capable binary against real
hardware/infrastructure (pointless — no radio to validate against; on-air
validation is operator-only per `rf_validation_onair_only`), or add in-app
transmit safeguards beyond legacy WLE behavior (no added airtime caps / TOT
timers / consent modals). Transmit code must still have a working abort and no
runaway-TX bug — a correctness bar, not an authorship gate. CMS telnet over the
internet is not a transmission and is authorized for agent dev testing.

Canonical: ADR 0018 + [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md);
detailed rationale in the RADIO-1 entry in
[docs/pitfalls/implementation-pitfalls.md](docs/pitfalls/implementation-pitfalls.md).

## Wire-walk gate — feature reachability before any "done" claim

Before claiming any feature is **end-to-end shipped / done / complete / working**, before marking a PR ready, and before closing a feature issue — run the **`wire-walk`** skill (`.claude/skills/wire-walk/`). It is a **hard gate**, not advice: the operator supplies the key user flows **greenfield** (you do NOT draft them — anchoring launders your own blind spots), you trace each flow verbatim to code (`file:line`), and **any broken primary/motivating flow means the feature is NOT shipped** — partially-wired is a defect, not a follow-up.

This exists because "backend shipped, CI-green, connected to nothing a user can touch" recurred across features and agents (identity epic tuxlink-6wz3/z6yi; APRS/UV-Pro tuxlink-ve3j; PR #347/#392/U1) despite the standing "features shipped end-to-end" rule. Registration ≠ a caller; CI-green ≠ reachability. The orchestrator runs it at the **integration boundary** (a subagent only sees its slice; the cross-phase seam is where it breaks). Capture the flows at feature **start** as the definition-of-done; the gate then traces them at done-time. Grep + read only — no build, no external service, so it always runs (it cannot be deferred for quota the way an external reviewer can). Append it to the end of `build-robust-features` and the final review of `subagent-driven-development`.

Canonical: the skill itself (`.claude/skills/wire-walk/SKILL.md`). This is a pointer.

**Features are built whole ([ADR 0022](docs/adr/0022-ban-autonomous-agent-issue-splitting-and-deferrals.md)).** A feature — a complete, user-reachable capability — ships whole and wire-walked, or stays in progress until it is. Do NOT carve one feature into a shippable inert slice plus a deferred remainder ("Phase 2 now, Phase 3 later"; "wire the transport now, add connect later"). There is **no authorization escape hatch**: completeness is an invariant, not negotiable scope, for the agent or the operator-in-a-hurry. Distinct, independently-complete capabilities are separate features (that is not splitting). Filing a spec'd, buildable piece as a "follow-up" is the banned deferral. Canonical: ADR 0022; this is a pointer. `wire-walk` gates the work as built; this ADR forbids the split-and-defer that creates the un-revisited stub.

## Commit and release discipline

- Use conventional commit types: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, `perf:`, `ci:`, `build:`. Match the commit `type:` to the actual intent. Never use `fix:` for docs fixes or `feat:` for internal refactors.
- Prefer scoped commits (`feat(<scope>): ...`) when the change is localized to one subsystem. Scopes will be defined after office-hours sets the project structure.
- Breaking changes: add `!` suffix and a `BREAKING CHANGE:` footer with a one-line user-facing explanation.
- Update `dev/implementation-log.md` (once created) after any significant work item: plan executed, feature shipped, bug hunt cycle completed, adversarial review completed. Entry goes at the top, reverse-chronological, keyed by date + topic.
- **Polish before push.** Per [ADR 0010](docs/adr/0010-no-squash-merge.md): squash-merge is banned, so the integration branch will preserve every task-branch commit. Clean up WIP / fixup / "oops" commits via non-interactive `git rebase <base>` on **local un-pushed commits** before `git push`. Once pushed, commits are immutable (the destructive-git ban on `--amend` of pushed commits and on `git rebase -i` ensures this). The push gates the polish.
- **Releases are two-channel; agents never cut or promote one.** release-please keeps one rolling `chore: release X.Y.Z` PR (branch `release-please--branches--main`), opened as a **draft** (`draft-pull-request`) so it cannot be merged ad-hoc — a plain `gh pr merge` on it fails. The swarm merging it per-PR is what inflated the version 0.41→0.58 in days.
  - **Nightly pre-releases:** `release-merge.yml` (daily cron) readies + merges that PR → release-please tags → `release.yml` builds artifacts and marks the GitHub release **pre-release**, so it never claims the "Latest" badge / `releases/latest`. Batches a day's merges into one version. Off-cadence: `gh workflow run release-merge.yml`.
  - **Stable milestones:** `promote-release.yml` marks a chosen pre-release as stable + Latest (`gh workflow run promote-release.yml -f tag=vX.Y.Z`). This is the **only** release end users perceive as current. It is an **operator** decision, run at milestones — agents never promote.
  - Pause everything with `.github/RELEASE_FREEZE`. Feature-branch PRs merge as normal; this governs only the release PR + promotion.

## Remote, CI, release, and artifact evidence discipline

Remote-state claims are evidence-bound. Before asserting anything about GitHub
Actions, PR checks, release assets, tags, deleted branches, or workflow
contents, inspect the remote source of truth with `gh` and/or `git show
origin/main:<path>`.

Required distinctions:

- PR merge checks are not the same as post-merge push/tag workflows.
- A release-please PR can pass CI without proving release artifacts were built
  as a merge gate.
- A GitHub Release page's current asset list may differ from what the operator
  observed earlier; compare timestamps instead of contradicting the operator.
- Local workflow files may be hundreds of commits stale. Do not infer remote
  behavior from local files until the branch relationship to `origin/main` is
  known.

When a user challenges a factual claim, stop the line of argument, verify the
claim against primary evidence, and surface the commands/results that support
the corrected conclusion.

## Verification provenance

Every verification report must say what was tested and where: worktree path,
branch, commit SHA when available, local vs CI, and whether the run exercised a
branch build, converged build, packaged artifact, or release asset. Do not let a
successful branch-local run imply that the operator's converged build or a
published release artifact has been verified.

## Documentation propagation contract

For any project-policy claim — an ADR, a spec section, an operator decision — the **canonical source is the ADR or spec itself**. CLAUDE.md, AGENTS.md, plan templates, pitfalls docs, and memory entries are **pointers**, not parallel statements.

**Maximum three propagation sites per ADR:**

1. The ADR itself (always).
2. The spec section it amends, if any.
3. One operational doc — CLAUDE.md OR plan template OR pitfalls — pick one.

Memory entries cite the ADR; they do not restate it. Narrowly-scoped operational recipes that are inherently a how-to (e.g., the exact JSON shape for `.claude/session-leases/main-checkout.json` once D1 lands, or the worktree-disposal ritual step-by-step) MAY live in CLAUDE.md or pitfalls docs where the operator will look for them. The rule is "don't restate what the spec/ADR already says," not "don't write recipes."

**Why:** Without this contract, ADRs and CLAUDE.md drift apart. The same rule appears in three places with slightly different wording; one place is updated, the others rot. The propagation contract makes the ADR/spec the single canonical source.

**Cross-project authority:** [`standing-conventions-cross-project.md` §9](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) carries the portable version of this rule. The two should stay aligned; if they diverge, the standing-conventions doc wins and this section gets a corrective commit.

## Parity with `AGENTS.md`

[AGENTS.md](AGENTS.md) is a deliberate **summary with links** to this file's sections, intended for non-Claude agent harnesses (Codex CLI, `codex review`, and future tooling that picks up the standard `AGENTS.md` convention) where pulling the whole CLAUDE.md inline would be wasteful. It is NOT a full mirror; the substantive rules live here and AGENTS.md points to them.

Codex primary-agent checklist: [docs/agent-workflows/codex-primary-agent-parity.md](docs/agent-workflows/codex-primary-agent-parity.md). This checklist is procedural glue for non-Claude harnesses; it does not supersede CLAUDE.md.

**Upkeep discipline.** Every PR that changes a rule in CLAUDE.md MUST also do the AGENTS.md parity check, in the same PR. The check:

1. Locate the AGENTS.md section that summarizes the CLAUDE.md section you changed.
2. If the change is purely-additive content (clarification, expanded example, new link) AND the AGENTS.md summary line is still accurate, no AGENTS.md update is needed.
3. If the change adds, removes, or renames a CLAUDE.md section, OR alters the load-bearing summary AGENTS.md was providing, update AGENTS.md in the same PR.
4. If a CLAUDE.md change introduces a load-bearing rule for non-Claude agents and no AGENTS.md section currently summarizes it, add one.

Drift between CLAUDE.md and AGENTS.md is a defect. It violates the project's propagation contract (see [§"Documentation propagation contract"](#documentation-propagation-contract) above: CLAUDE.md is the source of truth for substantive rules; AGENTS.md is a pointer).

**When in doubt, ship the AGENTS.md update alongside the CLAUDE.md change.** A redundant tweak is cheaper than a drift bug; the parity check is meant to be light, not skipped.

**Cross-project authority:** [`standing-conventions-cross-project.md` §10](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md).

## Tool referee — which tool owns which job

This project uses both Claude Code's built-in primitives (TodoWrite, auto-memory) and `bd` (Beads). They serve overlapping but **non-redundant** roles. When `bd`'s auto-managed section below (`<!-- BEGIN BEADS INTEGRATION -->`) prescribes a rule that conflicts with the table here, **the table wins.** See [docs/adr/0006-override-bd-claude-md-defaults.md](docs/adr/0006-override-bd-claude-md-defaults.md) for full rationale and watched failure modes.

| Concern | Owns it | Notes |
|---|---|---|
| Cross-session task tracking with deps | `bd` | Primary. Use `bd ready` / `bd update --claim` / `bd close`. |
| In-turn micro-progress within one session | TodoWrite | Claude Code primitive; ephemeral; correct for "read X, edit Y, run Z" lists. |
| User profile + cross-cutting feedback | Auto-memory at `~/.claude/projects/<slug>/memory/` | Harness-native, auto-loaded each session via `MEMORY.md` index. Already seeded; do not migrate to bd. |
| Issue-adjacent factoids discovered during a task | `bd remember` | Use for knowledge linked to a specific issue. Cross-project user/feedback stays in auto-memory. |
| Branch model | Per-task branch + merge-commit (no-ff) | See [ADR 0004](docs/adr/0004-per-task-branch-model.md) (per-task model) + [ADR 0010](docs/adr/0010-no-squash-merge.md) (no-squash) + [ADR 0008](docs/adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) (worktree-issue ownership). |

**Specific overrides of bd's BEADS INTEGRATION block** (rules below the BEADS INTEGRATION marker that this section explicitly supersedes):

- bd says *"do NOT use TodoWrite, TaskCreate, or markdown TODO lists"* → **Override:** TodoWrite is the right primitive for in-turn working memory; bd is the right primitive for cross-session work units. Use both, for their respective layers.
- bd says *"Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files"* → **Override:** the Claude Code auto-memory directory at `~/.claude/projects/<slug>/memory/` is harness-native and remains canonical for user / feedback / project memory. Use `bd remember` for issue-tracker-adjacent factoids only.
- bd says *"Work is NOT complete until `git push` succeeds … YOU must push"* → **No longer overridden** as of 2026-05-17. Per [§Session Completion](#session-completion) and standing-conventions §7, push is now mandatory at session end. bd's directive on this point now agrees with project policy.

**If you discover a fourth bd directive that conflicts with project commitments:** extend the table above AND ADR 0006's override list. Do NOT silently soften an override.

## Session Completion

Work is not complete until `git push` succeeds AND a session-end handoff document exists. This rule is **unconditional** per [`standing-conventions-cross-project.md` §7](https://github.com/cameronzucker/cz-agent-skills/blob/main/docs/standing-conventions-cross-project.md) and Decision 1 of the 2026-05-17 LFST→tuxlink port catalog.

**Required steps before ending any session:**

1. File issues for remaining work discovered during the session (`bd create ...`).
2. Run quality gates if code changed (tests, linters, builds).
3. Update issue tracker status (`bd close <id>` / `bd update <id>`).
4. **`git push`** — mandatory. If push fails, resolve the failure and retry until it succeeds. Do NOT stop before pushing.
5. Clean up: clear stashes, ensure remote task branches are deleted (`gh pr merge --delete-branch` handles this automatically for landed PRs; manual `git push origin --delete <branch>` for branches that didn't reach merge).
6. Write a session-end handoff document to `dev/handoffs/<YYYY-MM-DD>-<short-slug>.md` enumerating: branch state, working-tree state, in-flight worktrees + their untracked + gitignored-stateful content (per [ADR 0009](docs/adr/0009-worktree-disposal-ritual.md) §"Handoff documents enumerate worktree state"), what was completed, what is in-progress, what is pending decision.
7. **Surface the operator's next-session starting prompt** as your final user-facing message of the session, AFTER step 6's handoff is committed. The prompt is a concise (~10-line) paste-ready code block the operator copies into a fresh Claude Code session's first message. Include:
   - One sentence framing what happened this session (so the next session's reads-before-action choices are right-sized).
   - A pointer to the canonical handoff doc by path.
   - The **critical first action or gate** the next session must not skip — particularly anything implicit-droppable (e.g., a review gate before substantive work, a brainstorm before UI tasks).

   Format: a single ```fenced markdown code-block``` the operator can copy whole. The session-start-briefing hook surfaces the most-recent handoff filename automatically; this prompt tells the next agent to READ the handoff and emphasizes anything the agent might otherwise miss while scanning `bd ready` for the next sequential task.

**Never say "ready to push when you are."** Push is the session's responsibility, not the operator's. The handoff document closes the context loop so the next session — possibly on a different machine — can continue without manual reconstruction from `git log`.

**Why step 7 matters:** without an operator-pasteable starting prompt, the operator must either (a) paste a verbatim block out of the handoff doc (verbose; the doc was written for the next agent, not for paste-time), or (b) type freeform "continue where we left off" (relies on the next agent stumbling into the gate-emphasis correctly). Step 7 gives the operator a 10-second copy-paste with the gate emphasis pre-stated, reducing session-change friction.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
