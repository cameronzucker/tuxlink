# Design: Native UV-Pro Benshi control backend (APRS tactical chat Phase 2 / Layer 2)

**Date:** 2026-06-12 · **bd:** tuxlink-nx95 (depends-on tuxlink-2f2n) · **Status:** SPEC (pre-adrev)
**Branch:** bd-tuxlink-nx95/uvpro-benshi-control (off main) · **Agent:** thistle-willow-chasm
**Parent epic spec:** [docs/design/2026-06-12-aprs-tactical-chat-design.md](2026-06-12-aprs-tactical-chat-design.md) §"Layer 2"
**Protocol grounding:** `dev/scratch/benshi-re/GROUNDING-FINDINGS.md` (local; derived from benlink + HTCommander source)

---

## Scope

Build the **backend** for native on-screen control of the BTECH UV-Pro over the
radio's own Bluetooth link: read live status (channel, frequency, mode, battery,
RSSI, TX/RX) and send control (set channel / set frequency / set mode,
connect / disconnect). Expose it as a documented Tauri command + event API that
a parallel frontend session consumes. The agent never transmits; the operator's
on-air smoke validates that control takes effect (RADIO-1 / ADR 0018).

This phase does **NOT** build: APRS messaging over the native link (that is the
data path — Phase 1a already ships it over KISS; native-data is a later
integration), the chat UX, position/beacon, or any frontend beyond the API
contract doc. It is the control profile and its command surface, nothing more.

### Why this is build-robust-features work, not plumbing
A net-new RF wire protocol reverse-engineered from prior art is a hard-to-undo,
interop-sensitive decision (a wrong framing/endianness choice silently bricks
on-air control). Per `discipline_triage_rule` this is squarely in the
adrev-required class, and `no_carveout_on_cross_provider_adrev` forbids skipping
the Codex round. The bd issue alone is not a sufficient spec for a protocol RE.

## Protocol ground truth (sanctioned RE)

Per `winlink_re_authoritative_sources`, prior-art implementations are truth.
Two independent ones agree 1:1 (the RE equivalent of a passing test):

- **benlink** (github.com/khusmann/benlink) — Python; the project's *product is
  the protocol documentation*. Its typed bitfields are the spec.
- **HTCommander** (github.com/Ylianst/HTCommander) — C# client "based on the
  decoding work … and the BenLink project." Command enums + connect flow match
  benlink byte-for-byte.

⚠️ This is the **opposite** of the clean-sheet VARA rule. RE here is explicitly
sanctioned by the task and the parent spec. Do NOT apply the VARA prohibition.

Both prior-art projects are Apache-2.0; tuxlink reimplements the protocol
independently in Rust (no source copied). Full attribution + license-compliance
review: [`docs/reference/uvpro-benshi-protocol-attribution.md`](../reference/uvpro-benshi-protocol-attribution.md).

Full grounded protocol facts live in `GROUNDING-FINDINGS.md`. The load-bearing
ones:

- **Transport = RFCOMM + GAIA framing.** The UV-Pro is on-air-proven on this
  project over classic RFCOMM/SPP (`uv-pro-kiss-tnc-transport`), NOT BLE-GATT.
  benlink supports both; we use RFCOMM, the same physical link type as the
  Phase-1a KISS path. (BLE-GATT is a documented alternative we do not build.)
- **GAIA frame:** `FF 01 <flags:u8> <n_payload:u8> <data[n_payload+4]> [csum:u8 if flags&1]`.
  We send `flags=NONE` (no checksum), matching benlink.
- **Message header (the "4 bytes"):** `command_group:u16 (BASIC=2)` +
  `is_reply:1bit` + `command:15bit` + `body`. Big-endian bit packing.
- **Commands used** (group BASIC): `GET_DEV_INFO=4`, `READ_STATUS=5` (battery),
  `REGISTER_NOTIFICATION=6`, `EVENT_NOTIFICATION=9`, `READ_RF_CH=13`,
  `WRITE_RF_CH=14`, `GET_HT_STATUS=20`. (`HT_SEND_DATA=31` is the data path — NOT
  this phase.)
