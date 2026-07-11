# Handoff — 2026-07-10 — `harrier-glade-osprey` — P2P design: 5-round adversarial review COMPLETE, ready for the plan

Picks up directly from `kite-sandbar-vetch`'s 2026-07-10 handoff (adrev halted
after R2). **BRF Step 2 is now fully done.** The next session starts at BRF
Step 3 (writing-plans).

## What this session did

Resumed the `build-robust-features` flow at the adversarial-review gate and ran
the four outstanding rounds, then folded everything and committed:

- **R1 (Codex)** — 20 findings. 10,411-line review; ran clean in the
  background (the classifier interrupts *Fable's own generation*, not the Codex
  subprocess).
- **R3 (protocol, Claude subagent)** — 11 findings, **primary-source grounded**:
  the agent fetched the EA5HVK "VARA Protocol Native TNC Commands" PDF and read
  the WLE `VaraSession.cs`/`VaraFMSession.cs` decompile.
- **R4 (data-model/integration, Claude subagent)** — 12 findings.
- **R5 (Codex re-attack on the revised spec)** — 10 findings; 21,478-line
  review; explicitly confirmed the fold worked (HF/FM split, readiness gate,
  PUBLIC-ON, grid clamping all pronounced sound).

Three independent lenses (Codex R1, Claude R3, Claude R4) converged on the same
P1 clusters — high signal. The **four load-bearing code facts were verified
in-tree**, not taken on the reviewers' word:

- `command.rs:249` — bare `REGISTERED` only parses as the tail of `LINK
  REGISTERED`; the m9kcd gate could never see its release token. CONFIRMED.
- `commands.rs:2762` — outbound CONNECTED-echo match compares the SSID'd dial
  string against VARA's bare-callsign echo → gbb05's SSID'd dial rejected as
  "unexpected CONNECTED peer." CONFIRMED.
- `winlink_backend.rs native_packet_exchange` — `ExchangeConfig { intent:
  Cms }` hardcoded both directions; packet has no P2P intent. CONFIRMED.
- `listener.rs:94-110 parse_peer_call` — no charset filter. CONFIRMED.

## Deliverable

- **Spec (committed + pushed):**
  `docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md`, commit
  `02e4336b` on branch `bd-tuxlink-c39af/vara-p2p-session`. 544 insertions /
  208 deletions. Status line: "full 5-round adversarial review folded … Ready
  for BRF Step 3." Every folded finding is cited inline as `[R#-N]`.
- **Disposition ledger (gitignored, LOCAL ONLY):**
  `dev/adversarial/2026-07-10-p2p-design-consolidated-dispositions.md` — the
  full finding→disposition table across all 5 rounds. Also local: the raw
  round transcripts (`…-r1-codex.md`, `…-r5-codex.md`, `…-r2-security.md`,
  `codex-p2p-design-r5-prompt.txt`). These are on this machine's disk under
  `worktrees/bd-tuxlink-c39af-vara-p2p-session/dev/adversarial/`; not on origin.

## How the design changed (the fold GREW the build — this is honest, per ADR 0018)

The pre-review spec was too small. The hardened design now includes:

- **Engine-split VARA protocol (§7).** HF/SAT vs FM are separate command
  plans. FM sends only `MYCALL`/`LISTEN`/`CONNECT[-VIA]`/`ABORT`/`DISCONNECT`
  (WLE `VaraFMSession`) — no SessionType/COMPRESSION/RETRIES/PUBLIC/BW. Setter
  `WRONG` is non-fatal (WLE suppression-list parity). REGISTERED gate: accept
  any REGISTERED line, latch per transport-open, `T_min` settle + `T_max`
  **fail-open** (never wedge — the ARDOP ARQTimeout lesson).
- **8-site recorder (§3)**, not "one chokepoint" — incl. pre-exchange
  connect-fail sites for VARA *and* ARDOP, and **packet `SessionIntent::P2p`
  plumbing** threaded through `TransportConfig`/`PacketConnectCtx` + the
  intent-aware proposal builder (not just `ExchangeConfig`).
- **Identity model (§1)** — `canonical_base` (dedup anchor only) +
  `presented_callsigns` + `identity_kind` + manual merge/split with
  post-split routing by exact presented callsign.
- **Trust boundary (§4)** — mandatory `curate_peer`; display-sanitizer vs
  transport-grammar validation; agent telnet dial takes `(peer_id,
  endpoint_id)` never raw host; DNS-rebinding-safe egress denylist (resolve
  once, denylist every candidate); monotonic endpoint provenance; keyring
  re-key by `peer_id:endpoint_id` with conservative migration.
- **Storage (§2)** — `PeersFile { schema_version, peers }`, `#[serde(other)]`
  on every enum, corrupt-file quarantine; per-transport dedup keys (incl. `via`
  digipeater path); auto-vs-manual growth caps that spare a legit field
  exercise.
- **Integration matrix** — 10 must-land rows (incl. engine-aware RF egress and
  favorites `peer_id`) with a capability-bit hide mechanism.

## Branch / worktree state

- Worktree `worktrees/bd-tuxlink-c39af-vara-p2p-session/`, branch
  `bd-tuxlink-c39af/vara-p2p-session`, **up to date with origin**, clean tree.
- `pnpm install` already run here; `pnpm lint:docs` passes; the pre-push docs
  linter is green.
- bd `tuxlink-c39af` remains `in_progress` (design done, not built); its note
  is updated to "adversarial review COMPLETE." Coordination edge to
  `tuxlink-sg5zw.2` (telnet_p2p agent-tool rebuild consumes the peer store)
  still stands; that issue has its own in_progress worktree.
- **R2 is powered off.** The two-rig bench (spec §8) is operator-executed
  later; NOT needed to write the plan or do most implementation.

## Next-session order — do NOT skip to code

1. Read the spec (source of truth) + the disposition ledger.
2. **BRF Step 3: `/writing-plans`** → `docs/plans/2026-07-…-p2p-peer-model-plan.md`.
   Minimum 3 plan-review rounds (BRF Step 4). The plan must be subagent-proof:
   the spec is large and several pieces MUST land together (see §Integration
   matrix — a partial build ships a stub per ADR 0018).
3. The wire-walk gate captures greenfield flows at build *start* (operator
   supplies them); the finder→connect→session→record→map chain is the seam to
   watch.
4. Then BRF Step 5: recommend an execution approach (subagent-driven here vs a
   fresh `/executing-plans` session vs Agent Teams). The plan is big and mostly
   sequential-with-parallel-leaves; lean toward a dedicated execution session.

## Process note

The classifier keeps interrupting Fable's own generation mid-adrev (not Codex).
This session got through both Codex rounds by running them as background
subprocesses and only reading their findings blocks — Fable never had to
generate a summary of attack content mid-stream. If it bites again, the same
pattern works.

Agent: harrier-glade-osprey
