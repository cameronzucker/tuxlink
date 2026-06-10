# VARA HF/FM dial — design (tuxlink-xglf)

Date: 2026-06-10 · Agent: moraine-butte-badger · Issue: tuxlink-xglf (P1)

## Problem

On 0.44.0 the VARA HF/FM pane ([`src/radio/modes/VaraRadioPanel.tsx`](../../../src/radio/modes/VaraRadioPanel.tsx))
renders only the transport Start/Stop (open/close the TCP cmd socket to the VARA
modem). It has no target-callsign input, no Send/Receive, and no favorites
surface. Net: a Winlink RMS station cannot be dialed over VARA HF/FM from the
app at all — even though the backend command `modem_vara_b2f_exchange(target,
intent, transport_kind)` is implemented and waiting. ARDOP, Packet, and Telnet
all received this Phase-3 connect+favorites integration; VARA never did (its
pane was built transport-only in Phase 2).

## Decision

Mirror the ARDOP/Packet dial flow onto the VARA pane, and **retire the M7 VARA
special-case** so saved VARA favorites are shown and dialable.

### M7 retirement (operator-approved 2026-06-10)

[`FavoritesTabs.tsx`](../../../src/favorites/FavoritesTabs.tsx) `isManualOnly()`
currently groups `vara-hf`/`vara-fm` with `telnet` as "Manual content only — no
favorites/recents, no Connect." That grouping was decided at M7 (B5,
2026-06-08) **when VARA had no working dial path**, so a favorite's Connect
button would have been dead. This work adds the dial, so the premise dissolves.
VARA HF dials RMS gateways exactly like ARDOP HF — favoriting nearby gateways is
equally meaningful. Telnet stays Manual-only (fixed CMS host — nothing to
favorite). Operator chose the full ARDOP mirror over honoring M7.

## Connection model (confirmed from backend)

VARA is two-step; the existing pane already owns step 1:

1. **Open Session** (existing "Start" → `vara_open_session`) opens the TCP cmd
   socket. **Not on-air.** Unchanged.
2. **Send/Receive** (new) → `modem_vara_b2f_exchange({ target, intent: 'cms',
   transportKind: mode.kind })`. On-air dial: a **single blocking**
   connect→B2F→disconnect call that requires an open session. Resolve = the link
   was reached; reject = failed. Pattern-identical to Packet's `packet_connect`.
3. **Abort** = the existing **Stop** (`vara_close_session` → `abort_in_flight`,
   which sends `ABORT\r` under a bounded ~2 s contract already in the backend).
   No new backend command.

## UI changes — `VaraRadioPanel.tsx`

Add a `Connect` section between Transport and Listen:

- `FavoritesTabs mode={mode.kind}` (`mode.kind` is literally `'vara-hf'` /
  `'vara-fm'` — same strings as the favorites `RadioMode` union, no mapping)
  with `manualContent` = a target-callsign input (`vara-target-input`).
- **Send/Receive** button rendered *outside* the tabs (always visible),
  `data-testid="vara-send-receive-btn"`.
- `useFavorites(mode.kind)` → `recordAttempt(dial, 'reached', tsLocal())` in the
  resolve and `recordAttempt(dial, 'failed', tsLocal())` in the catch of the
  blocking exchange call (never the finally — a pre-air guard must not log a
  spurious gateway failure).
- `handlePrefill` + `pendingDialRef` + `buildRecordDial(call)` + the
  `listenGatewayPrefill(mode.kind, handlePrefill)` subscription — copied from
  Packet. A favorite's Connect is **prefill-only**; it sets the target and never
  transmits (RADIO-1). A hand-typed target clears `pendingDialRef`.

### Gating

Send/Receive disabled unless: session `status.state === 'open'` **and** target
non-empty **and** no exchange in flight **and** not platform-blocked. While
exchanging it shows "Exchanging…" and is disabled; **Stop stays enabled** so
abort is always one click away.

## RADIO-1 / ADR 0018 posture

The agent writes + tests this transmit-path code freely (no on-air run here).
Correctness bar: working abort (Stop → `abort_in_flight`, already bounded) and
no runaway TX. Favorites Connect is prefill-only; the operator's Send/Receive
click is the Part 97 consent gate. On-air validation is operator-only.

## Tests (TDD)

New `VaraRadioPanel.test.tsx` cases:

1. Send/Receive invokes `modem_vara_b2f_exchange` with the typed target +
   `intent: 'cms'` + `transportKind` matching `mode.kind` (`vara-hf` and a
   `vara-fm` variant).
2. Records `reached` on resolve, `failed` on reject.
3. Send/Receive disabled until session open + target present; re-enabled after.
4. A favorite Connect prefills the target and does **not** invoke the exchange.
5. FavoritesTabs renders the tabs (Favorites/Recent/Manual) for VARA.

Harness: wrap render in `QueryClientProvider` (`retry: false`) and extend the
invoke mock to answer `favorites_read` / `favorites_recents` /
`position_current_fix` (mirrors `ArdopRadioPanel.test.tsx`).

`FavoritesTabs.test.tsx`: update the case asserting VARA is manual-only to
assert VARA now renders tabs; telnet remains manual-only.

## Discipline calibration

Mechanical port of an existing, reviewed pattern against a ready backend → per
the discipline-triage rule (impl-against-spec → the bd issue is the spec, go
straight to TDD), skip the heavy cross-provider adrev. Still run one Codex pass
on the final diff as the independent gate, and lean on the CI verify gate
(clippy `--all-targets`, full vitest both arches).

## Out of scope

- Favorites edit/delete/rename UI — that is tuxlink-oi1g (separate issue).
- The `take_transport_for_outbound` listener-coexistence wiring (TODO
  tuxlink-17u9 in the backend) — unchanged; this pane uses `take_transport`.
