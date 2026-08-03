# The Elmer battery — v1 era archive and transition to tuxlink-bench

**Benchmarking did not stop here — it moved.** As of 2026-08-03 the agent
battery graduated into its own project, **`tuxlink-bench`** (sibling repo,
`~/Code/tuxlink-bench` on the dev Pi), which carries the expanded ladder,
the virtual-radio harness and fixtures, and everything else inappropriate to
ship inside a client application repo. If you are a future agent asking "why
did benchmarking activity stop in tuxlink" — this is why. Look there.

## What this directory is

The complete v1-era record: per-arm reports (reverse-chronological by
filename), the cross-model comparison artifact
(`comparison/battery-comparison.html`, self-contained), its generator and
fingerprint-joined per-run data (`comparison/data/*_joined.json`). The v1
ladder was 18 cells × 10 attempts, deliberately small to build the harness;
its own instrument defects, found by cross-model sweep and a frontier
ceiling-check, are documented in the reports and drove the v2 design.

## Why this program matters (the operator's framing, recorded)

Two properties very few evaluation setups have:

1. **A diverse, difficult, private benchmark** — RAG, tool calls,
   troubleshooting, routine authoring — that no model has ever seen. There
   is no "agent was optimized for the standard benchmarks" confound.
2. **Scores that mean something by themselves.** "82.3 on SWE-Bench" needs
   extrapolation from personal experience with a benched agent. Battery
   scores self-report by capability bucket with two pass rates (strict /
   lenient), so a number carries its own interpretation.

## v1-era results (generation 40fd9b7e unless noted)

inkling1 35.6% strict > control2 31.1 > q235 21.8 > dsv4 20.7 (38.5
idiom-adjusted, see the ae1pt addendum) > gptoss1 17.4 > mistral2 13.9;
laguna2 and opus1 (frontier ceiling-check) were in flight at transition
time and their reports land here before porting. Older-generation runs
(control1, laguna1, mistral1) are in the comparison with caveats.

## Port manifest for tuxlink-bench

In-repo (lift this directory wholesale): reports, `comparison/` complete.

Off-repo assets to port or archive:
- **Run bundles + ladder driver**: `r2-poe:~/6i8jz-run/battery-results/*`
  (per-arm dirs incl. quarantined `*-invalid*` forensics),
  `~/6i8jz-run/ladder3-cluster.sh`, `launch_*.sh`, `*_join.py`.
- **Judge daemons + per-arm corpora**: Pi `dev/scratch/<arm>-judge/`
  (fingerprint-keyed sonnet-5 judging; `ladder2-judgments.jsonl` stores).
- **Subscription shim** (frontier arms without OpenRouter): Pi
  `dev/scratch/claude-shim/shim.py` — design rationale in its docstring.
- **Wall-power data**: `wall-power.jsonl` on both Sparks (60s cadence,
  kWh counters) + the Shelly plug runbook (auto-memory).
- **Serving records**: `dev/runbooks/inkling-dual-spark/` (in-repo),
  Spark recipes `inference:~/spark-vllm-docker/recipes/*.yaml`.
- **bd issues that belong to the bench**: lmrd4 (egress-guard scoping),
  tx870 (outcome classifier), pvlyh / opyuy (cell design), ae1pt (consent
  gate + simulated-operator responder), m7j2a (mined troubleshooting
  corpus), 69qtv (expansion epic), bohfp (skill arm), 4stvz (Solar-Open2).

Hard-won process rules worth carrying: pin HF revisions and record the
revision hash in every arm ledger (the laguna Aug-1 silent-repush incident,
`bd show tuxlink-jwdsa`); one engine session per valid arm; burn-in gates
before full runs; persistent whole-chain monitors with stall+disk alarms;
50G disk floor before any serve.
