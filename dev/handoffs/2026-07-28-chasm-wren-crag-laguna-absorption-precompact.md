# Pre-compaction state, chasm-wren-crag (2026-07-29 ~01:30Z): Laguna evaluated, absorption round in CI, ladders held for cluster

Same session continues after manual compaction. This doc is the
authoritative state. Prior handoff (same day):
2026-07-28-chasm-wren-crag-wirecompat-bakeoff-baseline1-inflight.md.

## Shipped today (all merged)

- PR #1286 baseline1-base report: 18/54, no wire-compat regression, gate
  cleared for the lift queue.
- PR #1287 Qwen-lift queue: voacapl env staging (R2
  ~/battery-propagation/{voacapl,itshfbc}, .deb-extracted, sudo-free) +
  C1/C2 predicate corrections + Codex P1/P2 fixes.
- PR #1288 lift1-base report: 15/54, no lift; findings: C1 confabulation
  family not predicate-bound; C2 deny-text coaching (-> shopf); E3 used the
  staged engine but structural failure binds.
- PR #1289 Laguna probe report: full story of both probes + redirects.
- Operator artifact: dev/scratch/2026-07-28-qwen-vs-inkling-battery.html
  (self-contained, hover scenarios, sanitized; takeaway bullet kept
  narrow after two operator corrections about overclaiming).

## IN FLIGHT: PR #1290 (absorption round) — THE ACTIVE ITEM

Branch agent-chasm-wren-crag/absorption-round, head 2bda2d50 (borrow fix
after an E0382 on both arches). CI re-running at compaction time; monitor
armed in-session (will not survive compaction — re-arm: `gh pr checks
1290` loop). Content: le9h9 stringify diagnosis (validate.rs +
first_validation_error), generic repeat-notice (runner.rs
annotate_repeats, 3x identical call+result), Valid-disposition completion
sentence (ports.rs), shopf deny-text reword (router.rs denial_remedy;
OPERATOR REVIEW of the copy requested in the PR body, not yet given).

**On CI green: merge (standing grant, intent stated in PR body).** Then:
rebuild elmer_battery on R2 in ~/eefln-ab at the merge SHA — MUST use
`~/.cargo/bin/cargo` (cargo 1.96; bare `cargo` in non-interactive ssh
resolves to apt's 1.75 and fails on edition2024) — then strings-gate:
existing markers (parity-v1, forms-sequence-counters.json, NESTED under
the, pass_expected_revision, battery propagation: engine staged; deny
teaching ABSENT) PLUS new: "fails to parse as JSON", "repeat_notice",
"routine is COMPLETE", "CONTINUE the parts".

**Then HOLD the ladders.** Operator rulings (all in battery-methodology
memory): qwen is the CONTROL — every absorption change validates against
a qwen ladder before conclusions; n=10 per cell regime (18x10=180/arm)
gated on the dual-Spark cluster; if the cluster is ready when #1290
merges, the qwen control ladder + Laguna t07 ladder run at n=10 as the
first cluster runs; do NOT burn the comparison on n=3 unless the operator
says so.

## Second Spark

In hand; operator provisioning on the rack KVM (hours of updates), was
"a few hours off" ~22:00Z. On ready: qealk clustering (bifurcated model
storage, 200GbE cross-mount, dual-endpoint driver patch — per-worker QEP
+ box/model fields in manifest/latency rows). Check dmidecode +
AC-restore on the new unit (first unit is ASUS GX10: ships stay-off
after power loss; enclosure blocks the power button — see the morning's
smart-plug/auto-power-on findings). Spark #1 currently SERVES LAGUNA
(laguna-s21-nvfp4, :8000); switch back to qwen via dashboard
/api/switch/q122 when ladders need it.

## Laguna evaluation (complete; report merged; bd tuxlink-07vaa)

t=0.2: 4P/2~/12F. t=0.7: 5P/3~/10F. Union-of-best 8/18 single-attempt
passes incl. S2 (qwen 0/6). Redirects: 3 of 4 loop classes correct under
one operator sentence; self-explanations transcript-verified (P1:
"chasing a clean validation" -> the completion-sentence fix). Loop
survivors at 0.7: EU1/EU2 research-forever, E1 31x byte-identical
docs_search (-> the repeat-notice fix). le9h9 killed S4+P3 at 0.7.
Bundles preserved: R2 ~/laguna-probe, ~/laguna-probe-t07,
~/laguna-probe/redirect. Judgments in session scratchpad
(laguna-probe-judgments.jsonl, laguna-t07-judgments.jsonl).

## Open queue and decisions

- Operator review pending: shopf deny copy (PR #1290 body).
- Validator-depth lints (send-leg-unreachable, missing-delay,
  branch-inversion): NEXT feature PR after #1290, then ladder.
- pyd3d (raw stream capture) + 8mofz belt (deferred, shape evidence):
  before any cloud re-probe.
- Operator calls open: m71mu (parked), qaq54 true-frontier probe
  (Sonnet/Opus/GPT-5.6 first-party, ~$5-15/model), BYOK wall retest
  (optional), Inkling/GLM teacher question MOOT pending surface fixes
  (self-distillation is the working fine-tune path).
- Fine-tune target families (post-absorption evidence): confabulation
  (A1/C1), stall/agenda (EU1/EU2), structural completeness (validator
  lints first).

## Standing cautions (unchanged + today's additions)

- Shell cwd RESETS between calls: standalone `cd` into the worktree
  before git ops; it bit three times today (once against the operator's
  main checkout — aborted safely).
- Probe sweep drivers rm -rf non-completed cells at round start: STOP
  the driver at "round 1 end" before re-runs destroy loop-evidence
  bundles.
- R2 builds: ~/.cargo/bin/cargo, never bare cargo.
- elmer_battery needs OPENROUTER_API_KEY in env even for local runs;
  key via secret-tool pipe, never argv/disk.
- ssh+nohup fd-hang: expect timeout-kill of the wrapper; verify from a
  fresh connection; literal-PID kills only, whole trees.
- Monitors do not survive compaction; re-arm from recipes here.

Agent: chasm-wren-crag
