# Spec: tuxlink-arv lease-dir parity fix

**Date:** 2026-05-18
**Agent:** willow-raven-arroyo
**bd issue:** `tuxlink-arv` (P1 bug; see `bd show tuxlink-arv`)
**Branch:** `bd-tuxlink-arv/lease-dir-fix` (worktree off `feat/v0.0.1`)

## 1. Problem statement

The script `.claude/scripts/get_tuxlink_sessions.py` (an informational tool that lists live tuxlink sessions) and the hook `.claude/hooks/block-main-checkout-race.sh` (the enforcement mechanism that denies main-checkout writes when another session holds the lease) **read and write the same session-leases JSON files** — but they resolve the lease directory to different filesystem locations:

- **Hook** (`block-main-checkout-race.sh:41`): resolves via `git rev-parse --git-common-dir` → `<git-common-dir>/session-leases/`. This is shared across the main checkout and all worktrees by construction (the `--git-common-dir` answer is identical from any worktree).
- **Script** (`get_tuxlink_sessions.py:55`): resolves via `Path(__file__).resolve().parent / ".." / ".." / ".claude" / "session-leases"` → `<repo>/.claude/session-leases/`. This is per-checkout and not where the hook writes.

The two locations are different. Concretely, on this Pi at 2026-05-18 08:51 UTC:

```
.git/session-leases/      → 5 active leases + 16k denied-attempts.jsonl  ← hook writes here
.claude/session-leases/   → 1 stale lease from 2026-05-17 + tiny denied-attempts.jsonl   ← script reads here
```

The script silently under-reports live sessions because it reads from the (mostly empty) wrong directory. The `2026-05-18` main-checkout-race incident chain (see `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md`) is grounded in this disagreement: the reporting agent ran the script, saw "No live tuxlink sessions in this repo," concluded the hook had a false positive, and tried to take the lease / delete stale lease files / propose hook enhancements — the textbook "argue with the hook" anti-pattern HOOK-1 now codifies.

Hypothesis #1 from the bd issue body (timestamp parse failure in `parse_iso_utc`) was rejected during diagnosis on 2026-05-18 08:30 UTC: the function correctly parses the bash hook's `date -u +%Y-%m-%dT%H:%M:%S.%6NZ` output. Hypothesis #2 (directory path mismatch) was confirmed by grep + comparison of the two candidate directories. Hypothesis #3 (file-extension/glob mismatch) was not investigated because #2 sufficed to explain the symptom.

## 2. Scope

**In scope:**
1. Fix the path-resolution bug in `get_tuxlink_sessions.py`.
2. Add regression tests covering both the headline bug (directory parity) and the rejected-but-locked-down hypothesis #1 (timestamp parse).
3. Add a pitfalls entry (Section 2 — Safety-Stack Coordination, alongside HOOK-1 and LEASE-1) codifying the script/hook path-resolution parity requirement.
4. Set up a minimal Python test-infrastructure directory at `.claude/scripts/tests/` so future safety-stack-script tests have a home.

