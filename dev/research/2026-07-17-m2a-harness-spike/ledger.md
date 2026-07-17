# M2a spike ledger — every dispatch, intervention, verification, Spark state change

2026-07-17T02:53:04Z PR #1128 (M1-close handoff) merged (c4bf7847); worktree
  bd-tuxlink-7raoe-m1-close-handoff disposed per ADR 0009 (inventory: tracked
  clean, no untracked, ignored=node_modules only, no worktree stashes).
2026-07-17T02:56:00Z Spark live-checked: /v1/models = qwen3-coder-next
  (262144 ctx) — as-found state matches ladder close. SSH + passwordless
  sudo docker on gx10-65aa verified; vllm container args confirmed to include
  --enable-auto-tool-choice --tool-call-parser qwen3_coder (chat-completions
  tool calling available to Pi without a Spark state change).
2026-07-17T03:01:00Z mini-swe-agent 2.4.5 installed (uv tool). DESIGN
  CORRECTION recorded: v2 default config uses a bash TOOL call; the
  no-tool-protocol treatment is the bundled mini_textbased.yaml (one fenced
  mswea_bash_command block per turn) — spike uses the text-based config.
2026-07-17T03:05:00Z Pi 0.80.10 installed at ~/.local/share/m2a-harnesses/pi
  (pnpm; system npm 9.2.0 hits an arborist bug on its shrinkwrap). Pi needs
  node >=22.19.0 — private node22 runtime at ~/.local/share/m2a-harnesses/
  node22, invoked by binary path so worker subshells keep system PATH.
  INCIDENT (resolved): first install attempt ran pnpm inside the repo
  (dev/scratch) and pnpm walked up to the workspace root, adding the
  dependency to the REPO package.json + lockfile on the operator's main
  checkout; reverted immediately via git restore of exactly those two named
  files (diff verified = only the added line before restore). Harness
  installs live OUTSIDE the repo now.
2026-07-17T03:10:00Z OpenRouter key retrievable from OS keyring
  (service=elmer-openrouter account=teacher) — verified without printing.
2026-07-17T03:12:00Z Orchestrator worktree bd-tuxlink-7raoe-m2a-spike
  (branch bd-tuxlink-7raoe/m2a-harness-spike @ origin/main 334a9779).
  Six worker worktrees created at base b82b404d (pre-ladder; contamination
  control — main now carries grading keys): m2a-{pi,mini}-{cn-r3,q122-r3,
  e122-r5}, branches bd-tuxlink-7raoe/m2a-<cell>, never-merge. pnpm install
  --frozen-lockfile OK in all six.
