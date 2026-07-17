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