**Out of scope (intentional YAGNI cuts):**
- Extracting `resolve_lease_dir` into a shared helper module (e.g., `_safety_stack_paths.py`). Only one consumer today; the function is ~10 lines; over-engineering for hypothetical future scripts. The pitfalls entry plus the regression test are sufficient prevention.
- Hypothesis #3 (file-extension/glob mismatch) investigation. Hypothesis #2 fully explains the symptom; chasing #3 without evidence is speculative.
- Adding a `SessionEnd` hook to clean up orphan leases. LEASE-1 explicitly rejects this style of fix (it doesn't address the crashed-session case anyway). Out of scope here.
- Adjusting the 30-min lease TTL. Separate concern; would need its own bd issue.
- Backport / port-quality audit of other LFST scripts that may have similar shape. Cameron's call whether to pursue; the audit done during diagnosis (grep `session-leases`/`lease_dir`/`git-common-dir` across `.claude/scripts/*.py` and `.claude/hooks/*.sh`) found no other instances in tuxlink, so the audit is already complete for this repo.

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

      If `git rev-parse` is unavailable (not a git repo, git missing), returns
      a path that is unlikely to exist — main() will then print the "no
      sessions" path instead of crashing.
      """
      try:
          common_dir = subprocess.check_output(
              ["git", "rev-parse", "--git-common-dir"],
              cwd=repo, text=True, stderr=subprocess.DEVNULL,
          ).strip()
      except (subprocess.CalledProcessError, FileNotFoundError):
          return repo / ".git" / "session-leases"
      cd = Path(common_dir)
      if not cd.is_absolute():
          cd = (repo / cd).resolve()
      return cd / "session-leases"
  ```

- Replace `lease_dir = repo / ".claude" / "session-leases"` (current line 55) with `lease_dir = resolve_lease_dir(repo)`.
- Update the module docstring (current line 4):
  - Before: `Reads .claude/session-leases/*.json, filters to live sessions (lastSeenUtc within --ttl-minutes), and prints a table.`
  - After: explicit reference to `<git-common-dir>/session-leases/`, statement of the parity invariant, citation of bd issue tuxlink-arv.

### 3.2 Test infrastructure (`.claude/scripts/tests/`)

- New directory `.claude/scripts/tests/`.
- New file `test_get_tuxlink_sessions.py` with two `unittest.TestCase` classes:
  - `LeaseDirResolution.test_lease_dir_matches_git_common_dir` — the headline regression test. Calls `git rev-parse --git-common-dir` independently via subprocess, joins `/session-leases`, asserts equality with `resolve_lease_dir(REPO_ROOT)`. Resolves both paths to handle symlink / relative variation.
  - `ParseIsoUtc.test_parses_six_digit_microseconds_with_trailing_z` + `_zero_microseconds` + `_empty_string_returns_none` + `_invalid_string_returns_none` — defensive coverage for hypothesis #1 (rejected during diagnosis but locked down).
- Run via `python3 -m unittest discover .claude/scripts/tests/` from any worktree's repo root.
- Test files contain a module-level docstring explaining the bug they regression-test.

### 3.3 Pitfalls entry (`docs/pitfalls/implementation-pitfalls.md`)

- Add new entry **PARITY-1: Script/Hook Path-Resolution Parity** to Section 2 (Safety-Stack Coordination), positioned after LEASE-1.
- Use the same Flaw / Why / Fix / Lesson shape as HOOK-1 and LEASE-1.
- Lead with the 2026-05-18 incident as grounding.
- Update the Section 2 review checklist with one new bullet for PARITY-1.
- Update the Appendix B unified summary table with the PARITY-1 row.
- Add an Appendix A changelog entry for 2026-05-18.

### 3.4 Manual smoke verification (post-fix, pre-commit)

- Run `python3 .claude/scripts/get_tuxlink_sessions.py` after the script changes.
- Confirm it shows the live leases that the hook currently sees. Before the fix this returned "No live tuxlink sessions in this repo"; after the fix it should show a non-empty table.

## 4. Testing strategy

- **Unit tests** (described in §3.2) — fast, deterministic, no env coupling beyond `git rev-parse --git-common-dir`.
- **Smoke verification** (described in §3.4) — single manual run; not automated this PR because the script's output format is text-table that's awkward to assert against. A future test could capture stdout and parse the table, but that's over-engineering for v0.0.1.
- **No integration test that creates a fake lease and runs the script end-to-end.** The unit test on `resolve_lease_dir` plus the smoke verification cover the bug surface. End-to-end would add value but is out of scope per §2.

## 5. Commit shape

One focused commit:

- Subject: `fix(safety-stack): get_tuxlink_sessions.py lease-dir parity with hook (tuxlink-arv P1)`
- Body summarizes the bug, the fix, the test, and references the incident docs.
- Trailer: `Agent: willow-raven-arroyo` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`
- Files: `.claude/scripts/get_tuxlink_sessions.py`, `.claude/scripts/tests/test_get_tuxlink_sessions.py` (new), `docs/pitfalls/implementation-pitfalls.md`.

Mixed-scope (fix + tests + pitfalls) bundled per the project's pattern for tightly-coupled remediation work (the same shape as PR #37's bundling of SCOPE-1 codification with the incident write-up, accepted by the reviewer in PR #38).

## 6. PR

- Title: `[willow-raven-arroyo] fix(safety-stack): get_tuxlink_sessions.py lease-dir parity (tuxlink-arv P1)`
- Base: `feat/v0.0.1`
- Body: summary + scope-discipline notes (what's IN scope vs. intentionally OUT per §2) + test plan checklist.
- Closes `tuxlink-arv` on merge.

## 7. Risks and watched failure modes

- **`git rev-parse --git-common-dir` returns a relative path on some platforms.** Handled by the `cd.is_absolute()` branch + resolve against the repo root. This matches the hook's handling at `block-main-checkout-race.sh:43-46`.
- **The script is run outside a git repo** (e.g., tarball extraction). The `except (subprocess.CalledProcessError, FileNotFoundError)` branch returns `repo / ".git" / "session-leases"`, which won't exist; main() then prints the "no sessions" message instead of crashing.
- **`git` binary is missing from PATH.** Same as above — caught by `FileNotFoundError`.
- **Test discovery picks up other test files.** None currently exist in `.claude/scripts/tests/`; the discovery pattern is `test_*.py`. Future files following the same naming will run together — which is the goal.
- **Pitfalls Appendix A changelog out of date after future entries.** Standard pitfalls-doc hygiene; the maintenance framework in the doc covers this.

## 8. Out-of-scope follow-ups to consider after merge

- Audit other `.claude/scripts/` for non-lease state stores that the hook(s) also touch (none currently exist; revisit if the safety stack grows).
- Add a `python3 -m unittest discover .claude/scripts/tests/` invocation to a pre-commit or CI step so the regression test runs automatically. Out of scope here (CI is Task 19 / `tuxlink-n65`); file a follow-up bd issue if Cameron wants this before Task 19 lands.
