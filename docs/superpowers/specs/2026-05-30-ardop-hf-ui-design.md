# ARDOP HF UI — design spec

> **bd issue:** `tuxlink-4ek` · **Created:** 2026-05-30 · **Author:** towhee-gorge-tanager
>
> **Pairs with:** [ARDOP MVP transport plan](../plans/2026-05-27-ardop-mvp-transport.md), [ardop-deployment-findings.md](../../design/ardop-deployment-findings.md), [ADR 0015 — modem integration and rig-control foundation](../../adr/0015-modem-integration-and-rig-control-foundation.md).

## Goal

Wire the v0.2.0 ARDOP HF backend MVP (`ModemTransport` trait, `ArdopTransport`,
`ManagedModem` supervisor — already on `main` per PR #138) into the tuxlink
frontend so the operator can dial an ARDOP gateway, observe a live session, and
disconnect — entirely from the UI, with no CLI fallback for routine use.

## Non-goals

- **Rig control (CAT)** — ardopcf's `-p RTS PTT` MVP only; Hamlib comes via a
  separate `sonde-rig` crate per ADR 0015. The UI surfaces a PTT serial port
  setting only.
- **VARA HF / Dire Wolf / first-party-modem UI** — the dock is designed to
  generalize across `ModemTransport` implementations, but only ARDOP is wired
  in this PR. VARA-HF and others land in follow-up work.
- **Constellation / waterfall visualizations** — the dock is compact-only;
  charts live in the full Modem Console (separate phase).
- **PSK / packet-radio over ARDOP** — ARDOP carries B2F as a byte-stream; no
  custom payload framing.

## Operator decisions already locked

From [ardop-deployment-findings.md](../../design/ardop-deployment-findings.md)
and CHANGELOG v0.2.0:

1. **tuxlink spawns + owns ardopcf** (managed-spawn). The UI does not expose
   an "operator-managed daemon" mode. Connect → spawn ardopcf; Disconnect →
   SIGINT clean-stop, confirm audio device released.
2. **Single arbiter of the one-sound-card conflict.** When the operator
   switches from VHF (Dire Wolf) to HF (ARDOP), tuxlink spins the running
   modem down before spinning the other up. This is invisible to the UI
   except as a transitional state in the dock.
3. **Generic `ModemTransport` abstraction** — ARDOP is the first concrete; the
   dock UX should be transport-agnostic (Dire Wolf, VARA, sonde will plug
   in with the same surface). Concretely: the dock's state grid, meters, and
   mono status block are driven by a `ModemStatusFeed` shape, not by ARDOP
   specifics.
4. **Rig control deferred.** PTT is ardopcf-internal (`-p` RTS via the
   configured serial port). No CAT in this phase.

## UX shape — decisions from 2026-05-30 brainstorm

5. **ARDOP-HF is a new protocol** under existing intents (`cms`, eventually
   `p2p` + `radio-only`), peer of `vara-hf` in `sessionTypes.ts`. *Not* a
   new top-level session type.
6. **Right-hand modem dock** (~290 px wide) appears the moment ardopcf is
   running. The main grid becomes `200 / 340 / 1fr / 290` instead of
   `200 / 380 / 1fr`. The reading-pane slot shrinks but mailbox + reading
   stay visible. Dock disappears when modem is stopped.
7. **Dial UX lives in the dock** — when stopped, the dock body shows a tiny
   `Target callsign [____] [Connect]` form in place of the meters. After
   Connect: form is replaced by live meters. Single surface, single context.
8. **Operator-specific config (binary, audio, PTT) lives in
   Settings → ARDOP.** Set once, edit rarely. The dock links to it.
9. **Full Modem Console** is a separate sidebar entry under a new `Modem`
   section. Takes over the reading-pane slot when opened. Shows
   constellation + throughput chart + advanced controls + Disconnect. Built
   in a follow-up phase; the slim dock ships first.

## Reference mock

The approved-during-brainstorm dock layout (active state) is in
[2026-05-30-ardop-hf-ui-dock-active.png](2026-05-30-ardop-hf-ui-dock-active.png).
Cross-reference with [docs/design/mockups/images/modem-compact.png](../../design/mockups/images/modem-compact.png)
which set the visual density target.

## Architecture

### Frontend (`src/`)

**New / changed files:**

| Path | Purpose |
|---|---|
| `src/connections/sessionTypes.ts` | Add `'ardop-hf'` to `ProtocolId`. Add `{ ...ARD, built: true }` to the `cms` intent's protocols. Define `ARD = { id: 'ardop-hf', label: 'ARDOP HF' }`. |
| `src/modem/ArdopDock.tsx` *(new)* | The right-hand dock. Dispatches between three states: `stopped` (Connect form), `connecting`/`running` (live meters + status), `error` (last-error display + retry). |
| `src/modem/ArdopDock.css` *(new)* | Panel-local styles matching the mock. Tokens from `App.css :root`. |
| `src/modem/useModemStatus.ts` *(new)* | React hook that subscribes to a Tauri `modem-status` event channel and exposes a `ModemStatus` shape with state, peer, mode, S/N, VU, throughput, byte counters, uptime. |
| `src/shell/AppShell.tsx` | Conditional grid: `200 / 340 / 1fr / 290` when `useModemStatus().state !== 'stopped'`, else current `200 / 380 / 1fr`. |
| `src/shell/SettingsPanel.tsx` | Add an `ARDOP` section: binary path, capture device, playback device, PTT serial port, cmd port. |
| `src/connections/ArdopHfStub.tsx` *(new, tiny)* | Reading-pane content shown when sidebar's `Winlink (CMS) → ARDOP HF` is selected and modem is stopped. One line: *"Use the modem dock on the right to dial."* Auto-opens the dock if hidden. |

**Status-feed shape** (`useModemStatus.ts`):

```ts
export type ModemState =
  | 'stopped'           // ardopcf not running
  | 'spawning'          // ardopcf process starting
  | 'initializing'      // ardopcf running, cmd-socket handshake in progress
  | 'idle'              // initialized, no ARQ session
  | 'connecting'        // sent CONREQ, awaiting peer
  | 'connected-irs'     // info receiving station
  | 'connected-iss'     // info sending station
  | 'disconnecting'     // operator hit Disconnect; SIGINT in flight
  | 'error';            // see lastError

export interface ModemStatus {
  state: ModemState;
  peer: string | null;            // target callsign while connected
  mode: string | null;            // e.g. '4FSK 500'
  widthHz: number | null;         // e.g. 500
  pttBackend: 'rts' | 'cat' | 'vox' | null;
  snDb: number | null;
  vuDbfs: number | null;
  throughputBps: number | null;
  bytesRx: number;
  bytesTx: number;
  uptimeSec: number;
  arqFlags: { busy: boolean; rx: boolean; tx: boolean };
  lastError: string | null;
}
```

This is the **generic** shape the dock renders. Future VARA / Dire Wolf
backends produce the same shape (most fields may be `null` if not applicable).

### Backend (`src-tauri/`)

**New Tauri commands** (`src-tauri/src/ui_commands.rs`):

| Command | Args | Returns | Purpose |
|---|---|---|---|
| `modem_ardop_connect` | `{ target: String }` | `Result<(), String>` | Spawn ardopcf if not running, ARQ-connect to target. Errors back as user-friendly string. |
| `modem_ardop_disconnect` | `()` | `Result<(), String>` | Operator-initiated DISC. SIGINT ardopcf after clean disconnect. |
| `modem_get_status` | `()` | `ModemStatus` | One-shot read of current status (for initial render before events fire). |
| `config_set_ardop` | `{ binary, captureDevice, playbackDevice, pttSerialPath?, cmdPort }` | `Result<(), String>` | Persist Settings → ARDOP form. |
| `config_get_ardop` | `()` | `ArdopConfigForm` | Read current ARDOP settings for the form. |

**New Tauri event channel:**

| Event | Payload | When |
|---|---|---|
| `modem-status` | `ModemStatus` | Every 250 ms while modem is running; immediately on any state transition. |

A backend `ModemStatusBroadcaster` task owns the `ManagedModem` handle, polls
the ardopcf cmd-socket for state, and emits `modem-status` to the frontend.

**Config persistence** (extends `src-tauri/src/config.rs`):

```rust
pub struct ArdopConfig {
    pub binary: String,                 // default "ardopcf"
    pub cmd_port: u16,                  // default 8515
    pub capture_device: String,         // e.g. "plughw:1,0"
    pub playback_device: String,        // e.g. "plughw:1,0"
    pub ptt_serial_path: Option<String>, // e.g. "/dev/ttyUSB0"
}
```

Persisted in the existing TOML config file alongside `[connect]`, `[packet]`,
etc. — new `[modem.ardop]` table.

## RADIO-1 consent handling

ARDOP transmits under the operator's callsign. Per the project's
[live-cms-testing-policy](../../live-cms-testing-policy.md) and the RADIO-1
pitfall entry, every transmission needs **explicit, per-invocation operator
consent**.

**Default UX for v0.2:**

1. **First-time Connect in a session** opens a modal:
   > **About to transmit on amateur radio.**
   > Target: `W7RMS-10`. Estimated airtime: ~2–8 minutes typical (depends on
   > traffic). Frequency under operator control via rig/ardopcf.
   > **I confirm I am the licensee or authorized to operate under
   > this callsign and authorize this transmission.** [Cancel] [Connect]
2. **Within the same modem-running session**, subsequent Connect/Disconnect
   actions do *not* re-prompt (the session is already operator-authorized).
3. **Stopping the modem** (Disconnect → SIGINT) clears the in-session
   authorization. The next Connect re-prompts.
4. The dock always shows the active peer and an ardopcf-process indicator
   so the operator knows tuxlink is currently running a transmitter.

Bounded airtime + abort-before-TX guarantees from the AX.25 RADIO-1
safety bundle (tuxlink-2y4) are inherited — operator can hit Disconnect at
any time and the SIGINT must reach ardopcf before any further TX frame.

## Data flow

```
Operator clicks Connect in dock
  │
  ▼
modem_ardop_connect({target:"W7RMS-10"})  ──► Rust: ensure ManagedModem(ardopcf)
                                                  │
                                                  ▼
                                              spawn child if not running, wait for cmd-socket ready
                                                  │
                                                  ▼
                                              ArdopTransport::connect_arq("W7RMS-10")
                                                  │  (emits ModemStatus events at each transition)
                                                  ▼
                                              ARQ session established
                                                  │
                                                  ▼
modem-status events ──► useModemStatus hook ──► ArdopDock renders live meters
```

## Test plan

- **Unit**: `useModemStatus.ts` handling of out-of-order events; `ArdopDock` state-machine rendering for each `ModemState`.
- **Component**: `ArdopDock.test.tsx` — vitest + testing-library — renders form when stopped, meters when running, consent modal on first Connect.
- **Integration**: backend `modem_ardop_connect` against the in-process mock TNC harness already inline in `src-tauri/src/winlink/modem/ardop/session.rs` (per PR #138 — `ScriptedPeer`-style `TcpListener` + threads mirroring `datalink.rs` / `telnet.rs` tests) — no real ardopcf, no radio.
- **RADIO-1 gate**: explicit test that `modem_ardop_connect` returns an error if the consent token (passed from frontend after modal) is missing. Backend never trusts the frontend to gate; the consent is a per-Connect arg.
- **No on-air tests in this PR.** On-air validation is operator-driven post-merge per the RADIO-1 protocol.

## Phasing within this PR

The PR is small enough to land in one go but commits are organized as:

1. `feat(connections): add 'ardop-hf' protocol to sessionTypes catalog`
2. `feat(modem): ModemStatus shape + useModemStatus React hook + Tauri event channel`
3. `feat(backend): modem_ardop_connect/disconnect/get_status + config_get/set_ardop Tauri commands`
4. `feat(modem): ArdopDock component + AppShell conditional 4-col grid`
5. `feat(settings): ARDOP section in SettingsPanel (binary/audio/PTT/cmd-port)`
6. `feat(modem): RADIO-1 consent modal + per-session authorization flow`
7. `test(modem): ArdopDock + useModemStatus + RADIO-1 gate coverage`

The Full Modem Console (charts, advanced controls) is **not** in this PR —
filed as a follow-up (likely a new bd issue once the dock ships).

## Open items for plan-writing

These are details the implementation plan resolves, not spec-level:

- ALSA device-list enumeration: shell out to `arecord -l` / `aplay -l` in a Tauri command, or freeform string input only? Default: shell out + freeform fallback.
- `ModemStatus` event polling interval (250 ms is a starting guess; tune against jitter).
- Where the consent modal lives: inline overlay in the dock, or a full-window dialog? Default: inline overlay scoped to the dock for low-friction.
- Backend Pat code strip (`pat_*.rs`) — *not* this spec's concern; tracked under `tuxlink-cyt`.

## What this spec does **not** decide (deliberately)

- Visual styling of the Full Modem Console (deferred to its own design pass).
- Whether VARA HF reuses `ArdopDock` directly or extends it (likely a small refactor when VARA lands).
- ardopcf binary bundling strategy (AppImage sidecar vs PATH-only — deferred to packaging work).

## References

- [PR #138 — ARDOP MVP transport](https://github.com/cameronzucker/tuxlink/pull/138) (the backend MVP this UI wires up).
- [docs/design/mockups/2026-05-17-modem-placements-v05.html](../../design/mockups/2026-05-17-modem-placements-v05.html) — the modem-compact / modem-full pattern this dock implements.
- [src/connections/TelnetCmsPanel.tsx](../../../src/connections/TelnetCmsPanel.tsx) — the closest existing pattern for a per-protocol config surface (referenced for code style, not layout — ARDOP uses the dock pattern, not the reading-pane-takeover pattern).
- ardopcf host-protocol reference: `dev/scratch/ax25-prior-art/wl2k-go/transport/ardop/`.
