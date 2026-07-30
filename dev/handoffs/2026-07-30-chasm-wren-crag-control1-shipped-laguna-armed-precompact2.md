# Pre-compaction state 2, chasm-wren-crag (2026-07-30 ~20:35Z): control1 shipped, laguna1 self-armed, TP2 parked

Same session continues after a second manual compaction. This doc is the
authoritative state. Prior handoff:
2026-07-28-chasm-wren-crag-laguna-absorption-precompact.md.

## Shipped since the last handoff (all merged)

- PR #1293 validator-depth lints (tuxlink-0hjm4) + advisory disposition
  split; GPT-5.6 adrev 8/8 dispositioned. PR #1294 wire-copy ASCII sweep
  (tuxlink-103y0). Generation binary bc9bc648, 14/14 strings-gate.
- PR #1295 (tuxlink-aymi7, merge 10efaa25): authority denials permit
  continued work; taint one-shot unchanged; spec amended. NEXT generation.
- PR #1297 (merge af7935fd): control1-base report,
  dev/battery/2026-07-30-control1-base-report.md. READ IT: headline is the
  S3 absorption-loop win (0/3 to 5/10 PASS, lint observed firing mid-
  authoring in 8/10); A1/C1/EU1 0/30 = reproducible fine-tune targets;
  13 aymi7 exclusions; grc1j panic; 3 width-16 turn-timeout cancellations.

## Cluster / infrastructure state

- Both Sparks rehoused in the printed enclosure, AC-restore ENABLED by the
  operator (boot on power restore verified: both booted 8s apart).
  Firmware/kernel/driver parity verified (preview fw, 6.17.0-1029, 580.173.02);
  1029 idles GPUs at P8/208MHz BY DESIGN, boosts ~2400 under load.
- 2x200GbE QSFP p2p links: 10.55.0.1/2 (A) + 10.55.1.1/2 (B), MTU 9000,
  NetworkManager-persisted. Unit1<->unit2 root-less ssh works via 10.55.0.2.
- Single balanced Elmer endpoint LIVE:
  https://inference.twin-bramble.ts.net:8444 (nginx least_conn container
  elmer-lb on unit1 over both :8000 backends; restart unless-stopped).
- Dashboards (spark-dashboard on both): dynamic identity, cluster-role
  Serving-card awareness (head/worker), TP2 "Cluster serve" button
  (q122-tp2 profile, UNVALIDATED). TP2 parked as tuxlink-wkp2z: Ray
  registers nodes on LAN IPs -> rendezvous unroutable across LAN subnets;
  HIGH-CONFIDENCE fix lead: ray start --node-ip-address={qsfp ip} both
  sides + pin VLLM_PORT, in _do_cluster_switch (app.py). Image for TP2 is
  the local v0.25.1-ray derivative (stock images ship WITHOUT ray), present
  on both boxes.

## IN FLIGHT at compaction (all self-driving)

1. **laguna1-t07 ladder is SELF-ARMED on R2**: launcher pid on R2 waits for
   laguna on BOTH boxes (they were loading at compaction) then execs
   ladder3-cluster.sh. Config in the launcher + PROVENANCE.md in
   ~/6i8jz-run/battery-results/laguna1-t07/: 18x10, temp 0.7, conc 16,
   TURN_TIMEOUT=2700 (deliberate delta, the control1 censoring lever;
   duration comparisons must note it). Key was keyring-piped into the
   launcher env, never disk.
2. **laguna1 judge daemon** live on the Pi (dev/scratch/laguna1-judge/,
   fingerprint-keyed, self-exits on LADDER3-CLUSTER COMPLETE).
3. **Run monitor** armed on laguna1 run.log (persistent, tail -F).
4. Battery dashboard :8899 on R2 still points at control1-base; repoint to
   laguna1-t07 once the run starts (kill listener pid by LITERAL pid, cd
   into laguna1-t07, LADDER2_ROOT=abs-path nohup python3 dashboard.py
   --port 8899 with trailing &).

## Next actions in order

1. When laguna1-t07 completes + judge drains: clone control1_join.py
   (in the run dir, R2DIR/JUDG paths) -> joined json -> report
   dev/battery/2026-07-30-laguna1-t07-report.md; cross-model comparison vs
   control1 within the SAME generation (temp differs by design). Docs PR.
2. THEN the generation guard lifts: rebuild elmer_battery in ~/eefln-ab at
   CURRENT main (includes 10efaa25 aymi7 + whatever landed), strings-gate
   (14 markers still apply; aymi7 adds no wire copy), fix tuxlink-grc1j
   (point_at panic + unbounded tool dispatch) into that generation, then
   its control run makes C2 measurable for the first time.
3. TP2 validation retry (wkp2z lead) in any idle-GPU window; flip profile
   validated:true only after a real completion returns tokens.

## Cautions (hard-won today; memory updated)

- pgrep -f in watch loops SELF-MATCHES the wrapper argv: bit FOUR times.
  Use the [b]racket trick or poll PIDs/files/endpoints. Memory:
  feedback_pgrep_self_match_bracket_trick.
- Do not restart docker or spark-dashboard while a transfer/switch thread
  is in flight (killed a docker-load stream and a switch thread today).
- ssh heredoc quote-mangling: write scripts locally, pipe via
  `ssh host python3 - < file`.
- Chains pin to boxes; the ladder tail decays by design (fan-in).
- elmer_battery still needs OPENROUTER_API_KEY in env (keyring pipe).
- Stale 2-day cargo test PID 247403 in ~/hwo1b-check on R2: prior
  session's, NOT ours, left untouched.

## Open queue

tuxlink-grc1j (P1, next-gen fix), tuxlink-wkp2z (TP2), qwen speculative
drafter (optional latency play), m71mu (parked), qaq54 frontier probe,
BYOK retest, battery dashboard repoint.

Agent: chasm-wren-crag
