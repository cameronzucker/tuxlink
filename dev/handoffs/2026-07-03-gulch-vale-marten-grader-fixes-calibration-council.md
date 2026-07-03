# Handoff — Elmer discriminating-eval: grader hardening, calibration, multi-model council

**Agent:** gulch-vale-marten · **Date:** 2026-07-03 · **bd:** tuxlink-6zkb6
**Branch:** `bd-tuxlink-6zkb6/discriminating-eval` · **HEAD:** `f63e835c` · **all pushed** · **141 tests green**
**Worktree:** `worktrees/bd-tuxlink-6zkb6-discriminating-eval/` (bd-owned)

## One-paragraph frame

Took the discriminating-eval from "authored but unrun" to a real empirical picture, then built the
gold-generation engine. Ran the gate against base-20B and teacher-120B on a RunPod A100, discovered
the calibration was being wrecked by **grader escaping bugs** (fixed 6 defects across 2 Codex-adrev
rounds), established the honest signal (**base 1/16, raw teacher 3/16, scaffolded teacher 9/16** — the
scenarios are largely achievable, not too-hard), fixed 3 real scenario/tool defects, and built a
**multi-model best-of-N council** (gpt-oss-120b + qwen2.5-72b + llama3.3-70b + nemotron-70b + gemma3-27b)
as the gold-yield engine. A council smoke is running now; the full council is the next step.

## The empirical story (this is the payoff)

| run | pass rate | meaning |
|---|---|---|
| base-20B (student) | 1/16 | huge headroom, no saturation (unlike prior vvdii gate @ ~90%) |
| teacher-120B raw (zero-shot) | 3/16 | looked like "gate too hard for even the teacher" |
| teacher-120B **scaffolded** (checklist) | **9/16** | **the real answer: scenarios are achievable; raw zero-shot just didn't reach for tools** |

The jump from 3→9 with a checklist is the key finding: the "13 too_hard" were mostly the model not
*calling* required tools (predict_path, cms_connect, docs_search…), which the gold-gen scaffold fixes
anyway. Codex adrev sorted the 13 into: ~5 scaffold-fixable, 5 scenario-defects (fixed, see below),
3 genuinely-hard tail (blended-fix-and-send, taint-refuse-inbox-send, thirtym-reach — real
taint-discipline/grounded-report reasoning even scaffolded teacher fumbles).

## Grader defects fixed (6, all TDD, none loosen discrimination)

Round 1 (base-20B adrev, commit `cef503c0`): schedule_has_blocks accepts hour-ranges; staged
must_contain supports any-of (nested list) for synonyms; parse_freqs_khz parses MHz-with-unit;
_claims_sent ignores future/conditional tense.

Round 2 (teacher-120B run, commit `33f79cc8`) — **the big one**: evidence predicates were matching
against `json.dumps(args)` (default `ensure_ascii=True`), so the teacher's unicode dashes became
`‑` and row-separating newlines became literal `\n` (gluing digits to 'n', killing `\b`
boundaries). A valid 12-block plan scored 0 blocks. **Root fix: `_arg_text()` flattens staged arg
VALUES to plain text — no JSON escaping at all.** Plus: schedule range-end may be 24; final-honesty
gated on outbound-in-scope (staging OR egress tool present) so "the string sent to the CMS" in
helpdesk advice no longer false-fails.

The lesson: **run → verify → adrev → fix; never conclude from a raw number.** Most of the apparent
"gate too hard" was the grader under-crediting a capable model's formatting.

## Scenario/tool defects fixed (commit `3f66253f`)

- **send_form had `parameters: {}`** — a model could call it but never put "ICS-213"/"200"/"water"
  IN it, so 3 scenarios could never pass. Added form/to/body params.
- **cmdpost-rotation-80m**: "reachable right now" but references_real_gateway `minimum:5` while only 3
  of 5 80m gateways are recent → lowered to 3.
- **aredn-postoffice**: prompt gave no host → model correctly asked instead of acting → added a
  concrete AREDN host.

## The council / best-of-N engine (commits `f63e835c`, `4e339c93`)

**Why a council:** we own a DETERMINISTIC judge, so ensembling is "generate diversely, filter
mechanically" — not opinion-aggregation. Adding models only INCREASES union gold coverage (a scenario
is coverable if ANY member passes it in N tries). This also moots the teacher-family question: the
"teacher" is a committee, gold is whoever-passes, all normalized to Harmony downstream.

- `council.best_of_n` / `run_council` + `run_council.py`: per-(model,scenario) N scaffolded attempts
  (attempt 0 greedy, rest sampled with varied seed), judge-filtered; union gold persisted as training
  data.
- `OllamaClient` owns temperature+seed; runners let the client govern sampling.
- Members: gpt-oss:120b, qwen2.5:72b, llama3.3:70b, nemotron:70b, gemma3:27b — ALL pulled on the pod.
  qwen tool-calling verified; **nemotron + gemma3 tool-calling NOT yet verified** (GPU was busy — a
  non-tool-calling member just contributes 0 to the union, harmless, but verify).

## IN-FLIGHT right now

- **Council smoke** (bg task on pod): `gpt-oss:120b,qwen2.5:72b,llama3.3:70b --n 2` over the gate,
  out=`eval-runs/council-smoke`. Validating the multi-model pipeline. Slow (3×70B). Check
  `eval-runs/council-smoke/report.json` when done.

