# Handoff — cnz5o shipped + walked, jfpj2 P0 root-caused, Elmer provider-config bug 1 fixed

- **Agent:** delta-basil-fen
- **Date:** 2026-07-06
- **Scope:** long session. Built + validated tuxlink-cnz5o (sim harness), root-caused the Elmer Stop/OOM P0, diagnosed the Elmer provider-config regression cluster, fixed one of them, documented keyring access.

## Where the NEXT session starts

**Start on remote native Ollama (tuxlink-xnenf).** The operator explicitly wants it ("common way for people to run self-hosted on-prem models") and it's the real stability win — it unblocks the context meter + num_ctx for the operator's remote Ollama (Framework over tailnet). It is **security-sensitive** (crosses the loopback SSRF/egress boundary — design the allowlist carefully). It also naturally carries BUG 3 (provider-kind visibility) since it reworks the same provider-selection code path. Do NOT start with more cnz5o A/B runs — those are done (see below).

Second priority is the P0 (tuxlink-jfpj2) if the operator wants it: bound generation + block stacking, plus the operator checks `ollama --version`.

---

## Workstream 1 — tuxlink-cnz5o (Tuxlink-as-simulation-harness): DONE + validated

Full pipeline: brainstorm → ADR 0021 → **5-round Codex adrev** (materially reshaped the design; caught the parity-claim overstatement) → subagent-proof plan → full build.

- **Branch/PR:** `bd-tuxlink-cnz5o/sim-harness-poc`, **PR #1019**, **CI green both arches** (SHA b96123fd + later a0eb7931). Worktree: `worktrees/bd-tuxlink-cnz5o-sim-harness-poc`.
- **Built:** Rust testserver scenario-driven ports over the real MCP router (fixtures, JSON-schema, CI grep-gate), d3zwe `--json` + configurable `D3ZWE_TURN_TIMEOUT_SECS`; Python `Scenario.world` + the content-grounding judge + contract diff + A/B harness.
- **Verified:** Python 58 tests, Rust 37 tests + clippy `-D warnings` clean, CI green.
- **Wire-walk = the A/B, run API-based via OpenRouter (gpt-4o-mini):** grounded arm cited the real seeded gateways (0 fabrication); void arm honestly declined (0 fabrication) → decision "NO-GO" is a **model artifact** (a well-grounded model doesn't fabricate, so there's nothing to eliminate), NOT a harness failure. Harness validated end-to-end. The fabrication *number* is better measured downstream in the real distillation eval, not via synthetic A/B — do not chase it further.
- **Mergeable** on operator say-so. Wire-walk gate satisfied by the live A/B.
- **Design corrections folded into ADR 0021:** parity is shape/schema + real-guard, NOT business-logic (curation lives in Monolith*Port below the seam); logic-parity = below-seam injection, future work.
- **Two worktrees exist:**
  - LIVE: `worktrees/bd-tuxlink-cnz5o-sim-harness-poc` (branch `bd-tuxlink-cnz5o/sim-harness-poc`).
  - DEAD: `worktrees/bd-tuxlink-cnz5o-tuxlink-as-sim-harness` (branch merged via PR #1017). ⚠️ It still holds kingfisher-cove-yew's **uncommitted staged edit to `docs/notes/2026-07-05-choosing-a-distillation-teacher.md` (192+/146−)** — real orphaned work; inspect/rescue before disposing (ADR 0009 ritual).

## Workstream 2 — P0 tuxlink-jfpj2 (Elmer Stop / Ollama OOM): root-caused, escalated

- **Confirmed on-host** (clean d3zwe repro, single 35b run): cancel **works but is slow** — Ollama is slow to abort generation on client disconnect (qwen3.5:35b stayed generating ~48s+ after SIGINT + process exit). Tuxlink's cancel plumbing is correct. OOM mechanism = Stop looks done but the generation keeps running; a repeat Send **stacks** a second generation → OOM.
- **Not the code-only hypothesis** (streaming switch 8c37c2a4 is likely NOT the root — Ollama ignores disconnect regardless of stream mode).
- **Fix direction (operator decision, not implemented):** client-side = bound `num_predict` + block a new Send while a gen is in-flight (poll `/api/ps`); Ollama-side = check the Framework's `ollama --version` (the real regression may be Ollama's disconnect handling). No Ollama cancel API exists; connection-close is the only client lever.
- **My mistake, owned:** my orphaned A/B `d3zwe` loops were hammering the Framework's Ollama concurrently and contributed to the crash you hit. Killed; nothing of mine touches any host now. Lesson logged: clean up child processes; don't run heavy concurrent inference on a shared host.

## Workstream 3 — Elmer provider-config regression cluster

