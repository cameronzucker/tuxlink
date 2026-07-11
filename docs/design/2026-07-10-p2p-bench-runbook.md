# Two-rig VARA bench runbook: P2P peer model validation

- **Issue:** tuxlink-c39af (VARA protocol completeness), tuxlink-gbb05 (SSID
  path), tuxlink-m9kcd (REGISTERED gate), tuxlink-sg5zw.8 (peer store)
- **Date:** 2026-07-10
- **Status:** operator-executed after merge (RADIO-1)
- **Source:** [`docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md`
  §8](../superpowers/specs/2026-07-10-p2p-peer-model-design.md#section-8-two-rig-bench-verification-operator-executed-radio-1)

## Purpose

This is the operator's step-by-step for validating the shipped P2P peer model
on a real two-rig bench. It exercises the paths no agent can validate: an
actual over-the-air VARA HF handshake, the wire-level VARA command sequence,
and the resulting peer record in the roster. Nothing in this document runs
automatically and nothing in it is agent-executed. Every dial below is a
manual Connect click by the station licensee, per
[ADR 0018](../adr/0018-radio1-gates-operator-execution-not-agent-authorship.md).
Prior CI, mock, and loopback test coverage proves the protocol logic. This
runbook proves the on-air behavior matches it.

Two questions from the design are answerable only on a real rig and are
called out explicitly below:

- Is `RETRIES 10` a live TCP command VARA's engine accepts, or is it
  `VARA.ini`-only? If it is ini-only, the setting has to be provisioned in the
  ini rather than sent at runtime.
- Does the modem's `REGISTERED` readiness token re-arrive on a TCP reconnect,
  or only on the modem's first cold start? This determines whether the
  readiness gate can rely on it across repeated opens within one bench
  session.

## Pre-flight checklist

Confirm every item before Step 1. Do not proceed on a "close enough."

- [ ] Build under test is the merged P2P peer model branch (not a WIP branch).
      Confirm `git log -1` on the running build's source matches the merge
      commit. A stale converged build silently re-tests old code.
- [ ] Two VARA modem instances are available on the bench host (**R2**, the
      x86_64 box that runs both Tuxlink native and WLE-under-WINE): one
      driving Tuxlink, one driving WLE under WINE (`~/.wine-wle`).
- [ ] Two rigs, each with its own audio interface. **G90 is the validated
      VARA pairing.** Do not substitute the FT-710, which crashes VARA on this
      bench (`project_ft710_usb_audio_rfi_reset_on_tx`,
      `rig_test_path_g90_digirig_vara`). Rig A: primary G90 + Digirig, driving
      the Tuxlink-side VARA instance. Rig B: the second G90 self-decode rig +
      Digirig, driving the WLE-side VARA instance
      (`g90_selfdecode_rig` memory).
- [ ] **Rig B transmit capability confirmed.** The G90 self-decode rig is
      documented for off-air VARA decode, which may be a receive-only wiring.
      Steps 3 and 4 require Rig B to key out (WLE dials Tuxlink). Confirm Rig B
      is wired for transmit on this bench before relying on it. If Rig B is
      receive-only, substitute a different transmit-capable second rig for the
      WLE side (still a G90, never the FT-710) before running Steps 3 and 4.
- [ ] Station identity: operate as **N7CPZ** on the Tuxlink side and
      **N7CPZ-1** on the WLE side for Steps 2, 3, and 5. Two SSIDs under one
      license follows the same convention already used for the AX.25
      sticky-SSID design. Step 4 reassigns the WLE side's MYCALL to
      **N7CPZ-7** specifically to exercise the SSID dial path.
- [ ] Grid: **CN85** (or the operator's current 4-character grid) configured
      on both sides so B2F position fields are non-empty and comparable.
- [ ] `socat` is installed on the bench host for the wire tap in Step 1.
- [ ] `/tmp/corrected_dials.py`-style manual frequency correction is retired.
      Do not reintroduce it. The app expects and sends **center** frequencies
      (post-#1064). Do not hand-correct a dial frequency before entering it.
- [ ] Antenna and dummy-load path confirmed safe for the power level in use,
      and the operator is prepared to key both rigs personally. Nothing in
      this runbook auto-dials.

## Step 0: bench topology (must precede any traffic)

Both VARA instances default to control-port 8300 and data-port 8301. Run them
side by side without a port collision:

| Instance | Role | Control port | Data port | Rig | Audio device |
|---|---|---|---|---|---|
| 1 | Tuxlink (native) | 8300 | 8301 | Rig A, primary G90 | dedicated to Rig A |
| 2 | WLE under WINE | 8310 | 8311 | Rig B, G90 self-decode rig | dedicated to Rig B |

Set Instance 1's VARA control port in Tuxlink's transport settings to `8300`
and Instance 2's in WLE's VARA setup to `8310`. Confirm in each app's
connection status that it is bound to the intended instance before Step 1. A
swapped port map silently tests the wrong pairing.

`socat` sits between Tuxlink and Instance 1 as a third listener (see Step 1)
on a distinct port from both 8300 and 8310 so the tap never collides with
either modem's control port.

## Step 1: wire tap: capture the VARA command stream

Insert a `socat` hex tap between Tuxlink and its VARA control socket
(Instance 1, port 8300) so every command and response is visible in real
time and logged for later inspection:

```bash
socat -x -v TCP-LISTEN:8320,reuseaddr,fork TCP:127.0.0.1:8300 \
  2>&1 | tee ~/tuxlink-bench-vara-wiretap-$(date -u +%Y%m%dT%H%M%SZ).log
```

Point Tuxlink's VARA control-port setting at `8320` (the tap) instead of
`8300` directly, so the tap sits inline rather than sniffing a mirrored port.

Confirm these lines appear in the tap log during Steps 2 through 5, and
record the answer for each:

- **`P2P SESSION`.** Sent immediately before every `CONNECT` on the P2P
  intent. Confirm it appears once per dial attempt, not once per bench
  session.
- **`REGISTERED`** (bare, or SSID'd). Confirm it arrives within the readiness
  window at the *first* Instance 1 open. Then close and reopen the TCP
  connection to Instance 1 (disconnect and reconnect the app, or bounce the
  socket) without restarting the VARA engine itself, and confirm whether
  `REGISTERED` **re-arrives on the reconnect** or only appears once per cold
  modem start. Record the answer in the bench log. This determines whether the
  readiness gate's latch-per-open assumption holds across a same-session
  reconnect or only across a fresh modem launch.
- **`COMPRESSION TEXT`.** Confirm it is accepted (`OK`) and never draws
  `WRONG` on this VARA build. If it does draw `WRONG`, confirm the session
  still proceeds (setter `WRONG` is designed to be non-fatal) rather than
  aborting the dial.
- **`RETRIES 10`.** This is the confirmable-only-on-air question. Confirm
  whether the modem replies `OK` (accepted as a live TCP command) or `WRONG`
  (rejected, meaning the retry count has to be provisioned in `VARA.ini`
  instead of sent at runtime). Record which applies to this VARA build and
  version.
- **`PUBLIC ON`.** Confirm it is sent at HF open and does not draw `WRONG`.

**What a pass looks like:** the tap log shows `MYCALL`, then readiness wait,
then `PUBLIC ON`, then `SessionType` (`P2P SESSION`), then `COMPRESSION TEXT`,
then `RETRIES 10`, then optional `BW`, then optional `LISTEN ON`, in that
order at open. `SessionType` is re-sent immediately before each `CONNECT` at
dial time. Every setter returns either `OK` or a non-fatal `WRONG` that does
not abort the session.

## Step 2: outgoing (HF): Tuxlink dials the WLE peer

**Before dialing:** WLE's own **Vara P2P Session** window must be open on the
far end. Winlink P2P requires mutual intent. A WLE far end only answers a P2P
call from an open P2P session window, not from its normal Winlink-RMS
listening state. If that window is not open, the call will not be answered and
the failure will look like a dead channel rather than a protocol bug.

1. Confirm the channel is clear by ear and BUSYDET before transmitting.
2. In Tuxlink, use **Find a Station**, filter to station type **Peer** (or
   dial the WLE-side identity `N7CPZ-1` directly if it is not yet in the
   roster), and click **Connect** with VARA HF selected. This click is the
   RADIO-1 consent. Nothing before it transmits.
3. Confirm the connect flow completes B2F both directions (message send and
   receive), then disconnects cleanly (no wedge, no manual radio power-cycle
   required).
4. Open **Tools → Settings → P2P Peers** and confirm a peer record now exists
   for `N7CPZ-1` with:
   - **origin: outgoing**,
   - the correct channel: transport VARA HF, target callsign `N7CPZ-1`,
     center frequency matching the dial, no `via`,
   - `last_connected_at` updated to this session.
5. Confirm the peer is discoverable on the shipped surface: open **Find a
   Station** with the **Peer** type filter and confirm `N7CPZ-1` appears as a
   peer row, and confirm it paints on the tac map as a **circle** (peer shape,
   distinct from the gateway diamond).

**What a pass looks like:** one new (or updated) peer record, origin
`outgoing`, correct channel fields, the peer visible in the Find a Station
peer rows and painted as a circle on the tac map, clean B2F both directions,
and a clean disconnect.

## Step 3: incoming (HF): WLE dials Tuxlink

1. In Tuxlink, arm P2P listen (auto-arm on the P2P intent, or the equivalent
   manual listen toggle) on Instance 1 and Rig A.
2. On the WLE side, with its own **Vara P2P Session** window open, dial
   Tuxlink's identity (`N7CPZ`).
3. Confirm the channel is clear before the WLE operator keys. This is still a
   real transmission even though Tuxlink is the answering side.
4. Confirm Tuxlink accepts the call, completes B2F in the answer role, and
   disconnects cleanly.
5. In **Tools → Settings → P2P Peers**, confirm the resulting record has
   **origin: incoming**. This is the direct test of the inbound-tracking gap
   the peer model closes. WLE never persisted an accepted inbound P2P call at
   all.
6. Confirm `PUBLIC ON` (sent at Tuxlink's own HF open, visible in the Step 1
   tap) did **not** block the accept. The session must complete despite
   Tuxlink advertising itself as a public station.
7. Confirm the incoming peer is discoverable on the shipped surface: it
   appears in **Find a Station** under the **Peer** filter and paints as a
   circle on the tac map, the same as the outgoing peer in Step 2.

**What a pass looks like:** the call is accepted without operator intervention
beyond the listen-arm, B2F completes in the answer role, the peer record shows
`origin: incoming`, the peer is visible in the Find a Station peer rows and on
the tac map, and `PUBLIC ON` in the tap log has no observable effect on
whether the call is accepted.

## Step 4: SSID variant

Reassign the WLE-side MYCALL to an SSID'd identity, **`N7CPZ-7`**, and repeat
Step 2 (Tuxlink dials out) against that identity.

1. Confirm the finder and roster entry and the dial target all carry the full
   SSID'd form (`N7CPZ-7`), not the bare base callsign.
2. Confirm in the Step 1 tap log that the wire `CONNECT` string carries the
   SSID (for example `CONNECT N7CPZ N7CPZ-7`), and that the `CONNECTED` echo,
   which VARA returns as the **bare** callsign (`N7CPZ`, no SSID), is still
   recognized as a successful connection to the SSID'd target. This is the
   gbb05 echo base-match fix: a bare echo must not be read as "unexpected
   peer" and reject a successful SSID'd dial.
3. Confirm the completed B2F exchange and resulting peer record both carry the
   SSID (`N7CPZ-7`) in the channel's `target_callsign`, not the stripped base
   form.

**What a pass looks like:** the dial completes exactly as Step 2, the tap log
confirms the SSID rode the wire on the `CONNECT` line, the bare `CONNECTED`
echo does not cause a false "unexpected peer" rejection, and the peer record's
channel preserves the SSID.

## Step 5: FM leg (if an FM-capable pairing is available)

Run only if the bench has an FM-capable VARA pairing available for both rigs.
If not, record this leg as **bench-deferred**. The HF leg above (Steps 2
through 4) already proves the shared peer-recording path works end-to-end, and
FM-specific wire behavior is a distinct, explicitly-recorded limit rather than
an unproven slice of this feature.

If run:

1. Confirm the FM command set on the wire is exactly `MYCALL`,
   `LISTEN ON/OFF`, `CONNECT src dst [VIA d1 [d2]]`, `ABORT`, `DISCONNECT`. No
   `SessionType`, `COMPRESSION`, `RETRIES`, or `PUBLIC` lines appear in the
   tap log for this leg.
2. Confirm the `CONNECTED` bandwidth token is `WIDE` or `NARROW` (not a
   numeric Hz value), and that the resulting peer record's channel stores the
   matching `Bandwidth::Wide` or `Bandwidth::Narrow` enum rather than a `None`
   from a failed numeric parse.
3. If digipeaters are in the path, confirm the `via` list on the resulting
   channel is populated and not silently dropped.

**What a pass looks like:** no HF-only setter commands appear on the FM wire,
the bandwidth token parses to the enum (not `None`), and, if digipeaters are
used, the `via` path is recorded.

## Step 6: consent discipline (applies to every step above)

Every dial in this runbook, in both directions, is operator-initiated:

- Confirm the channel is clear by ear and BUSYDET immediately before each
  transmission, including the WLE-side dials in Steps 3 and 4.
- The **Connect** click (or, on the WLE side, the equivalent dial action) is
  the RADIO-1 consent for that specific transmission. No cached setting and no
  prior session substitutes for a fresh click on this run.
- If any step wedges (session hangs with no progress and no clean abort), do
  not force a radio power-cycle before attempting the app's own abort path
  first, and record the wedge. This is exactly the failure class the ARDOP
  `ARQTimeout` lesson and the VARA readiness `T_max` fail-open behavior are
  designed to bound.

## Recording results

After completing Steps 1 through 6 (or recording Step 5 as bench-deferred),
record in the bench log:

- The `RETRIES 10` acceptance answer (Step 1) and whether `VARA.ini`
  provisioning is required instead.
- The `REGISTERED` reconnect-recurrence answer (Step 1).
- Pass or fail for each of Steps 2 through 5 with the peer-record evidence
  described in each step's "what a pass looks like."

A full pass across Steps 1 through 4 (with Step 5 either passing or explicitly
recorded as bench-deferred) is what promotes this feature from
"CI-green, on-air-unverified" to "on-air validated" per
`rf_validation_onair_only`. Only a real over-the-air run against the intended
target proves the RF path. CI and loopback tests prove the protocol logic that
this bench then confirms against a live VARA engine.
