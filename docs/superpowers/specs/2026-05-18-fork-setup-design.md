# Spec: fork-setup — establish `tuxlink-pat` and wire tuxlink to it

**Date:** 2026-05-18
**Agent:** oak-fjord-swallow
**bd issue:** `tuxlink-84i` (P1; fork-setup blocks cred-handling refactor `tuxlink-mib`, plan amendments `tuxlink-54p`, AppImage dep doc `tuxlink-gdo`)
**Branch:** `bd-tuxlink-84i/fork-setup` (worktree at `worktrees/bd-tuxlink-84i-fork-setup/`)
**Status:** Revised post-adrev (5 rounds: 4 Claude subagents + 1 Codex cross-provider; 49 findings; 5 P0 + 15 P1 applied in this revision; see §8 for full disposition)

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
| `cameronzucker/tuxlink-pat` (GH repo) | Cameron (creates via `gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false`) | Fork of `la5nta/pat`; lives on Cameron's personal account per §5.1 decision |
| Branch protection on `tuxlink-pat/main` | Cameron (configures via GH UI or `gh api`) | Mirrors tuxlink's: no force-push; no direct commits; PR-required; merge-commit only. **Branches RETAINED on merge** (not deleted) — the upstream-PR contribution policy (ADR 0011 §4) requires source-branch survival for cherry-pick portability. Differs intentionally from tuxlink's `gh pr merge --delete-branch` convention. |
| GitHub Issues on `tuxlink-pat` | Cameron (enable in repo settings) | Issue-tracker home for fork-only concerns (Pat-side bugs, upstream-sync conflicts, patch feature requests). `bd` remains tuxlink-only; tuxlink-pat uses GH Issues since the audience may eventually include external contributors. |
| `tuxlink/external/tuxlink-pat/` submodule | Agent (`git submodule add https://github.com/cameronzucker/tuxlink-pat external/tuxlink-pat`) | Git submodule at repo-root (NOT under `src-tauri/`). HTTPS URL; SSH-preferring operators set `insteadOf` in their `~/.gitconfig`. Initial pin = `tuxlink-pat/main` HEAD post-PR-A. Pin commits in tuxlink are explicit per ADR 0011 §3 workflow (no `--remote` auto-tracking). |
| `tuxlink/src-tauri/build.rs` extension | Agent | (1) Resolves submodule path via `Path::new(env!("CARGO_MANIFEST_DIR")).join("../external/tuxlink-pat")` — NOT relative-to-CWD (Codex P2 catch: src-tauri/ is the crate root, repo root is one level up). (2) Strengthened pre-flight: `.git` presence + `make.bash` existence + non-empty working tree (catches deinit / partial clone / `--recurse-submodules=false`; R2 P0 catch). (3) **Gated to `release` profile only** via `if std::env::var("PROFILE").as_deref() == Ok("release")` so `cargo test` and `cargo build` (debug) do NOT trigger Go build (Codex P2 catch: build.rs runs during tests; "tests unchanged" claim was wrong). (4) Invokes `SKIP_TESTS=1 bash make.bash` (upstream Pat's documented build entry point per `make.bash`; NOT bare `go build`). (5) Renames the produced `pat` binary to `pat-<TARGET-TRIPLE>` at a stable path that tauri.conf.json references statically (NOT `$OUT_DIR`; the produced sidecar must be findable by Tauri's static lookup); current target read via `env!("TARGET")`. (6) Errors clearly per §3.4 on: Go missing, Go <1.24, libax25 missing on Linux+CGO, submodule incomplete, `make.bash` non-zero exit. (7) Emits `cargo:rerun-if-changed=../external/tuxlink-pat` so submodule pin bumps invalidate cargo's incremental cache (R2 P1 catch). |
| `tuxlink/src-tauri/tauri.conf.json` `bundle.externalBin` | Agent | References the sidecar path per Tauri 2.x's `<binary-name>{-<target-triple>}{.<system-extension>}` lookup convention (verified via context7 — R3 P0 / R4 P1 cross-provider catch; without the target-triple suffix, AppImage bundle step fails). Recommended path: `src-tauri/sidecars/pat` (configured value); actual file on disk: `src-tauri/sidecars/pat-<triple>`. build.rs writes to the suffixed file; Tauri's externalBin lookup finds it by the convention. |
| `.github/workflows/release.yml` (or equivalent CI) | Agent | (1) Pins Go via `actions/setup-go@v6` (current major per context7) with `go-version-file: 'external/tuxlink-pat/go.mod'` — delegates Go-version contract to upstream Pat's own declaration (R1 P1 + R3 P1 source-of-truth pattern; no version literal in tuxlink CI). (2) Cache key includes `${{ hashFiles('external/tuxlink-pat/**') }}` so submodule SHA bumps invalidate cached Pat builds (R2 P1). (3) Calls `git submodule update --init --recursive` before any cargo invocation. (4) Installs libax25 dev headers (`apt install libax25-dev` for Debian/Ubuntu) so Pat's Linux+CGO build path resolves. |
| `tuxlink-pat/README.md` (PR'd into the fork as PR-A) | Agent | Documents: fork rationale (cites ADR 0011); per-patch workflow including the **explicit `git remote add upstream https://github.com/la5nta/pat.git`** step (Codex P2 catch: upstream remote was implicit before); opportunistic-sync model; upstream-PR policy; branch-retention exception. ~60-80 lines; rationale + workflow + ADR 0011 pointer for strategic context. |
| `tuxlink/docs/development.md` build deps note | Agent | One section explaining: Go 1.24+ build dep for source builds (per Pat's `go.mod`); libax25-dev (Debian/Ubuntu) for full Pat AX.25 functionality on Linux; AppImage users do NOT need Go or libax25 (bundled). Includes the exact `apt install golang-go libax25-dev` line. |
| End-to-end build verification | Agent | (1) Clean clone with `git clone --recurse-submodules` → `cargo build --release` from repo root (triggers build.rs since profile=release) → `src-tauri/sidecars/pat-<triple>` exists at stable path → `pat_process.rs::spawn` finds it. (2) Clean clone WITHOUT `--recurse-submodules` followed by `git submodule update --init --recursive` produces the same end state (validates the documented bootstrap path). (3) `cargo test` (debug profile) succeeds without invoking Go or requiring submodule content (validates the release-only gate). |

### 3.3 Data flow — lifecycle of a fork patch

This flow describes what happens when a future Pat-side patch (e.g., the cred-refactor `tuxlink-mib`) is implemented:

1. Agent (or Cameron) claims the patch's bd issue: `bd update <id> --claim`.
2. Creates a worktree **on `tuxlink-pat`** (not on tuxlink): `git worktree add -b patch-<slug> <path> origin/main`. Per-patch branch name follows the same convention as tuxlink's task branches.
3. **Verify the `upstream` remote exists** (one-time setup per fresh clone): `git remote get-url upstream || git remote add upstream https://github.com/la5nta/pat.git`. The fork-setup task itself documents this in the tuxlink-pat README; per-patch workflow re-asserts via the `||` idiom so a fresh clone bootstraps the remote.
4. From the worktree's branch (Step 1 of the patch's pipeline): `git fetch upstream && git merge upstream/<upstream-default-branch>` where `<upstream-default-branch>` is upstream Pat's default-branch name. **As of 2026-05-18 that is `master`** (verified via `gh api repos/la5nta/pat --jq '.default_branch'`); if la5nta/pat migrates to `main` in the future, the per-patch agent verifies via the same `gh api` call and uses the current value — the tuxlink-pat README documents the verification pattern so the branch name is not hardcoded anywhere agents would need to remember to update. This is the **opportunistic sync** — happens at patch time, not on a separate schedule.
5. Resolve any upstream conflicts as part of the patch's brainstorm/plan. The merge-strategy is `merge`, not `rebase`, per ADR 0011 §5.
6. Run the full `build-robust-features` pipeline on the patch: brainstorm → 5-round adrev (≥1 cross-provider Codex) → `writing-plans-enhanced` → `plan-review-cycle` → TDD impl → Codex on impl diff.
7. PR against `tuxlink-pat/main`; merge with merge-commit (no-ff). **Branch is RETAINED** (not deleted) per §3.2 branch-protection note — the upstream-PR contribution policy needs the source branch for cherry-pick. This differs intentionally from tuxlink's `--delete-branch` convention.
8. **Switch to tuxlink:** update the submodule pin to the new `tuxlink-pat` commit:

   ```
   cd <tuxlink-worktree>/external/tuxlink-pat
   git fetch origin
   git checkout <new-tuxlink-pat-sha>
   cd <tuxlink-worktree>
   git add external/tuxlink-pat
   git commit -m "build(pat): bump submodule to <sha-short>"
   ```

9. Tuxlink CI rebuilds Pat from the new pin (the `cargo:rerun-if-changed` directive in build.rs makes this automatic on submodule SHA bump); smoke tests exercise; PR the submodule bump back to `feat/v0.0.1`.
10. Upstream contribution: pursue or skip per ADR 0011 §4 (the policy lives there; this spec does not restate it).

The two-PR pattern (one against `tuxlink-pat`; one against `tuxlink` to bump the submodule pin) is intentional and preserves the "tuxlink-pat is its own project" framing. **Automation of the submodule-pin-bump PR is a deferred follow-up** (post-2-3 manual cycles to see what's worth automating; not in fork-setup scope per R1 P1 disposition).

### 3.4 Error handling

All error cases below trigger only on release-profile builds (build.rs is gated to `PROFILE=release` per §3.2; debug builds and `cargo test` skip the Go-build path entirely).

- **Go toolchain missing:** `build.rs` runs `Command::new("go").arg("version").output()`; on `Err`, errors with:

  ```
  error: Go toolchain required to build Pat from the tuxlink-pat submodule.
         Install: apt install golang-go libax25-dev (Debian/Ubuntu) or equivalent.
         Pat requires Go 1.24 or later (per external/tuxlink-pat/go.mod).
         End-users: use the prebuilt AppImage instead of building from source.
         See docs/development.md.
  ```

- **Go version too old:** `build.rs` parses `go version` output; if <1.24, errors with:

  ```
  error: Go 1.24 or later required (per external/tuxlink-pat/go.mod and Pat's make.bash).
         Detected: go<VERSION>
         Upgrade: see https://go.dev/doc/install
  ```

- **Submodule incomplete:** `build.rs` checks THREE conditions (R2 P0 catch — `.git` alone is insufficient):

  1. `external/tuxlink-pat/.git` exists (file or directory; submodules use a file pointing to the parent's `.git/modules/<name>`)
  2. `external/tuxlink-pat/make.bash` exists (canary file from upstream Pat; confirms the submodule has actual content, not just an empty initialized state)
  3. The submodule's HEAD SHA matches what `git ls-tree HEAD external/tuxlink-pat` reports for the parent (catches SHA-mismatch where someone updated the parent index without running `git submodule update`)

  On any failure, errors with:

  ```
  error: external/tuxlink-pat submodule is not in a buildable state.
         Detected: <which check failed>
         Recover:
           git submodule deinit -f external/tuxlink-pat   # clean prior state
           git submodule update --init --recursive        # re-bootstrap
  ```

- **libax25 missing on Linux+CGO:** Pat's `make.bash` warns about this; `build.rs` propagates the warning to cargo via `cargo:warning=...`. Not a hard error — Pat builds without libax25, just without AX.25 hardware modem support, which v0.0.1 doesn't exercise. Documented in `docs/development.md`.

- **`make.bash` non-zero exit:** `build.rs` forwards Pat's `make.bash` stderr verbatim via `eprintln!` and exits non-zero. Don't try to be clever about Go diagnostics — let the toolchain's error speak for itself.

- **Sidecar rename fails** (rare; disk-full, permissions): `build.rs` reports the exact file path and underlying I/O error.

- **Upstream merge conflict during opportunistic sync:** handled within the patch's brainstorm/plan (not an auto-rollback case). The patch's `build-robust-features` pipeline surfaces the conflict; the agent + Cameron decide resolution.

- **AppImage CI lacks Go:** CI config uses `actions/setup-go@v6` with `go-version-file: 'external/tuxlink-pat/go.mod'` (per §3.2). If a CI runner doesn't support `actions/setup-go`, the workflow file's own setup step fails fast with the action's standard error — not a tuxlink-side concern.

- **Submodule pin drift across local clones:** standard git submodule semantics. `git status` flags out-of-date submodules; CI fails if the pinned SHA doesn't match what's in `external/tuxlink-pat` HEAD (caught by §3.4 condition 3 above). No special handling beyond clear error.

- **PR-A merged but PR-B not yet open** (state between the two-PR landing per §4): tuxlink's `feat/v0.0.1` continues to build without the submodule (since no submodule reference is added until PR-B); the `external/tuxlink-pat` directory simply doesn't exist. PR-B is the diff that introduces the dependency. No partial-state to handle.

- **PR-B opened against tuxlink before fork exists** (operator forgot to run `gh repo fork`): the submodule URL `https://github.com/cameronzucker/tuxlink-pat` returns 404. PR-B's CI run fails at `git submodule update --init`. **Pre-flight gate** (R2 P0 catch): the first agent action in PR-B's worktree is `gh api repos/cameronzucker/tuxlink-pat --jq '.full_name' || { echo "ERROR: tuxlink-pat repo does not exist; run 'gh repo fork la5nta/pat --fork-name tuxlink-pat' first"; exit 1; }`. PR-B will not be opened until this check passes.

### 3.5 Testing

- **`cargo build --release` smoke (tuxlink-side):** release build triggers build.rs's Go-build path (debug doesn't, per §3.2 release-only gate); `make.bash` produces `pat` in the submodule; build.rs renames to `src-tauri/sidecars/pat-<TARGET-TRIPLE>`; the sidecar file exists at the stable path tauri.conf.json references; subsequent runtime calls in `pat_process.rs::spawn` find it. Build-script assertion: if the sidecar doesn't exist at expected path after rename, `build.rs` panics with the exact path checked.
- **`cargo build` (debug; tuxlink-side):** build.rs's release-only gate causes the Go-build path to be skipped; debug build succeeds even without Go installed or submodule initialized. Verifies the gate works.
- **`cargo test` (tuxlink-side):** same release-only gate — Go-build path skipped (Codex P2 catch: previously the spec claimed "tests unchanged" but build.rs runs during test builds and would have failed clean clones; the release-only gate fixes this). The existing `pat_process_test` suite (if present) continues unchanged; Pat is not directly exercised by unit tests.
- **AppImage build (CI):** full release CI builds Pat from submodule via build.rs's release path; bundles the sidecar into AppImage via Tauri's externalBin; runs the AppImage and verifies the bundled Pat is invocable (e.g., `./tuxlink.AppImage --pat-version` if tuxlink exposes such a probe, or smoke via `pat_process.rs::spawn` followed by HTTP probe of Pat's `/api/version`).
- **Branch protection enforcement (`tuxlink-pat`):** post-setup verification via automated readback (R2 P1 catch — operator-manual setup needs automated post-condition check, not just attempted-push smoke):

  ```bash
  gh api repos/cameronzucker/tuxlink-pat/branches/main/protection --jq '{
    required_status_checks, enforce_admins, required_pull_request_reviews,
    restrictions, allow_force_pushes, allow_deletions
  }'
  ```

  Assert: `allow_force_pushes.enabled == false`, `allow_deletions.enabled == false`, `required_pull_request_reviews != null`. Documented as a one-shot post-setup check; subsequent settings drift would surface on the next per-patch workflow when the agent confirms the readback before merging.

Tests for the per-patch workflow itself (`tuxlink-mib` cred-refactor and subsequent patches) are owned by each patch's own task, not by fork-setup.

### 3.6 Operational split

Ordered for explicit pre-flight gates between operator-action steps and agent-action steps that depend on them (R2 P0 catch: the original spec had no pre-flight checks; agent could open PR-B before fork existed and 404 silently).

| # | Step | Operator | Agent | Pre-flight gate before agent proceeds |
|---|---|---|---|---|
| 1 | Create `cameronzucker/tuxlink-pat`: `gh repo fork la5nta/pat --fork-name tuxlink-pat --clone=false` | ✓ | | — |
| 2 | Configure branch protection on `tuxlink-pat/main`: no force-push, no deletions, PR-required, merge-commit only, **branches RETAINED on merge** (NOT `delete-branch`) per §3.2 + ADR 0011 §4 cherry-pick requirement | ✓ | | — |
| 3 | Enable GitHub Issues on `tuxlink-pat` (repo Settings → Features → Issues) | ✓ | | — |
| 4 | **Pre-flight check before agent opens PR-A:** `gh api repos/cameronzucker/tuxlink-pat --jq '.full_name'` returns `cameronzucker/tuxlink-pat` (not 404) AND `gh api repos/cameronzucker/tuxlink-pat/branches/main/protection --jq '.allow_force_pushes.enabled'` returns `false`. Agent halts + reports to operator if either fails. | | ✓ (verify) | Steps 1-3 must be done |
| 5 | Write `tuxlink-pat/README.md` + open as PR-A against `tuxlink-pat/main` | | ✓ | Step 4 passed |
| 6 | Operator reviews + merges PR-A | ✓ | | — |
| 7 | **Pre-flight check before agent adds submodule:** `gh api repos/cameronzucker/tuxlink-pat/branches/main/protection` confirms branch protection still in place AND `git ls-remote https://github.com/cameronzucker/tuxlink-pat main` returns a SHA (not empty/404). Agent halts if fails. | | ✓ (verify) | PR-A merged |
| 8 | Add submodule to tuxlink at repo-root: `git submodule add https://github.com/cameronzucker/tuxlink-pat external/tuxlink-pat`. Initial pin = current `tuxlink-pat/main` HEAD (just-merged PR-A's merge commit). | | ✓ | Step 7 passed |
| 9 | Extend `src-tauri/build.rs` per §3.2 (release-only gate; `CARGO_MANIFEST_DIR/../external/tuxlink-pat` path; submodule pre-flight; `SKIP_TESTS=1 bash make.bash`; sidecar rename to `pat-<TARGET-TRIPLE>`; `cargo:rerun-if-changed`) | | ✓ | Step 8 passed |
| 10 | Update `src-tauri/tauri.conf.json` `bundle.externalBin` to reference `sidecars/pat` (the unsuffixed configured value per Tauri convention; actual file is `sidecars/pat-<triple>`) | | ✓ | Step 9 done |
| 11 | Update `.github/workflows/release.yml` (or equivalent) with `actions/setup-go@v6` + `go-version-file: 'external/tuxlink-pat/go.mod'` + cache key including `hashFiles('external/tuxlink-pat/**')` + `apt install libax25-dev` + `git submodule update --init --recursive` step | | ✓ | Step 9 done |
| 12 | Write `tuxlink/docs/development.md` build-deps note (Go 1.24+, libax25-dev) | | ✓ | — |
| 13 | First end-to-end build verification: clean clone → `--recurse-submodules` → `cargo build --release` → sidecar exists at expected path → `cargo test` succeeds without Go (validates release-only gate) | | ✓ | Steps 8-11 done |
| 14 | Open PR-B against `tuxlink/feat/v0.0.1` (submodule + build.rs + tauri.conf + workflow + docs) | | ✓ | Step 13 passed |
| 15 | Operator reviews + merges PR-B; `tuxlink-84i` closes | ✓ | | — |

## 4. Commit shape

Two PRs land for this task, against different repos. Sequencing is enforced via the §3.6 pre-flight gates (steps 4 and 7), not just by convention.

**PR A (against `cameronzucker/tuxlink-pat/main`):** the fork's README.md. Small diff (~60-80 lines). Title: `[<moniker>] docs(readme): tuxlink-pat fork README (closes tuxlink-84i partial)`. The fork's `main` starts at upstream `la5nta/pat` HEAD at fork-creation time; this PR adds the README on top.

**PR B (against `cameronzucker/tuxlink/feat/v0.0.1`):** the submodule + build.rs + tauri.conf.json + workflow + docs/development.md changes. Larger diff. Title: `[<moniker>] build(pat): wire tuxlink-pat submodule + release-only Go-build integration (closes tuxlink-84i)`. Body cites this spec + the plan + the adrev transcripts (gitignored).

PR B depends on PR A merging first AND on Cameron's GH-ops steps (1-3 of §3.6) completing. The §3.6 step 4 + step 7 pre-flight gates make the dependency explicit and machine-checkable. Sequence:

1. Operator: §3.6 steps 1-3 (create fork, configure branch protection, enable Issues)
2. Agent: §3.6 step 4 (pre-flight verify) → §3.6 step 5 (open PR-A)
3. Operator: §3.6 step 6 (review + merge PR-A)
4. Agent: §3.6 step 7 (pre-flight verify) → §3.6 steps 8-13 (submodule + build.rs + tauri.conf + CI + docs + build verify)
5. Agent: §3.6 step 14 (open PR-B)
6. Operator: §3.6 step 15 (review + merge PR-B); `tuxlink-84i` closes via this merge

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

**Responsibility:** whoever's doing the patch (agent or Cameron). No scheduled jobs, no dedicated sync agent. No paper triggers — re-evaluation happens by operator perception when upstream drift becomes painful in practice (R1 P0 catch: the original spec had a "patch cadence drops to less than monthly" trigger with no observer mechanism; removing speculative trigger in favor of "react when reactor needed").

### 5.4 Conflict resolution: per-patch, no standing policy

**Decision:** When upstream merge produces conflicts, resolution happens inside the patch's brainstorm/plan/adrev cycle. No pre-documented "our patches always win" or "upstream always wins" rule.

**Reasoning:** At current patch scale (1-2 patches expected initially), per-incident resolution under the build-robust-features discipline is sufficient. A formal precedence policy is premature optimization until the patch count + conflict frequency justifies it.

**Re-evaluate when** the fork accumulates 4+ patches AND upstream-merge conflicts become routine (more than 1-in-3 syncs conflicting).

### 5.5 Branch model on `tuxlink-pat`: per-patch branches + PRs into `main`

**Decision:** Each fork patch lives on its own branch (`patch-<slug>` or `<bd-id>/<slug>`); PR'd against `main`; merged with merge-commit (no-ff). **Branches are RETAINED on merge** (NOT deleted) — this is the one intentional divergence from tuxlink's own `--delete-branch` convention (R1 P1 catch: the upstream-PR contribution policy per ADR 0011 §4 requires the source branch to survive so we can cherry-pick the patch onto upstream `master` and submit as a clean PR; deleted branches don't survive the cherry-pick).

**Reasoning:** Same disciplines that work for tuxlink work for tuxlink-pat (per-task branches, no force-push, no squash, merge-commit no-ff, agent moniker trailers, destructive-git hooks). The retention exception preserves cherry-pick portability for upstream contribution. Provides PR-level review surface for fork patches. Preserves integrated-state-in-main-with-merge-commits forensic history per ADR 0010 reasoning applied to the fork.

**Anti-pattern explicitly rejected** (alternative B from brainstorm): long-lived `tuxlink-patches` branch rebased onto new upstream. Rebasing patches is destructive-git-ban territory; force-pushing the patches branch hits the project's hook. Workable but works against the project's git discipline.

## 6. Risks and watched failure modes

Organized by failure-mode class. The classification was the R1 + R5 pattern-observation: the original §6 had only "fail at setup" risks and lacked the "rot quietly over time" class.

### 6.1 Build-time failures (fail at setup; loud)

- **Go toolchain missing or too old:** `build.rs` errors clearly per §3.4. Mitigated by `go-version-file: 'external/tuxlink-pat/go.mod'` in CI (delegates to upstream's own version contract) + the build.rs version check (errors with the exact required version).
- **Submodule incomplete:** strengthened pre-flight in build.rs (3 conditions per §3.4) catches partial states.
- **libax25 missing on Linux+CGO:** `make.bash` warns; build.rs propagates as cargo warning. Pat builds without it — AX.25 hardware modem support absent — which v0.0.1 doesn't exercise.
- **AppImage CI lacks Go:** `actions/setup-go@v6` standard action handles. Not a tuxlink-side concern.
- **Branch protection mis-configured on `tuxlink-pat`:** mitigated by automated post-condition readback per §3.5; not just attempted-push smoke. Re-verified on each subsequent per-patch workflow before merge.

### 6.2 Coordination / handoff failures (fail at PR-A/PR-B boundary; loud)

- **Operator opens PR-B before fork exists or before branch protection set:** §3.6 pre-flight gates (step 4 and step 7) catch via `gh api` readback; agent halts + reports to operator.
- **Submodule URL HTTPS hardcoded; SSH-preferring operator:** documented in §3.2 to use `~/.gitconfig` `insteadOf` for SSH preference; URL stays HTTPS in tuxlink so SSH-less environments (CI) work without configuration.
- **PR-A merged but submodule not yet added in tuxlink:** transient state; tuxlink continues to build without the submodule (the dep doesn't exist yet). PR-B is the diff that introduces the dep. No partial-state breaks.

### 6.3 Rot-quietly-over-time failures (NEW class per R1 + R5 pattern observations; the originally-missed risk surface)

- **Upstream-divergence accumulates without observer:** opportunistic-sync model + merge-not-rebase means patches become harder to extract for upstream PR contribution over time. **Acknowledged cost** of ADR 0011's chosen sync model. No automated tracking in v0.0.1. Re-evaluate observability mechanism when upstream-PR contribution attempts become friction (currently zero such attempts; can't predict cadence). Cherry-pick portability is preserved via branch retention on `tuxlink-pat` (§5.5) so the divergence can always be undone per-patch.
- **Go version pin goes stale silently:** mitigated by `go-version-file` delegation to upstream `go.mod`; upstream-Pat-bumps-Go ⇒ tuxlink CI automatically picks up the new floor on next submodule SHA bump. No tuxlink-side version literal to update.
- **CI workflow grows stale on action versions** (e.g., `actions/setup-go` advances past v6): low impact; surfaces as deprecation warning in CI logs. Periodic CI-action audit recommended; not in fork-setup scope.
- **Upstream Pat changes default branch from `master` to `main`:** §3.3 step 4 documents the verification pattern (`gh api repos/la5nta/pat --jq '.default_branch'`) so the per-patch agent always uses the current value. No hardcoded branch name in build infrastructure.
- **Upstream Pat changes its build entry point** (e.g., `make.bash` → `Makefile` or Bazel): low-frequency event; surfaces as a build break on the next sync; build.rs's `make.bash` invocation needs updating in coordination with the submodule SHA bump. Documented in design that build.rs is tightly coupled to Pat's current build entry point.
- **Tauri `bundle.externalBin` lookup convention changes** (Tauri 3.x future): surface at Tauri-upgrade time, not silently. Mitigated by pinning Tauri version in `Cargo.toml`.
- **Per-patch two-PR pattern overhead compounds:** small per-patch cost; deferred-follow-up `bump-pat-submodule.sh` helper script post-2-3 manual cycles to see what's worth automating. Not in fork-setup scope.

### 6.4 Cross-task transition risk (handoff to next task)

- **Pat config.json compatibility during cred-refactor transition:** when `tuxlink-mib` lands keyring-backed cred reads in Pat, existing Pat installs may still have config.json with the password. Backward compatibility owned by `tuxlink-mib`, not fork-setup. Flagged here as a known follow-on concern.

### 6.5 What's NOT a risk

(Items the adrev rounds raised that turned out to be non-risks under closer inspection; documented to prevent re-raising.)

- **Submodule byte-reproducibility:** R5 P1 cut. Reproducibility was acknowledged untestable in the original spec; not a v0.0.1 goal; removed as a testing claim.
- **AppImage hermeticity from host deps:** R5 P1 cut. The original spec said "without external dependencies on host" — that's an AppImage runtime promise we don't independently guarantee; the host's `libsecret` / `glibc` versions matter at runtime regardless. Removed the unjustified hermeticity claim.

## 7. References

- [ADR 0011 — Fork Pat as `tuxlink-pat`](../../adr/0011-fork-pat-for-tuxlink.md) — the strategic decision this spec operationalizes
- [ADR 0003 — Pat owns the mailbox](../../adr/0003-no-sqlite-pat-owns-mailbox.md) — amended by ADR 0011 (dependency target shift); the mailbox-ownership decision still holds
- [ADR 0008 — Worktrees mandatory under bd-issue ownership](../../adr/0008-worktrees-mandatory-under-bd-issue-ownership.md) — applies to per-patch branches on tuxlink-pat
- [ADR 0010 — No-squash merge](../../adr/0010-no-squash-merge.md) — applies to tuxlink-pat's PR model
- [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — relevant context for the future cred-refactor task; not directly applicable to fork-setup itself
- `bd show tuxlink-84i` — this task's bd record (claim + status; closes via PR B's merge)
- `bd show tuxlink-mib` — next task (cred-handling refactor), blocked by this one
- Adrev transcripts (gitignored; per CLAUDE.md): `dev/adversarial/2026-05-18-fork-setup-adrev-R{1..5}.md`

## 8. Adrev disposition summary

5-round adversarial review completed 2026-05-18 on commit `b80ba78` (the initial spec draft). 4 Claude subagents per-lens (R1 scale, R2 partial-input, R3 dep-contract-drift, R5 YAGNI) + 1 Codex cross-provider (R4). 49 findings: **5 P0, 15 P1, 21 P2, 8 P3**.

### Findings landed in this revision (all 5 P0 + 15 P1 + 5 high-value P2)

| Finding | Round | Severity | Action taken |
|---|---|---|---|
| Tauri target-triple binary naming (was: triple-less `$OUT_DIR/tuxlink-pat`) | R3 + R4 (cross-provider convergence) | P0 / P1 | §3.2 build.rs + tauri.conf.json rewritten to produce `pat-<TARGET-TRIPLE>` at stable `src-tauri/sidecars/` path per Tauri 2.x convention (verified via context7) |
| PR-A/PR-B sequencing has no pre-flight gates | R2 | P0 | §3.6 restructured into 15 ordered steps with explicit pre-flight `gh api` checks at steps 4 and 7 |
| build.rs submodule check too shallow (`.git` only) | R2 | P0 | §3.4 strengthened to 3-condition check (`.git` + `make.bash` + SHA-match-with-parent-index) |
| Upstream branch `master` hardcoded | R1 | P0 | §3.3 step 4 documents verification pattern via `gh api repos/la5nta/pat --jq '.default_branch'`; no hardcoded branch name anywhere agents need to update |
| Opportunistic sync has no observer for speculative "cadence drops" trigger | R1 | P0 | §5.3 trigger removed; replaced with "react when reactor needed" framing |
| `delete branch on merge` destroys cherry-pick artifacts | R1 | P1 | §3.2 + §5.5 + §3.6 step 2 — branches RETAINED on `tuxlink-pat` merges (intentional divergence from tuxlink's convention) |
| Go pin stale silently | R1 + R3 + R2 (3-way converged) | P1 ×3 | §3.2 CI row + §3.4 + §6.1 — adopted `go-version-file: 'external/tuxlink-pat/go.mod'`; no version literal in tuxlink CI |
| `go build ./` ≠ upstream Pat's actual build | R3 | P1 | §3.2 build.rs row — invokes `SKIP_TESTS=1 bash make.bash` (verified `make.bash` is Pat's documented build entry point) |
| No tuxlink-pat issue-tracker home | R1 | P1 | §3.2 added GitHub Issues row + intent for external-contributor audience |
| CI cargo cache + fresh submodule = stale Pat in AppImage | R2 | P1 | §3.2 CI row — cache key includes `hashFiles('external/tuxlink-pat/**')` + build.rs emits `cargo:rerun-if-changed=../external/tuxlink-pat` |
| Submodule URL hardcoded HTTPS | R2 | P1 | §3.2 + §6.2 — HTTPS retained for CI compatibility; SSH-preferring operators set `insteadOf` in `~/.gitconfig` |
| Branch protection no automated post-condition readback | R2 | P1 | §3.5 added `gh api .../branches/main/protection` readback as a documented post-setup check + re-verified per patch |
| Submodule path from crate root (`src-tauri/external/...` wrong) | R4 | P2 | §3.2 build.rs row — uses `Path::new(env!("CARGO_MANIFEST_DIR")).join("../external/tuxlink-pat")` |
| build.rs runs during `cargo test` ("tests unchanged" was wrong) | R4 | P2 | §3.2 build.rs row + §3.5 — gated to `PROFILE=release` only; debug + test paths skip Go build |
| Upstream remote add missing from workflow steps | R4 | P2 | §3.3 step 3 made explicit (`git remote add upstream` with `||` idempotent idiom) |
| §3.5 submodule reproducibility test target (acknowledged untestable) | R5 | P1 | CUT from §3.5; documented in §6.5 as "not a risk" with reasoning |
| §3.5 AppImage hermeticity framing | R5 | P1 | CUT from §3.5; documented in §6.5 |
| §6 Go-version-drift `build.rs` warn check (speculative) | R5 | P1 | CUT from §6; replaced by go-version-file delegation in §6.1 |
| §3.3 Step 9 restated ADR 0011 §4 (propagation-contract leak) | R5 | P3 (pattern) | §3.3 step 10 reduced to "pursue or skip per ADR 0011 §4 (the policy lives there)" |
| §6 missing "rot quietly over time" risk class | R1 + R5 (pattern) | structural | §6 restructured into 5 sub-sections; §6.3 added with 7 entries populated from R1/R5 findings |

### Findings deferred to follow-up tasks

| Finding | Disposition |
|---|---|
| Two-PR pattern has no automation | Deferred to a `bump-pat-submodule.sh` helper script after 2-3 manual cycles surface what's worth automating. Filed as a follow-up bd issue post-merge (NOT in fork-setup scope). |
| No upstream-divergence bound | Accepted as acknowledged cost in §6.3; cherry-pick portability preserved via branch retention. Re-evaluate observability when upstream-PR contribution friction becomes evident. |

### Findings accepted as P2/P3 not requiring spec changes

(21 P2 + 8 P3 = 29 findings.) These were either (a) covered by P0/P1 fixes that addressed the underlying concern, (b) operational details belonging in the implementation plan rather than the spec, or (c) cosmetic/wording suggestions not material to the design. Full per-finding dispositions in the adrev transcripts at `dev/adversarial/2026-05-18-fork-setup-adrev-R{1..5}.md` (gitignored; local-only per CLAUDE.md).

### Cross-provider convergence note

R3 (Claude) and R4 (Codex) independently identified the Tauri target-triple bundling problem — the highest-severity finding in the entire cycle. This convergence is the precise signal cross-provider adrev is designed to produce: when two providers' blind spots don't overlap on the same specific defect, the defect is real and not a same-provider hallucination. The `build-robust-features` Step 2 cross-provider requirement (per ADR 0011 §3 + the `feedback_no_carveout_on_cross_provider_adrev` memory) earned its keep on this round.
