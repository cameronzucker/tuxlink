# Pre-compaction state 3, chasm-wren-crag (2026-07-31 ~11:35Z): control2 running, Inkling queue armed, TP2 is the critical path

Same session continues after a third manual compaction. This doc is the
authoritative state. Prior handoff:
2026-07-30-chasm-wren-crag-control1-shipped-laguna-armed-precompact2.md.

## Shipped since the last handoff (all merged)

- PR #1299 laguna1-t07 report (dev/battery/2026-07-30-laguna1-t07-report.md):
  20.5 percent vs qwen 31; S2/P2 wins; 21 percent turn-cap exhaustion class;
  node-1 vllm silent-wedge incident (17W vs lying util) + mid-run recovery.
  Laguna since ruled a DEAD END by the operator; weights evicted from both
  Sparks (69G each).
- PR #1300 tuxlink-grc1j fix (CLOSED): tool-dispatch deadline (child-token
  cooperative abort, conversation completion for resumability, battery
  dispatch_dropped log row) + point_at stub managed. Codex adrev 3/3 accepted.
- PR #1302 call-id shape fix: Mistral serving validates tool-call ids as
  EXACTLY 9 alnum chars; synthetic call_N ids 400'd every replay. Found by
  the mistral false start (~60 attempts dead in 4 min, archived + purged).
- PR #1303 mistral1-t015 report: raw 10.4 / non-censored 17.5 percent; 42
  percent context-censored by the 32k host ceiling; grc1j wedge recurrence
  on the pinned generation (5h43m, reaped + redone); 180/180 clean.
- PR #1304 comparison artifact (tuxlink-bssvw): OPEN, CI verify pending at
  compaction. dev/battery/comparison/: generator + 3-run self-contained
  HTML mirroring the :8899 badge grammar. MERGE ON GREEN (intent stated).

## RUNNING at compaction (all self-driving)

1. **control2-base**: NEXT-GENERATION control (main 40fd9b7e = bc9bc648 +
   aymi7 + grc1j + idfix; strings-gate passed incl. "tool dispatch
   exceeded" marker). qwen 122B temp 0.2 conc 16 n=10, started 10:10Z,
   expect COMPLETE ~14:30Z. FIRST RUN where C2/EU2 are measurable. Judge
   daemon pid 1417338 (Pi, dev/scratch/control2-judge), monitor bnj5pbkwj,
   :8899 dashboard on it. On COMPLETE: drain -> scp judgments -> clone
   join -> report -> docs PR (control1 comparison carries the generation
   delta deliberately).
2. **Fetches**: Inkling-Small-NVFP4 -> unit 1 (11/159 GB at compaction);
   Qwen3-235B-A22B-Instruct-2507-NVFP4 -> unit 2 (93/129 GB; poll
   bp9qtmk7s). Both via dashboard fetch (its .locks perms fix: chown -R
   administrator on hub/.locks; dashboards cache profiles at startup,
   restart after ANY profile/app.py patch).
3. **vllm/vllm-openai:latest pulling on BOTH boxes** (/tmp/vllm-pull.log):
   needed for Inkling (cu130-nightly does NOT know inkling_mm_model) and
   is the 2mwoz gptoss retry lever. VERIFY inkling in the new image's
   registry before trusting it.

## The operator-confirmed run queue (order is a directive)

control2 -> **TP2 bring-up** (tuxlink-wkp2z: ray --node-ip-address={qsfp}
both sides + pin VLLM_PORT in _do_cluster_switch, patch BOTH dashboards +
restart them, validate with q122-tp2 first) -> **Inkling-Small arm**
(tuxlink-fa6x4, P1: 159G TP2, ~330k KV pool; QSFP weights to unit 2 after
fetch; thinking-effort setting at card default, note it; NVFP4-vs-BF16 gap
explicitly unmeasurable locally - plain is 495G BF16, flagship is 952B) ->
**q235 arm** (Instruct-2507, 262k NATIVE - the 40960 pre-2507 checkpoint
was the WRONG repo, operator caught it; temp 0.2 same-family ladder vs
122B control) -> **mistral2 arm** (tuxlink-nuke4; 32k ceiling stands -
MLA re-verified broken on current kernel 2026-07-31, Triton 256v512 at
first decode, engine STARTS then dies so probe generation not just
startup) -> **gptoss retry + arm** (tuxlink-2mwoz; try vllm:latest
backends) -> regenerate dev/battery/comparison after EACH run (drop
joined json in data/, add RUNS entry, python3 generate_comparison.py).

## Hard-won rules from this stretch (memory updated)

- **Verify every kill with kill -0 in the SAME call**: the mistral->gptoss
  chain watcher survived an unverified kill and fired switches on both
  boxes 8h later (both wedged mid-switch; cleaned). NO autonomous chain
  watchers remain; all model transitions are manual on wake events.
- Spark dashboards cache profiles.json + app.py at startup; every patch
  needs systemctl restart spark-dashboard or it silently does nothing
  (cost two phantom gptoss experiments).
- GB10 liveness = power draw + token-counter delta; util%/health lie.
- One compat fix per new vendor so far (qwen args-stringify, mistral
  call-id); expect the same class for Inkling.

## Operator context

Cameron left his ~7-year support-engineering role last week (memory
updated). Stale-premise catching is the professional muscle: expect and
welcome challenges; verify against live state, never notes. Two premise
challenges this session both resolved by direct probe (MLA: note held;
235B context: note was stale, 2507 repo is 262k native).

## Open queue beyond the runs

tuxlink-2mwoz (gptoss), wkp2z (TP2), fa6x4 (Inkling), nuke4 (mistral2),
bssvw (comparison artifact, regenerate per run), qwen speculative drafter,
m71mu, qaq54, BYOK retest, stale cargo PID 247403 on R2 (NOT ours).

Agent: chasm-wren-crag
