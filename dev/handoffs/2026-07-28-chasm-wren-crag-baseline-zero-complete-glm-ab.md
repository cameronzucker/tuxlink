# Session end, chasm-wren-crag (2026-07-27/28): baseline zero complete + the GLM teaching arc

Extended autonomous overnight run after the operator's compaction. Everything
below is merged, pushed, and recorded; nothing is in a half-state.

## Completed this session (post-compaction arc)

1. **Baseline zero: COMPLETE and reported.** Launched 14:22Z on the parity
   binary (provenance gate passed, PROVENANCE.md in run dir), 216/216 scored
   after the EU2 sweep, 216/216 judged (fresh sonnet-5 store). Report merged to
   main: `dev/battery/2026-07-28-baseline-zero-report.md` (PR #1282). Headline:
   69 PASS / 74 PARTIAL / 73 FAIL; eight cells 0/12 everywhere (the qaq54
   probe list); EU3 honest-diagnosis 12/12; skill arm does not improve
   absolutes; loop family at 5.6 percent. bd v13t1 + j1nle closed.
2. **PR #1268 (parity harness) MERGED** (7ffba672); tuxlink-y9a6l closed.
3. **EU2 wedge mid-run**: ContactsStore state-before-manage panic wedged the
   cell past both timers. Fix merged same night (PR #1279, tuxlink-gcy3m
   closed): four missing managed states mirrored from lib.rs. Mid-run binary
   cutover recorded; swept chain ran clean. Lessons filed: tuxlink-l02v0
   (dead tool future defeats in-band timers, P2 open); kill the WHOLE process
   tree (wrapper + binary + Xvfb child) or the chain display wedges the rest.
4. **GLM-5.2 harness check + forced self-debrief + cross-model A/B**
   (operator-directed): GLM looped 28 identical find_stations calls to cap;
   debrief + source trace proved the tool under-teaches its refinement
   protocol (tuxlink-eefln). Fix built on branch + A/B'd: loop class
   ELIMINATED (GLM 0/3 to 1/3 with zero cap-outs and an 8-turn best-in-matrix
   run; qwen3.6-plus 3/3 both surfaces, no regression). Remaining GLM failures
   = routines_save stringified-def rejection, filed tuxlink-ryyhi (P1) —
   second model family with the qwen stringify quirk. Total OpenRouter spend
   ~$1.50; account at ~$156 of $200.
5. Cleanup: merged remote branches deleted; merged worktrees (y9a6l, gcy3m,
   v13t1) disposed per ADR 0009 (all clean at inventory).

## Open decisions — OPERATOR

- **PR #1281 (eefln wire-teaching) is a DRAFT awaiting your merge call.** The
  A/B data (on the bd issue and in the baseline0 report, finding 3) says: loop
  class eliminated cross-model, no qwen regression, and base/P1's own 1-in-12
  loop is the same signature. The frontier probe (qaq54) should run on
  whichever surface you pick — probing the unfixed surface confounds
  capability-vs-surface attribution.
- **PR #1267 (ADR 0029) still awaits your voice pass.**
- C1-class corpus predicate review (baseline0 report finding 4) before the
  0/12 list feeds fine-tune targets.

## Open work queue

- tuxlink-ryyhi (P1): routines_save stringify absorber (de_stringy_or_native
  precedent). Last blocker to plausible GLM 3/3.
- tuxlink-qaq54: frontier probe on the 0/12 set; harness + Novita pin proven;
  measured costs $0.22/cap-out, $0.03-0.08/success per GLM cell.
- tuxlink-l02v0 (P2): out-of-band response deadline.
- tuxlink-3cal1, voacapl staging on R2, Spark boot-autostart (needs sudo:
  container restart policy + dashboard unit), spark_hwmon MOK (physical
  console), old merged-dead worktree set (zq44u, kz4rg, hwo1b, 6i8jz, jaer0,
  x43aa) still awaiting ADR 0009 disposal.
- Second Spark arrives Tue 2026-07-28 (qealk prep list).

## Machine state

- R2 `~/6i8jz-run`: branch bd-tuxlink-gcy3m/contacts-manage @ d38a8746
  (content = merged main; remote branch deleted, local checkout fine).
  baseline0 run dir complete + archives; dashboard (PID ~338621) and the run
  are DONE, dashboard can be killed or repointed next run. `~/eefln-ab`:
  B-arm clone @ b10e0342 for the A/B; keep until the eefln merge decision.
  `~/glm-probe/`: all probe + A/B artifacts incl. per-run costs (serial.log)
  and the GLM debrief JSON.
- Pi: judge daemon still running against baseline0 (idle now; kill PIDs
  389230/389232 when convenient); `dev/scratch/baseline0-judge/` holds the
  judgments store + aggregate.{json,md}; aggregator script in the session
  scratchpad (fingerprint join matches the daemon: per-file sha256 + NUL).
- Worktrees live: eefln (open draft PR), agent-chasm-wren-crag-handoff (this
  doc), plus the old set listed above.

Agent: chasm-wren-crag
