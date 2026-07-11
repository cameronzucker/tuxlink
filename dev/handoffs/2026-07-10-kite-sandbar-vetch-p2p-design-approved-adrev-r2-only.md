# Handoff — 2026-07-10 — `kite-sandbar-vetch` — P2P peer-model design approved; adversarial review halted after R2

Session halted deliberately by the operator: a safety classifier interrupted
the adversarial review mid-stream and the model was switched, so the operator
called a clean handoff rather than continue on a tainted/switched session.
Pick this up FRESH.

## What this session was

Objective: "the missing P2P modes" via **build-robust-features** (design-first).
The operator escalated scope from a narrow VARA-protocol fix to a full
**"P2P as a complete mode"** design (Scope A). Reached BRF Step 2 (adversarial
review); did NOT reach the plan or any implementation.

## Branch / worktree state

- **Worktree:** `worktrees/bd-tuxlink-c39af-vara-p2p-session/`, branch
  `bd-tuxlink-c39af/vara-p2p-session` (off origin/main). Claims bd
  `tuxlink-c39af` (in_progress).
- **Pushed + on the branch (clean tree):**
  - `docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md` — the
    operator-approved design (commits `6fc54a5f` + `a5c89bd2` interop
    analysis). **This is the source of truth. Read it first.**
- **On disk, gitignored (dev/adversarial/, local-only — do NOT expect on origin):**
  - `2026-07-10-p2p-design-r2-security.md` — the ONE completed review round.
  - `2026-07-10-p2p-design-r1-codex.md` — Codex R1, KILLED mid-exploration,
    just grep noise, NO findings block. Ignore / re-run.
  - `codex-p2p-design-prompt.txt` — the Codex round-1 prompt (reusable).
- **Node deps:** `pnpm install` was run in this worktree (the pre-push
  docs-link linter needs `tsx` from node_modules — a fresh worktree push will
  fail the hook until `pnpm install` runs; not a bypass, just setup).

## The design in one paragraph (but READ THE SPEC)

Unified **Peer** entity anchored on base callsign, with SSID'd per-channel
observations and provenance-tagged telnet endpoints; new `peers.json` store
(NOT a config section); session-layer auto-tracking for BOTH outgoing dials
and accepted incoming answers (fills the gap that inbound sessions leave no
durable trace today); agent trust boundary (telnet endpoints agent-dialable
only if provenance=Operator); **Find a Station** becomes one dialog with a
Gateway/Peer type filter (no finder split — operator reversed my first mock);
map symbology is **shape-only** (diamond=gateway, circle=peer, sprite=APRS) —
color stays reachability/outcome (operator caught that identity-hue collided
with the reachability gradient); VARA protocol completeness (P2P/WINLINK
SESSION command per c39af, compression vocab OFF/TEXT/FILES, owned RETRIES 10,
REGISTERED gate per m9kcd, SSID end-to-end per gbb05); two-rig WLE-interop
bench runbook (FT-710↔G90 on R2, WLE under `~/.wine-wle` as far end).

Grounded in: three codebase survey passes + the WLE decompile
(`~/Code/library-of-hamexandria/winlink-re/decompiled/RMS Express/`) — WLE has
NO unified entity (per-mode flat files, inbound callers never persisted); we
intentionally diverge. Interop analysis (spec §7.5) concluded the divergence
is **off-wire** and actually increases WLE conformance; operator asked for and
approved that analysis explicitly.

## CRITICAL — adversarial review is 1/5 done, NOT incorporated

**BRF Step 2 requires a 5-round adversarial review (≥1 Codex). Only R2
(security) completed.** Its 11 findings are NOT yet folded into the spec.
Three are P1 and WILL reshape the design:

- **S1 — `find_peers` must CURATE, not "mirror," find_stations.** The spec's
  "mirrors find_stations, does not taint" is wrong-as-written: find_stations
  is safe only because `curate_gateway` (mcp_ports.rs:2311-2344) drops
  free-text, shape-validates callsigns, strips control chars. Peer callsign/
  note/grid are RF-sourced → prompt-injection into the dial-capable cloud
  agent unless a `curate_peer` is mandated.
- **S2 — stored XSS.** Attacker callsign (unvalidated; `parse_peer_call`
  vara/listener.rs:94-110 has no charset filter; `allow_all` defaults TRUE)
  → Leaflet divIcon/popup raw HTML → Tauri webview command access. Needs
  charset validation at the write boundary + escaped DOM sinks.
- **S3 — SSRF laundering.** RF/telnet P2P has no identity proof, so a spoofed
  callsign makes the operator promote an ATTACKER's endpoint to
  Operator-provenance → agent dials it. Needs: provenance sticky/never
  auto-promotable, no auto-recorded promotable inbound endpoints, egress
  denylist (loopback/RFC1918/link-local/metadata) regardless of provenance
  (S4), and `find_peers` gated/redacted as private data (S5).

Full table + attack sequences + provisional dispositions:
`dev/adversarial/2026-07-10-p2p-design-r2-security.md`. Everything read as
valid and design-level (cheap to fix pre-build).

## Next-session order (do NOT skip the review gate)

1. Read the spec + the R2 security findings file.
2. **Re-run the adversarial review: R1 (Codex — prompt file is saved), R3
   (protocol correctness — the VARA-FM question is open: spec marks
   session-type commands HF/SAT-only and Tuxlink supports vara-fm; the open
   command sequence must handle FM), R4 (data-model/integration — schema-
   clobber class, chokepoint completeness, finder shape mismatch), R5 (Codex
   on the revised spec).** Per memory `brf-rounds-not-compressible`, the
   5 rounds are not compressible on fragile RF-path work.
3. Fold ALL findings (R2 already in hand + the re-run rounds) into the spec;
   commit.
4. THEN BRF Step 3: writing-plans → subagent-ready implementation plan
   (min 3 review rounds).
5. Coordination: sg5zw.8 (peer store) + sg5zw.2 (telnet_p2p agent-tool rebuild
   consumes it) — add bd dep edges; sg5zw.2 has its own in_progress worktree.

## Machine / repo notes

- **R2 (the operator's build host) is POWERED OFF.** The two-rig bench (spec
  §8) is operator-executed later; not needed to finish design/plan/most
  implementation. Relaunch per the 2026-07-09 handoff §machine-state when it
  returns; retire pre-#1064 `/tmp/corrected_dials.py` (expects DIAL, app now
  wants CENTER).
- **Process note for the next session (operator correction this session):**
  handoffs are committed directly to the working branch, NEVER PR'd. I erred
  early by opening+merging PR #1066 to land the prior jay-marsh-yew handoff
  branch; operator interrupted ("We don't merge handoffs via PRs"). #1066 is
  already merged (done, no action) — but do not repeat the pattern. Memory
  `no-pr-for-handoffs` updated with the stranded-branch case.
- Visual companion server was used (mockups persisted under
  `.superpowers/brainstorm/386097-1783672813/content/`) and stopped cleanly.
  `.superpowers/` should be gitignored.
- Worktree disposal (ADR 0009) is NOT due — this branch has unmerged design
  work; keep it.

Agent: kite-sandbar-vetch
