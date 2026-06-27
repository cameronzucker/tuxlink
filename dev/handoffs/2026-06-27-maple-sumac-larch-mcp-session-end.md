# Handoff — Tuxlink MCP epic shipped + hardened + station-intel; ready for COLD acceptance

Date: 2026-06-27 · Agent: maple-sumac-larch · Epic: tuxlink-cvx84

## TL;DR for the next session
The MCP server epic is **code-complete on `main`**, security-hardened (two adversarial passes), AGPLv3-relicensed, and now has the station-intelligence read surface + an agent-onboarding guide. **Nothing has had a live full-app round-trip on the operator's machine yet** beyond the tier-2 testserver smokes. Your job: **cold acceptance** — converged rebuild, drive the real MCP against the running app, run the EmComm task, run a *real greenfield* wire-walk, then the queued local-agent experiment. You did NOT build this; that's the point — your read is less contaminated than the builder's.

## Shipped this session (all merged to `main`)
- **6-phase MCP epic** (driven by a hands-off execution harness): 3.1 transport spine (rmcp-over-UDS + `tuxlink-mcp` stdio shim + `server_info`), 3.2 Tier-1 reads (ports-and-adapters, redaction-at-sink, taint), 3.3 egress gate (`guarded_egress`, poison-fail-closed, Operator/Agent, abort), 3.4 Tier-2 writes (armed+taint, validators) + Tier-3 compose, 3.5 knowledge layer (16 resources + 3 prompts), 3.6 operator arm UI. PRs #911/#915/#916/#917/#918/#919.
- **AGPLv3 relicense** (PR #921, tuxlink-tm0cp) — closes GPL's network/SaaS hole the MCP server opens; network-exposable packaging unblocked.
- **Whole-shape security review** (Claude + Codex converged) + **failure-path redaction fix** (PR #923): `redact_freeform` on every error string, all session-log sources, and `backend_status.reason` (echoed `;PQ`/`;PR` could leak on a failed connect).
- **Runtime-dir fallback fix** (PR #924) — the MCP server now **auto-starts even when `/run/user/<uid>` is 0770** (group-writable); it falls back to a hardened private `/tmp/tuxlink-<uid>` dir instead of skipping. (This was a LIVE tier-3 finding — the server silently didn't start on the operator's real machine; tiers 1+2 missed it because the testserver used a 0700 tmpdir.)
- **Station-intelligence surface** (PR #925) — `find_stations` (RMS gateway directory by mode/band, structured-only, injection-y listings dropped), `predict_path` (offline VOACAP 24h REL/SNR/MUFday to a target grid; operator tx_grid injected + 4-char-clamped, agent never supplies it), `solar_conditions` (SFI/A/K + SSN). All inert Tier-1 reads, no gate, no taint. Codex adrev done + remediated.
- **`tuxlink://agents/guide`** (PR #925) — the read-first onboarding resource: what Tuxlink is, the full tool surface by tier, the arm/taint model, typical flows, docs pointer. `get_info()` opens by pointing at it.

## Crates / surfaces
`tuxlink-security` (EgressGuard + guarded_egress), `tuxlink-mcp-core` (ports/DTOs/validators/router/content/transport_uds), `tuxlink-mcp` (shim), `tuxlink-mcp-testserver` (tier-2 harness), monolith `src/mcp_ports.rs` (real port impls; `.setup()` spawns the server), `src/shell/EgressArmControl.tsx` + `src/security/*` (arm UI).

## OPEN follow-ups (filed)
- **tuxlink-cvx84.7** — re-expose `packet_listen` + `telnet_p2p` as agent tools once the backend can gate the AX.25 UA-emit point + provide real p2p/listen aborts (dropped from 3.3 rather than ship a leaky gate).
- **tuxlink-cvx84.11** — RTX/DGX-Spark local-agent experiment (research leading-edge local/portable AI hardware → map to cloud-agent equivalents → scope + run a capability-matched sub-agent on Tuxlink-assistant tasks). **QUEUED — run AFTER acceptance testing.**
- **last_update_ms** — station-intel dropped the free-text `last_update` (injection surface); re-add it as a *parsed* `last_update_ms: Option<u64>` so the agent gets last-heard freshness as structured data (better gateway selection). Small follow-up; file a bd issue.

## Remaining gates (NOT yet done — this is your session)
1. **Operator tier-3 full-app acceptance** — the converged rebuild + live MCP drive. Station-intel has had **no live round-trip**; your acceptance is its first.
2. **A REAL greenfield wire-walk** — per the wire-walk Iron Law, the operator supplies the key flows COLD; do NOT reuse the 5 flows the builder self-authored (they launder the builder's blind spots — this session proved it: the operator's real flow exposed the missing station tools). Run `.claude/skills/wire-walk/SKILL.md` properly with operator-supplied flows before claiming the feature shipped end-to-end.

## The EmComm acceptance task (verbatim, run it live via MCP)
> "I'm an emcomm operator with a truck-mounted low multi-band resonant dipole in Houston, TX after a hurricane. I need to know which stations I should try calling on WARC bands to minimize interference with regular traffic. We'd like to establish a 24 hour reliable comms cycle. Identify the stations, bands, and schedule they should be called on. Then, put that in a list and send it to cameron.zucker@gmail.com for outside coordination using Winlink over Telnet."

Drive: read `tuxlink://agents/guide` → `find_stations` (WARC: 30m/17m/12m) → `predict_path` per candidate gateway for the 24h band-by-UTC-hour schedule → `solar_conditions` for context → compose the list (`message_send`) → **operator arms in the GUI** → `cms_connect` (Telnet) flushes the outbox to cameron.zucker@gmail.com.

## RADIO-1
Real on-air RF transmission is **operator-only** (control-operator act). **CMS Telnet** (internet) is **agent-drivable** dev testing — the EmComm send above goes over Telnet, so the agent may drive it; the operator still arms the egress gate in the GUI.

## Notes
- **is_some_and** is MSRV-1.75-safe and clippy-*preferred* (over `map_or(false, …)`); **is_none_or** is the >MSRV trap to avoid. (The builder's dispatch trap-lists wrongly listed is_some_and as forbidden — no harm, but correct it going forward.)
- Merged-dead phase/fix worktrees under `worktrees/` can be disposed per ADR 0009 when convenient — all content is on `main`.
- Adversarial transcripts in `dev/adversarial/` are local-only (gitignored): 3.1, 3.3, 3.4, the whole-shape security review, station-intel.

## State
- Branch: all phase branches merged + dead. This handoff lands via `bd-tuxlink-cvx84/mcp-session-handoff`.
- Epic `tuxlink-cvx84` stays `in_progress` (cvx84.7 + cvx84.11 + the acceptance gate remain).
