# Handoff — 2026-07-08 — hawk-kingfisher-spruce (session 2)

**pf6re is BUILT end-to-end and CI-green; it awaits the OPERATOR's merge (I can't
self-merge — two-party review). Releases are FROZEN so the whole agent-operability
cluster lands together in 0.86.0. The next cluster item (7ppfq perception) is
queued on a fresh branch, ready to build.**

## pf6re — DONE, PR #1053, ready for operator merge

Graceful egress denial + arm/taint perception (the `-32600` turn-kill from the
live armed test). All 7 tasks built, CI-green on amd64+arm64 (`verify` = clippy
-D warnings + full tests + typecheck; `build-linux`; `.deb` install tests). Local:
typecheck 0, vitest 13/13. Wire-walk gate passed (3 flows traced).

- T1 content-free `TaintReason` on the egress guard (security crate dependency-free).
- T2 `server_info` exposes `taint_reason`; description teaches taint-dominates-arming.
- T3 cause-split denial remedy (taint says "re-arm DISCARDS the conversation", not
  a false "resume"; expiry says "ARM, then continue").
- T4 **core fix**: runner narrates instead of killing the turn; one-shot bounded
  finalization (exactly one narration turn; tainted/injected model gets zero
  post-denial working window); durable `RunEvent::ToolDenied`.
- T5 frontend `denied` chip (flips the in-flight chip, persists) + amber styling.
- T6 re-arm UI discard-truth — verified already present.
- T7 `agents-guide.md` taint semantics corrected.

**Invariants held:** egress lock absolute, `injection_tests.rs` untouched,
`-32600` wire + `Denied` classification unchanged.

**MERGE STATE:** `MERGEABLE`. A merge conflict with main (main restructured the
runner loop: tool-turn count cap → wall-clock `max_response_duration` budget) was
RESOLVED — `denial_final` and `start` coexist; pf6re's narrate-not-kill merged
cleanly on top of the new budget model. CI is re-running post-merge-resolution.

**➡️ OPERATOR ACTION: once CI is green, merge #1053** —
`gh pr merge 1053 --merge` (no-squash per ADR 0010; skip `--delete-branch`).

## Release freeze — ACTIVE (lands with the #1053 merge)

`.github/RELEASE_FREEZE` is on the pf6re branch → active the moment #1053 merges.
release-please keeps accumulating the cluster into the pending 0.86.0 release PR;
the nightly release-merge cron + promotion are paused. **Operator un-freezes
(deletes the file) once the whole cluster has landed** → next nightly cuts 0.86.0
with everything.

## 7ppfq perception — QUEUED, ready to build

- Branch `bd-tuxlink-7ppfq/perception` off origin/main; worktree
  `worktrees/bd-tuxlink-7ppfq-perception` (deps installed, build-ready). bd
  tuxlink-7ppfq is `in_progress` and claims it.
- **The old `bd-tuxlink-7ppfq/session-handoff` branch is the pf6re PR — do NOT
  build perception there.**
- **Design + adrev already DONE** (do not re-adrev): cluster spec Contract 1
  (reachability + read-only `vara_probe`) + Contract 2 (active-modem SoT), and the
  full 5-round adrev corrections in tuxlink-7ppfq's bd `--notes` (try_lock probe,
  bare cmd-port connect ≥5s, read-only deep probe, ARDOP running from
  `ModemState`/`snapshot_transport_present` — test via `modem_ardop_connect`, reuse
  `activeConnection` + persist at the state transition, `CONFIG_SCHEMA_VERSION`
  5→6, `kind=running`, MCP `VaraStatusDto`, deduped `useActiveModemMode`).
- Build via TDD → PR → CI (Pi can't compile Rust) → wire-walk gate. `vara_start` is
  SPLIT to u269g (not in scope).

## Remaining cluster (all filed)

- tuxlink-z2nwx (P2) — print (CUPS) + report export. **Forward flag: a NETWORK
  printer is a real egress side-channel — gate it when built.**
- tuxlink-77seh (P2) — audio-device inspection + VARA setup guidance. HW grounding:
  FT-710 = DRA-100 audio + separate FT-710 USB for CAT/RTS PTT (digital preset).
- tuxlink-u269g (P2) — vara_start (local-vs-remote launch; own brainstorm).
- tuxlink-etjp9 (P2) — predict_path runaway. tuxlink-iicsh (P2) — listen + cooldown.

## Worktree / tree state

- `bd-tuxlink-7ppfq/session-handoff` (pf6re, PR #1053): pushed, mergeable, CI running.
- `bd-tuxlink-7ppfq/perception`: fresh off main, deps installed, this handoff on it.
- No stashes. All work pushed.

## Pending decisions

- Operator: merge #1053 (green CI); un-freeze after the cluster lands.
- Next session: build 7ppfq perception from the queued spec.
