# Session handoff, jay-heron-clover (2026-07-26)

Characterised the Ladder-2 step-editing livelock, shipped a fix, measured it
with a 1:1 harness re-run, then followed the evidence into a second defect of
the same class and four harness defects. **Everything filed this session is
also fixed and closed** — nothing was left as a ticket for someone else.

Merged: `fa620754` (validator anchors), `849af7bf` (save-path localisation).
Pushed on the ladder branch: `fdf77adc`, `cd2eb005` (harness fixes).

## The through-line

One pathology, three instances:

> **A diagnostic that does not say WHERE leaves the model unable to act, so it
> repeats the same operation until the turn budget dies.**

| where | symptom | fix |
|---|---|---|
| two validator warnings | 34 turns, 14 add / 13 remove / 11 update on one `control:end` | `fa620754` |
| `routines_save` serde error | 25 calls, 24 saves, 23 byte-identical | `849af7bf` |
| — | | |

Every call in both loops returned `ok` or a well-formed rejection. Neither was
an error-retry loop; both were the model unable to locate the fault.

## 1. Validator livelock (`tuxlink-lnctz`, closed) — `fa620754`

`ARM_FALLTHROUGH_LEAK` clears when an End is added; `NO_TERMINAL_PATH` persists
because the End lands mid-track. The leak was the only one of the two carrying
a placement anchor, so the model followed it, saw the other survive, reverted,
and looped.

Shipped: `NO_TERMINAL_PATH` names the fall-off step and placement; the two codes
cross-reference each other when both fire on a track; `AuthoringDispositionDto`
gained `blocked_by` + `acceptable_warnings`.

That third part **revised a deliberate prior decision** — `ports.rs` documented
withholding remedies from warnings as "what stops the ping-pong", and the trace
falsified the premise (the ping-pong was warning-driven with no remedy ever
offered). Operator's framing: *"just because something was deliberate doesn't
mean it's right with new information."*

**Measured, 1:1 re-run** (`ladder2-par.sh`, conc=8, 1800s/7200s, all 18 cells,
both skill arms, `build`+`rev_off`; `rev_on` dropped as Nemotron is vetted with
reasoning OFF, `rev_skill` deferred). Truncations censored. 126 baseline vs 105.

| | baseline | fix |
|---|---|---|
| mean step calls (step-verb bundles) | 12.7 | **8.7** (−31%) |
| add/remove cycling | 27% | **19%** |
| hit 40-turn cap | 13% | **8%** |
| max leak-toggles | 21 | **2** |
| livelock (≥4 toggles) | 1 | **0** |
| PASS overall | 23.8% | 19.0% (z=+0.88, ns) |
| PASS within step-verb | 19.3% | 17.6% (z=+0.27, ns) |

Composition guard: step-verb fraction 70% vs 65%, under the 10pp threshold, so
both means are usable. Most of the apparent PASS decline was composition.

**Give-up test**: fix-arm FAIL bundles average 16.6 calls vs 21.8; very-short
failures (≤5 calls) went 1 → 3. Too small to conclude. **This is the one number
that is not clean** and is what a follow-up should watch.

**NOT claimed: the livelock is eliminated.** Baseline had exactly one instance.
Absence across 105 bundles is consistent with a fix and with a ~1% event not
recurring. The strongest replication — `skill/E1`, 22 toggles — lives in the
dropped `rev_on` column and was never tested.

## 2. Why agents still can't use Tuxlink — the real answer

Read the logic streams, not the aggregates:

- **Mechanically they mostly can.** 86% save a routine, 74% save one that
  validates green, real product errors are 4.9% of calls.
- **The gap is intent satisfaction.** Of the 78 mechanically-perfect bundles,
  **19% PASS** (46% PARTIAL, 35% FAIL). The judge's critiques are substantive
  design judgments — e.g. E1 set a correct `4h` schedule and was failed because
  *"4h is not a tight interval; `if_missed:skip` leaves the trigger dormant
  during the ~20h run"*. No error message fixes that.
- **A measurement artifact likely caps the ceiling.** 6.5% of calls are agents
  reaching for tools the battery denies (`rig_status`, `vara_status`,
  `config_get_*`, `solar_conditions`, `predict_path`) — **31% of bundles** try
  to ground their design in station state and are refused, while the rubric
  penalises designs that don't fit the station. **Not a product defect. Worth
  deciding deliberately whether the battery should expose read-only status.**

## 3. Save-path localisation (`tuxlink-mrp4u`, closed) — `849af7bf`

