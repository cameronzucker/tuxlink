# Session handoff — condor-basalt-hemlock (2026-07-25 into 2026-07-26)

Long session. Picked up an autonomous Ladder-2 run mid-flight with no handoff,
ran it to completion, found and repaired a self-inflicted contamination, analysed
the failure modes, spot-checked a frontier model, and chained two follow-on runs
that are STILL EXECUTING as this is written.

## READ FIRST: three processes are in flight right now

| what | where | state |
|---|---|---|
| follow-up pass (45 conditions, 90 bundles) | R2, 8 workers | running |
| rev_skill orchestrator | Pi, pid 2373514 | waiting for the above |
| Sonnet judge daemon | Pi, pid 1598465 | grading continuously |

The rev_skill orchestrator fires automatically when the follow-up completes and
the judge drains. **Do not start Spark work until both are done** — that box is
compute-bound and a second workload just splits throughput.

Watch: `http://r2-poe.twin-bramble.ts.net:8899/` (purple rings = re-run scope).

## Durable artifacts

- [`dev/battery/2026-07-25-ladder2-results-and-failure-analysis.md`](../battery/2026-07-25-ladder2-results-and-failure-analysis.md)
  — results, validity caveats, failure taxonomy. **Read its validity section
  before quoting any number.**
- [`dev/battery/ladder2-run-2026-07-25/`](../battery/ladder2-run-2026-07-25/)
  — 1.4 MB of committed raw results (judgments, outcomes, tool-call errors,
  saved defs) with a README. The source tree is gitignored and lives only on R2.
- [`dev/battery/2026-07-25-parallelization-analysis.md`](../battery/2026-07-25-parallelization-analysis.md)
  — throughput + the measured cost model.

## What was done

**Recovered the run.** Judge daemon had died in a Pi reboot; restarted with a
durable workdir and an `@reboot` crontab entry. Fixed three latent judge defects
(brittle JSON extraction, no failure evidence, no single-instance lock).

**Made it 2.36x faster, for free.** `--max-num-seqs` on the twin-bramble vLLM was
hand-set to 2 against a memory ceiling of 9.79. Raised to 8: prefill went 786 ->
1,852 tok/s. Also reconciled `profiles.json`, which had drifted from the
container that was actually serving (131072 vs 262144 max-model-len) and would
have silently halved context if the container were ever recreated.

**Then contaminated the dataset with that same change** and repaired it. The
harness deadline is WALL-CLOCK; running wide slowed each bundle 4.8x and
truncated 50 of 220 bundles, recorded as `needs_operator` and readable as
capability failure. Re-ran the 19 affected conditions with raised deadlines
(7200s total / 1800s per-turn, the latter via a CLI flag that already existed and
the driver never passed). Timeouts went 57/57 -> 1/26.

**Analysed the failures**, separating agent-side (branch polarity inversion,
orphan duplicates, schedule-to-manual, final_text dishonesty) from Tuxlink-side
(the untagged-enum serialization loop, `routines_get` erroring on a miss,
`DUPLICATE_STEP_ID`).

**Spot-checked qwen3.7-max** (frontier, API-only) on the 5 hardest cells, 3 runs
each, $2.62 total. Mean score 1.80 vs the 122b's 1.27. Notably the two cells
where it gave ZERO benefit (E1, EU2) are exactly the two independently diagnosed
as product-surface rather than capability problems.

**Fixed the reviewer's structural blindness.** `review.py` now passes
`final_text` and the routine inventory; `review-skill.md` gained matching
sections. Verified omitted-safe: with the new args absent the prompt is
byte-identical, so `rev_off`/`rev_on` stay comparable across runs.

## Corrections I made mid-session (all in the docs)

Recorded because each was a wrong confident claim caught by evidence:

1. "prefill is compute-saturated" — retracted; `nvidia-smi` utilization is not
   evidence, the cap was a hand-set flag.
2. "26 cases of qwen ignoring feedback" — the real number is 3; my regex matched
   "schedule" in unrelated judge text.
3. "there is no incremental way to set a trigger" — **wrong**.
   `routines_trigger_set` exists, is exposed, and is 30/30 reliable. I inferred
   its absence from `routines_meta_set`'s error message and never checked the
   tool registry. Nothing needs fixing on that path.
4. "36% revise harm" — only orphan/duplicate survives rate-normalisation (13.2x
   the build rate); branch polarity and unreachable steps are build-origin.

## Open work

- **tuxlink-lnctz (P2, NEW)** — step-editing churn: 38 of 41 calls cycling
  add/repoint/remove on one `control:end` step. This is the real mechanism behind
  the schedule failures. The issue lists what to characterise BEFORE proposing a
  fix; do not skip that, this exact question already produced one wrong diagnosis.
- **Untagged-enum error message** — highest-value codebase fix. With deadlines
  raised, E1 loops 35-36 identical `routines_save` retries against a diagnostic
  naming neither step nor field, burning whole 40-turn bundles.
- **tuxlink-l264r** — the per-cell cost ceiling meters ACCOUNT-WIDE credits;
  unusable for any parallel OpenRouter run.
- **tuxlink-0dj6d** — Ladder-3 reviewer-skill column; partly executing now.
- **`rev_on` swap decision** — deliberately deferred. The evidence it is
  dominated rests on the 45 single-observation conditions the follow-up is
  firming up. Decide with real rates.

## Machine state

- R2 `~/tuxlink-eig6e-build` is on branch `bd-tuxlink-kbh4t/consent-authoring-disposition`
  @ d9f68abb **plus an uncommitted local patch** to `src-tauri/src/elmer/session.rs`
  (env-var override for `max_response_duration`). `binary-git-sha.txt` records
  this; its previous contents were stale. **The original 220 bundles were built
  WITHOUT that patch.** The patch is deadline-only, so nothing about model
  behaviour differs.
- Spark vLLM container `vllm-q122` has `restart=no` and will NOT come back after
  a power loss. It survived one outage this session only because I restarted it.
  A 502 on the tailnet name means the model is still loading, not a broken proxy.
- OpenRouter: **$151 of $200 used.** ~$41 of today's spend is NOT the ladder —
  the ladder's entire OpenRouter footprint is $0.53 (Nemotron only; the builder
  is local). Something else shares that key. Worth identifying.
