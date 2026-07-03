# Handoff — Elmer discriminating-eval: greenfield scenarios + Codex fixes + Stage-2 MoE blocker

**Agent:** vetch-sage-oak · **Date:** 2026-07-02 · **bd:** tuxlink-6zkb6
**Branch:** `bd-tuxlink-6zkb6/discriminating-eval` · **HEAD:** `234bcbd2` · **all pushed** · **108 tests green**

## One-sentence frame

Continued the discriminating-eval gate: expanded the eval tool surface, converted the operator's
7 greenfield tasks into gate scenarios, hardened the grader against a Codex adversarial round
(7 false-pass holes fixed), and ran the pod micro-LoRA smoke — which **correctly failed** and
surfaced a real **Stage-2 training-path blocker** (MoE experts don't get LoRA on the current
unsloth/gpt-oss env).

## What shipped this session (all committed + pushed, in order)

1. `ad0670e1` — APRS agent tool infra (read tools + simulator fixtures + `aprs_positions_cited`).
2. `70f99ad1` — 3 APRS scenarios from operator chat examples (RESCUE tracking, N7CPZ-7 locate, injection-refuse).
3. `ae2ac270` — **surface expansion** (operator-approved "add all"): `aprs_send_message` (char-limited egress),
   APRS weather + `aprs_gust_alert_cited`, WARC gateway density, sim clock + current-slot, `config_set_transport`,
   per-station connect outcomes + `achieved_radio_connect`. Tool surface 53→55.
4. `ba62ad1f` — **7 greenfield scenarios** from `1-greenfield-operator-tasks.md`, all `operator_authored:true`
   (calibration won't select them by teacher-success). Bank now **16 candidates**. Discrimination-guarded.
5. `1987f596` — **Codex adrev fixes** (7 grading-integrity holes): predicates now clause-bound (callsign must sit
   with its OWN grid/freq/gust value in one clause — kills swapped/anywhere-in-JSON games); INVALID egress fails
   a required send; "sent" honesty needs a message-delivering egress (not bare rig_tune); evidence merges across
   same-tool calls. Regression tests in `tests/test_codex_exploits.py`.
6. `234bcbd2` — **smoke result + resolved pins + MoE blocker** (see below).

## Bank state — 16 candidates, all lint + discrimination guarded

`gate/candidates/`: 6 original (cypress) + 3 APRS + 7 greenfield. Red-team materials in `gate/redteam/`:
`1-greenfield-operator-tasks.md` (operator filled), `2-candidates-redteam.md` (16 scenarios rendered, awaiting
VERDICT/NOTES), `2-candidates-ORIGINAL.md` (byte-identical diff anchor), `3-coverage-gaps.md`.

## PENDING — operator gate (BLOCKS the freeze)

Operator must add `VERDICT:` (keep/revise/cut) + `NOTES:` per scenario in `2-candidates-redteam.md`. When saved,
next session runs `diff 2-candidates-ORIGINAL.md 2-candidates-redteam.md`, folds edits in, scales toward ~40,
then FREEZES. Only after freeze does the calibration run (over the frozen suite).
Known-weaker scenarios flagged for red-team: `aprs-locate-followup` + `aprs-injection-refuse` grade tool-behavior,
not answer prose (Codex finding 6, documented limitation).

## Stage-2 training pipeline — DE-RISKED (smoke ALL STAGES PASS end-to-end)

The micro-LoRA smoke initially failed (attention-only), which caught a real trap; it is now **RESOLVED and the
full pipeline PASSES**: load → attn+expert LoRA (router excluded) → 10 trl steps → GGUF → ollama → valid tool call.
- **Fix:** unsloth 2026.6.9's `FastLanguageModel.get_peft_model` trains attention only (its MoE targeting
  hard-codes singular `mlp.experts.gate_up_proj`; the model has 1536 per-expert `experts.{gate_up_projs,down_projs}.<i>`
  Linear4bit modules). Bypass it: `peft.get_peft_model` directly with per-expert `target_modules` +
  `enable_input_require_grads()` (see `smoke/micro_lora_smoke.py::_attach_lora`). trainable=3264, EXPERT=3072, router=0.
- **GGUF gotcha:** unsloth `save_pretrained_gguf` FAILS on bnb-4bit ("Quant method not supported: bitsandbytes").
  Workaround proven in `smoke/gguf_export.py`: reload base bf16 → merge adapter → convert that → ollama. OR skip
  GGUF and serve merged bf16 via vLLM for gold-gen.
- Full recipe + resolved pins + diagnostics (`diag_experts.py` cracked it) in `dev/elmer-distill/smoke/`
  (SMOKE-RESULT-2026-07-02.md, requirements-train.txt). Stage-2 gold-gen + real LoRA is unblocked pending the frozen gate.

## Pod state (RunPod A100-SXM4-80GB)

- Reachable this session at `root@154.54.102.37 -p 13944 -i ~/.ssh/id_ed25519` (**port changes each stop/start**).
- **SSH saga / persistence fix:** the Pi now has its own keypair `~/.ssh/id_ed25519` (pubkey `elmer-eval-pi`,
  `SHA256:WCYV…`). RunPod was injecting the OLD `elmer-eval-runpod` key (its private half isn't on this Pi).
  This pod was unblocked by appending `elmer-eval-pi` to the running pod's `~/.ssh/authorized_keys` (one-time).
  **Operator TODO for real persistence:** prune the stale `elmer-eval-runpod` from RunPod account SSH keys so
  future pods inject `elmer-eval-pi` at create time.
- **Env built on the pod:** ollama 0.31.1 (+zstd), torch 2.10.0+cu128, unsloth 2026.6.9 / peft 0.19.1 / trl 0.24.0.
  gpt-oss-20b is in the HF cache. NOTE: transformers currently left at 4.55.4 (breaks trl — re-pin to 4.57.6 next run).
- **⚠️ The pod is IDLE and still BILLING.** Nothing runs on it now. **Recommend STOPPING it** — the calibration
  needs the frozen suite (post red-team), so there's no reason to keep the A100 hot. Agents can't stop the RunPod
  allocation from ssh; operator stops it in the dashboard.

## Environment gotchas (local tests — next session)

Scratch venv + Harmony vocab (re-create; session-specific):
```
python3 -m venv $SCRATCH/edvenv && $SCRATCH/edvenv/bin/pip install pytest openai_harmony requests
mkdir -p $SCRATCH/tiktoken_base && curl -sSL -o $SCRATCH/tiktoken_base/o200k_base.tiktoken \
  https://openaipublic.blob.core.windows.net/encodings/o200k_base.tiktoken
cd dev/elmer-distill && PYTHONPATH=src $SCRATCH/edvenv/bin/python -m pytest -q   # conftest autowires the vocab
```
Work happens in the worktree `worktrees/bd-tuxlink-6zkb6-discriminating-eval/` (bd-owned).

## Key durable facts

- Codex adrev findings were REAL false-passes; all fixed + regression-guarded. Grading integrity is the eval's whole point.
- The eval tool surface (55 tools) LEADS router.rs for APRS + config_set_transport — `build_tools.py` regen would drop them (warned in reference/README).
- The smoke's FAIL is the de-risker WORKING — attention-only LoRA would have silently underfit Stage-2.
