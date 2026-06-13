# UV-Pro native control — frontend API contract (tuxlink-nx95)

The native UV-Pro "Benshi" control backend (APRS tactical chat Phase 2 / Layer 2)
exposes the Tauri commands + event below. A frontend builds a device-control
panel against this contract without reading Rust. Backend: `src-tauri/src/winlink/
ax25/uvpro/`. Spec: [`2026-06-12-uvpro-benshi-control-phase2-design.md`](2026-06-12-uvpro-benshi-control-phase2-design.md).

## Lifecycle

```
uvpro_connect  →  (uvpro:status events stream)  →  uvpro_set_* / uvpro_get_*  →  uvpro_disconnect
```

Control is **non-transmitting**: no command keys the radio. The agent never runs
on-air; the operator validates that control takes effect.

## Commands

All commands are `invoke('<name>', <args>)`. Argument keys are the Rust parameter
names below. On error, commands that return `Result` reject with
`{ kind: string, message: string }` — switch on `kind`.

| Command | Args | Resolves with | Notes |
|---|---|---|---|
| `uvpro_connect` | `{ mac?: string }` | `UvproStatus` | `mac` defaults to the configured packet `Bluetooth.mac`. Opens the RFCOMM link, hydrates device info + channels + status, subscribes to push events. Rejects `LinkBusy` if the KISS/packet path holds the radio, `BadMac` if no MAC resolves. |
| `uvpro_disconnect` | — | `UvproStatus` | Drops the link (radio reverts to manual control), releases the owner-lock. Idempotent; returns the disconnected snapshot. |
| `uvpro_get_status` | — | `UvproStatus` | Cached snapshot, no round-trip. `state:"disconnected"` shape before connect. |
| `uvpro_get_channels` | — | `UvproChannel[]` | The hydrated channel table. Rejects `NotConnected`. |
| `uvpro_set_channel` | `{ channelId: number, vfo?: "a"\|"b" }` | `UvproStatus` | Selects the active memory channel (writes `Settings.channel_a`/`_b`). `vfo` defaults to `"a"`. |
| `uvpro_set_frequency` | `{ channelId: number, rxMhz: number, txMhz?: number }` | `UvproStatus` | Read-modify-write of the channel's frequency. `txMhz` defaults to `rxMhz` (simplex). |
| `uvpro_set_mode` | `{ channelId: number, mode: "fm"\|"am"\|"dmr", bandwidth?: "narrow"\|"wide" }` | `UvproStatus` | Read-modify-write of the channel's modulation (+ optional bandwidth). |

## Event

- **`uvpro:status`** — emitted on every state change while connected (channel /
  frequency / mode / battery / RSSI / TX-RX / connection), and once on link loss.
  Payload = `UvproStatus`. Driven by the radio's push notifications plus a 2 s
  status poll. Subscribe via `listen('uvpro:status', e => …)`.

## DTOs (camelCase JSON)

```ts
type ConnState = "disconnected" | "connecting" | "connected";

interface UvproStatus {
  state: ConnState;
  deviceModel?: string;     // e.g. "0x1234"
  firmware?: string;        // e.g. "1.7"
  currentChannelId?: number;
  rxMhz?: number;
  txMhz?: number;
  mode?: "fm" | "am" | "dmr";
  bandwidth?: "narrow" | "wide";
  channelName?: string;
  isTx: boolean;            // radio is transmitting (control never sets this)
  isRx: boolean;
  squelchOpen: boolean;
  powerOn: boolean;
  gpsLocked: boolean;
  rssi?: number;            // 0..100, only on extended-status firmware
  batteryPercent?: number;
  linkBusyHolder?: string;  // set when not connected because KISS holds the radio
}

interface UvproChannel {
  channelId: number;
  name: string;
  rxMhz: number;
  txMhz: number;
  mode: "fm" | "am" | "dmr";
  bandwidth: "narrow" | "wide";
  txDisable: boolean;
}
```

## Error kinds

`LinkBusy` (radio held by the KISS/packet path or an existing session) ·
`NotConnected` · `Timeout` (radio didn't answer) · `Protocol` (bad/unexpected
frame) · `RadioRejected` (radio replied with a non-success status) · `Io`
(Bluetooth socket failure) · `BadMac`.

## Single-Bluetooth-host arbitration (IMPORTANT)

The UV-Pro accepts **one** Bluetooth connection at a time, and its data path is
either KISS (the Winlink packet / APRS-over-KISS path) **or** native control —
never both. The frontend MUST treat these as mutually exclusive:

- Calling `uvpro_connect` while a KISS/packet session holds the radio rejects
  with `LinkBusy { holder }`. Surface a tooltip ("the radio is in use by …";
  disconnect that first).
- **Phase-2 limitation:** the KISS/packet path does not yet consult the native
  owner-lock, so a conflict in the *other* direction (starting a packet session
  while native control holds the radio) currently surfaces as a generic socket
  error on the packet side, not a clean `LinkBusy`. Tracked as a follow-up.

## RADIO-1

Control commands never key the transmitter; `isTx` reflects the radio's own PTT
and stays `false` for any control action. Disconnect = drop the socket (the
abort). There is no auto-reconnect — on link loss the status goes `disconnected`
and the operator re-connects.
