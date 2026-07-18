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

## POST-HOC (2026-07-18, hemlock-maple-clover) — Responses-route probe

2026-07-18T03:22:06Z pi-e122-r5-responses FALSE START: dispatched under
  the harness Bash tool (10-min timeout risk to the envelope); killed
  ~80s in, worker tree verified untouched, relaunched detached. Not an
  attempt.
2026-07-18T03:23:24Z pi-e122-r5-responses attempt-1 dispatched
  (post-hoc probe #2, flagged): identical treatment to pi-e122-r5 except
  provider registered with api:"openai-responses"
  (pi-openrouter-responses.js). Pre-flight curl smoke: /responses
  returns reasoning items for this model (111 reasoning tokens on a
  one-word task).
2026-07-18T03:23:57Z INCIDENT: orchestrator created an unrelated
  worktree (origin/main checkout incl. grading keys) INSIDE the worker
  tree via relative-path git worktree add; moved out ~03:26Z. Session
  audit: zero worker references to the nested path; no dev/research
  reads. Deviation found by the same audit: worker's first 4 commands
  walked the PARENT repo root and read the operator checkout's
  StationsView.tsx once (Pi has no fs sandbox) before re-anchoring.
2026-07-18T03:28:30Z pi-e122-r5-responses attempt-1 FINISHED exit 0 —
  5m06s CLEAN COMPLETION (vs 3x 30-min at-cap on completions route).
  Report delivered, Status DONE, gates honest (typecheck + 7/7 vitest
  re-verified orchestrator-side). GRADE: WRONG — frontend
  listener-race theory + microtask-delay workaround; capability ACL
  never mentioned. Reasoning ≈34 tokens across 25 turns (collapse after
  opening turns). Candidate diff committed on local arm branch
  (da1057db, NEVER MERGE). Full analysis: addendum-responses-probe.md
  (finding F5 — route necessary-but-not-sufficient; F2 refined).

## POST-HOC continuation (2026-07-18, hemlock-maple-clover) — probe #3

2026-07-18T04:40–05:25Z DIAGNOSIS of the F5 reasoning collapse: 3 ablation
  rounds against /responses + logging-proxy capture of Pi's exact
  per-turn requests (4-turn mini-task; collapse reproduced 44/0/1/1).
  Controlling variable isolated: input ending at function_call_output
  vs a trailing user message (0 vs 439 reasoning tokens, both
  directions, 2/2). FINDING F6: Qwen3.5 template opens <think> only
  after a USER turn — agentic loops never re-enter thinking.
2026-07-18T05:25Z EXTENSIONS BUILT: pi-think-reviver.js (context-event
  transient user-turn nudge; validated on mini-task 36/13/21/280 vs
  44/0/1/1) and pi-toolsyntax-detector.js (mandatory work item 2,
  message_end pseudo-tool-call retry, budget 3).
2026-07-18T05:31:39Z pi-e122-r5-responses2 attempt-1 dispatched (fixed
  harness: responses route + reviver + detector).
2026-07-18T05:47:45Z attempt-1 FINISHED exit 0 — 16m06s clean. Reasoning
  EVERY turn (47 turns, 87k reasoning tok incl. one 81,920 runaway
  spiral). GRADE: WRONG (emit-is-window-local theory, emitTo
  workaround; ACL never named). Commit 98e79c18; tree reset (185584b7).
2026-07-18T05:50:48Z attempt-2 dispatched.
2026-07-18T05:59:04Z attempt-2 FINISHED exit 0 — 8m16s clean. Reasoning
  every turn (56 turns, 7.5k tok). GRADE: WRONG (webview-scoping
  theory; new Rust backend commands, non-minimal wrong layer; ACL never
  named). Commit 3b0990b1. Detector never triggered either attempt.
  CELL VERDICT: FAILED 0/2 with harness fixed. FINDING F7 (definitive):
  rung-5 diagnosis is a MODEL-capability limit for E122, not harness —
  the ladder's "harness-limited" verdict for this cell is overturned.
  Full analysis: addendum-responses-probe2.md.

## POST-HOC continuation 2 (2026-07-18, hemlock-maple-clover) — Mistral round

2026-07-18T06:44Z SPARK STATE CHANGE (operator-authorized "run whatever
  you'd like on the Spark"): docker stop vllm (CN preserved); first-ever
  launch of vllm-mistral119 (Mistral-Small-4-119B-2603-NVFP4,
  mistral-format flags + mistral tool parser). Loaded; CRASHED on first
  inference: TRITON_MLA Triton kernel shape error (256v512) — finding M1.
2026-07-18T~07:05Z relaunch VLLM_ATTENTION_BACKEND=FLASHINFER — same
  crash; log proves TRITON_MLA is the ONLY MLA backend candidate on the
  GB10 nightly.
2026-07-18T~07:15Z relaunch VLLM_MLA_DISABLE=1 + max-model-len 32768 —
  HEALTHY; inference + native tool-call smoke verified. profiles.json
  updated with the working recipe (env-var caveat noted for the
  dashboard first-run path); dashboard restarted.
2026-07-18T07:24:13Z pi-mistral119-r3 a1 FALSE START (9s): 400
  "Unexpected role 'user' after role 'tool'" — the F6 think-reviver is
  ILLEGAL in Mistral's role grammar (finding M2). Reviver removed from
  the Mistral runner (model-conditional adapters mandated for M2).
2026-07-18T07:25:23Z r3 a1 false start #2 (3m09s): Pi sent 36-37k-token
  prompts against the 32k ceiling (tokenizer divergence, M3) — hard
  400s, one-token "Now" final, tree untouched.
2026-07-18T07:30:53Z r3 a1 (28k-margin config, 45s) + 07:32Z r3 a2
  (~60s): same ceiling deaths — the rung-3 working set exceeds 32k
  outright (batch reads leap the window in one turn). CELL VERDICT
  pi-mistral119-r3: FAILED 0/2, envelope-infeasible.
2026-07-18T07:33:52Z r5 a1 false start (87s, output clamp collapsed to
  1 token) → root cause isolated: Pi NEVER auto-compacts mid-run in -p
  mode (compaction checked on agent_end only) — finding M4.
2026-07-18T07:37:38Z r5 a1 (true-ceiling config, 23 turns, died ~29k
  in) + 07:40Z r5 a2 (14 turns, died 31k in, 4-token final). CELL
  VERDICT pi-mistral119-r5: FAILED 0/2 — envelope-blocked, NOT
  capability-graded; a1's trace had reached src-tauri backend
  command/event registration (closer to the key's layer than any Qwen
  attempt). Detector loaded all runs, never triggered (native tool
  calls only); truncated one-token finals are a NEW silent-death shape
  it does not catch.
2026-07-18T07:44Z SPARK STATE CHANGE: docker stop vllm-mistral119;
  docker start vllm; CN re-verified healthy (/v1/models =
  qwen3-coder-next). AS-FOUND RESTORED.
  Consolidated analysis + M2 build list: definitive-report.md.

## POST-HOC continuation 3 (2026-07-18, hemlock-maple-clover) — OpenRouter comparison arm

2026-07-18T07:57:56Z pi-mistralor-r3 a1 + pi-mistralor-r5 a1 dispatched
  in parallel (mistralai/mistral-small-2603, full precision, 262k ctx,
  --thinking medium, detector loaded, no reviver). Both finished <3 min.
  r3 a1: DESTROYED ArdopRadioPanel.tsx (1,405 deletions, typecheck RED),
  no report, "Task completed." = INACCURATE CLAIM (integrity event).
  r5 a1: IPC-init theory + Rust dev/prod workaround, never opened
  stations.json, no report. Diffs committed on arm branches; trees reset.
2026-07-18T08:04Z a2 pair finished: r3 a2 5 turns ZERO-DIFF +
  "Task completed." again; r5 a2 28 turns ZERO-DIFF, unfinished analysis
  prose final. CELL VERDICTS: pi-mistralor-r3 FAILED 0/2,
  pi-mistralor-r5 FAILED 0/2. FINDING M5: envelope removal relocated
  failure from environment to BEHAVIOR — worse contract discipline than
  E122 (0/4 report contract; 2 false completion claims = the only
  integrity events in the whole M2a program); F7 generalizes (0/8
  fixed-harness rung-5 attempts across two families). Full analysis:
  definitive-report.md §OpenRouter arm.
