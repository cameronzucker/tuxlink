# Handoff — Elmer model config: spec + plan ready, build NOT started

**Date:** 2026-06-29 · **Agent:** redwood-falcon-bluff · **bd:** tuxlink-1wi5w (in_progress)

## One-sentence frame
This session shipped the merged Elmer×Agent-send ribbon control (PRs #952 + #964, both **merged to main**), then took the **Elmer model-config feature** (`tuxlink-1wi5w`) through the full build-robust-features pipeline up to — but **not including** — execution: brainstorm + approved mock → spec → **5-lens adversarial review** → hardened spec (Rev 2) → **implementation plan** (reviewed + tightened) → menu decision resolved. The next session **executes the plan** via subagent-driven-development.

## Branch / tree state
- Branch `bd-tuxlink-1wi5w/elmer-model-config` (off current main, which includes #952 + #964), **pushed**. No PR yet (build hasn't started → no code).
- Commits on the branch: spec (`8404944d` Rev 2 hardening), plan (`531d32a8` + tightenings), menu fix (`6f4626d2`).
- Worktree: `worktrees/bd-tuxlink-1wi5w-elmer-model-config/` (repo root; relocated from a nested path the create-script produced — that's fixed). `node_modules` installed.
- **Gitignored-but-on-disk** in this worktree (NOT pushed, local-only reference): `dev/adversarial/2026-06-29-elmer-model-config-consolidated.md` (the 5-lens dispositions) + `…-codex.md` (raw Codex transcript); `dev/scratch/elmer-model-config-mock.{html,png}` + `font-probe.*` + `elmer-merged-control-mock.*`.

## What's DONE
- **Merged ribbon control** (Elmer launcher + Agent-send fused into one ✦ chip; arm relocated to the drawer header): PR #964 **merged**. Root cause of the 1080p crowding was investigated (not a font bug — the Pi loads the intended Inter; the ribbon was just too dense for a second control). Operator confirmed it **looks good on R2**; the **Pi converged-build confirmation was still pending** when the session ended (operator was recompiling).
- **Model-config feature, design+plan complete:**
  - Spec: `docs/superpowers/specs/2026-06-29-elmer-model-config-design.md` — **read Revision 2 in full** (R2.1–R2.7 are the binding contracts; they SUPERSEDE the original §Backend).
  - Plan: `docs/plans/2026-06-29-elmer-model-config-plan.md` — 22 tasks / 6 groups, TDD, sequenced for file-conflict safety, with a verification matrix + per-group review loops + a final wire-walk.
  - 5-lens adrev (Codex + SSRF + credential + live-apply + UX) — every lens found real P0/P1s; all ADOPTED except the cleartext-`http` note (operator: **no note**).
  - **Menu decision (resolved):** `ConnectAgentModal` ("Connect an AI agent…") is the external-agent MCP-connect helper — a **different** feature, **NOT** retired. Add a distinct `menu:tools:elmer_model` "Set up Elmer's model…" in the Tools AI grouping (purely additive). H1 reflects this.

## What's NEXT (the build — NOT started)
Execute the plan with **superpowers:subagent-driven-development**. Key constraints baked into the plan:
- **The Pi cannot compile Rust** — Rust tasks verify via **CI** (push the branch; `cargo clippy --all-targets -D warnings` + `cargo test` on both arches). TS/vitest runs locally per-file. MSRV 1.75 (no `inspect_err`).
- **Sequencing matters:** Group 1 (egress) before Group 3; **Group 3 is STRICT serial** (C2→C3→E1→E2→D1→D2→D3 — they share `provider.rs`/`session.rs`/`lib.rs`); B1 is the only Rust task safely parallel; frontend Groups 5/6 mock `invoke` so they can be authored alongside the Rust merge.
- **Do NOT touch** the arm/taint TRANSMIT gate (`EgressGuard`, `quarantine_and_rearm`, `WITHHELD_EGRESS_TOOLS`). The egress relaxation here is the **model endpoint** only.
- The codebase is well-positioned: `validate_endpoint(.,allow_remote)` already exists (metadata literals already refused), `OpenAiProvider::new` already takes an injected client + `Option` key, `identity/service.rs` has the 3-state keyring pattern to mirror, `tiles/host.rs`+`fetch.rs` have the SSRF infra to ADAPT (their permit-set is inverted — copy the pattern, write Elmer's own `egress.rs`).

## Pending / watch
- **Pi converged-build confirmation** for the merged control (#964) — operator was recompiling; confirm Connect fits at 1080p (it does on R2). Non-blocking.
- bd `tuxlink-1wi5w` stays **in_progress** (build pending).
