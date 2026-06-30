# Handoff — Elmer model-config feature SHIPPED (merged to main)

**Date:** 2026-06-29 · **Agent:** oriole-magnolia-condor · **bd:** tuxlink-1wi5w (CLOSED) · **PR:** #966 (merged, `99aa2366`)

## One-sentence frame
The Elmer model-config feature (`tuxlink-1wi5w`) was executed end-to-end this session via `superpowers:subagent-driven-development` — all 17 plan tasks, per-task review, five Codex adversarial rounds, CI-green on both arches, final whole-branch review (Ready to Merge), an operator wire-walk, and merge to main.

## What shipped
Operator can connect Elmer to any OpenAI-compatible model (local Ollama or cloud) from **Tools → "Set up Elmer's model…"**, key in the OS keyring, SSRF-guarded egress, live-apply to the next turn (no restart). The arm/taint transmit gate is untouched; config commands are Tauri-only (never MCP tools).

- **Backend (Rust):** `AgentEndpoint` (relaxed egress + userinfo/metadata refusal), `egress::build_vetted_client` (SSRF/DNS-rebind guard, redirect-none/no-proxy/IP-pin, permits public+RFC1918, denies loopback-unless-literal/link-local/metadata/multicast/unspecified/0.0.0.0-8), `ApiKey` redacting newtype + `redact_and_cap` (scrub-before-truncate), origin-keyed fail-closed keyring, `ElmerProvider::new_vetted`, `ElmerModelConfigState` async lock + per-turn `build_turn_provider` (NeedsOperator on failure, never panics; keyring read only on `!is_loopback` via spawn_blocking), `elmer_config_read/set/detect_models` (transactional key-first set, fixed 401 reason, derived /models URL re-validated through the gate), MCP-boundary + prompt-injection regression corpus.
- **Frontend (TS):** the Model form (preset/endpoint/`[Replace]/[Remove]` key affordance never `••••`-seeded/model+Detect/Save), empty-state "Connect a model" button, loopback/preset-keyed detect remedies, per-turn attribution marker, the additive menu door + `expandModel` prop.

## Validation (all green at merge)
- CI on HEAD `5068397e`: clippy `--all-targets -D warnings` + full `cargo test` + `pnpm build`, **both amd64 + arm64**.
- Final whole-branch review (opus): **Ready to Merge** — all 7 integration seams sound (serde contract, cross-language `origin()`, end-to-end credential path incl. detect-origin, transmit-gate isolation, live-apply single shared Arc, MCP boundary, no drift/stubs).
- **5 Codex rounds** (egress · keyring · live-apply · injection · final integration); every finding fixed, including: localhost named-host egress; scrub-before-truncate + 0.0.0.0/8; command-path keyring fail-closed/spawn_blocking; `SetKey`/`KeySource` type-level Debug redaction; F2 all-withheld-tools loop + arm-gate isolation; and the final round's **2 frontend credential-seam bugs** (key action + Detect not bound to the endpoint origin).
- Local: typecheck clean + **3418/3418 vitest** (whole app, no regressions).
- **Wire-walk (operator flows):** both primary flows traced to file:line — (1) local Ollama gpt-oss-20b and (2) cloud Haiku — menu→form→`elmer_config_set`→live snapshot→`elmer_send`→`ElmerSession::send`→`build_turn_provider` (live config)→`run_with_conversation(provider, invoker)` with the `TuxlinkMcp` tool surface. Plumbing proven; **live model behavior is operator-runtime-validated** (the static trace can't assert the model answers correctly / picks the right tool).

## CI cascade note (for future Rust-on-this-Pi work)
The Pi cannot compile Rust, so CI is the only clippy/cargo signal. CI uses a newer toolchain (clippy 1.96) than MSRV 1.75, and `clippy --all-targets -D warnings` surfaced a CASCADE — each error masked the next (clippy aborts the lib build before the test targets): `manual_strip` → `doc_lazy_continuation` → `Debug`-on-provider in test asserts (`{x:?}`/`expect_err` on `ElmerProvider`/`dyn Provider`) → missing `use ToolInvoker` (trait methods) → `manual_contains`. Took 5 CI rounds. **Lesson:** when a Rust task pushes and clippy fails, fix ALL flagged lints AND proactively grep the new test code for the recurring lint classes (manual_strip/manual_contains, Debug-format on non-Debug types, trait-method-needs-`use`) before the next push.

## Follow-ups (filed / noted, all non-blocking)
- **bd tuxlink-te6vl** (P3): remove now-dead `ElmerProvider::new` + `LoopbackEndpoint` (pub, no production caller post-migration; migrate the 2 smoke tests then delete).
- **bd (d3zwe egress)** (P2, filed earlier this session): migrate the d3zwe headless spine to `build_vetted_client` (still legacy `validate_endpoint + --allow-remote + default client`; out of this plan's scope).
- Minor logged (not filed): IPv6 `::1` origin vector to add to the A1↔G1 cross-language test table; the `elmer-detect-zero` success branch may be dead (backend rejects on zero models); `useElmer.activeModel` exposed-but-unused; `configSet` doesn't refresh `modelConfig` post-save; H1 `expandModel` lazy-init reads at mount only.

## State
- Branch `bd-tuxlink-1wi5w/elmer-model-config` is **merged-dead** (PR #966 landed; do not commit to it). Worktree at `worktrees/bd-tuxlink-1wi5w-elmer-model-config/` retains gitignored dev scratch: `dev/adversarial/2026-06-29-elmer-{g1,g2,g3,g4,final}-*-codex.md` (the 5 Codex transcripts) + the SDD ledger at `.superpowers/sdd/`. The worktree can be disposed per ADR 0009 when convenient.
- bd `tuxlink-1wi5w` CLOSED; synced via `bd dolt push`.

## Operator next action
Run the two wire-walk flows against a **live model** (Ollama gpt-oss-20b loopback; then a cloud OpenAI-compatible endpoint) to validate the model-behavior layer the static wire-walk can't prove: that the model answers via the Elmer pane AND correctly drives Tuxlink MCP tools (e.g. "Where is N7CPZ-7? When last heard?"). Fix-forward anything the live run surfaces.
