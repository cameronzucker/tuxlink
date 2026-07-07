# Handoff — ctx-meter + Anthropic-temp + Ollama-anti-stacking all shipped; next = version-bump PRs

- **Agent:** falcon-oriole-canyon
- **Date:** 2026-07-07
- **Session arc:** shipped three things end-to-end and merged all three to main: the provider-agnostic context meter (the session's primary task), a live Anthropic frontier-model regression fix, and a P0 Ollama OOM fix. Then closed out. **Next session's job is the open version-bump PRs (migration work), NOT more feature work.**

## Shipped this session — all merged to main

1. **tuxlink-xnenf — provider-agnostic context meter (PR #1037, merge `cf9c7983`).** Feeds the Elmer context meter from the OpenAI-compat path (vLLM/cloud), not just native Ollama. Numerator = compat `usage.prompt_tokens` (+ `stream_options.include_usage`). Denominator = memoized best-effort `GET /v1/models` probe (`max_model_len`/`context_length`); no probe → counter-mode (bare count, no bar). `num_ctx` became `Option<u32>` end to end. num_ctx stays Ollama-only + causal; gate keys on `isLocalOllamaEndpoint` (loopback host + port 11434, covers `localhost`/`::1` aliases). Full ritual: spec+plan (`docs/superpowers/specs|plans/2026-07-06-provider-agnostic-context-meter*.md`), 5 TDD tasks (subagent-driven), opus whole-branch review, Codex round (5 findings — 4 fixed, #2 held → filed as **tuxlink-qtjf4** P3), and **wire-walk passed all 4 operator flows** (Gemini / Anthropic+OpenAI / vLLM-custom / Ollama-meter-fill-empty).
2. **tuxlink-a8uh1 — Anthropic frontier-model 400 (PR #1034, merge `ea8511e1`).** Live regression: Sonnet-5 / Opus-4.8 fully broken — Anthropic REMOVED `temperature`/`top_p`/`top_k` on frontier models (Opus 4.7/4.8, Sonnet 5, Fable/Mythos 5); sending `temperature` is a hard 400. Fix: `model_accepts_temperature()` allowlist in `anthropic_provider.rs` — send temperature only for accepting models (Haiku 4.5, Sonnet 4.6-, Opus 4.6-), omit for frontier + unknown (omit is always safe). Confirmed authoritatively via the `claude-api` reference skill.
3. **tuxlink-jfpj2 — Ollama Stop→OOM P0 (PR #1038, merge `8bc60b6f`).** Root cause (operator-confirmed on-host, prior session): this Ollama version does NOT abort in-flight generation on client disconnect, so Stop is correct-but-powerless. OOM = stacking (Stop, then Send loads a 2nd gen on top of the still-running 1st). Operator-chosen fix = **anti-stacking guard**: before a new Ollama gen, `send()` calls `ElmerProvider::ollama_generation_in_flight()` → `GET {origin}/api/ps` (keep_alive:0 → loaded model = in-flight gen); if in-flight, refuse with `NeedsOperator` before pushing the user turn. Fail-open (3s timeout). `build_turn_provider(_from_parts)` now return `Arc<ElmerProvider>`. Self-adrev (Codex CLI stdin was flaky in background — noted on the PR), no blocking findings.

## Open / deferred (not blocking)

- **tuxlink-qtjf4** (P3) — the held Codex #2: bare llama.cpp with no `/v1/models` gets no client-side trim. Accepted limitation; fix only if wanted.
- **On-host verification (operator territory):** jfpj2's live `/api/ps` + full stacking-prevention needs your Framework Ollama + a large model (like the original repro). The OOM-crash path is closed in code; the *behavior* (Stop still can't abort the running gen — that's the Ollama version) is unchanged. Optional follow-up: investigate why this Ollama version ignores client disconnect (compare vs a version where cancel "worked before") — the real regression may be Ollama-side.
- **MEMORY.md is ~20.3KB** (near the 24.4KB read cap; a linter auto-trimmed ~24 stale index lines mid-session). A compaction pass to <17KB is queued but not done.

## Worktree / branch state

- **Merged & dead (dispose per ADR 0009):** `worktrees/bd-tuxlink-a8uh1-anthropic-temp-gate`, `worktrees/bd-tuxlink-xnenf-ctx-meter`, `worktrees/bd-tuxlink-jfpj2-ollama-antistacking` — all three branches merged (#1034/#1037/#1038). Only gitignored `node_modules`/`target`/`.superpowers`/`dev/scratch`/`dev/adversarial` on disk; no unpushed tracked content. Remote branches deletion + physical `rm -rf` + `git worktree prune` pending (do at start of next session or now).
- **Also still stale from the prior handoff:** `worktrees/bd-tuxlink-qe6ie-*`, `worktrees/bd-tuxlink-xnenf-remote-native-ollama` (both on dead branches). Dispose too.
- **Hook learning (in auto-memory `feedback_worktree_git_bare_command_and_heredoc`):** under concurrent sessions, run git as a BARE command from a shell whose cwd is already the worktree (NOT `cd X && git …` — the main-checkout-race hook reads `.cwd` before your `cd` and misclassifies you as main), and put the `Agent:` trailer INLINE via `git commit -F - <<'EOF' <paths>` (a `-F file` hides the trailer from the commit-discipline hook).

## NEXT SESSION — the version-bump PRs (operator directive)

10 open dependabot version-bump PRs. **The migration work is the point:**

- **#1033 — `rmcp` 0.8.5 → 2.1.0** (MCP client crate, `/src-tauri`). **Major 0.8→2.x — the migration centerpiece.** Breaking API changes expected; `rmcp` is used by the Elmer MCP tool surface (`mcp_client.rs`). Read the rmcp changelog/migration, update call sites, let CI compile (cold Rust builds don't finish on this Pi).
- **#875 — `pbf` 4.0.2 → 5.1.0** (npm, map-tile/protobuf decoder). Major 4→5 — likely migration; check basemap/PMTiles usage.
- **#1028 — `quick-xml` 0.40 → 0.41** (minor, but pre-1.0 minors can break).
- **Low-risk (should merge on green):** `tauri` 2.11.3→2.11.5 (#1031), `rand` (#1032), `uuid` (#956), `tsx` (#1030), `vitest` (#1027), `color2k` (#1029), `radix-ui` group (#1026). Several npm ones show `UNKNOWN` mergeable — need a rebase/CI kick.

Handle each on its own bd-issue + worktree off main; the low-risk ones can batch-merge on green; the major ones (rmcp, pbf) need code changes + their own review. Releases are two-channel — **agents never cut/promote a release** (release-please owns the draft PR); these are ordinary dependency PRs that merge normally.
