# Evening state, chasm-wren-crag (2026-07-31 ~19:30Z): control2 shipped, codernext1 running, inkling blocked on an operator decision

Same session, fourth compaction point. Prior handoff:
2026-07-31-chasm-wren-crag-crossvendor-queue-precompact3.md.

## Shipped

- **PR #1306 (merged 16:00Z)**: control2-base report + comparison artifact
  regenerated to 4 runs. Topline 31.1 percent (56 PASS / 71 PARTIAL / 53
  FAIL, 180/180 judged) vs control1's 28.9 percent on the old generation:
  the aymi7 + grc1j + call-id generation change did NOT move the control,
  so cross-vendor arms need no re-baselining. First valid C2 (0/8/2:
  denial recovery runs, stalls at confirmation) and first clean EU2
  (0/0/10, a true fail-family cell). grc1j dispatch deadline armed, never
  fired in 180 bundles.
- **tuxlink-wkp2z CLOSED**: dual-Spark TP2 works. q122-tp2 validated with
  a real completion (29 tokens, 2.3s) through the prod endpoint. Three
  stacked fixes, all live in the dashboard app.py on both boxes:
  ray `--node-ip-address={qsfp}` both roles; `--object-store-memory=2G`
  both roles; TP2 profile util 0.90 -> 0.80/0.85. Plus `VLLM_PORT=42061`
  pinned and a pre-load page-cache drop in the switch flow.
- **Shelly plugs configured and PROVEN** (operator model: AP-isolated, no
  LAN membership, no cloud). Both Gen4 plugs: `initial_state=on` so a plug
  reboot can never strand a Spark, cloud disabled, WPA2 on the AP, RPC
  digest auth on. Credentials in the Pi keyring under service
  `shelly-plug`, accounts `b70-ap` / `b70-rpc` / `4cc-ap` / `4cc-rpc`.
  Power cycle #3 ran entirely from the Pi through them.

  Recipe (Pi): `sudo nmcli connection up shelly-b70` (or `shelly-4cc`),
  then `curl --digest -u admin:<rpc-cred> "http://192.168.33.1/rpc/Switch.Set?id=0&on=false"`,
  wait, same with `on=true`, then `sudo nmcli connection up "Mohaverad.io Alt"`.
  Plug-to-Spark mapping is still unlabeled; cycle one and watch which box
  drops to record it.

## RUNNING at compaction

**codernext1** (tuxlink-t32pt, operator-directed): Qwen3-Coder-Next-FP8,
75G on both boxes, two replicas, ladder started 18:32:43Z, ETA ~23:30Z.
Judge daemon live on the Pi (`dev/scratch/codernext1-judge`, pid 3186355),
run monitor on run.log, ladder dashboard on :8899 repointed at this run.
Wire smoke was clean on the first try: proper tool_calls finish, JSON
object arguments, sane server-side call ids. First vendor in the program
needing ZERO wire-compat fixes.

On COMPLETE: judge drain -> scp judgments -> clone join script (sed the
run name) -> joined json -> report -> docs PR -> regenerate
`dev/battery/comparison` with a 5th RUNS entry.

Purpose: coder-tuned vs instruct on identical cells, same generation.
Caveat for the report: FP8 here vs NVFP4 on the controls.

## Inkling: blocked on an operator decision, not on our stack

Seven bring-up attempts. Everything on our side is solved: scipy layered
into the image, ray networking fixed, and pipeline parallelism (PP2) loads
all 159G cleanly where tensor parallelism wedged both boxes three times
(that model's day-one TP loader materializes host-side; two of those
wedges cost physical power cycles, the third used the new plugs).