Investigated holistically. **They are INDEPENDENT root causes, not one refactor:**
- **tuxlink-lubim (BUG 1) — FIXED, PR #1022:** tile switching carried stale fields. `ModelTilePicker` keys `GetKeyCard` (remount+reset) but not `ModelForm`; the un-keyed ModelForm (`useState`-once) kept stale endpoint/model/num_ctx across the localOllama/openrouter/custom tiles. Fix: `key={selectedPreset?.id ?? 'custom'}` on ModelForm. Regression test added (fails without the key, passes with it); typecheck + 304 elmer vitest green. Branch `bd-tuxlink-lubim/elmer-tile-switch-stale`, commit 97a18386.
- **tuxlink-xnenf (BUG 2/3 + the feature) — NEXT SESSION:** the context meter + num_ctx were built **local/native-Ollama-ONLY by design** (plan tuxlink-65qhn, D6). `RunEvent::ContextUsage` is emitted only by `OllamaProvider`, which is built **only for loopback endpoints** (`src-tauri/src/elmer/provider.rs:246` is_loopback gate — an SSRF/DNS-rebind guard). The operator runs Ollama on the **Framework (remote, non-loopback)** → always falls to OpenAI-compat → no meter, no num_ctx, even though the backend IS Ollama. **Operator decided: build remote native Ollama** — opt-in native path for an operator-designated non-loopback host + SSRF allowlist + persist a tile-kind flag (don't re-derive from isLoopback). Fold in BUG 3 (add `providerKind` to `ConfigReadDto` from the real probe result + a "Provider: Ollama (native)" badge; fixes the silent frontend(string-isLoopback)/backend(isLoopback+probe-fallback) mismatch). Un-bury num_ctx into the in-pane "Endpoint / model" disclosure (currently form-less).
- **tuxlink-1zh3g** (invisible "New conversation" SVG): CSS/WebKitGTK glyph paint, unrelated. **tuxlink-s587** (WX chip): map migration, unrelated.
- **Test gap** across this surface: no end-to-end test that a provider emitting nothing → meter absent; the isolated ContextMeter test uses a stand-in wrapper. Add real coverage when doing xnenf.

## Keyring / OpenRouter access — SOLVED + documented

The OpenRouter key for headless runs is in the OS keyring under a **hand-stored** entry (NOT the app's ElmerKeyring `service=tuxlink` namespace):
```
secret-tool lookup service elmer-openrouter account teacher   # sk-or-... (73 chars)
```
Documented in auto-memory `reference_openrouter_key_keyring_access.md`. Use a **single targeted lookup**, never broad `secret-tool search --all` (the auto-classifier blocks enumeration — flagged twice). Grep `dev/handoffs/` + memory FIRST for the documented command.

## Issues filed this session

- **tuxlink-cnz5o** (feature) — sim harness. DONE, PR #1019.
- **tuxlink-jfpj2** (P0 bug) — Elmer Stop/OOM. Root-caused, escalated.
- **tuxlink-lubim** (bug) — tile-switch stale fields. FIXED, PR #1022.
- **tuxlink-xnenf** (feature) — remote native Ollama + provider visibility. NEXT SESSION.
- **tuxlink-1p2as** (bug) — context UI. Clarified (local-only by design); resolve via xnenf.
- **tuxlink-fr25d** (feature) — message window renders Markdown by default + raw toggle.
- **tuxlink-1zh3g** (bug) — invisible "New conversation" button (glyph paint / WebKitGTK).
- Closed **tuxlink-grg1i** (won't-do: refusal-restraint training is not a goal); corrected **tuxlink-0mudm** scope (grounded-honesty ≠ injection refusal).

## Branch / worktree / push state

- `bd-tuxlink-cnz5o/sim-harness-poc` — pushed, PR #1019, CI green. Worktree live.
- `bd-tuxlink-lubim/elmer-tile-switch-stale` — pushed, PR #1022, CI running. Worktree live (this handoff is committed here).
- Main checkout: operator state (bd-tuxlink-ant8s/ardop-connect-fixes, 88 uncommitted — untouched).
- SSH tunnels + all my d3zwe/testserver/A-B processes: killed. Nothing of mine on the Framework or R2.
- Dead cnz5o worktree with orphaned docs/notes change: see Workstream 1 — dispose per ADR 0009 after rescuing the change.

## Ops learnings

- R2 (N305) rustc 1.75 cannot build the workspace (edition2024 deps); the Pi's cargo 1.96 builds the small crates in ~2-4 min with a warm target; CI is the Rust gate. See memory `reference_inference_hosts_r2_framework`.
- N305 too slow for agentic local inference (120s/turn wall); Framework is the inference host (no SSH; network Ollama). API-based (OpenRouter) is fastest + off-host for A/B.