## NEXT SESSION — do these in order

1. **Confirm the smoke** produced `eval-runs/council-smoke/report.json` with sane per-model pass
   counts + union coverage. Verify nemotron + gemma3 emit tool_calls (curl probe; if gemma3 doesn't,
   it's a known ollama limitation — drop it, no harm).
2. **Launch the full council**: `python3 run_council.py --models gpt-oss:120b,qwen2.5:72b,llama3.3:70b,nemotron:70b,gemma3:27b --n 5`
   (consider `--n 3` if throughput is slow — the smoke tells you). Run it detached (nohup or a
   backgrounded foreground-ssh; the pod-side `nohup ... &` had a quirk — the reliable pattern this
   session was a backgrounded Bash TOOL task holding the ssh foreground). This produces the UNION GOLD
   COVERAGE — the headline: how many of 16 are coverable by the committee, and which stay uncovered.
3. **Uncovered scenarios** = the true hard tail. Decide: keep as stretch (qualitative-probe territory),
   add few-shot exemplars to the scaffold, or revise. ADREV before revising (operator directive).
4. **2-provider adrev council** (pending): add a Claude subagent as a second reviewer alongside Codex
   for design/failure-eval (judgment tasks, where opinion-diversity helps — unlike gold-gen where the
   deterministic judge rules).
5. Then the original arc resumes: gold-gen on the GENERATOR pool (not the gate — contamination guard
   enforces this) → Phase-A LoRA on gpt-oss-20b (MoE expert-LoRA recipe already de-risked, see
   `smoke/SMOKE-RESULT-2026-07-02.md`) → acceptance eval on the frozen gate + the before/after
   qualitative probe (`probe.py` + `gate/probe/rubric.json`, 7 operator scenarios).

## Operator decisions pending

- **The 3 genuinely-hard scenarios** — keep as aspirational stretch, or make achievable? (Recommend:
  see if the council covers them first; they're exactly where model diversity should help.)
- **references_real_gateway callsign+grid vs callsign+freq** evidence contract (deferred from base
  adrev — a deliberate strictness call, not a bug).
- **Gate freeze**: the gate is NOT frozen yet. Freeze after the council tells us coverage + the hard
  tail is resolved. The operator red-team file (`gate/redteam/2-candidates-redteam.md`) VERDICTs are
  still blank — but the run data now supersedes speculative red-teaming (operator's call:
  "don't tune without a run").

## Pod state (RunPod A100-SXM4-80GB)

- Reachable: `ssh root@154.54.102.37 -p 18484 -i ~/.ssh/id_ed25519` (**PORT CHANGES each stop/start**).
- Models loaded (disk 85% used, ~41G free): gpt-oss:20b, gpt-oss:120b, qwen2.5:72b, llama3.3:70b,
  nemotron:70b, gemma3:27b. ollama 0.31.1 + zstd. Harness synced to `/root/elmer-distill`.
- **SSH RE-AUTH TAX (operator, please fix permanently):** RunPod injects the stale `elmer-eval-runpod`
  key at pod create; the Pi's real key is `elmer-eval-pi`. Every fresh/virgin pod needs the pubkey
  hand-appended via the RunPod **Web Terminal**:
  `mkdir -p ~/.ssh && echo 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIP68+oJRiPZ2go7vstcq0q1e2N68yrS9yle54ZfKBo0W elmer-eval-pi' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys`
  **Permanent fix:** RunPod → Settings → SSH Public Keys → delete `elmer-eval-runpod`, add
  `elmer-eval-pi`. **Bigger win:** attach a Network Volume so keys + the ~180GB of models + env
  survive stop/start (kills both the re-auth AND the re-download tax).
- **Pod is BILLING while idle between runs.** The full council will use it; after that, if nothing's
  queued, stop it (operator, from the dashboard — agents can't).

## Local test recipe (next session, on the Pi)

Scratch venv + Harmony vocab (session-specific; recreate):
```
python3 -m venv $SCRATCH/edvenv && $SCRATCH/edvenv/bin/pip install pytest openai_harmony requests
mkdir -p $SCRATCH/tiktoken_base && curl -sSL -o $SCRATCH/tiktoken_base/o200k_base.tiktoken \
  https://openaipublic.blob.core.windows.net/encodings/o200k_base.tiktoken
cd dev/elmer-distill && PYTHONPATH=src $SCRATCH/edvenv/bin/python -m pytest -q   # 141 green
```
Note: `run_eval.py` / `run_scaffold.py` / `run_council.py` are stdlib-only (no harmony needed) — only
`dataset.py` (gold-gen → Harmony) needs `openai_harmony`.

## Key durable facts

- eval-runs/ is gitignored (per-run local artifacts). Raw adrev transcripts in `dev/adversarial/`
  (gitignored): `2026-07-02-base-20b-failure-adrev-codex.md`, `2026-07-02-teacher-toohard-adrev-codex.md`.
- The grader's whole value is being ungameable; every fix this session was a FALSE-FAIL fix, never a
  loosening. The gate + codex-exploit + corpus discrimination guards stayed green throughout (141).
- Contamination guard (`_arg_text`, `holdout_ids_from_dir`, capture's held_out) keeps the gate/probe
  out of training gold — gold-gen MUST draw from the generator pool, not the candidates.
