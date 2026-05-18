# Spec: tuxlink-arv lease-dir parity fix

**Date:** 2026-05-18
**Agent:** willow-raven-arroyo
**bd issue:** `tuxlink-arv` (P1 bug; see `bd show tuxlink-arv`)
**Branch:** `bd-tuxlink-arv/lease-dir-fix` (worktree off `feat/v0.0.1`)
**bd deps:** `tuxlink-arv` depends on `tuxlink-6ro` (PR #40 HOOK-1+LEASE-1 must merge first; arv rebases on feat/v0.0.1 after).
**Revision:** v3 (post 4-round Claude adrev + Codex round 5 + Cameron decisions 2026-05-18).

## 1. Problem statement

The script `.claude/scripts/get_tuxlink_sessions.py` (an informational tool that lists live tuxlink sessions) and the hook `.claude/hooks/block-main-checkout-race.sh` (the enforcement mechanism that denies main-checkout writes when another session holds the lease) **read and write the same session-leases JSON files** — but they resolve the lease directory to different filesystem locations:

- **Hook** (`block-main-checkout-race.sh:41`): resolves via `git rev-parse --git-common-dir` → `<git-common-dir>/session-leases/`. This is shared across the main checkout and all worktrees by construction (the `--git-common-dir` answer resolves to the same directory from any worktree after normalization — git returns `.git`, `../.git`, or an absolute path depending on cwd; the hook + script both normalize).
- **Script** (`get_tuxlink_sessions.py:55`): resolves via `Path(__file__).resolve().parent / ".." / ".." / ".claude" / "session-leases"` → `<repo>/.claude/session-leases/`. This is per-checkout and not where the hook writes.

The two locations are different. Concretely, on this Pi at 2026-05-18 08:51 UTC:

```
.git/session-leases/      → 5 active leases + 16k denied-attempts.jsonl  ← hook writes here
.claude/session-leases/   → 1 stale lease from 2026-05-17 + tiny denied-attempts.jsonl   ← script reads here
```

The script silently under-reports live sessions because it reads from the (mostly empty) wrong directory. The 2026-05-18 main-checkout-race incident chain (see `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md` — currently on PR #38's branch; will be on `feat/v0.0.1` once #37 + #38 merge) is grounded in this disagreement: the reporting agent ran the script, saw "No live tuxlink sessions in this repo," concluded the hook had a false positive, and tried to take the lease / delete stale lease files / propose hook enhancements — the textbook "argue with the hook" anti-pattern HOOK-1 now codifies.

Hypothesis #1 from the bd issue body (timestamp parse failure in `parse_iso_utc`) was rejected during diagnosis on 2026-05-18 08:30 UTC: the function correctly parses the bash hook's `date -u +%Y-%m-%dT%H:%M:%S.%6NZ` output (verified empirically). Hypothesis #2 (directory path mismatch) was confirmed by grep + comparison of the two candidate directories. Hypothesis #3 (file-extension/glob mismatch) was not investigated because #2 sufficed to explain the symptom, but a defensive test for the .json/.jsonl distinction is added in §3.2 to lock it down.

## 2. Scope

**In scope:**
1. Fix the path-resolution bug in `get_tuxlink_sessions.py`.
2. Add regression tests: ONE headline test (lease-dir parity with hook) + ONE defensive test (glob excludes `.jsonl`). Per Cameron's YAGNI cut 2026-05-18: dropped the 4 `parse_iso_utc` tests since hypothesis #1 was REJECTED — defensive locking of rejected hypotheses is noise, not coverage.
3. Add pitfalls entry **PARITY-1: Script/Hook Path-Resolution Parity** to Section 2 of `docs/pitfalls/implementation-pitfalls.md`. **NOTE:** Section 2 is presently the `EXAMPLE-DOMAIN-2` placeholder on `feat/v0.0.1`; HOOK-1 + LEASE-1 (which stand up Section 2 as "Safety-Stack Coordination") are on PR #40's branch. Per `bd dep add tuxlink-arv tuxlink-6ro` (2026-05-18), this PR rebases on a merged `feat/v0.0.1` after PR #40 lands; the PARITY-1 entry then lands alongside HOOK-1 + LEASE-1 with section title **extended to "Safety-Stack Coordination and Cross-Component Parity"** to reflect PARITY-1's code-structure (rather than purely agent-behavior) focus.
4. Set up a minimal Python test-infrastructure directory at `.claude/scripts/tests/` so future safety-stack-script tests have a home.
5. **Refresh auto-memory entry `feedback_stale_lease_means_worktree.md`** at `~/.claude/projects/-home-administrator-Code-tuxlink/memory/`. The current entry says "the script lies — when block-main-checkout-race.sh denies, the canonical response is `new_tuxlink_worktree.py`; NEVER try to take the lease, ask the operator to clean lease files, or propose hook enhancements." After this fix the script no longer lies, but the worktree response STILL applies. The memory needs an in-place update to reflect: (a) script now accurate, (b) worktree recipe remains authoritative regardless of whether script + hook agree.

**Out of scope (intentional YAGNI cuts):**
- Extracting `resolve_lease_dir` into a shared helper module (e.g., `_safety_stack_paths.py`). Only one consumer today; the function is ~10 lines; over-engineering for hypothetical future scripts. The pitfalls entry + regression test are sufficient prevention.
- Hypothesis #3 (full file-extension/glob mismatch investigation). Hypothesis #2 fully explains the symptom; the §3.2 defensive glob test is enough.
- Adding a `SessionEnd` hook to clean up orphan leases. LEASE-1 explicitly rejects this style of fix (it doesn't address the crashed-session case anyway). Out of scope here.
- Adjusting the 30-min lease TTL. Separate concern; would need its own bd issue.
- The 4 `parse_iso_utc` defensive tests originally specified (hypothesis #1 was rejected during diagnosis; defensive locking of a rejected hypothesis is noise, per YAGNI cut).
- Port-quality audit of other `.claude/scripts/` for the same shape. Audit done during diagnosis (grep `session-leases`/`lease_dir`/`git-common-dir` across `.claude/scripts/*.py` and `.claude/hooks/*.sh`) found no other instances — the safety stack today is `get_tuxlink_sessions.py` (this fix), `new_tuxlink_worktree.py` (no lease-dir handling), `get_agent_moniker.py` (no lease-dir handling), `block-main-checkout-race.sh` (canonical source). PARITY-1 is the future-proofing.

## 3. The fix

### 3.1 Script changes (`get_tuxlink_sessions.py`)

- Add `import subprocess` to the imports block.
- Add a new function below `resolve_repo()`:

  ```python
  def resolve_lease_dir(repo: Path) -> Path:
      """Resolve <git-common-dir>/session-leases/ for the given repo path.

      Matches the hook's resolution (.claude/hooks/block-main-checkout-race.sh
      line 41) so script and hook agree on what is and isn't a live lease. The
      git-common-dir is the same across the main checkout and all worktrees,
      making the lease set genuinely repo-scoped rather than per-checkout.

      If `git rev-parse` is unavailable (not a git repo, git missing), writes a
      one-line warning to stderr (so the operator notices when their environment
      isn't supported, per adrev round 1 finding on silent fallback) and returns
      a path that is unlikely to exist — main() will then print the "no
      sessions" path instead of crashing.
      """
      try:
          common_dir = subprocess.check_output(
              ["git", "rev-parse", "--git-common-dir"],
              cwd=repo, text=True, stderr=subprocess.DEVNULL,
          ).strip()
      except (subprocess.CalledProcessError, OSError) as e:
          sys.stderr.write(
              f"warning: git rev-parse --git-common-dir failed ({type(e).__name__}); "
              f"falling back to {repo}/.git/session-leases (may not exist)\n"
          )
          return repo / ".git" / "session-leases"
      if not common_dir:
          # Empty stdout — git ran but returned nothing. Treat as fallback per
          # Codex round 5 finding 4; the hook also bails on empty git-common-dir.
          sys.stderr.write(
              f"warning: git rev-parse --git-common-dir returned empty; "
              f"falling back to {repo}/.git/session-leases (may not exist)\n"
          )
          return repo / ".git" / "session-leases"
      cd = Path(common_dir)
      if not cd.is_absolute():
          cd = (repo / cd).resolve()
      return cd / "session-leases"
  ```

- Replace `lease_dir = repo / ".claude" / "session-leases"` (current line 55) with `lease_dir = resolve_lease_dir(repo)`.
- Update the module docstring (current line 4):
  - Before: `Reads .claude/session-leases/*.json, filters to live sessions (lastSeenUtc within --ttl-minutes), and prints a table.`
  - After: `Reads <git-common-dir>/session-leases/*.json, filters to live sessions (lastSeenUtc within --ttl-minutes), and prints a table. The lease directory location MUST match \`.claude/hooks/block-main-checkout-race.sh\`'s resolution (it uses \`git rev-parse --git-common-dir\`) — disagreement causes the script to silently under-report live sessions and gives agents false grounds to argue with a hook deny. See bd issue tuxlink-arv for the 2026-05-18 incident.`

### 3.2 Tests (`.claude/scripts/tests/test_get_tuxlink_sessions.py`, new)

Three `unittest.TestCase` classes, each killing a specific mutant class:

- `LeaseDirResolution.test_lease_dir_matches_git_common_dir` — the headline unit test. Calls `git rev-parse --git-common-dir` independently via subprocess, joins `/session-leases`, asserts equality with `resolve_lease_dir(REPO_ROOT)`. Resolves both paths to handle symlink / relative variation. **Acknowledged gaps** (per Codex round 5 findings 1 + 2): this test does NOT catch (a) a mutant that adds the helper correctly but forgets to update `main()`'s call site, or (b) a mutant that returns hardcoded `repo/.git/session-leases/` — coincidentally correct from the main checkout. `MainEndToEnd` below kills both classes.
- `GlobIgnoresJsonl.test_glob_pattern_excludes_jsonl` — defensive coverage for hypothesis #3 (per adrev round 3). Creates a temp dir with one `<session>.json` and one `denied-attempts.jsonl`, points the script at the temp dir (via monkeypatching `resolve_lease_dir` or an env override), runs the script's main listing pass, asserts the `.jsonl` is not parsed-or-silently-skipped.
- `MainEndToEnd.test_main_lists_lease_from_git_common_dir_via_main_and_worktree` (per Codex round 5 finding 1 + 2 — the mutation killer) — sets up a temp git repo via `git init`, adds a linked worktree via `git worktree add`, writes a known lease (with a unique moniker like `mutation-killer-abc123`) into `<git-common-dir>/session-leases/<uuid>.json`, runs `get_tuxlink_sessions.main()` (or invokes the script as a subprocess with `CLAUDE_PROJECT_DIR` set) from BOTH the main checkout root AND the linked worktree root, captures stdout from each, asserts the unique moniker substring appears in both. This kills the "forgot to update main()" mutant (the helper change alone doesn't change main()'s output) AND the "hardcoded `.git/session-leases/`" mutant (from the linked worktree, that path is a file-not-a-dir, so the broken impl would crash or skip).

Run via `python3 -m unittest discover .claude/scripts/tests/` from any worktree's repo root.

Test files contain a module-level docstring explaining the bug they regression-test.

### 3.3 Pitfalls entry (`docs/pitfalls/implementation-pitfalls.md`)

**Predicate:** PR #40 must merge first (it stands up Section 2 with HOOK-1 + LEASE-1 — without that, this work has no Section 2 to add to). After PR #40 merges + this branch rebases on `feat/v0.0.1`:

- Extend Section 2 title from "Safety-Stack Coordination" → **"Safety-Stack Coordination and Cross-Component Parity"** (per adrev round 4 finding 3: PARITY-1 is about code structure / cross-component invariants, not pure agent behavior).
- Add new entry **PARITY-1: Script/Hook Path-Resolution Parity** to Section 2, positioned after LEASE-1.
- Use the same Flaw / Why / Fix / Lesson shape as HOOK-1 and LEASE-1.
- Lead with the 2026-05-18 incident as grounding.
- **Lesson must explicitly reinforce** (per adrev round 4 finding 10): even when the script and hook agree, the worktree recipe (HOOK-1) remains the authoritative answer to a hook deny. Fixing the script restores informational accuracy; it does NOT reopen the "argue with the hook by consulting the script" anti-pattern in a new shape ("the script confirms there IS another session — let me take the lease since I now believe the hook's view"). HOOK-1 still wins; the script is informational only.
- Update the Section 2 review checklist with one new bullet for PARITY-1.
- Update the Appendix B unified summary table with the PARITY-1 row.
- Add an Appendix A changelog entry for 2026-05-18.

### 3.4 Manual smoke verification (post-fix, pre-commit)

- **First**, inspect `<git-common-dir>/session-leases/` for live leases (per Codex round 5 finding 5 — the smoke check is unreliable if all leases have expired between spec time and implementation time). If empty / all stale, skip the script-output assertion and rely on `MainEndToEnd` from §3.2 (which is deterministic).
- If live leases exist: run `python3 .claude/scripts/get_tuxlink_sessions.py`. Confirm it shows the live leases the hook currently sees. Before the fix this returned "No live tuxlink sessions in this repo"; after the fix it should show a non-empty table.
- This complements (does NOT replace) `MainEndToEnd` from §3.2 as a mutation killer. `MainEndToEnd` is the deterministic defense; this smoke check is the "did we ship the right thing into the real environment" sanity check.

### 3.5 Auto-memory refresh

- File: `~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md`.
- Update in place (do NOT remove). Keep the title + main rule (worktree response on hook deny) intact. Add a 2026-05-18 dated note that:
  - The script's "the script lies" framing is no longer accurate as of `tuxlink-arv` fix (link to PR).
  - The worktree recipe remains the canonical hook-deny response **regardless of whether the script and hook agree** — fixing the script restores informational accuracy, it does not authorize agents to use the script's output as license to take the lease, delete lease files, or propose hook enhancements.
- This reinforcement parallels the PARITY-1 Lesson in §3.3.

## 4. Testing strategy

- **Unit tests** (described in §3.2) — 2 tests total: directory parity (headline regression) + glob-excludes-jsonl (defensive). Fast, deterministic, no env coupling beyond `git rev-parse --git-common-dir`.
- **Smoke verification** (§3.4) — single manual run; defends against the symmetric-logic mutant class.
- **No integration test that creates a fake lease and runs the full script end-to-end.** The unit test on `resolve_lease_dir` + glob test + smoke verification cover the bug surface. End-to-end would add value but is out of scope.

## 5. Commit shape

One focused commit (post-rebase):

- Subject: `fix(safety-stack): get_tuxlink_sessions.py lease-dir parity with hook (tuxlink-arv P1)`
- Body summarizes the bug, the fix, the test, references the incident docs + PARITY-1 entry, notes the PR #40 dependency was satisfied via rebase.
- Trailer: `Agent: willow-raven-arroyo` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`
- Files: `.claude/scripts/get_tuxlink_sessions.py`, `.claude/scripts/tests/test_get_tuxlink_sessions.py` (new), `docs/pitfalls/implementation-pitfalls.md`.

(Note: the spec doc itself + the worktree-creation work is already committed in earlier commits on this branch and travels with the PR — they're separate work artifacts.)

**Codex round on implementation diff** (per adrev round 4 finding 11): before the commit lands, run `codex review --commit <SHA>` (or `--uncommitted`) against the staged work + integrate any P1/P2 findings. Implementation diff is small (~20 lines + 2 tests + 1 pitfalls entry), so the Codex round is fast.

Mixed-scope (fix + tests + pitfalls + auto-memory refresh) bundled per the project's pattern for tightly-coupled remediation work (same shape as PR #37's bundling of SCOPE-1 codification with the incident write-up, accepted by the reviewer in PR #38). The auto-memory refresh in §3.5 is NOT in the PR diff (it's outside the repo) — it's a parallel manual update, noted in the commit body for traceability.

## 6. PR

- Title: `[willow-raven-arroyo] fix(safety-stack): get_tuxlink_sessions.py lease-dir parity (tuxlink-arv P1)`
- Base: `feat/v0.0.1` (after rebase on merged PR #40)
- Body: summary + scope-discipline notes (what's IN scope vs. intentionally OUT per §2) + test plan checklist + Codex-round findings (or "no findings").
- Closes `tuxlink-arv` on merge.

## 7. Risks and watched failure modes

- **`git rev-parse --git-common-dir` returns a relative path on some platforms.** Handled by the `cd.is_absolute()` branch + resolve against the repo root. This matches the hook's handling at `block-main-checkout-race.sh:43-46`. **Empirically verified on this Pi:** `git rev-parse --git-common-dir` from the main checkout returns relative `.git` (NOT absolute), so the branch IS load-bearing here (per adrev round 2 verification + Codex probe).
- **The script is run outside a git repo** (e.g., tarball extraction). The `except (subprocess.CalledProcessError, FileNotFoundError)` branch writes a stderr warning + returns `repo / ".git" / "session-leases"`, which won't exist; main() then prints the "no sessions" message. (Warning addresses adrev round 1 finding on silent fallback.)
- **`git` binary is missing from PATH.** Same as above — caught by `FileNotFoundError`.
- **Test discovery picks up other test files.** None currently exist in `.claude/scripts/tests/`; the discovery pattern is `test_*.py`. Future files following the same naming will run together — which is the goal.
- **Incident-doc citation in §1 references files on unmerged PR branches.** `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md` lives on PR #37's branch; the reviewer-response doc on PR #38's. After those PRs merge to `feat/v0.0.1`, the citations resolve. If this arv PR rebases BEFORE PRs #37/#38 land, the citations point to files-not-yet-on-base — verify at impl time per adrev round 4 finding 14.
- **Symmetric-logic test risk** (per adrev round 1 + Codex round 5 finding 1+2): the `LeaseDirResolution` unit test alone is vulnerable to mutants that (a) add the helper but skip the `main()` call-site update, OR (b) return hardcoded `.git/session-leases/` (coincidentally correct from main checkout). `MainEndToEnd` in §3.2 is the kill-shot for both mutants. The §3.4 smoke verification is a third layer.
- **subprocess injection risk** (per Codex round 5 finding 6): `subprocess.check_output([...], cwd=repo, shell=False)` is NOT vulnerable to shell injection from unusual repo path characters because `shell=False` (Python default) treats argv as a list, not a shell string. Residual trust is PATH and git-environment trust, same class as the bash hook. Future hardening (sanitizing `GIT_DIR` / `GIT_WORK_TREE` env vars) would have to be done in lockstep with the hook.
- **PR #40 dependency**: per `bd dep add tuxlink-arv tuxlink-6ro` (2026-05-18), arv waits for PR #40 to land. If PR #40 changes substantively during review, arv may need to re-validate the Section 2 title proposal + the PARITY-1 placement.
- **Pitfalls Appendix A changelog out of date after future entries.** Standard pitfalls-doc hygiene; the maintenance framework in the doc covers this.

## 8. Out-of-scope follow-ups to consider after merge

- Audit other `.claude/scripts/` for non-lease state stores that the hook(s) also touch (none currently exist; revisit if the safety stack grows).
- Add a `python3 -m unittest discover .claude/scripts/tests/` invocation to a pre-commit or CI step so the regression test runs automatically. Out of scope here (CI is Task 19 / `tuxlink-n65`); file a follow-up bd issue if Cameron wants this before Task 19 lands.

## 9. Revision history

- **v1 (2026-05-18 ~08:51 UTC):** initial design, approved by Cameron, committed at `d001253`.
- **v2 (2026-05-18 ~09:10 UTC, not committed; superseded by v3):** revised per 4-round Claude adrev + Cameron's decisions on PR #40 dep + test scope cut.
- **v3 (2026-05-18 ~09:20 UTC):** incorporate Codex round 5 findings:
  - HIGH #1 + HIGH #2 → added `MainEndToEnd` test as a 3rd test (mutation killer for "forgot to update main()" and "hardcoded `.git/session-leases/`" mutants).
  - MED #4 → broaden subprocess fallback to catch `OSError` (parent class) + handle empty stdout explicitly.
  - LOW #5 → smoke verification §3.4 made conditional on live leases existing; `MainEndToEnd` is the deterministic defense.
  - LOW #6 → added security note to §7 (subprocess shell=False; residual trust same as bash hook).
  - LOW #7 → §1 prose softened from "identical" to "resolves to the same directory after normalization."
  - MED #3 → already addressed in v2 (bd dep + rebase plan); Codex's parallel observation confirms the approach.
  - Net test count: **3** (LeaseDirResolution unit + GlobIgnoresJsonl defensive + MainEndToEnd mutation killer). The 4 ParseIsoUtc tests stay cut.