The wall is upstream: vLLM's SM120 FA4 path asserts `Paged KV not
supported on SM 12.0 in this PR`, present in both `:latest` and the
2026-07-31 nightly. Inkling's relative attention has exactly one kernel
path and it passes a page table unconditionally, so there is no fallback
and no env override.

Operator rulings this evening, all recorded on tuxlink-fa6x4:

- OpenRouter arm REJECTED (day-0 availability unverified, provider
  confound makes the data not useful).
- A community HF kernel shim REJECTED outright on supply-chain grounds
  (recent HF compromise incident). It was fully purged from the unit-1
  image and verified gone; nothing untrusted was ever downloaded or run.
- 3-bit GGUF path (Unsloth, which does name DGX Spark as a target device)
  REJECTED: quant compromise too large.

Where it actually stands: dual-Spark Inkling serving IS a solved problem
publicly. `eugr/spark-vllm-docker` ships a named `inkling-small-nvfp4`
recipe requiring "at least dual DGX Spark", and its config independently
matches every fix we derived (page-cache drop, a Spark-specific weight
loader, disabling the MNNVL-only fused op) plus the paged-KV workaround.
Clone for review at `dev/scratch/spark-vllm-audit/`. A delegated audit of
that mod came back CLEAN at 19:35Z: the vendored kernel tree matches its
pinned upstream source, and no malicious capability was found. Two
operational caveats the operator should weigh before building it in:
one required mod runs an unbounded root background loop on both boxes,
and the bundle stacks several still-open upstream PRs rather than the
single one its provenance note names, so it should be dropped in favour
of upstream once those land (watch Dao-AILab/flash-attention#2348 and the
vLLM SM120 split-KV work; re-grep the assertion string in a fresh nightly
at session starts).

**The build decision is the operator's and has NOT been made.** Nothing
from that repo has been applied to either box.

Note for whoever picks this up: security-analysis work must not run in
the main loop. Delegate it, and keep the analysis content out of the
parent context entirely.

Everything else for attempt 8 stays staged: weights on both boxes, PP2
profile, R2 launcher, Pi judge dir, PROVENANCE.

## Queue after codernext1

q235 (Instruct-2507, 130G on both boxes, `q235-tp2` profile staged with
the hermes parser and 262144 native context) -> mistral2 (tuxlink-nuke4,
32k host ceiling stands) -> gptoss retry (tuxlink-2mwoz, try the newer
image backends) -> regenerate the comparison artifact after EACH run.

## Also captured today

- **Fan A/B baseline** at `dev/scratch/thermal-fan-ab/` (180 samples per
  box under battery load). The finding that matters: every throttle event
  on both boxes decodes to SwPowerCap, ZERO thermal-slowdown events, even
  with zone peaks at 90.5 C. The clock dips are power-cap-driven, so the
  industrial intake fan will buy thermal margin but should NOT be expected
  to recover clocks. Side B recipe is in that README; the sampler script
  lives in /tmp and needs re-pushing after any reboot.
- **Disk**: node 1 went from 13G free to 324G (evicted gpt-oss duplicate
  format copies, an unused GGUF, retired Nemotron weights, Laguna
  residue), then absorbed q235's 130G and sits at ~194G.
- **Watcher lesson, twice**: a watcher that probes by an unknown-host-key
  path reports a healthy box as dead forever. Probe the same way the
  working path does, and prefer probing the container/process directly
  over a service's own status field.

Agent: chasm-wren-crag

## ADDENDUM (2026-08-01 ~09:00Z, pre-compaction): the Inkling marathon

Everything below supersedes the Inkling section above. Full resume state
lives in `bd show tuxlink-fa6x4` (the compaction anchor note) - THAT is
canonical. Summary: Inkling SERVES on dual Spark (eugr prebuilt
af83e54adfad + audited paged-KV mod; correct completions + tool calls)
but the engine dies with probabilistic CUDA illegal-access under REAL
battery load only, across every topology and image tried. Three garbage
runs quarantined (inkling1-invalid*): instant CHAIN DONE = dead-engine
fast-fail, check provider_turns and durations before trusting ANY run.
Current attempt: --enforce-eager, in flight. Two identified missing
pieces vs the community's working setup: the streaming tool-call parser
patch (extracted, anchors need adapting) and ELMER_MAX_TOKENS=3000 decode
cap (not yet added to launch_inkling1.sh). Validation sequence is in the
bd note and is non-negotiable: synthetic probes passed twice while real
load crashed the engine.

Also this period: codernext1 killed at operator direction (15/180,
resumable); DP+EP topology scripts staged on both nodes; docker group
granted both Sparks; nodes cleaned to 182G/172G free; Shelly plugs
hardened (WPA2+digest) and PROVEN as remote recovery (power cycle #3 ran
from the Pi). Operator rulings: no OpenRouter arm, no 3-bit GGUF, no
unaudited binary kernels (later relaxed to allow the eugr prebuilt image
after the source build proved inconsistent); NFS cross-node weight cache
approved AFTER a valid ladder run.

Agent: chasm-wren-crag