- **`RfCh`** carries channel_id, tx/rx mod (FM/AM/DMR), tx/rx freq (u30 ×1e-6
  MHz), sub-audio, bandwidth, power flags, `name_str[10]`. "Set freq/mode" =
  read current channel → mutate → `WRITE_RF_CH`.
- **`StatusExt`** carries tx/rx/squelch/power flags, current channel id, GPS
  lock, and **rssi** (u4 ×100/15 → ~0..100). `is_in_tx` is the RADIO-1
  no-runaway-TX witness.
- **Async push events** via `REGISTER_NOTIFICATION`: `HT_STATUS_CHANGED=1`,
  `HT_CH_CHANGED=5`. The radio pushes state — no polling needed.
- **Connect/hydrate sequence:** open RFCOMM socket → `GET_DEV_INFO` →
  `READ_RF_CH` per channel → `READ_SETTINGS` → `GET_HT_STATUS` →
  `REGISTER_NOTIFICATION(HT_STATUS_CHANGED)` → consume `EVENT_NOTIFICATION`.

## Architecture: a capability profile in `winlink/ax25/uvpro/`

The native control profile is a new module that **reuses the existing RFCOMM
socket layer** and adds the GAIA + Benshi codec + a session driver on top. It
does not touch `aprs/` (Phase 1a, PR #642) and does not import the AX.25 state
machine — the Benshi command protocol is a different application protocol that
happens to ride the same socket type.

```
uvpro_* Tauri commands  (ui-facing; connect/disconnect/set_*/get_status)
        │
   UvproSession  (managed Arc; cached RadioState + abortable read loop + event emit)
        │
   benshi codec  (Message encode/decode, GAIA frame, RfCh/Status/PowerStatus DTOs)
        │
   RfcommSocket  (REUSED from winlink/ax25/rfcomm.rs — parse_bdaddr,
                  resolve_spp_channel, non-root AF_BLUETOOTH socket)
```

**Module layout (new):** `src-tauri/src/winlink/ax25/uvpro/`
- `mod.rs` — re-exports; the `UvproControl` capability surface.
- `gaia.rs` — GAIA frame encode + a streaming deframer (mirrors benlink
  `GaiaFrame.from_bitstream_batch`: buffer bytes, emit complete frames).
  **Hardened (adrev):** (a) we send `flags=NONE` but the deframer MUST handle RX
  frames *with* checksum (`flags & CHECKSUM` → consume the trailing csum byte)
  or it desyncs; (b) **resync** — if the buffer head is not `FF 01`, scan
  forward to the next `FF 01` start sentinel rather than stalling; (c) cap the
  buffer (e.g. 4 KiB) so a garbage / never-completing stream can't grow
  unbounded.
- `message.rs` — `Message` header codec (group/is_reply/command big-endian
  bit-pack) + the body enums for the commands above. Hand-rolled big-endian bit
  cursor (small, ~6 message types) — no new crate dependency. **Golden vectors**
  for the bit-pack are DERIVED offline from benlink's pure Python encoder (no
  radio needed: `Message(...).to_bytes()`) for at least `GET_HT_STATUS`,
  `WRITE_RF_CH` @146.520 MHz simplex FM, and `EVENT_NOTIFICATION(HT_CH_CHANGED)`,
  and pinned as test fixtures. freq encodes as `round(mhz * 1e6) as u32` (round,
  not truncate). `set_*` is **read-modify-write that preserves every untouched
  field** (sub-audio, flags, reserved/`unknown` bits, name) — re-encode the
  whole `RfCh` faithfully or the radio corrupts/rejects the channel.
- `model.rs` — the serde DTOs exposed to the frontend (`UvproStatus`,
  `UvproChannel`, `UvproDeviceInfo`) with `#[serde(rename_all="camelCase")]`
  AND per-enum `rename_all` (the documented Codex catch).
