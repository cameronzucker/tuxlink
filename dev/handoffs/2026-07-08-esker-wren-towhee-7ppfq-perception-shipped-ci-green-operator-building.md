# Handoff — tuxlink-7ppfq perception shipped (PR #1055, CI green), operator building + manual 0.87.0 cut

**Agent:** esker-wren-towhee · **Date:** 2026-07-08

## One-sentence frame

The agent-operability cluster's perception item (tuxlink-7ppfq, Contracts 1+2)
is built, Codex-reviewed, and CI-green on PR #1055; the operator is taking the
merge + a converged build themselves and will do a **manual 0.87.0 cut** when it
lands (the release freeze stays until they un-freeze).

## Branch / worktree / PR state

- **Branch:** `bd-tuxlink-7ppfq/perception` — head `0f5f850b` (pushed).
- **Worktree:** `worktrees/bd-tuxlink-7ppfq-perception` — **clean tree, all committed + pushed**. No stashes. No untracked feature files (the gitignored `dev/adversarial/2026-07-08-7ppfq-perception-codex.md` is the local Codex transcript).
- **PR:** [#1055](https://github.com/cameronzucker/tuxlink/pull/1055) — open, against `main`.
- **CI:** `verify` job **success on amd64 AND arm64** (SHA `0f5f850b`); ECT build success. (Release build is the non-gating packaging path.)
- **Branch is current with `origin/main`** — merged origin/main mid-session (it had advanced: pf6re #1053 merged + `RELEASE_FREEZE` added); the merge restored `RELEASE_FREEZE` and pf6re's denial/taint changes that a stale-base diff had been reverting.

## What shipped (Contracts 1 + 2, perception-only)

Commit trail (after `04c09a11` plan):
- `7038436a` — Contract 1a: `vara_status.reachable: Option<bool>` (try_lock classify; Open/Connecting lean on heartbeat, else bare cmd-port `connect_timeout` TTL-cached ~3s; contended lock → `unknown`, never waits; timeout shared with transport via `build_transport_config`).
- `ebf1e2c8` — Contract 1b: read-only `vara_probe` tool + `transport::deep_probe` (banner / single `VERSION` query; classifies down / socket-not-vara / vara-ok; a test asserts NO stateful setter crosses the wire).
- `a20248a1` — Contract 2 config: `Config.active_connection` + `CONFIG_SCHEMA_VERSION` **5→6** (ulrz-safe: golden set + additive-load test) + `config_set_active_connection` command + `ConfigViewDto` read-through.
- `ccdae4c4` — Contract 2 MCP: `modem_get_status` reports `selected` + `running` + `conflict`, `kind` on the SoT. ARDOP liveness from `snapshot_transport_present()`, **NOT** `active_transport_kind()` (the coverage trap — a trap-guard test proves it). Split `derive_modem_status` (pure) + `gather_modem_status` (session-taking) so it's testable without a Tauri harness.
- `792016d5` — Contract 2 frontend: `useActiveModemMode` (selected protocol drives the panel; falls back to `ardop-hf` for non-radio selections since `useModemIsActive` is ARDOP-specific), persist at the `activeConnection` transition (both writers — hoi1), hydrate from `config_read` on mount.
- `191e75c9` — fix: `active_connection: None` added to all 14 `Config` struct literals (CI E0063; Config has no `Default`).
- `cd387016` — merge origin/main (stale base fix).
- `e9d55da5` — Codex fixes: private-leak lint (`lock_inner_for_test` → module-private), `vara-fm` panel mapping, hydration-race guard.
- `0f5f850b` — test fix: `deep_probe` fake listener loops past read timeouts (the one CI test failure after the merge; 2847/2848 had passed).

Design source: `docs/superpowers/specs/2026-07-08-agent-operability-cluster-design.md` (Contract 1 + 2). Plan: `docs/superpowers/plans/2026-07-08-7ppfq-perception.md`. `vara_start` is SPLIT to tuxlink-u269g (out of scope, deliberately).

## Verification done

- **Frontend:** `pnpm typecheck` clean; full `src/modem` + `src/shell` vitest **582 → 587 tests green** (locally on the Pi).
- **Rust:** Pi cannot compile Rust — **CI (amd64+arm64) is the oracle**; verify green.
- **Codex adversarial review** of the diff: 5 findings, all resolved (see commit trail). Raw transcript local-only (gitignored).
- **Wire-walk:** NOT run with operator-supplied flows. The operator chose to validate reachability via their own converged build (a stronger check than a grep-trace). Code-level seam self-check: `config_set_active_connection` is invoked at `AppShell.tsx:900` (real consumer, not orphan); hydrate reads `active_connection` at `AppShell.tsx:881`; MCP tools are in the `#[tool_router]`. **Not a "done" claim — the operator's build is the gate.**

## In-progress / pending decision (operator)

1. **Merge PR #1055** on CI green (operator doing this).
2. **Converged build + validate** the perception surface (operator).
3. **Manual 0.87.0 cut** when the PR lands — `RELEASE_FREEZE` is intact on the branch and stays until the operator un-freezes (`.github/RELEASE_FREEZE`; per the two-channel release discipline agents never cut/promote).
4. Do **not** `bd close tuxlink-7ppfq` until the operator's build validates.

## Cluster — what's next

Per the design build order (pf6re ✓, 7ppfq ✓):
- **tuxlink-z2nwx** (P2) — print (CUPS) + report export (sandboxed `~/Documents/Tuxlink/reports/`).
- **tuxlink-77seh** (P2) — audio-device inspection surface (ALSA/USB VID:PID/port path/in-use; guidance not a code-side ranking).
- Split-out follow-ups (own brainstorms): **tuxlink-u269g** (`vara_start` local-vs-remote launch), **tuxlink-etjp9** (predict_path runaway), **tuxlink-iicsh** (listen + cooldown).