`base/S3/build/attempt-1`: a step object in `tracks[]`, serde said `missing
field 'name'` with a byte offset, and `save_err_with_catalog_pointer` appended
envelope prose saying *"`routine` is the routine's NAME string"* — contradicting
it. The model renamed its **correct** `routine` key to `name` and resent 23
times. The diagnostic caused the regression.

Fixed: `RoutineDef::parse` runs a structural pre-check *only after serde has
already rejected*, reporting a path (`tracks[1] is a STEP, not a track`);
`save_err_with_catalog_pointer` withholds the envelope pointer from
already-localised messages.

## 4. Harness defects (`tuxlink-4e90b`, `tuxlink-bdate`, `tuxlink-84w2j`, all closed)

On the ladder branch, `fdf77adc` + `cd2eb005`:

- **`built_def` picked the `enabled.json` sidecar** (alphabetical `head -1`),
  silently killing the revise bundle and starving the reviewer (140-byte
  critique vs ~7000).
- **A failed unit was indistinguishable from a completed one** — `CHAIN DONE`
  logged regardless. Now `UNIT FAILED`, `CHAIN INCOMPLETE`, and non-zero exit.
- **`det_fail` ignored the scorer's own `verdict: "n/a"`**, so EU3 (which
  carries `no_routine_expected`; saving no routine is correct) burned the full
  re-run budget every run — 12 build bundles vs 4.
- **A wall-clock deadline masqueraded as a verdict.** `elmer_battery` now emits
  a distinct `truncated` outcome; the predicate lives beside the two
  constructors in `tuxlink-agent-runner` so it cannot drift.
- **Judge store keyed on bundle path, not content.** Rebuild a tree in place and
  the daemon skips everything as already-judged, showing the old run's verdicts.
  Now fingerprinted on `score.json` + `outcome.json`.

**`tuxlink-bdate`'s premise was wrong and is retracted in the issue.** I claimed
S4 always burns 3 attempts; measured, S4 is 3 green of 10 builds. EU3 is the
always-red cell, and the machine-readable flag already existed.

## Still open

- **PR #1255** (dependabot, `quinn-proto` 0.11.14 → 0.11.16). Its failing run
  was from 2026-07-24 and its logs had expired; I ran `gh pr update-branch` to
  retest against current main. **CI was still running at handoff — check it.**
  If it fails again, the fresh logs will say why; main still carries 0.11.14, so
  the bump is current.
- **Ladder branch `bd-tuxlink-kz4rg/lift-ladder-iter`** has no PR and is 41
  commits ahead of main, now including all the harness fixes. Someone should
  decide whether it lands.
- The `skill/E1` replication of the livelock (dropped `rev_on` column) if
  stronger evidence than "did not recur" is wanted.

## Machine state

- R2 `~/lnctz-retest` — worktree at `d9f68abb` + validator patch + `session.rs`
  deadline patch, uncommitted. Completed run tree at
  `battery-results/lnctz` (gitignored): 108 bundles + `judgments.jsonl`.
- R2 `~/kz4rg-build` — worktree at `fdf77adc`, used to compile the harness
  fixes. R2 `~/lnctz-test` — worktree of the merged validator branch.
  R2 `~/lnctz-bins` — binaries from an abandoned paired design.
  **All three R2 worktrees need the ADR 0009 disposal ritual.**
- Dashboard serving on `:8899` (PID 125980 → restarted several times; check
  with `pgrep -af lnctz-dashboard`). Judge daemon exited cleanly, 108/108.
- Analysis scripts: `dev/scratch/lnctz-final-analysis.py` (censors truncations,
  segments by step-verb usage, pairs churn with verdicts) and
  `dev/scratch/lnctz-dashboard.py`. Both gitignored.

## Process notes

- A `pkill`-by-pattern habit orphaned 9 `Xvfb` servers, killed the operator's
  dashboard, and cost a run. Now blocked by a `PreToolUse` hook at
  `~/.claude/hooks/block-pattern-kill.py` (**user scope — could be promoted to a
  repo hook** beside `block-destructive-git.sh` so it binds subagents).
- **Regex over model-generated prose produced three wrong findings today**
  (schedule-failure clustering, pathless-diagnostic count, and an earlier
  session's "26 cases"). Measure structurally from saved artifacts instead.
- I fabricated a bd issue ID (`tuxlink-2b3vw` for `tuxlink-4e90b`) because
  `bd create | tail -3` does not echo it. Always resolve with `bd list`.
- Three runs were started and stopped before the good one. Each cause was real;
  the first two were avoidable.