- `session.rs` — `UvproSession`: connect/hydrate, a background read-loop thread
  that deframes + dispatches frames, disconnect/abort. **Frame routing (adrev):**
  route by `is_reply` — `is_reply=true` + matching command → the one outstanding
  request's reply channel; `is_reply=false` (`EVENT_NOTIFICATION`) → the event
  handler (mutate cached state + emit). **Serialize commands: at most ONE
  outstanding request at a time**, behind a command mutex, each with a timeout —
  this makes correlation race-free and bounds radio load. Tolerate/skip unknown
  command + event ids without erroring (`DATA_RXD` *will* arrive — benlink notes
  enabling `HT_STATUS_CHANGED` auto-enables it). **No auto-reconnect:** a socket
  drop → `disconnected` state (operator re-invokes `uvpro_connect`); never an
  auto-retry loop that could hammer the radio/BT stack. The owner-lock is
  released on **every** disconnect path (clean disconnect, socket death, error)
  via a drop guard. A non-`SUCCESS` `reply_status` surfaces as
  `UvproError::RadioRejected`.
- `commands.rs` — `#[tauri::command]` wrappers (live in `ui_commands` style;
  may sit in this module and be re-exported to `generate_handler!`).

### Reuse vs net-new
- **Reused as-is:** `RfcommSocket` + `parse_bdaddr` + `resolve_spp_channel`
  (winlink/ax25/rfcomm.rs); the `ByteLink` trait; the `ModemSession`-style
  managed-Arc + background-broadcaster + `app.emit` pattern.
- **Net-new:** the GAIA codec, the Benshi `Message` codec + DTOs, the
  `UvproSession` driver + request/reply correlation, the 5 Tauri commands +
  status event, config for the control link, the single-BT-host arbitration.

## Single-Bluetooth-host arbitration (load-bearing)

The UV-Pro accepts **one RFCOMM connection at a time**, and its data path is
either KISS (radio "KISS TNC" menu mode — Phase 1a) **or** native GAIA (this
phase), never both. HTCommander confirms: it does everything (incl. APRS) over
native GAIA and never uses KISS.