2026-07-17T03:35:00Z HARNESS SMOKES (gate) — all four PASS with real shell
  round-trips + file writes verified on disk:
  (1) Pi × Spark CN: bash+write tools via chat-completions tool calls, clean
      final message. Transcript decision: --mode json emits per-token deltas
      carrying full partial state (quadratic transcript bloat at rung scale)
      → cells run --mode text for the tee; the structured record is the Pi
      session file (--session-dir into the worker's sdd dir).
  (2) mini × Spark CN: REQUIRED FIXES found by smoke — (a) first-run guard
      env MSWEA_CONFIGURED (set via mini-extra config set); (b) litellm
      import chain needs fastapi+orjson (uv tool install --with); (c) local
      model unmapped in litellm cost table → model.cost_tracking:
      ignore_errors; (d) CRITICAL: bundled mini_textbased.yaml carries only
      TEMPLATES — the fenced-bash parser lives in model_class
      litellm_textbased / openrouter_textbased; without it mini's default
      model class demands tool calls and dies RepeatedFormatError. Overlays
      now set model_class explicitly. Fenced-bash loop then passed clean.
  (3) Pi × OpenRouter E122: pass (built-in catalog lists
      qwen/qwen3.5-122b-a10b, thinking:yes). --thinking medium validated by
      separate round-trip — E122 cell runs it (parity with the Codex
      Responses arm's default reasoning effort).
  (4) mini × OpenRouter E122 (openrouter_textbased direct client): pass.
  Q122 smokes deferred to the Spark swap wave, per ladder discipline.
2026-07-17T03:38:50Z WAVE 1 dispatched: pi-cn-r3 (Spark) ∥ pi-e122-r5
  (OpenRouter), timeout 1800 each.
2026-07-17T03:43:32Z pi-e122-r5 attempt-1 FINISHED (4.6m wall, exit 0 —
  NO delivery seam; clean final message with Status contract honored).
  VERIFICATION: gates re-run by orchestrator — typecheck green, vitest
  7/7 green (claims TRUE, integrity honest); diff = exactly the 2 claimed
  files; report file present. MECHANISM GRADE vs rung-5 key: WRONG —
  window-scoped-emit theory (changed host reply to emitTo('stations',...)
  and rewrote the snapshot test to pin emitTo); capability ACL never
  considered; fix is at the reply site while the DENIED call is the
  stations-window request, so the handshake would still not deliver in a
  real runtime. Completion: FAILED (acceptance criterion 1). Usage from
  Pi session: 44 assistant turns, 1.80M input / 13.2k output, reasoning
  tokens ≈ 24 — the model essentially did not deliberate under
  chat-completions --thinking medium, vs the Codex Responses run whose
  REASONING reached the key-exact diagnosis before the seam ate delivery.
  a1 candidate committed on the arm branch; tree reset to b82b404d
  (named-file restore, committed); attempt-2 dispatched 03:52Z.
2026-07-17T03:51:26Z pi-e122-r5 attempt-2 FINISHED (4.7m wall, exit 0).
  FAILED with a NEW SEAM VARIANT: same wrong emit-broadcast theory moved
  to the backend (lib.rs app.listen→emit forwarding — non-functional; the
  ACL-denied request never leaves the stations webview). Spawned a cold
  cargo check mid-run (doomed-cargo class, cf. ladder CN r5 a1). Final
  turn emitted XML-style function-call syntax Pi does not parse → landed
  as thinking content → empty final message, session ended on stop. NO
  report, NO status line, no false claims (integrity honest). Usage: 47
  turns, 2.20M in / 18.1k out, reasoning ≈ 44 tokens. a2 committed.
  CELL VERDICT pi-e122-r5: FAILED both attempts — the Codex Responses
  seam did NOT reproduce (both Pi attempts had working delivery
  mechanics), but E122's correct-ACL-diagnosis capability ALSO did not
  reproduce under chat-completions: near-zero reasoning tokens both
  attempts vs the Codex arm's deliberation that reached the key. The
  ladder's "reasoning-limited vs harness-limited" split is thus
  API-ROUTE-DEPENDENT, not just harness-dependent.
2026-07-17T03:53:41Z mini-e122-r5 attempt-1 dispatched (OpenRouter slot
  freed by pi-e122-r5 close; 2-concurrent cap respected).
2026-07-17T04:08:47Z pi-cn-r3 attempt-1 FINISHED: 30m AT-CAP (exit 124),
  killed mid-test-writing. All 4 target files touched (sites 1-7 wired +
  partial tests — FURTHER than the Codex baseline a1, which had sites and
  ZERO tests). Orchestrator gates: typecheck RED (mockReport import shape
  in both test files), vitest 3 failed / 123 passed. No report; no claims
  (integrity n/a). Completion: FAILED at cap. Usage: 36 turns, 2.94M in /
  16.4k out. Committed; tree reset; attempt-2 dispatched 04:12Z.
2026-07-17T04:10:12Z mini-e122-r5 attempt-1 FINISHED (16.5m, clean
  submit; report + in-file Status line per contract). Mechanism graded
  WRONG vs key: emit-inside-.then() listener-timing race theory — the
  exact class the key excludes; ACL never considered; fix moves emit
  BEFORE listener registration (would still be ACL-denied in reality).
  Gates re-verified green (typecheck + 7/7) — claims TRUE, integrity
  honest. Step 33, $0.16. Completion: FAILED (criterion 1). Committed;
  tree reset; attempt-2 dispatched 04:15Z.
2026-07-17T04:42:40Z pi-cn-r3 attempt-2 FINISHED: 30m AT-CAP (exit 124)
  again. Tree further than a1: sites + tests in both files, typecheck
  GREEN, 3 tests failing (124 passing), no report. Usage: 34 turns,
  2.76M in / 13.1k out. Committed. CELL VERDICT pi-cn-r3: FAILED both
  attempts — envelope failure REPRODUCES under Pi (matches Codex
  baseline), though both Pi attempts stood further along at cap than the
  Codex counterparts (a1 sites+partial tests vs sites-only; a2
  typecheck-green vs syntax-broken). Spark slot freed; mini-cn-r3
  attempt-1 dispatched 04:47Z.
2026-07-17T04:45:14Z mini-e122-r5 attempt-2 FINISHED: 30m AT-CAP (exit
  124), ZERO edits — still exploring when killed (a1's report file
  remains in sdd; tree untouched, nothing to commit). CELL VERDICT
  mini-e122-r5: FAILED (a1 wrong listener-timing mechanism delivered
  clean; a2 no delivery). CROSS-HARNESS E122 rung-5 tally: the correct
  ACL diagnosis appeared ONLY under Codex's Responses API (where
  delivery died on the seam); Pi and mini both produced fast shallow
  wrong theories with near-zero deliberation. Removing the seam did NOT
  recover the capability — the reasoning route itself was the enabler.
  Post-hoc probe planned (flagged per rubric): one pi-e122-r5 run at
  --thinking high after registered cells close, to isolate the thinking
  knob.
2026-07-17T05:17:05Z mini-cn-r3 attempt-1 FINISHED: 30m AT-CAP (exit
  124) at step 91. All 4 files touched; typecheck RED (1 unused-var),
  4 tests failing / 123 passing; no report. Notable: model attempted
  `cd /testbed` near cap — SWE-bench training prior leaking through the
  mini loop. Committed; tree reset; attempt-2 dispatched 05:21Z.
2026-07-17T05:19:48Z pi-e122-r5 POST-HOC PROBE (--thinking high,
  flagged) FINISHED: 30m AT-CAP, tree = another Rust-side workaround
  (stations_window.rs + lib.rs + frontend edits), no report. Usage: 40
  turns, 1.36M in / 10.1k out, reasoning ≈ 38 tokens — the thinking
  knob (medium→high) did NOT change deliberation volume on the
  OpenRouter completions route; capabilities/allow-emit mentioned ZERO
  times in session. CONCLUSION for milestone 2: E122's key-exact
  diagnosis under Codex was a Responses-API-route property (reasoning
  preserved per-turn), not recoverable via Pi's thinking flag on
  chat-completions; a milestone-2 harness for hybrid-reasoning models
  must use a reasoning-preserving wire route. Probe committed on the arm
  branch.
2026-07-17T05:53:21Z mini-cn-r3 attempt-2 FINISHED: 30m AT-CAP. Panels
  wired, typecheck green at kill, but ZERO test files touched and both
  stale pinning tests red. CELL VERDICT mini-cn-r3: FAILED both attempts
  (envelope reproduces under text-based loop). Committed.
2026-07-17T05:58:00Z SPARK STATE CHANGE: docker stop vllm (CN container
  PRESERVED); docker run vllm-q122 per the ladder recipe reconstructed
  from the live container config + ledger flags (image cu130-nightly,
  HF-cache + patched-template ro mounts, served qwen35-122b-nvfp4,
  131072 ctx, --enable-auto-tool-choice --tool-call-parser qwen3_coder).
  Model load in progress; health poll running.
2026-07-17T06:23:00Z SPARK STATE CHANGE (launch incident + fix): first
  vllm-q122 launch exited immediately — the image ENTRYPOINT is already
  ["vllm","serve"], and passing "serve <model>" doubles the subcommand
  (the earlier docker-inspect .Args view merges the entrypoint tail,
  which misled the reconstruction). Container removed, relaunched with
  the model passed positionally — loading normally. Same latent bug
  fixed in the spark-dashboard profile launcher (app.py) + committed on
  the Spark repo + service restarted. ~26 min lost to the failed launch
  window.
2026-07-17T06:34:00Z Q122 healthy (~11 min load, in line with ladder's
  ~13). Q122 harness smokes: Pi PASS (tool-call round-trip + file write),
  mini text-based PASS. pi-q122-r3 attempt-1 dispatched 06:40Z.
2026-07-17T07:07:09Z pi-q122-r3 attempt-1 FINISHED: 30m AT-CAP (exit
  124). All 4 files touched, typecheck GREEN, 3 tests failing / 124
  passing, no report. Further at cap than the Codex Q122 baseline a1
  (sites, zero tests). Committed; tree reset; attempt-2 dispatched
  07:12Z.
2026-07-17T07:40:12Z pi-q122-r3 attempt-2 FINISHED: 30m AT-CAP.
  Typecheck RED (mockReport import shape — third occurrence of this
  exact trap across Pi attempts), 2 collection errors. CELL VERDICT
  pi-q122-r3: FAILED both attempts (envelope, matches Codex baseline;
  a1 was the furthest Q122 tree measured: typecheck green + both test
  files). Committed. mini-q122-r3 attempt-1 (final registered cell)
  dispatched 07:45Z.
2026-07-17T08:12:55Z mini-q122-r3 attempt-1 FINISHED: 30m AT-CAP. 4
  files, typecheck GREEN, 3 tests red; hit the mockReport trap and was
  mid-repair at kill. Committed; reset; attempt-2 dispatched 08:15Z.
2026-07-17T08:45:47Z mini-q122-r3 attempt-2 FINISHED: 30m AT-CAP.
  Typecheck RED (mockReport trap — 5th occurrence across the spike),
  6 tests red. CELL VERDICT mini-q122-r3: FAILED both attempts.
  ALL SIX REGISTERED CELLS CLOSED — every cell FAILED both attempts;
  the spike's discriminating value is in the failure ANATOMY (see
  report.md findings F1-F4).
2026-07-17T08:49:00Z SPARK STATE CHANGE: CN restore executed VIA THE
  SPARK DASHBOARD's switch API (its first live exercise, as planned):
  vllm-q122 stopped cleanly (Exited 0), vllm container started, health
  poll streaming container logs through the switch status endpoint.
2026-07-17T08:54:00Z CN RESTORED (~4.5 min from warm container start);
  /v1/models = qwen3-coder-next 262144 ctx; dashboard switch state
  returned to idle. SPARK AT AS-FOUND STATE. Dashboard switch control:
  live test PASS.
2026-07-17T08:56:00Z All 6 worker worktrees disposed per ADR 0009:
  inventories clean (candidates committed on their LOCAL-ONLY never-merge
  bd-tuxlink-7raoe/m2a-* branches, ladder-arm pattern); .superpowers/sdd
  forensics archived to .claude/worktree-archives/
  bd-tuxlink-7raoe-m2a-<cell>-sdd-forensics-<ts>.tar.gz (6 archives);
  rm -rf + git worktree prune. Only the orchestrator spike worktree
  remains (disposed after its PR merges).
