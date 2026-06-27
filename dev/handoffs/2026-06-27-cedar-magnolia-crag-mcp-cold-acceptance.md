# Handoff — MCP epic COLD ACCEPTANCE passed; one real verify bug + follow-ups

Date: 2026-06-27 · Agent: cedar-magnolia-crag · Epic: tuxlink-cvx84 · bd issue: tuxlink-l9sq4

## TL;DR
The Tuxlink MCP epic **passes cold tier-3 acceptance**, validated live against the
running converged app (not mocks). Drove the full EmComm flow over the real MCP:
`find_stations`, `predict_path` (24h VOACAP), `solar_conditions`, compose
(`message_send`), and the **arm-gate round-trip** (GUI arm → MCP egress allowed)
all work. One real defect found (`sswik`), plus smaller follow-ups. A long
identity-"locked" rabbit hole turned out to be a **misdiagnosis** — corrected and
retracted (details below) so the next session doesn't re-chase it.

## What was validated live (against the real backend, app pid built from `origin/main` @ 8d01533a)
- Transport + `tuxlink://agents/guide` read-first (47 tools, 19 resources, 3 prompts).
- **PR #924 runtime-dir fallback exercised for real**: `/run/user/1000` is `0770`
  (not private), so the socket correctly landed at `/tmp/tuxlink-1000/tuxlink/mcp.sock`.
- `find_stations` (WARC band filter correct), `predict_path` (real 24h REL/SNR/MUFday
  VOACAP — 30m reliable 24h, 17m daytime — physically correct), `solar_conditions`
  (bundled SSN), `message_send` (staged `2HEZMKEAQNMV`).
- Security model holds live: unarmed `cms_connect` denied → operator armed in GUI →
  armed `cms_connect` "ok". Taint model understood + respected.
- The operator's prior EmComm scenario **was** the greenfield wire-walk flow (came
  cold via the handoff). Traced + executed end-to-end → **passes**.

## How the acceptance was driven (reusable)
- Built the converged app from a worktree off main (`bd-tuxlink-l9sq4`,
  `pnpm dev:converged`). Cold build ~12 min; `predict_path` needs the prediction
  engine staged (see `0ljqd`).
- Drove the MCP over its UDS via the prebuilt shim
  (`worktrees/bd-tuxlink-cvx84.5-mcp-knowledge/src-tauri/target/debug/tuxlink-mcp`,
  which is byte-identical to main — the shim hasn't changed since 3.1) piping
  newline-delimited JSON-RPC. `claude mcp add` registered it but the health-probe
  shows "failed" (the shim is a per-connection pump that exits 0 on EOF); the
  pipe-the-shim harness is reliable.

## Real findings filed
- **`tuxlink-sswik` (P2, bug) — THE real defect.** MCP `verify_cms_connection`
  (mcp_ports.rs:829) is wired to `wizard::verify_cms_connection_impl`, which builds
  an **ephemeral** `NativeBackend` (wizard.rs:504) with no active identity → it
  returns `NoActiveIdentity` even when the live session is authenticated and sending.
  `cms_connect` (live `ui_commands`) is fine. Fix: agent-surface verify must report
  the LIVE backend state, not the wizard's throwaway probe.
- **`tuxlink-5xdrx` (P2, bug)** — `TEST-CALL` activation-secret entries in the
  operator's REAL OS keyring (service `tuxlink`), one created **during this session
  (06-27 12:59)**. Tests are exercising the real keyring backend. Isolate test
  keyring + clean up the stray `TEST-CALL` entries on the dev host.
- **`tuxlink-0ljqd` (P3)** — pristine-main `pnpm dev:converged` doesn't stage the
  HF-prediction engine (voacapl + itshfbc); `release.yml` does at bundle time, and
  the operator's local converge-build.sh has a `stage_prediction_engine` step that
  was never merged. Land it on main so dev `predict_path` works out of the box.
- **`tuxlink-supqp` (P3)** — re-add `last_update_ms` (parsed, structured) to
  `find_stations` gateways for last-heard recency.
- **`tuxlink-njcxj` (P3)** — emit a user-visible `emit_backend_line` on bootstrap
  `AutoAuth::Unavailable` (currently only `tracing::warn!`, unlike the Healed/
  HealFailed arms). Reliable backend signal for a genuinely-locked identity.

## The identity "locked" misdiagnosis (DO NOT re-chase)
I read MCP `verify_cms_connection`'s "no active identity" as the live identity being
locked, and built a wrong multi-layer theory (activation-secret desync → silent
auto-auth failure → a closed-chip "locked" indicator, shipped as PR #928). The
operator's ground truth — "N7CPZ sends via CMS-Z right now, it's not broken" — was
correct: the signal came from the mis-wired ephemeral verify backend (`sswik`), not
the live identity. **Retracted:** closed the misdiagnosed bug (`ezd5m`), **closed
PR #928 + `tuxlink-xzidc`** (the locked-chip also had a real startup-race
false-positive surface: `identity_active` is fetched once with no auto-auth-settled
refetch). The proper version is `njcxj` (backend emit, no race). Lesson: when two
signals from the same system disagree (`cms_connect` ok vs `verify` fails, same
session), investigate the contradiction before building on one.

## Codex cross-vendor rung (cvx84.11 experiment, part 2)
Comprehension **validated cross-vendor**: Codex/GPT-5.5, same EmComm task, read
`agents/guide` first, went to the band-plan reference (validates the guide edit),
and correctly internalized the tier + arm/taint model unprompted, refusing to
hallucinate when blocked. Execution blocked by the operator's *own* Codex config
(`approvals_reviewer = "guardian_subagent"` cancels tuxlink tool calls;
`trust_level` override didn't bypass it). Sonnet + smaller rungs still pending →
all recorded in `cvx84.11` for a dedicated session.

## State
- **Branch `bd-tuxlink-l9sq4/mcp-cold-acceptance` is DEAD** (PR #928 closed
  unmerged). Worktree `worktrees/bd-tuxlink-l9sq4-mcp-cold-acceptance/` is on it +
  is disposable per ADR 0009 (guide edit preserved on the branch below; nothing else
  tracked-dirty). Its `.local/converge-build-worktree/` is a stale disposable build.
- **Guide edit lives on `agent-cedar-magnolia-crag/agents-guide-domain-routing` →
  PR #930** (clean, docs-only, carries this handoff). Merge when CI is green.
- Live converged app (operator's relaunch, from the main checkout's
  `.local/converge-build-worktree`) was running on `/tmp/tuxlink-1000/.../mcp.sock`;
  it went unresponsive late in the session (socket FD present, no replies).
- bd: all findings above filed; `ezd5m` + `xzidc` closed; `cvx84.11` enriched.

## Open epic gates (cvx84 stays in_progress)
`sswik` (verify bug) · `cvx84.7` (packet UA-emit gate) · `cvx84.11` (agent ladder) ·
`0ljqd`/`supqp`/`njcxj`/`5xdrx` follow-ups. Acceptance gate itself: **passed.**