**Contract:** a single in-process owner of the UV-Pro Bluetooth link. While
`UvproSession` is connected it holds the link; the Winlink packet path
(`KissLinkConfig::Bluetooth`) and the Phase-1a APRS-over-KISS listener must not
dial the same radio concurrently. Conflicts surface as a **named error**
(`UvproError::LinkBusy { holder }`), not a hang or a double-open. `uvpro_connect`
fails fast with `LinkBusy` if the KISS/packet path holds the radio; the operator
disconnects one before using the other (the disconnect/reconnect handoff the
parent spec already accepts, with a tooltip — UX is the frontend's job).

Phase 2's deliverable implements the owner-lock **on the native side** and the
fail-fast error; full bidirectional arbitration with the live KISS path is
realized when both land (the APRS-native data integration is a later issue).
This is documented in the API contract so the frontend respects it.

## Tauri command + event API (the contract the frontend builds against)

Naming follows the `uvpro_*` convention the task specifies (parallel to
`aprs_*`/`modem_*`). All commands are async-safe (the blocking socket work runs
off the UI thread). DTOs are camelCase over the wire.

### Commands
| Command | Args | Returns | Notes |
|---|---|---|---|
| `uvpro_connect` | `{ mac?: string }` | `UvproStatus` | mac defaults to configured packet `Bluetooth.mac`; opens socket, hydrates, registers events. `LinkBusy` if KISS path holds the radio. |
| `uvpro_disconnect` | — | `()` | drops the socket; radio reverts to manual control. Idempotent. |
| `uvpro_get_status` | — | `UvproStatus` | cached snapshot (no round-trip); `state:"disconnected"` shape when not connected. |
| `uvpro_get_channels` | — | `UvproChannel[]` | the hydrated channel table. |
| `uvpro_set_channel` | `{ channelId: u32, vfo?: "a"\|"b" }` | `UvproStatus` | selects the active memory channel by writing `Settings.channel_a` (default) / `channel_b` via `WRITE_SETTINGS` (resolved from `Radio.cs` — see Q1). Full read-modify-write of `Settings`, identity-tested. |
| `uvpro_set_frequency` | `{ channelId: u32, rxMhz: f64, txMhz?: f64 }` | `UvproChannel` | read-modify-write `WRITE_RF_CH` on the named channel; txMhz defaults to rxMhz (simplex). |
| `uvpro_set_mode` | `{ channelId: u32, mode: "fm"\|"am"\|"dmr", bandwidth?: "narrow"\|"wide" }` | `UvproChannel` | read-modify-write of mod/bandwidth. |

### Event
- `uvpro:status` — fired on every state change (channel/freq/mode/battery/RSSI/
  tx/rx/connection). Payload = `UvproStatus`. Driven primarily by the radio's
  push events (`HT_STATUS_CHANGED`, `HT_CH_CHANGED`); mirrors the `modem:status`
  broadcaster pattern. Battery has no push event, so it is polled (`READ_STATUS`)
  on a **bounded** cadence (≥30 s) through the same single-in-flight command
  queue so the poll can never pile up or storm the radio.

### DTOs (camelCase)
```
UvproStatus {
  state: "disconnected" | "connecting" | "connected",   // replaces a bare bool so the UI can show a connecting spinner
  deviceModel?: string, firmware?: string,
  currentChannelId?: u32,
  rxMhz?: f64, txMhz?: f64, mode?: "fm"|"am"|"dmr", bandwidth?: "narrow"|"wide",
  channelName?: string,
  isTx: bool, isRx: bool, squelchOpen: bool, powerOn: bool, gpsLocked: bool,
  rssi?: u8,                       // 0..100, present only on StatusExt
  batteryPercent?: u8, batteryVoltage?: f64,
  linkBusyHolder?: string,         // set when not connected because KISS holds the radio
}
UvproChannel { channelId: u32, name: string, rxMhz: f64, txMhz: f64,
               mode: "fm"|"am"|"dmr", bandwidth: "narrow"|"wide",
               rxToneHz?: f64, txToneHz?: f64, txDisable: bool }
UvproDeviceInfo { model: string, firmware: string, channelCount: u32 }
```

`UvproError` (string `kind` over the wire): `LinkBusy`, `NotConnected`,
`Timeout`, `Protocol`, `RadioRejected(reply_status)`, `Io`, `BadMac`.

## RADIO-1 / ADR 0018

- The control commands (`READ/WRITE_RF_CH`, `GET_HT_STATUS`, `READ_STATUS`,
  `REGISTER_NOTIFICATION`) do **not** key the transmitter. `WRITE_RF_CH` changes
  what a future PTT would use; it does not itself transmit. This phase exposes
  **no transmit command** — it is non-transmitting by construction.
- **Disconnect/abort:** dropping the RFCOMM socket halts all command traffic and
  reverts the radio to manual control. There are **no retransmit timers** in the
  control path (unlike the APRS TX queue) — every command is a single bounded
  request/reply with a timeout. The read-loop thread observes an abort flag and
  exits (mirrors `AbortableByteLink`).
- The agent writes + tests against mocks/loopback; the operator runs the on-air
  smoke (confirming a set-frequency actually moves the radio). `is_in_tx` in the
  status stream is an observable witness that control commands never key TX.

## Testing strategy (TDD)

All testable without hardware:
- **Codec round-trips** (the bulk): encode→decode for `Message` headers, GAIA
  frames (incl. split/partial buffer reassembly), and each body type, using the
  exact byte sequences benlink/HTCommander produce. Pin real captured bytes from
  the grounding doc as golden vectors.
- **GAIA deframer**: feed bytes in arbitrary chunk boundaries; assert correct
  frame boundaries + leftover-buffer handling.
- **Bit-cursor**: big-endian pack/unpack of u30 freq (×1e-6 scale), u15 command,
  u4 rssi.
- **Session logic** against an in-memory fake `ByteLink` peer that answers
  `GET_DEV_INFO`/`READ_RF_CH`/`GET_HT_STATUS` and emits an event — assert
  hydrate populates state, an event mutates cache + would emit, set_frequency
  issues the right `WRITE_RF_CH` bytes, disconnect tears down.
- **Arbitration**: `uvpro_connect` returns `LinkBusy` when the owner-lock is held.
- **`socket()` smoke** already exists for RFCOMM; reuse the CI-tolerant pattern
  (EAFNOSUPPORT/EPROTONOSUPPORT on a Bluetooth-less CI host).
- No cold cargo locally (`no_cold_cargo_on_contended_pi`) — gate on GitHub CI via
  the draft PR; clippy `--all-targets -D warnings` + full test run is the bar
  (`scoped_vitest_misses_contract_tests` analog for Rust).

## Open questions
1. **RESOLVED — Channel selection vs VFO.** `Radio.cs` shows the active channel
   is `Settings.channel_a` (VFO A) / `Settings.channel_b` (VFO B); switching the
   active channel is a `WRITE_SETTINGS` with the new `channel_a`/`channel_b`, NOT
   a `WRITE_RF_CH`. So `uvpro_set_channel` = full read-modify-write of `Settings`
   (preserve all ~50 fields; patch only `channel_a`/`channel_b`, which are
   nibble-split `*_lower`+`*_upper`); `uvpro_set_frequency`/`set_mode` edit the
   `RfCh` memory via `WRITE_RF_CH`. This pulls the `Settings` codec into scope —
   identity round-trip (`encode(decode(b)) == b`) tested on a golden vector so a
   write never corrupts the radio's config.
2. **RfCh vs RfChDMR discriminator** is by bitfield length in benlink. Confirm the
   UV-Pro emits the non-DMR `RfCh` length for FM channels; handle both lengths.
3. **Event auto-enable side effect**: benlink notes enabling `HT_STATUS_CHANGED`
   "also enables DATA_RXD and maybe others." We must tolerate (ignore) `DATA_RXD`
   event frames in the control read-loop without erroring (they belong to the
   data path). Decode-or-skip, never panic on an unknown event/command id.

## Success criteria
- The `uvpro/` module compiles, clippy-clean, with codec + session + arbitration
  tests green on CI (both arches).
- A documented command/event API the frontend session can build against without
  reading Rust.
- Operator on-air smoke (deferred, operator-run): connect to the UV-Pro, read
  live status into the UI, set a frequency from the screen and watch the radio
  retune — with a working disconnect and `is_in_tx` never asserting from control
  commands.

---

## Adversarial review log

- **Round 1 — Codex (cross-provider): DEFERRED.** Codex ChatGPT-auth hit its
  daily usage limit (resets Jun 13 1:49 PM). Per `codex_quota_gotcha` this is a
  capacity-defer, not a skip. The real cross-provider round runs after reset, on
  the **code diff** (a stronger target than the spec). Raw transcript stub:
  `dev/adversarial/2026-06-12-uvpro-benshi-spec-codex-r1.md` (gitignored).
- **Rounds 1–7 — self-adrev (substituting Codex, per the task + Phase-1a
  precedent):** wire correctness, deframer, request/reply correlation,
  arbitration, RADIO-1, API gaps, subagent-readiness. Findings folded into the
  sections above (v2): RX-checksum handling + deframer resync/cap; golden vectors
  derived offline from benlink; read-modify-write field preservation; single
  outstanding command + `is_reply` routing + unknown-id tolerance; no
  auto-reconnect; bounded serialized battery poll; owner-lock release on all
  paths; `state` enum replacing the connected bool; `set_channel` semantics
  gated on `Radio.cs` grounding (don't ship a no-op).

## Outstanding follow-ups (file as bd issues)
- KISS-side arbitration: teach the Winlink packet / APRS-over-KISS path to
  consult the shared UV-Pro owner-lock so a conflict from that direction surfaces
  as `LinkBusy`, not a raw socket error.
- Native-data integration: route APRS messaging over the native `HT_SEND_DATA`
  path (collapses control + data onto one link — the parent epic's premium-tier
  thesis). Separate, larger; depends on Phase 1a landing.
