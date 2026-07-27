# Session state, chasm-wren-crag (2026-07-27) — pre-compaction capture; baseline zero staged

Written before an operator-initiated context compaction. The session continues
after compaction with an extended autonomous run: launch and babysit baseline
zero while the operator is at work. This doc is the authoritative state.

## The day's arc (compressed)

1. surface1 ran twice and was invalidated twice: first for binary provenance
   (pre-#1261 build), then — operator ruling — for **harness non-parity**: the
   battery's tool allowlist + goal-rewriting DENY_TEACHING presented an
   environment production Elmer never does ("junk science... the asterisk gets
   lost"). Both trees archived on R2 under `~/6i8jz-run/battery-results/`
   (`surface1-invalid-oldharness/`, `surface1-invalid-nonparity-harness/`),
   each with a README carrying the do-not-use terms. lnctz is equally invalid
   as a quantitative baseline. NO allowlist-era data feeds fine-tuning or
   quoted pass rates. Full ruling + records: bd tuxlink-y9a6l (P1), memory
   `battery-methodology-settled`.
2. **Parity harness shipped** (PR #1268, branch `bd-tuxlink-y9a6l/parity-harness`):
   allowlist + DENY_TEACHING removed (-145 lines), `ObservedInvoker` =
   logging/metering only, real consent-gate `Denied`s pass through terminal
   (production-true), manifests record `harness: parity-v1` + named residues
   (voacapl not staged → propagation Unavailable; scheduler not spawned).
3. **3-cell pilot PASSED** on the parity binary (R2, 13:37–13:44Z,
   `~/6i8jz-run/battery-results/pilot-parity/`): P1 saved green in 11
   turns/237s with NO editing loop; EU3 correctly diagnosed (armed:false via
   server_info) and saved no routine — the j1nle mis-teach confirmed by
   intervention; C1 took the direct-action reading (grib_send_request ×2,
   honest report, no routine) — corpus predicates for ambiguous cells need
   review at ANALYSIS time (corpus stays frozen for the run itself).
4. Spark moved indoors (new LAN IP 192.168.20.75, direct tailscale, idle 35°C).
   Reboot exposed: vllm-q122 container does NOT autostart; the spark-dashboard
   venv lived in /tmp and evaporated — rebuilt persistent at
   `~/serving/spark-dashboard/.venv`, relaunch:
   `cd ~/serving/spark-dashboard && setsid nohup .venv/bin/python app.py </dev/null > dashboard.nohup 2>&1 &`.
   Boot-autostart for both is queued work.

## BASELINE ZERO — staged, launch after compaction

Everything is in place; the launch is:

```bash
secret-tool lookup service elmer-openrouter account teacher | ssh r2-poe '
  read -r K; export ORKEY="$K" OPENROUTER_API_KEY="$K"
  cd ~/6i8jz-run
  LADDER2_CONC=8 LADDER2_TURN_TIMEOUT_SECS=1800 TUXLINK_MAX_RUN_SECS=7200 LADDER2_REVCONDS_SKILL=off \
    nohup bash battery-results/baseline0/ladder2-par.sh >> battery-results/baseline0/nohup.log 2>&1 &
  disown; echo "driver PID $!"'
```

- Tree: `r2-poe:~/6i8jz-run` on branch `parity-build` = PR #1268 head. Binary
  provenance-verified (old deny text absent, `parity-v1` marker present,
  TUXLINK_MAX_RUN_SECS read). **After #1268 merges, prefer `git fetch && git
  checkout -B main origin/main` in ~/6i8jz-run + rebuild + re-verify strings
  BEFORE launch** (content-identical, but provenance discipline is the rule
  that caught the first invalid run). If merge is still blocked on the flaky
  arm64 test, launching from the branch binary is acceptable WITH a provenance
  note in the run manifest — the content is what CI amd64 passed.
- Run dir staged: `battery-results/baseline0/` (driver with paths rewritten,
  fingerprint dashboard, review.py, review-skill.md, catalog.json — catalog
  unchanged, routines surface identical to main).
- 216-bundle target (18 cells × base/skill × build+rev_off × 3 attempts,
  rev_on retired, 3x unconditional).
- Post-launch re-arm (all recipes proven today):
  1. Dashboard: kill old PID if running, relaunch with
     `LADDER2_ROOT=$HOME/6i8jz-run/battery-results/baseline0 LADDER2_CORPUS=$HOME/6i8jz-run/tests/battery/corpus.json nohup python3 battery-results/baseline0/dashboard.py </dev/null > battery-results/baseline0/dashboard.log 2>&1 &`
     (ETA + fingerprint-join features live in this copy).
  2. Judge daemon (Pi): workdir `dev/scratch/surface1-judge/` — COPY to
     `dev/scratch/baseline0-judge/`, patch R2DIR to `.../baseline0`, START A
     FRESH `ladder2-judgments.jsonl` (do NOT reuse the invalid runs' store),
     relaunch `nohup python3 judge_daemon.py > judge_daemon.nohup 2>&1 & disown`.
  3. Monitor: wide-coverage non-completed watcher, scoped to the last
     `LADDER2-PAR START` segment of run.log (append-only-log lesson, twice
     today). Truncated (per-turn stall) = sweep re-run list; turn-cap
     cancelled = DATA, never cleared.
- End-of-run: sweep relaunch (idempotent) for stall-truncated slots only →
  verify 216 scored → analysis. **Committed analysis report to `dev/battery/`
  per ADR 0029** — this is baseline zero, the first valid measurement; no
  before/after vs lnctz (invalid), report absolute rates + duration
  distributions + editing-loop incidence under parity + C1-class
  direct-action-vs-authoring split + EU3 honest-diagnosis rate.

## Open PRs / branches / worktrees

- **PR #1268** (parity harness): amd64 verify PASS; arm64 verify was a flaky
  unrelated test (`concurrent_config_set_grid_and_position_set_source_serialize`,
  config.json NotFound race), rerun in flight, watcher armed → merge on green
  (standing grant, intent stated). Worktree `worktrees/bd-tuxlink-y9a6l-parity-harness`.
- **PR #1267** (ADR 0029 engineering record + dev/README): DRAFT, awaiting
  operator voice pass. Not blocking anything.
- Merged-dead worktrees awaiting ADR 0009 disposal: zq44u, plus older set
  (kz4rg, hwo1b, 6i8jz, jaer0, x43aa). sccrg (PR #1267) and y9a6l stay until
  their PRs resolve.

## Open bd (today's)

- tuxlink-y9a6l (P1, in_progress): parity harness — pilot passed; closes when
  #1268 merges and baseline zero launches clean.
- tuxlink-3cal1 (P1): provider first-token/idle-stream timeouts — between
  runs; the 2-stall class re-observed today (both rev_off, big prefills).
- tuxlink-j1nle: EU3 mis-teach finding — CONFIRMED by pilot; keep as record.
- tuxlink-atetd: editing-loop family — allowlist-era observations now
  qualitative leads; pilot P1 showed none under parity; re-assess at baseline
  zero analysis.
- tuxlink-qaq54 (frontier probe, provider pins recorded), tuxlink-qealk
  (dual-Spark A/B), tuxlink-qhyre (context pruning), tuxlink-sccrg (ADR 0029),
  tuxlink-2grt7 (NUT — two supporting incidents today), tuxlink-tii83.
- Second Spark arrives Tue 2026-07-28 (prep list in qealk).

## Standing rulings in force (do not re-litigate)

- Non-production tool environment = invalid, flat out. Parity harness gates
  EVERY battery run including the parallel session's troubleshooting battery.
- Corpus frozen for baseline zero (incl. E1's FT-701, which is a deliberate
  overclaim trap — verified authored-with-predicate).
- Defaults operator accepted: live-internet tools stay live; consent-gate
  cell-termination is recorded behavior; voacapl residue documented not
  blocking (staging = queued follow-up).

Agent: chasm-wren-crag
