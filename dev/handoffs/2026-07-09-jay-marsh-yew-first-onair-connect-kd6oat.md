# Handoff addendum — 2026-07-09 (evening) — `jay-marsh-yew` — FIRST ON-AIR CONNECT (KD6OAT @ BW500); new blocker found+fixed same night (PR #1063)

Companion to `2026-07-09-jay-marsh-yew-wle-staged-channel-data-cracks-campaign.md`
(same session, after the operator authorized the corrected-dial campaign).

## Headline

**KD6OAT answered Tuxlink's 500 Hz call at 01:01:57Z — the first on-air VARA
connect in project history** (retry 3/30, avg S/N −11.8 dB, 41 bps). The
bandwidth hypothesis is CONFIRMED: the same gateway ignored eleven 2300 Hz
calls the previous afternoon. The link then died 4 s after connect —
root-caused the same night to the data socket carrying the cmd socket's 2 s
read timeout (`Handshake(ConnectionClosed)` on the first SO_RCVTIMEO tick;
`tuxlink-xzxk1`), fixed on **PR #1063**.

## Corrected-dial campaign results (operator-consented, ~00:38–01:02Z)

| # | Target | Dial | BW | Result |
|---|---|---|---|---|
| 1 | NS7K-10 | 14103.4 | 2300 | No answer (full 30 retries; SSID corrected — 20m evening path unverified) |
| 2 | N0DAJ | 7106.5 | 500 | No answer (94 km; evening foF2 likely below 40m NVIS — low information) |
| 3 | KM7N | 7104.5 | 500 | No answer (192 km; same NVIS caveat) |
| 4 | **KD6OAT** | **14111.0** | **500** | **CONNECTED**, then B2F handshake died at +4 s (see below) |
| 5 | KB2PCN-5 | — | — | Skipped (stop-on-first-answer) |

Radio verified unkeyed (`TX0`) after every dial. Campaign script:
`/tmp/corrected_dials.py` on R2 (also in Pi scratchpad). Antenna is a
**Delta Loop** (operator corrected the stale "vertical" memory — hybrid
NVIS/DX characteristics; evening close-in 40 m still marginal).

## The +4 s disconnect — root cause + fix (tuxlink-xzxk1, PR #1063)

Timeline: `VARAHF.log` `Connected 01:01:57` → `DISCONNECT command received
01:02:01` (Tuxlink's own graceful wind-down) → app error
`Handshake(ConnectionClosed)`. Mechanism: `vara/transport.rs` stamped the DATA
socket with the cmd socket's 2 s read timeout; `handshake.rs:153` maps any
fill_buf error — including a timeout tick — to `ConnectionClosed`. At 41 bps
the SID cannot arrive in 2 s. Every B2F read shares the budget via cloned fds,
so this also would have killed mid-transfer reads. Fix:
`VaraConfig.data_read_timeout` (default 120 s, ARQ-timeout regime) on the data
socket only; cmd keeps 2 s. Regression tests: socket-level slow-SID pair
(handshake.rs) + kernel-level socket/clone timeout assertion (transport.rs).
ARDOP unaffected (`ardop/data.rs` is tick-tolerant by design). Codex
adversarial round output: `dev/adversarial/2026-07-09-vara-data-timeout-codex.md`
(local-only, gitignored).

## What is now PROVEN on-air

Dial → CAT tune → PTT keying → BW500 ConReq → gateway decode → **gateway
answer → ARQ link established** → PTT cycling through the ARQ turns. The
remaining unproven layer is B2F over VARA (handshake + proposals + mail),
blocked only by the timeout bug PR #1063 fixes.

## Next session / operator

1. **Merge PR #1063 when CI is green** (branch `bd-tuxlink-xzxk1/vara-data-read-timeout`,
   head `8e53a2f4`). Close `tuxlink-xzxk1`.
2. **Rebuild the R2 diagnostic app from the fixed branch** (R2 is x86_64, has a
   warm target dir at `~/tuxlink-yrrjq-build` — pull the new branch there or a
   fresh clone; relaunch app + Vite per prior handoff §machine-state).
3. **Confirmation dial (operator consent per RADIO-1): KD6OAT 14111.0 dial @
   BW500** — expect full B2F this time. KM7N 14098.6 @500 is the backup 20m
   target (192 km may be inside 20m skip; KD6OAT first).
4. WLE differential test remains staged (`~/.wine-wle`, runbook in the main
   handoff) — now it validates B2F parity rather than exonerating the TX chain.
5. Backlog: `tuxlink-hmoz8` (channels API ingest: SSID'd callsign + hours +
   per-channel BW auto-match) is now CONFIRMED as the product fix for dial
   targeting; `tuxlink-gbb05` (SSID stripping) P1.

## R2 state deltas (vs earlier-today handoffs)

- VARA1 died silently ~00:07Z (idle, no crash line); relaunched ~01:30Z
  (this session) — 8300/8301 listening. VARA2 untouched.
- **VARA bandwidth config left at 500** (campaign stops on answer without
  restoring; deliberate — next dial is the BW500 confirmation).
- WLE still parked at its license dialog on :1 (inert).
- New worktrees: `worktrees/tuxlink-xzxk1` (PR #1063), `worktrees/jay-marsh-yew-handoff`
  (this doc + bd snapshot; branch pushed, PR self-merge was policy-denied —
  operator merges).

Agent: jay-marsh-yew
