# Spec: fork-setup — establish `tuxlink-pat` and wire tuxlink to it

**Date:** 2026-05-18
**Agent:** oak-fjord-swallow
**bd issue:** `tuxlink-84i` (P1; fork-setup blocks cred-handling refactor `tuxlink-mib`, plan amendments `tuxlink-54p`, AppImage dep doc `tuxlink-gdo`)
**Branch:** `bd-tuxlink-84i/fork-setup` (worktree at `worktrees/bd-tuxlink-84i-fork-setup/`)
**Status:** Draft, pre-adrev

## 1. Context

[ADR 0011](../../adr/0011-fork-pat-for-tuxlink.md) (merged at PR #53, 2026-05-18) committed tuxlink to forking upstream `la5nta/pat` as a tuxlink-owned variant called `tuxlink-pat`. The forking decision was triggered by Pat upstream's `config.json` plaintext-password model + the broader bandaid-spiral pattern of working around each Pat limitation at the tuxlink call site. The structural fix is to own the engine.

ADR 0011 §"Implementation follow-ups" identifies four follow-up bd tasks; this spec covers the **first**: `tuxlink-84i` (fork-setup). The cred-handling refactor (`tuxlink-mib`) is the second and is blocked by this one — fork must exist before patches can land against it.

ADR 0011 settled the strategic direction (fork; full pipeline per patch; opportunistic upstream sync; merge-not-rebase; upstream contribution where patches fit). This spec settles the operational details left open in the ADR: where the fork lives, how tuxlink consumes Pat from it, sync workflow specifics, the fork's own branch model.

The brainstorm settled five decisions, listed in §5 below with their reasoning.

## 2. Scope

**In scope:**

1. Create `cameronzucker/tuxlink-pat` as a fork of `la5nta/pat`.
2. Configure branch protection on `tuxlink-pat/main` mirroring tuxlink's discipline (no force-push; PR-required; no-squash; delete branch on merge).
3. Add `tuxlink-pat` as a git submodule to tuxlink at `external/tuxlink-pat/`.
4. Extend tuxlink's `src-tauri/build.rs` to invoke `go build` in the submodule and produce a Pat binary at a known path.
5. Update tuxlink's `src-tauri/tauri.conf.json` `bundle.externalBin` to bundle the built Pat binary into AppImage releases.
6. Write `tuxlink-pat/README.md` documenting: fork rationale (points to ADR 0011), per-patch workflow, opportunistic-sync model, upstream-PR contribution policy.
7. Add a `docs/development.md` (or equivalent) note in tuxlink documenting the Go toolchain requirement for source builds, and explicitly stating end-users running the AppImage do NOT need Go.
8. Verify end-to-end: clean clone + `git submodule update --init --recursive` + `cargo build` produces a Pat binary; `pat_process.rs::spawn` finds it; AppImage build produces a working artifact.

**Out of scope** (deferred to subsequent tasks, dep-tracked in bd):

- Cred-handling refactor on `tuxlink-pat` (the first agentic patch against the fork). Owned by `tuxlink-mib`. Full `build-robust-features` pipeline per ADR 0011 §3.
- v0.0.1 plan amendments for Tasks 5/6/9/11 reflecting the fork + keyring cred model. Owned by `tuxlink-54p`.
- AppImage `secret-service` system-package dep documentation. Owned by `tuxlink-gdo` (blocked by `tuxlink-mib`).
- Re-evaluating personal-account vs GitHub-org ownership for tuxlink/tuxlink-pat. Deferred per brainstorm decision; revisit triggers documented in §5.1 below.
- Adding prebuilt-binary releases to `tuxlink-pat` (i.e., `tuxlink-pat` publishing its own GH Releases for external consumers). Deferred per brainstorm decision §5.2; revisit when external consumers emerge.
- Adding maintainers, contributor-onboarding docs (CONTRIBUTING.md, CODE_OF_CONDUCT.md), issue templates, etc. on `tuxlink-pat`. Deferred until external interest exists.

## 3. Design

### 3.1 Architecture overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                cameronzucker/tuxlink (this repo)                    │
│                                                                     │
│  src-tauri/                                                         │
│  ├── build.rs ────────► invokes `go build` in submodule ──┐         │
│  ├── tauri.conf.json (bundle.externalBin: <pat-binary>) ◄─┘         │
│  └── src/                                                           │
│      └── pat_process.rs ──► Command::new(bundled-pat-path)          │
│                                                                     │
│  external/                                                          │
│  └── tuxlink-pat/ ─────► git submodule (pinned commit SHA)          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                          │ submodule reference
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                  cameronzucker/tuxlink-pat                          │
│                                                                     │
│  main (integrated: upstream merges + tuxlink patches; merge-commits)│
│  ├── patch-<slug>/* branches per fork patch                         │
│  └── upstream remote: la5nta/pat                                    │
│                                                                     │
│  Workflow per patch:                                                │
│    1. branch off main                                               │
│    2. git fetch upstream && git merge upstream/master  (opportunistic sync)
│    3. brainstorm → 5-round adrev → writing-plans-enhanced →        │
│       plan-review-cycle → TDD impl → Codex on impl diff             │
│    4. PR against main; merge-commit (no-ff); no squash             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                          ▲
                          │ git fetch upstream (only during patches)
                          │
                   la5nta/pat (upstream; we do not push to upstream)
```

End-users see one artifact: the AppImage. Pat is bundled inside; end-users never install Go, never download Pat separately, never see the submodule.

### 3.2 Component inventory

| Component | Owned by | What it does |
|---|---|---|
| `cameronzucker/tuxlink-pat` (GH repo) | Cameron (creates via `gh repo fork`) | Fork of `la5nta/pat`; lives on Cameron's personal account per §5.1 decision |
| Branch protection on `tuxlink-pat/main` | Cameron (configures via GH UI or `gh api`) | Mirrors tuxlink's: no force-push; no direct commits; PR-required; merge-commit only; delete branch on merge |
| `tuxlink/external/tuxlink-pat/` submodule | Agent (`git submodule add`) | Pinned git submodule reference; URL points at `cameronzucker/tuxlink-pat`; initial pin = upstream `la5nta/pat` HEAD at fork-creation time |
| `tuxlink/src-tauri/build.rs` extension | Agent | Invokes `go build -o $OUT_DIR/tuxlink-pat ./` in the submodule directory; fails clearly if Go missing or submodule uninitialized |
| `tuxlink/src-tauri/tauri.conf.json` `bundle.externalBin` | Agent | Adds the built `tuxlink-pat` binary to the bundle so AppImage includes it |
| `tuxlink-pat/README.md` (PR'd into the fork) | Agent | Documents: fork rationale (cites ADR 0011), workflow per patch, opportunistic-sync model, upstream-PR policy, branch protection rules |
| `tuxlink/docs/development.md` Go-toolchain note | Agent | One section explaining the Go build dep for source builds and that AppImage users don't need it |
| End-to-end build verification | Agent | Clean clone + `git submodule update --init --recursive` + `cargo build` + `pat_process.rs::spawn` finds the binary |

### 3.3 Data flow — lifecycle of a fork patch

This flow describes what happens when a future Pat-side patch (e.g., the cred-refactor `tuxlink-mib`) is implemented:

1. Agent (or Cameron) claims the patch's bd issue: `bd update <id> --claim`.
2. Creates a worktree **on `tuxlink-pat`** (not on tuxlink): `git worktree add -b patch-<slug> <path> origin/main`. Per-patch branch name follows the same convention as tuxlink's task branches.
3. From the worktree's branch (Step 1 of the patch's pipeline): `git fetch upstream && git merge upstream/master`. This is the **opportunistic sync** — happens at patch time, not on a separate schedule.
4. Resolve any upstream conflicts as part of the patch's brainstorm/plan. The merge-strategy is `merge`, not `rebase`, per ADR 0011 §5.
5. Run the full `build-robust-features` pipeline on the patch: brainstorm → 5-round adrev (≥1 cross-provider Codex) → `writing-plans-enhanced` → `plan-review-cycle` → TDD impl → Codex on impl diff.
6. PR against `tuxlink-pat/main`; merge with merge-commit (no-ff); delete branch on merge.
7. **Switch to tuxlink:** update the submodule pin to the new `tuxlink-pat` commit:

   ```
   cd <tuxlink-worktree>/external/tuxlink-pat
   git fetch origin
   git checkout <new-tuxlink-pat-sha>
   cd <tuxlink-worktree>
   git add external/tuxlink-pat
   git commit -m "build(pat): bump submodule to <sha-short>"
   ```

8. Tuxlink CI rebuilds Pat from the new pin; smoke tests exercise; PR the submodule bump back to `feat/v0.0.1`.
9. Optionally: submit upstream PR to `la5nta/pat` per ADR 0011 §4. Pursue if the patch fits upstream's accepted scope; keep fork-only otherwise.

The two-PR pattern (one against `tuxlink-pat`; one against `tuxlink` to bump the submodule pin) is intentional and preserves the "tuxlink-pat is its own project" framing.

### 3.4 Error handling

- **Go toolchain missing at tuxlink build time:** `build.rs` checks `which go` (or `Command::new("go").arg("version").output()`); errors with a clear message:

  ```
  error: Go toolchain required to build Pat from the tuxlink-pat submodule.
         Install: apt install golang-go (Debian/Ubuntu) or equivalent.
         End-users: use the prebuilt AppImage instead of building from source.
         See docs/development.md.
  ```

- **Submodule not initialized:** `build.rs` checks `external/tuxlink-pat/.git` exists; errors with:

  ```
  error: external/tuxlink-pat submodule not initialized.
         Run: git submodule update --init --recursive
  ```

- **Pat (Go) build failure:** cargo error surface shows the Go compile error; `build.rs` forwards `go build` stderr verbatim via `eprintln!` and exits non-zero. Don't try to be clever about Go diagnostics — let the Go toolchain's error speak for itself.

- **Upstream merge conflict during opportunistic sync:** handled within the patch's brainstorm/plan (not an auto-rollback case). The patch's `build-robust-features` pipeline surfaces the conflict and the agent + Cameron decide resolution.

- **AppImage CI lacks Go:** CI config (`.github/workflows/release.yml` or equivalent) pins a Go version via `actions/setup-go@v5` with `go-version: '1.22'` (or current). Documented in the workflow file's comments.

- **Submodule pin drift across local clones:** standard git submodule semantics. `git status` flags out-of-date submodules; CI fails if the pinned SHA doesn't match the canonical `main` reference. No special handling needed.

### 3.5 Testing

- **`cargo build` smoke (tuxlink-side):** the build successfully invokes `go build` in the submodule; produces Pat binary at the expected path under `$OUT_DIR`; subsequent runtime calls in `pat_process.rs::spawn` find the binary via the bundled path. New build-script-level assertion: if the binary doesn't exist at expected path after `go build`, `build.rs` panics with a clear message.
- **`cargo test` (tuxlink-side):** unchanged; Pat is bundled, not directly exercised by unit tests. The existing `pat_process_test` suite (if present) continues to use its existing test fixtures.
- **AppImage build (CI):** full release CI builds Pat from submodule, bundles into AppImage, AppImage smoke-tests on a clean Ubuntu container — verify the bundled Pat runs (`./AppRun --pat-version` or equivalent) without external dependencies on the host.
- **Submodule reproducibility:** clean clone + `git submodule update --init --recursive` + `cargo build` produces a Pat binary byte-identical to a canonical reference build at the same submodule SHA. (Verified by ad-hoc check; not a CI assertion since byte-identical builds may need additional Go-reproducibility flags.)
- **Branch protection enforcement (`tuxlink-pat`):** post-setup verification — attempt a direct push to `tuxlink-pat/main` as Cameron; confirm GH rejects it.

Tests for the per-patch workflow itself (`tuxlink-mib` cred-refactor and subsequent patches) are owned by each patch's own task, not by fork-setup.

### 3.6 Operational split

| Step | Operator (Cameron) | Agent (oak-fjord-swallow or subagent) |
|---|---|---|
| Create `cameronzucker/tuxlink-pat` via `gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false` | ✓ | |
| Configure branch protection on `tuxlink-pat/main` (no force-push, PR-required, merge-commit only, delete branch on merge) | ✓ | |
| Add submodule to tuxlink (`git submodule add https://github.com/cameronzucker/tuxlink-pat external/tuxlink-pat`) | | ✓ |
| Extend `src-tauri/build.rs` with Go-build integration | | ✓ |
| Update `src-tauri/tauri.conf.json` `bundle.externalBin` | | ✓ |
| Write `tuxlink-pat/README.md` (PR'd into the fork) | | ✓ |
| Write `tuxlink/docs/development.md` Go-toolchain note | | ✓ |
| First end-to-end build verification (`cargo build` produces Pat binary; `pat_process.rs::spawn` finds it) | | ✓ |
| Update AppImage CI (`.github/workflows/release.yml` or equivalent) to pin Go version via `actions/setup-go` | | ✓ |
| Review + merge the fork-setup PRs (one against tuxlink, one against tuxlink-pat) | ✓ | |

## 4. Commit shape

Two PRs land for this task, against different repos:

**PR A (against `cameronzucker/tuxlink-pat/main`):** the fork's README.md and (if `gh repo fork` doesn't create them) initial branch-protection-equivalent CI workflow. Small diff. Title: `[<moniker>] docs(readme): tuxlink-pat fork README + workflow`. The fork's `main` starts at upstream `la5nta/pat` HEAD at fork-creation time; this PR adds the README on top.

**PR B (against `cameronzucker/tuxlink/feat/v0.0.1`):** the submodule + build.rs + tauri.conf.json + docs/development.md changes. Larger diff. Title: `[<moniker>] build(pat): wire tuxlink-pat submodule + Go-build integration (closes tuxlink-84i)`. Body cites this spec + the plan + the adrev transcripts.

PR B depends on PR A landing first (the submodule URL needs to resolve). Sequence:

1. Operator creates the fork + configures branch protection
2. Agent opens PR A against `tuxlink-pat`; operator merges
3. Agent opens PR B against `tuxlink`; operator merges
4. `tuxlink-84i` closes via PR B's merge

Both commits include `Agent: <moniker>` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailers. Heredoc commit-message syntax per CLAUDE.md (avoids destructive-git hook substring match).

## 5. Decisions captured during brainstorm

### 5.1 Repo home: `cameronzucker/tuxlink-pat` (personal account)

**Decision:** Both `tuxlink` and `tuxlink-pat` live under Cameron's personal account for now.

**Reasoning:** Cameron's stated goal is clear sole-creator attribution for a future-many-starred open source project addressing a longstanding technical need. GH commit history preserves attribution regardless of repo ownership; personal-account ownership additionally surfaces both repos on Cameron's profile during the early-stage discovery window when that visibility matters most. Migration cost forward (personal → org via 1-click GH repo-transfer; URLs redirect; stars/issues/PRs migrate) is near-zero at A-audience scale with no third-party dependents.

**Re-evaluate org migration when** (any one of):
- Dependent forks of `tuxlink` emerge (third parties building on it)
- Multi-maintainer additions (someone else wanting maintainer rights)
- Star count / community size where "this is a project, not Cameron's side thing" becomes the stronger signal
- A specific contributor onboarding is friction-y because of personal-account perception

### 5.2 Build integration: git submodule now; prebuilt releases additive later

**Decision:** Tuxlink consumes Pat via git submodule at `external/tuxlink-pat/`. `build.rs` invokes `go build`. AppImage bundles the resulting binary.

**Reasoning:** End-user installation simplicity is identical between submodule and prebuilt-release options (the AppImage is the user-facing artifact either way; users never see Pat or Go). The differentiating factor is developer/CI experience during the agentic-refactor arc. Submodule wins on:

- **Iteration friction:** patch → `cargo build` (~1-2 min) vs patch → tag → release CI → tuxlink picks up release (~10-15 min). The cred-refactor task alone (next) compounds this fast given build-robust-features per-patch pipeline + multi-round TDD.
- **Commit reproducibility:** submodule SHA is immutable; prebuilt release tags can be re-cut/replaced.
- **CI surface area:** commit CI only vs commit + release-matrix CI.
- **Network dep at build time:** none vs required.

Tradeoff: Go toolchain becomes a tuxlink build dep (one-line `apt install golang-go`). End-users running the AppImage are unaffected.

**Add prebuilt releases later if** external consumers emerge wanting to consume `tuxlink-pat` independently of tuxlink. Adding is additive (keep submodule path for dev iteration; add release CI when there's an audience).

### 5.3 Upstream sync: opportunistic-during-patches

**Decision:** Upstream `la5nta/pat` is merged into `tuxlink-pat/main` opportunistically — whenever an agent (or Cameron) opens a worktree for a fork patch, the first step is `git fetch upstream && git merge upstream/master`. No standing schedule.

**Reasoning:** During the agentic-refactor arc (cred-refactor + future patches), we'll be in `tuxlink-pat` regularly anyway; piggybacking sync on patch work is zero-overhead. Eliminates "remember to sync" cognitive load. Conflict resolution naturally happens inside the patch's `build-robust-features` pipeline.

**Responsibility:** whoever's doing the patch (agent or Cameron). No scheduled jobs, no dedicated sync agent.

**Re-evaluate when** patch cadence drops to "less than monthly" — at that point a weekly scheduled sync (e.g., via the `schedule` skill) may become useful to keep `tuxlink-pat/main` close to upstream so the next patch doesn't face a big merge.

### 5.4 Conflict resolution: per-patch, no standing policy

**Decision:** When upstream merge produces conflicts, resolution happens inside the patch's brainstorm/plan/adrev cycle. No pre-documented "our patches always win" or "upstream always wins" rule.

**Reasoning:** At current patch scale (1-2 patches expected initially), per-incident resolution under the build-robust-features discipline is sufficient. A formal precedence policy is premature optimization until the patch count + conflict frequency justifies it.

**Re-evaluate when** the fork accumulates 4+ patches AND upstream-merge conflicts become routine (more than 1-in-3 syncs conflicting).

### 5.5 Branch model on `tuxlink-pat`: per-patch branches + PRs into `main`

**Decision:** Each fork patch lives on its own branch (`patch-<slug>` or `<bd-id>/<slug>`); PR'd against `main`; merged with merge-commit (no-ff); delete branch on merge. Mirrors tuxlink's own per-task-branch model exactly.

**Reasoning:** Same disciplines that work for tuxlink work for tuxlink-pat. Lowest cognitive overhead — agents and Cameron already internalize this workflow. Provides PR-level review surface for fork patches. Preserves the integrated-state-in-main-with-merge-commits forensic history (per ADR 0010 reasoning, applied to the fork too).

**Anti-pattern explicitly rejected** (alternative B from brainstorm): long-lived `tuxlink-patches` branch rebased onto new upstream. Rebasing patches is destructive-git-ban territory; force-pushing the patches branch hits the project's hook. Workable but works against the project's git discipline.

## 6. Risks and watched failure modes

(Populated initially from brainstorm; augmented by 5-round adrev findings before plan-writing.)

- **Go toolchain version drift:** different operators / different CI environments may have different Go versions, producing subtly different Pat binaries. Mitigation: pin Go version in CI; document recommended Go version in `docs/development.md`; warn in `build.rs` if Go version is significantly older than the pinned version.
- **Submodule pin drift across worktrees:** a developer working on tuxlink in worktree A may update the submodule pin; a developer in worktree B picks up the new pin only when they `git fetch + git submodule update`. Mitigation: standard git submodule semantics + clear docs.
- **Upstream Pat changes its build system:** la5nta/pat could refactor its build (e.g., switch from `go build ./` to `make` or `bazel`). Mitigation: build.rs is intentionally minimal — fixing it is small if upstream changes. Documented in design that build.rs is tightly coupled to Pat's current build invocation.
- **Tauri changes `bundle.externalBin` semantics:** Tauri 3.x (future) may change how external binaries are bundled. Mitigation: tauri.conf.json bundle config follows current Tauri 2.x convention; future Tauri upgrades will surface the issue at upgrade time, not silently.
- **AppImage CI lacks Go:** mentioned in error handling; mitigation in CI config + documentation.
- **Branch protection mis-configured:** Cameron sets up branch protection manually; risk of typo / missing rule. Mitigation: post-setup verification step (attempt direct push, confirm rejection).
- **Pat config.json compatibility during the cred-refactor transition:** when `tuxlink-mib` lands keyring-backed cred reads in Pat, existing Pat installs may still have config.json with the password. Backward compatibility is out of scope for fork-setup (owned by `tuxlink-mib`), but flagged here as a known followon concern.
- **Risks identified by adrev rounds:** to be appended below by the agent after the 5-round adrev completes.

## 7. References

- [ADR 0011 — Fork Pat as `tuxlink-pat`](../../adr/0011-fork-pat-for-tuxlink.md) — the strategic decision this spec operationalizes
- [ADR 0003 — Pat owns the mailbox](../../adr/0003-no-sqlite-pat-owns-mailbox.md) — amended by ADR 0011 (dependency target shift); the mailbox-ownership decision still holds
- [ADR 0008 — Worktrees mandatory under bd-issue ownership](../../adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) — applies to per-patch branches on tuxlink-pat
- [ADR 0010 — No-squash merge](../../adr/0010-no-squash-merge.md) — applies to tuxlink-pat's PR model
- [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — relevant context for the future cred-refactor task; not directly applicable to fork-setup itself
- `bd show tuxlink-84i` — this task's bd record (claim + status; closes via PR B's merge)
- `bd show tuxlink-mib` — next task (cred-handling refactor), blocked by this one
