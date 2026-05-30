# ARDOP HF UI — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the v0.2.0 ARDOP HF backend MVP (`ArdopTransport`, `ManagedModem` — already on `main`) into the tuxlink frontend, exposing a right-hand modem dock + Settings → ARDOP section + RADIO-1 consent flow, so an operator can dial an ARDOP gateway entirely from the UI.

**Architecture:** A generic `ModemStatus` contract (Rust struct + TS type, persisted in config) drives a new `ArdopDock` React component that lives in a 4th right-hand grid column. A `ModemStatusBroadcaster` background task owns the `ArdopTransport` handle, polls ardopcf's cmd-socket every 250 ms, and emits a `modem:status` Tauri event the dock subscribes to. The dial UX lives in the dock itself: a tiny Connect form when stopped, live meters when running. RADIO-1 consent is a per-session token the operator mints via a modal on first Connect; the backend rejects any `modem_ardop_connect` whose token doesn't match the in-process session token.

**Tech Stack:** Rust 2021 (`std::thread`, `std::sync::{mpsc, atomic}`, `nix` 0.28 for SIGINT), Tauri 2.x (`tauri::command`, `tauri::Manager::emit`), React 18 + TypeScript, vitest + testing-library/react for frontend tests, the existing in-process mock TNC harness in `src-tauri/src/winlink/modem/ardop/session.rs` for backend tests.

**Spec:** [docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md](../specs/2026-05-30-ardop-hf-ui-design.md). Read it first — this plan implements that spec, no surprises.

**bd issue:** `tuxlink-4ek`. Worktree at `worktrees/bd-tuxlink-4ek-ardop-ui` on branch `bd-tuxlink-4ek/ardop-ui` (already created, spec PR is `#147`).

---

## File map

Map of every file this plan creates or modifies. Lock in decomposition decisions here so each task can stay focused.

### New files

| Path | Responsibility |
|---|---|
| `src/connections/sessionTypes.ts` *(modify, not new)* | Add `'ardop-hf'` to `ProtocolId` and to the `cms` intent's protocols. |
| `src/modem/types.ts` | Generic `ModemStatus` + `ModemState` TS types. Mirror of the Rust struct. |
| `src/modem/useModemStatus.ts` | React hook subscribing to the `modem:status` Tauri event. Exposes the latest `ModemStatus` snapshot + a `loading` flag. |
| `src/modem/ArdopDock.tsx` | The right-hand dock React component. Renders Connect form when stopped, live meters when running. |
| `src/modem/ArdopDock.css` | Panel-local styles matching the approved mockup; tokens from `App.css :root`. |
| `src/modem/ArdopDock.test.tsx` | Component tests for the dock — stopped → connecting → running → disconnecting transitions. |
| `src/modem/consentModal.tsx` | The RADIO-1 first-Connect modal. Issues a session token on confirm. |
| `src/modem/consentModal.test.tsx` | Tests the modal renders, the confirm-only path, and the in-session token replay. |
| `src/modem/useConsent.ts` | Hook owning the in-session consent token. Cleared on `disconnect` / modem stop. |
| `src/connections/ArdopHfStub.tsx` | One-line stub for the reading-pane slot when ARDOP HF is sidebar-selected but modem is stopped: *"Use the modem dock on the right to dial."* + opens the dock if hidden. |
| `src-tauri/src/modem_status.rs` | `ModemStatus` Rust struct + `ModemStatusBroadcaster` background task. Owns the `ArdopTransport` handle; polls every 250 ms; emits via `app.emit("modem:status", …)`. |
| `src-tauri/src/modem_commands.rs` | All `modem_*` Tauri commands: `modem_ardop_connect`, `modem_ardop_disconnect`, `modem_get_status`, `config_get_ardop`, `config_set_ardop`. RADIO-1 token check lives here. |

### Modified files

| Path | Change |
|---|---|
| `src-tauri/src/config.rs` | Add `pub modem_ardop: Option<ArdopUiConfig>` to `Config`; new `ArdopUiConfig` struct (binary/capture/playback/ptt/cmd_port — frontend-shaped). |
| `src-tauri/src/lib.rs` | Register the 5 new commands in `invoke_handler!`. Spawn `ModemStatusBroadcaster` at app startup, hand it the `app_handle`. |
| `src-tauri/Cargo.toml` | No change expected (existing deps cover this: `tauri`, `serde`, `nix`, `tokio`-not-used). |
| `src/shell/AppShell.tsx` | (a) Add ARDOP-HF dispatch case (renders `ArdopHfStub` in the reading pane when sidebar selection = ARDOP HF and modem stopped). (b) Conditionally render `<ArdopDock />` as a 4th pane when `useModemStatus().state !== 'stopped'`. |
| `src/shell/AppShell.css` | New grid template variant: `.layout-b .panes--with-dock { grid-template-columns: 200px 340px 1fr 290px; }`. |
| `src/shell/SettingsPanel.tsx` | Append an `ARDOP HF` section (binary, capture, playback, PTT, cmd port). |
| `src/shell/SettingsPanel.css` | Minor — section heading reuses existing tokens. |

### Out of scope (separate plans)

- Full Modem Console (charts + advanced controls + sidebar `Modem → Console` entry). The spec defers it; a stub sidebar entry is acceptable later but NOT in this plan.
- ALSA device enumeration via `arecord -l` / `aplay -l` (the spec's default plan is freeform string entry; enumeration is a polish PR after the dock ships).
- VARA HF / Dire Wolf integration into the dock (the generic `ModemStatus` shape makes it cheap when those backends land, but they're separate).
- Backend Pat strip — `tuxlink-cyt`.

---

## Phase 0 — Worktree, branch, plan commit

The worktree + branch already exist (created during spec writing). This phase just commits this plan and confirms the implementer is set up.

### Task 0.1 — Confirm worktree state and commit the plan

**Files:**
- Create: `docs/superpowers/plans/2026-05-30-ardop-hf-ui-plan.md` (this file)

- [ ] **Step 1: From the main checkout, verify the worktree exists**

Run:
```bash
git -C /home/administrator/Code/tuxlink worktree list | grep tuxlink-4ek
```
Expected: `worktrees/bd-tuxlink-4ek-ardop-ui  <sha>  [bd-tuxlink-4ek/ardop-ui]`

- [ ] **Step 2: From the worktree, confirm spec + plan are present, branch is current**

```bash
WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-4ek-ardop-ui
ls "$WT/docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md"
ls "$WT/docs/superpowers/plans/2026-05-30-ardop-hf-ui-plan.md"
git -C "$WT" branch --show-current
```
Expected: both files exist; current branch `bd-tuxlink-4ek/ardop-ui`.

- [ ] **Step 3: Commit the plan**

```bash
git -C "$WT" add docs/superpowers/plans/2026-05-30-ardop-hf-ui-plan.md
git -C "$WT" commit -m "$(cat <<'EOF'
docs(plan): ARDOP HF UI — implementation plan (tuxlink-4ek)

Bite-sized TDD plan implementing the operator-approved spec
(docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md, PR #147).

7 phases match the spec's commit outline:
  0. setup (this commit)
  1. ModemStatus contract (Rust struct + TS types + Tauri event channel)
  2. Backend config persistence (ArdopUiConfig + 2 Tauri commands)
  3. Backend session lifecycle (3 Tauri commands + ModemStatusBroadcaster + RADIO-1 token check)
  4. Frontend dock component (ArdopDock + AppShell conditional grid + ArdopHfStub)
  5. Settings ARDOP section
  6. RADIO-1 consent modal + per-session token flow
  7. Integration tests + Codex adversarial review

Agent: <YOUR-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 4: Push**

```bash
git -C "$WT" push
```
Expected: push succeeds; PR #147 auto-updates with the plan commit.

---

## Phase 1 — `ModemStatus` contract (Rust struct + TS types + Tauri event channel)

Establish the wire contract between backend and frontend BEFORE writing any logic that produces or consumes it. Both sides serialize/deserialize to the same JSON shape.

### Task 1.1 — Rust `ModemStatus` struct + serialization

**Files:**
- Create: `src-tauri/src/modem_status.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod modem_status;`)
- Test: inline `#[cfg(test)]` module in `src-tauri/src/modem_status.rs`

- [ ] **Step 1: Write the failing test for `ModemStatus::stopped()` default + JSON shape**

Create `src-tauri/src/modem_status.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ModemState {
    Stopped,
    Spawning,
    Initializing,
    Idle,
    Connecting,
    ConnectedIrs,
    ConnectedIss,
    Disconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArqFlags {
    pub busy: bool,
    pub rx: bool,
    pub tx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModemStatus {
    pub state: ModemState,
    pub peer: Option<String>,
    pub mode: Option<String>,
    pub width_hz: Option<u32>,
    pub ptt_backend: Option<String>,   // "rts" | "cat" | "vox"
    pub sn_db: Option<f32>,
    pub vu_dbfs: Option<f32>,
    pub throughput_bps: Option<u32>,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub uptime_sec: u64,
    pub arq_flags: ArqFlags,
    pub last_error: Option<String>,
}

impl ModemStatus {
    pub fn stopped() -> Self {
        Self {
            state: ModemState::Stopped,
            peer: None, mode: None, width_hz: None, ptt_backend: None,
            sn_db: None, vu_dbfs: None, throughput_bps: None,
            bytes_rx: 0, bytes_tx: 0, uptime_sec: 0,
            arq_flags: ArqFlags { busy: false, rx: false, tx: false },
            last_error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_serializes_to_documented_shape() {
        let s = ModemStatus::stopped();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["state"], "stopped");
        assert_eq!(json["bytesRx"], 0);
        assert!(json["peer"].is_null());
        assert_eq!(json["arqFlags"]["busy"], false);
    }

    #[test]
    fn connected_irs_roundtrips() {
        let s = ModemStatus {
            state: ModemState::ConnectedIrs,
            peer: Some("W7RMS-10".into()),
            mode: Some("4FSK 500".into()),
            width_hz: Some(500),
            ptt_backend: Some("rts".into()),
            sn_db: Some(8.4), vu_dbfs: Some(-18.0), throughput_bps: Some(540),
            bytes_rx: 4128, bytes_tx: 982, uptime_sec: 222,
            arq_flags: ArqFlags { busy: true, rx: true, tx: false },
            last_error: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: ModemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // confirm the wire form has camelCase + kebab-case for state
        assert!(json.contains("\"state\":\"connected-irs\""));
        assert!(json.contains("\"bytesRx\":4128"));
    }
}
```

- [ ] **Step 2: Register the module in `lib.rs`**

In `src-tauri/src/lib.rs`, add `mod modem_status;` near the other `mod` declarations (find the existing block — likely under `mod compose_window;` etc.).

- [ ] **Step 3: Run the tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_status::tests -- --nocapture
```
Expected: 2 tests pass. If `serde_json` is missing from `[dev-dependencies]`, add it (or use the existing one — most projects have it transitively via `tauri`).

- [ ] **Step 4: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_status.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(modem): ModemStatus struct + ModemState enum + serde wire contract (tuxlink-4ek)"
```

### Task 1.2 — TS `ModemStatus` type + mirror of the Rust shape

**Files:**
- Create: `src/modem/types.ts`
- Test: `src/modem/types.test.ts` (one round-trip test against a fixture matching the Rust json)

- [ ] **Step 1: Create the TS type file**

`src/modem/types.ts`:
```ts
// Wire-mirror of src-tauri/src/modem_status.rs. Field names match the Rust
// #[serde(rename_all = "camelCase")] output.
export type ModemState =
  | 'stopped'
  | 'spawning'
  | 'initializing'
  | 'idle'
  | 'connecting'
  | 'connected-irs'
  | 'connected-iss'
  | 'disconnecting'
  | 'error';

export interface ArqFlags {
  busy: boolean;
  rx: boolean;
  tx: boolean;
}

export interface ModemStatus {
  state: ModemState;
  peer: string | null;
  mode: string | null;
  widthHz: number | null;
  pttBackend: string | null;     // "rts" | "cat" | "vox"
  snDb: number | null;
  vuDbfs: number | null;
  throughputBps: number | null;
  bytesRx: number;
  bytesTx: number;
  uptimeSec: number;
  arqFlags: ArqFlags;
  lastError: string | null;
}

export const STOPPED: ModemStatus = {
  state: 'stopped',
  peer: null, mode: null, widthHz: null, pttBackend: null,
  snDb: null, vuDbfs: null, throughputBps: null,
  bytesRx: 0, bytesTx: 0, uptimeSec: 0,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
};
```

- [ ] **Step 2: Write the wire-contract round-trip test**

`src/modem/types.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import type { ModemStatus } from './types';
import { STOPPED } from './types';

describe('ModemStatus wire contract', () => {
  it('STOPPED matches the documented stopped shape from modem_status.rs', () => {
    expect(STOPPED.state).toBe('stopped');
    expect(STOPPED.peer).toBeNull();
    expect(STOPPED.arqFlags).toEqual({ busy: false, rx: false, tx: false });
  });

  it('accepts a sample connected-irs payload from the Rust serialization fixture', () => {
    const wire = {
      state: 'connected-irs',
      peer: 'W7RMS-10',
      mode: '4FSK 500',
      widthHz: 500,
      pttBackend: 'rts',
      snDb: 8.4, vuDbfs: -18.0, throughputBps: 540,
      bytesRx: 4128, bytesTx: 982, uptimeSec: 222,
      arqFlags: { busy: true, rx: true, tx: false },
      lastError: null,
    } as ModemStatus;
    expect(wire.state).toBe('connected-irs');
    expect(wire.peer).toBe('W7RMS-10');
  });
});
```

- [ ] **Step 3: Run the tests**

```bash
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-4ek-ardop-ui vitest run src/modem/types.test.ts
```
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git -C "$WT" add src/modem/types.ts src/modem/types.test.ts
git -C "$WT" commit -m "feat(modem): TS ModemStatus type mirroring the Rust wire shape (tuxlink-4ek)"
```

### Task 1.3 — `useModemStatus` hook subscribing to the `modem:status` Tauri event

**Files:**
- Create: `src/modem/useModemStatus.ts`
- Test: `src/modem/useModemStatus.test.ts`

- [ ] **Step 1: Write the failing test**

`src/modem/useModemStatus.test.ts`:
```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useModemStatus } from './useModemStatus';
import { STOPPED, type ModemStatus } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const listenMock = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: (e: { payload: ModemStatus }) => void) =>
    listenMock(event, cb),
}));

describe('useModemStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockResolvedValue(() => {}); // unsubscribe fn
  });

  it('starts with STOPPED and loading=true, fetches initial via modem_get_status', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(STOPPED);
    const { result } = renderHook(() => useModemStatus());
    expect(result.current.status.state).toBe('stopped');
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(invoke).toHaveBeenCalledWith('modem_get_status');
  });

  it('updates on modem:status events', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(STOPPED);
    let captured: ((e: { payload: ModemStatus }) => void) | null = null;
    listenMock.mockImplementation((_event: string, cb) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    const { result } = renderHook(() => useModemStatus());
    await waitFor(() => expect(result.current.loading).toBe(false));
    act(() => {
      captured!({ payload: { ...STOPPED, state: 'connecting' } });
    });
    expect(result.current.status.state).toBe('connecting');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
pnpm -C "$WT" vitest run src/modem/useModemStatus.test.ts
```
Expected: FAIL — `useModemStatus` doesn't exist.

- [ ] **Step 3: Implement the hook**

`src/modem/useModemStatus.ts`:
```ts
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { STOPPED, type ModemStatus } from './types';

export const MODEM_STATUS_EVENT = 'modem:status';

export function useModemStatus() {
  const [status, setStatus] = useState<ModemStatus>(STOPPED);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;

    // Initial snapshot — for the case where the dock mounts mid-session.
    invoke<ModemStatus>('modem_get_status')
      .then((s) => { if (!cancelled) setStatus(s); })
      .catch(() => { /* leave STOPPED */ })
      .finally(() => { if (!cancelled) setLoading(false); });

    // Subscribe to live updates.
    listen<ModemStatus>(MODEM_STATUS_EVENT, (e) => {
      if (!cancelled) setStatus(e.payload);
    }).then((u) => { unsubscribe = u; });

    return () => { cancelled = true; unsubscribe?.(); };
  }, []);

  return { status, loading };
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
pnpm -C "$WT" vitest run src/modem/useModemStatus.test.ts
```
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src/modem/useModemStatus.ts src/modem/useModemStatus.test.ts
git -C "$WT" commit -m "feat(modem): useModemStatus React hook subscribing to modem:status event (tuxlink-4ek)"
```

### Task 1.4 — Add `'ardop-hf'` to `sessionTypes.ts` (catalog entry, no panel dispatch yet)

**Files:**
- Modify: `src/connections/sessionTypes.ts`
- Modify: `src/connections/sessionTypes.test.ts` (extend existing tests)

- [ ] **Step 1: Read the existing tests**

```bash
cat "$WT/src/connections/sessionTypes.test.ts"
```
Read which assertions exist; add complementary tests for ARDOP HF.

- [ ] **Step 2: Append the ARDOP HF test**

In `src/connections/sessionTypes.test.ts` (append a new `describe` block or new test):
```ts
import { protocolsFor, isBuilt } from './sessionTypes';

describe('ARDOP HF catalog entry', () => {
  it('exposes ardop-hf as a built protocol under cms intent', () => {
    const protos = protocolsFor('cms');
    const ardop = protos.find((p) => p.id === 'ardop-hf');
    expect(ardop).toBeDefined();
    expect(ardop?.label).toBe('ARDOP HF');
    expect(ardop?.built).toBe(true);
  });

  it('isBuilt returns true for cms × ardop-hf', () => {
    expect(isBuilt({ sessionType: 'cms', protocol: 'ardop-hf' })).toBe(true);
  });
});
```

- [ ] **Step 3: Run — should fail (ardop-hf not in ProtocolId union)**

```bash
pnpm -C "$WT" vitest run src/connections/sessionTypes.test.ts
```
Expected: TypeScript error or runtime FAIL.

- [ ] **Step 4: Update `sessionTypes.ts`**

```ts
// In src/connections/sessionTypes.ts:
export type ProtocolId = 'telnet' | 'packet' | 'vara-hf' | 'vara-fm' | 'ardop-hf';

// Add near the other protocol constants (line ~10):
const ARD = { id: 'ardop-hf' as const, label: 'ARDOP HF' };

// In the 'cms' intent's protocols list, between PKT and VHF:
{
  id: 'cms',
  label: 'Winlink (CMS)',
  blurb: 'Sync your global mailbox. Credentialed secure-login.',
  built: true,
  protocols: [
    { ...TEL, built: true },
    { ...PKT, built: true },
    { ...ARD, built: true },     // ← new
    { ...VHF, built: false },
    { ...VFM, built: false },
  ],
},
```

- [ ] **Step 5: Run tests**

```bash
pnpm -C "$WT" vitest run src/connections/sessionTypes.test.ts
```
Expected: all tests pass (existing + 2 new).

- [ ] **Step 6: Commit**

```bash
git -C "$WT" add src/connections/sessionTypes.ts src/connections/sessionTypes.test.ts
git -C "$WT" commit -m "feat(connections): add 'ardop-hf' protocol to sessionTypes catalog (tuxlink-4ek)"
```

---

## Phase 2 — Backend ARDOP config persistence

A frontend-shaped struct + 2 Tauri commands. The frontend works with separate `binary` / `capture` / `playback` / `ptt` fields; the backend translates them into ardopcf's `extra_args: Vec<String>` at spawn time (a thin shim, Phase 3).

### Task 2.1 — `ArdopUiConfig` struct in `config.rs`

**Files:**
- Modify: `src-tauri/src/config.rs`
- Test: inline `#[cfg(test)]` in `config.rs` (existing pattern)

- [ ] **Step 1: Read the existing `Config` struct**

```bash
sed -n '12,60p' "$WT/src-tauri/src/config.rs"
```
Confirm location to add new field + new struct.

- [ ] **Step 2: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `config.rs`:
```rust
#[test]
fn ardop_ui_config_round_trips_through_toml() {
    let cfg = ArdopUiConfig {
        binary: "ardopcf".into(),
        capture_device: "plughw:1,0".into(),
        playback_device: "plughw:1,0".into(),
        ptt_serial_path: Some("/dev/ttyUSB0".into()),
        cmd_port: 8515,
    };
    let toml = toml::to_string(&cfg).unwrap();
    let back: ArdopUiConfig = toml::from_str(&toml).unwrap();
    assert_eq!(back.binary, "ardopcf");
    assert_eq!(back.cmd_port, 8515);
    assert_eq!(back.ptt_serial_path.as_deref(), Some("/dev/ttyUSB0"));
}

#[test]
fn config_with_ardop_some_then_none_round_trips() {
    let mut c = Config::default();
    c.modem_ardop = Some(ArdopUiConfig {
        binary: "ardopcf".into(),
        capture_device: "plughw:1,0".into(),
        playback_device: "plughw:1,0".into(),
        ptt_serial_path: None,
        cmd_port: 8515,
    });
    let toml = toml::to_string(&c).unwrap();
    assert!(toml.contains("[modem_ardop]"));
    let back: Config = toml::from_str(&toml).unwrap();
    assert!(back.modem_ardop.is_some());
}
```

- [ ] **Step 3: Add the struct + field**

In `src-tauri/src/config.rs`, before the test block:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArdopUiConfig {
    pub binary: String,
    pub capture_device: String,
    pub playback_device: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptt_serial_path: Option<String>,
    pub cmd_port: u16,
}

impl Default for ArdopUiConfig {
    fn default() -> Self {
        Self {
            binary: "ardopcf".into(),
            capture_device: String::new(),
            playback_device: String::new(),
            ptt_serial_path: None,
            cmd_port: 8515,
        }
    }
}
```

In the `Config` struct (line ~16):
```rust
pub struct Config {
    // ... existing fields ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modem_ardop: Option<ArdopUiConfig>,
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib config::tests -- --nocapture
```
Expected: all existing tests + 2 new tests pass.

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src-tauri/src/config.rs
git -C "$WT" commit -m "feat(config): ArdopUiConfig struct + Config.modem_ardop field (tuxlink-4ek)"
```

### Task 2.2 — `config_get_ardop` / `config_set_ardop` Tauri commands

**Files:**
- Create: `src-tauri/src/modem_commands.rs`
- Modify: `src-tauri/src/lib.rs` (`mod modem_commands;` + register handlers)

- [ ] **Step 1: Create the module skeleton**

`src-tauri/src/modem_commands.rs`:
```rust
//! Tauri commands for modem (ARDOP) operations.
//!
//! RADIO-1: `modem_ardop_connect` requires a per-session consent token issued
//! by the frontend's RADIO-1 modal. The backend rejects any connect attempt
//! whose token doesn't match the current session token. See Phase 6.

use crate::config::{self, ArdopUiConfig, Config};

#[tauri::command]
pub fn config_get_ardop() -> ArdopUiConfig {
    let cfg = config::load().unwrap_or_default();
    cfg.modem_ardop.unwrap_or_default()
}

#[tauri::command]
pub fn config_set_ardop(value: ArdopUiConfig) -> Result<(), String> {
    let mut cfg = config::load().unwrap_or_default();
    cfg.modem_ardop = Some(value);
    config::save(&cfg).map_err(|e| format!("save failed: {e}"))
}
```

> **Note for the implementer:** `config::load()` and `config::save()` are stand-ins for whatever functions `src-tauri/src/config.rs` exposes today. Grep that file (`grep -nE 'pub fn (load|save|read|write|persist)' src-tauri/src/config.rs`) and substitute the actual function names. If the project uses a `ConfigStore` handle stashed in `tauri::State`, follow that pattern instead (mirror `config_set_privacy` in `ui_commands.rs`).

- [ ] **Step 2: Register the module + commands in `lib.rs`**

In `src-tauri/src/lib.rs`:
- Add `mod modem_commands;` near other `mod` declarations.
- In the `tauri::generate_handler![...]` list, add:
  ```rust
  crate::modem_commands::config_get_ardop,
  crate::modem_commands::config_set_ardop,
  ```

- [ ] **Step 3: Write an integration test against the commands**

Add to `src-tauri/src/modem_commands.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_persists_through_config() {
        let _g = config::test_lock_for_isolated_tests();    // ← if the project has such a helper; else use a TempDir
        let initial = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
        };
        config_set_ardop(initial.clone()).unwrap();
        let read = config_get_ardop();
        assert_eq!(read, initial);
    }
}
```

> **Note:** if `config::test_lock_for_isolated_tests()` doesn't exist, the implementer should create a `TempDir` and override `TUXLINK_CONFIG_DIR` env var (per `tuxlink-efo` — `config.rs` honors that env var on `main`). Pattern: `let tmp = tempfile::tempdir().unwrap(); std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path());` (then remove the env var at end of test, or use `temp_env::with_var`).

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_commands::tests -- --nocapture
```
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_commands.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(backend): config_get_ardop / config_set_ardop Tauri commands (tuxlink-4ek)"
```

---

## Phase 3 — Backend session lifecycle (`modem_*` commands + status broadcaster)

The thread that owns the `ArdopTransport` handle, the 3 lifecycle commands, the RADIO-1 token check, and the `modem:status` event emission.

### Task 3.1 — `ModemSession` shared state (mutex-guarded handle + consent token)

**Files:**
- Modify: `src-tauri/src/modem_status.rs` (add `ModemSession` struct)
- Test: inline tests

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/modem_status.rs` `#[cfg(test)] mod tests`:
```rust
#[test]
fn modem_session_starts_stopped_with_no_token() {
    let s = ModemSession::new();
    assert_eq!(s.status_snapshot().state, ModemState::Stopped);
    assert!(!s.has_valid_token("any-token"));
}

#[test]
fn modem_session_accepts_minted_token_and_invalidates_on_clear() {
    let s = ModemSession::new();
    let t = s.mint_consent_token();
    assert!(s.has_valid_token(&t));
    s.clear_consent_token();
    assert!(!s.has_valid_token(&t));
}
```

- [ ] **Step 2: Add `ModemSession`**

In `src-tauri/src/modem_status.rs`:
```rust
use std::sync::{Arc, Mutex};

/// Shared per-app modem session state. Wraps the current `ModemStatus` snapshot
/// + the in-process RADIO-1 consent token. `Arc<ModemSession>` is stored in
/// Tauri state and shared between command handlers and the broadcaster.
#[derive(Debug)]
pub struct ModemSession {
    inner: Mutex<ModemSessionInner>,
}

#[derive(Debug)]
struct ModemSessionInner {
    status: ModemStatus,
    consent_token: Option<String>,
    // The actual ArdopTransport handle is added in Task 3.2 once we have a
    // sane Option<...> + Send story.
}

impl ModemSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ModemSessionInner {
                status: ModemStatus::stopped(),
                consent_token: None,
            }),
        }
    }

    pub fn status_snapshot(&self) -> ModemStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn set_status(&self, s: ModemStatus) {
        self.inner.lock().unwrap().status = s;
    }

    /// Generate + remember a new consent token. Returns the token so the
    /// frontend can pass it to `modem_ardop_connect`.
    pub fn mint_consent_token(&self) -> String {
        // 16 random hex chars — enough for in-process uniqueness; not a secret.
        let token: String = (0..16)
            .map(|_| {
                let n: u8 = rand::random::<u8>() & 0xF;
                std::char::from_digit(n as u32, 16).unwrap()
            })
            .collect();
        self.inner.lock().unwrap().consent_token = Some(token.clone());
        token
    }

    pub fn has_valid_token(&self, candidate: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.consent_token.as_deref() == Some(candidate)
    }

    pub fn clear_consent_token(&self) {
        self.inner.lock().unwrap().consent_token = None;
    }
}
```

> **Note on `rand`:** if it isn't already in `Cargo.toml`, the implementer should add `rand = "0.8"` to `[dependencies]`. Alternative: a deterministic counter is fine for in-process uniqueness (the token is not a security boundary — it's an intra-process replay check).

- [ ] **Step 3: Run the tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_status::tests -- --nocapture
```
Expected: 4 tests pass (2 from Task 1.1 + 2 new).

- [ ] **Step 4: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_status.rs src-tauri/Cargo.toml
git -C "$WT" commit -m "feat(modem): ModemSession shared state + RADIO-1 consent token mint/check (tuxlink-4ek)"
```

### Task 3.2 — `modem_get_status` + `modem_ardop_disconnect` (lifecycle without spawn yet)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs`
- Modify: `src-tauri/src/lib.rs` (register; pass `Arc<ModemSession>` as `State<...>`)
- Test: inline tests

- [ ] **Step 1: Write the failing test for `modem_get_status` against a `ModemSession`**

Append to `src-tauri/src/modem_commands.rs` tests:
```rust
#[test]
fn modem_get_status_returns_session_snapshot() {
    use crate::modem_status::ModemSession;
    let session = std::sync::Arc::new(ModemSession::new());
    let s = modem_get_status_inner(&session);
    assert_eq!(s.state, crate::modem_status::ModemState::Stopped);
}

#[test]
fn modem_ardop_disconnect_clears_consent_when_session_was_running() {
    use crate::modem_status::{ModemSession, ModemState, ModemStatus};
    let session = std::sync::Arc::new(ModemSession::new());
    let token = session.mint_consent_token();
    // simulate a running session
    let mut s = ModemStatus::stopped();
    s.state = ModemState::ConnectedIdle.into(); /* coerce — see note */
    session.set_status(s);
    modem_ardop_disconnect_inner(&session).unwrap();
    // After disconnect, consent token must be invalidated.
    assert!(!session.has_valid_token(&token));
}
```

> **Note:** the test references `ModemState::ConnectedIdle`. The enum from Task 1.1 used `Idle` (not `ConnectedIdle`). If the implementer prefers a different variant name, adjust here AND in the wire types. The point is the test must construct a representative "running" snapshot.

- [ ] **Step 2: Implement the `_inner` helpers + `#[tauri::command]` wrappers**

In `src-tauri/src/modem_commands.rs`:
```rust
use std::sync::Arc;
use tauri::State;
use crate::modem_status::{ModemSession, ModemStatus};

pub fn modem_get_status_inner(session: &Arc<ModemSession>) -> ModemStatus {
    session.status_snapshot()
}

pub fn modem_ardop_disconnect_inner(session: &Arc<ModemSession>) -> Result<(), String> {
    session.clear_consent_token();
    session.set_status(ModemStatus::stopped());
    // TODO(Task 3.3): tell the actual ArdopTransport to shutdown + SIGINT ardopcf.
    Ok(())
}

#[tauri::command]
pub fn modem_get_status(session: State<'_, Arc<ModemSession>>) -> ModemStatus {
    modem_get_status_inner(&session)
}

#[tauri::command]
pub fn modem_ardop_disconnect(session: State<'_, Arc<ModemSession>>) -> Result<(), String> {
    modem_ardop_disconnect_inner(&session)
}
```

- [ ] **Step 3: Wire `ModemSession` into Tauri state**

In `src-tauri/src/lib.rs`, in the `tauri::Builder::default()...` chain, before `.invoke_handler(...)`:
```rust
.manage(std::sync::Arc::new(crate::modem_status::ModemSession::new()))
```

And in the handler list, add:
```rust
crate::modem_commands::modem_get_status,
crate::modem_commands::modem_ardop_disconnect,
```

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_commands::tests -- --nocapture
```
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_commands.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(backend): modem_get_status + modem_ardop_disconnect commands + ModemSession state (tuxlink-4ek)"
```

### Task 3.3 — `modem_ardop_connect` (consent-gated; spawn ardopcf via `ArdopTransport::with_managed_modem`)

**Files:**
- Modify: `src-tauri/src/modem_commands.rs`
- Modify: `src-tauri/src/modem_status.rs` (add `transport: Option<Box<dyn ModemTransport>>` to `ModemSessionInner` + setter; keep behind a feature-friendly accessor)
- Test: inline test using a stub `ModemTransport` impl

- [ ] **Step 1: Add transport handle slot to `ModemSession`**

In `src-tauri/src/modem_status.rs` add to `ModemSessionInner`:
```rust
transport: Option<Box<dyn crate::winlink::modem::ModemTransport>>,
```
Update `ModemSession::new()` to init `transport: None`. Add:
```rust
impl ModemSession {
    pub fn install_transport(&self, t: Box<dyn crate::winlink::modem::ModemTransport>) {
        self.inner.lock().unwrap().transport = Some(t);
    }

    pub fn take_transport(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        self.inner.lock().unwrap().transport.take()
    }
}
```

> **Note:** `ModemTransport: Send` per `src-tauri/src/winlink/modem/mod.rs:47`. The `Box<dyn ModemTransport>` is `Send` so the `Mutex<ModemSessionInner>` remains `Sync`.

- [ ] **Step 2: Write the failing test against a stub transport**

In `src-tauri/src/modem_commands.rs` tests:
```rust
#[test]
fn modem_ardop_connect_rejects_when_token_missing() {
    use crate::modem_status::ModemSession;
    let session = std::sync::Arc::new(ModemSession::new());
    let err = modem_ardop_connect_inner(
        &session, "W7RMS-10", "wrong-token", &test_ardop_ui_config(),
    ).unwrap_err();
    assert!(err.contains("consent"), "got: {err}");
}

#[test]
fn modem_ardop_connect_succeeds_with_valid_token() {
    use crate::modem_status::{ModemSession, ModemState};
    let session = std::sync::Arc::new(ModemSession::new());
    let token = session.mint_consent_token();
    // For the unit test we install a stub transport via test-only seam
    // (see Step 3 — implementation passes a closure for test injection).
    let result = modem_ardop_connect_inner_with_factory(
        &session, "W7RMS-10", &token, &test_ardop_ui_config(),
        |_cfg, _target| Ok(stub_transport()),
    );
    assert!(result.is_ok());
    assert_eq!(session.status_snapshot().state, ModemState::ConnectedIrs);
}

// Helpers
fn test_ardop_ui_config() -> crate::config::ArdopUiConfig {
    crate::config::ArdopUiConfig {
        binary: "ardopcf-stub".into(),
        capture_device: "plughw:0,0".into(),
        playback_device: "plughw:0,0".into(),
        ptt_serial_path: None,
        cmd_port: 8515,
    }
}

fn stub_transport() -> Box<dyn crate::winlink::modem::ModemTransport> {
    // A minimal in-test impl that always succeeds. Real impl lives in
    // src-tauri/src/winlink/modem/ardop/transport.rs.
    use crate::winlink::modem::{ModemTransport, ReadWrite, InitConfig, ConnectInfo, SessionError};
    use std::time::Duration;

    struct Stub;
    impl ModemTransport for Stub {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> { Ok(()) }
        fn connect_arq(&mut self, _t: &str, _r: u32, _d: Duration) -> Result<ConnectInfo, SessionError> {
            // ConnectInfo construction is module-private — adapt to whatever
            // its public constructor or Default impl looks like.
            Ok(ConnectInfo::default())
        }
        fn disconnect(&mut self, _d: Duration) -> Result<(), SessionError> { Ok(()) }
        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "stub"))
        }
    }
    Box::new(Stub)
}
```

> **Note:** `ConnectInfo` may not have a `Default` impl on `main`. If not, construct one explicitly with the fields its struct exposes (read `src-tauri/src/winlink/modem/ardop/session.rs:238` for the actual shape).

- [ ] **Step 3: Implement the connect function with a factory seam for testing**

In `src-tauri/src/modem_commands.rs`:
```rust
use std::time::Duration;
use crate::winlink::modem::{ModemTransport, InitConfig};
use crate::winlink::modem::ardop::ArdopConfig;
use crate::winlink::modem::ardop::transport::ArdopTransport;
use crate::modem_status::ModemState;

const CONNECT_DEADLINE: Duration = Duration::from_secs(120);  // worst-case airtime cap (RADIO-1 review item)
const CONNECT_REPEAT: u32 = 3;

pub fn modem_ardop_connect_inner(
    session: &Arc<ModemSession>,
    target: &str,
    consent_token: &str,
    ardop_ui: &ArdopUiConfig,
) -> Result<(), String> {
    modem_ardop_connect_inner_with_factory(
        session, target, consent_token, ardop_ui,
        |cfg, _target| ArdopTransport::with_managed_modem(cfg)
            .map(|t| Box::new(t) as Box<dyn ModemTransport>)
            .map_err(|e| format!("spawn failed: {e}")),
    )
}

pub fn modem_ardop_connect_inner_with_factory<F>(
    session: &Arc<ModemSession>,
    target: &str,
    consent_token: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    if !session.has_valid_token(consent_token) {
        return Err("RADIO-1: missing or invalid consent token; mint one via the Connect modal first".into());
    }

    // Translate the frontend-shaped ArdopUiConfig into the backend ArdopConfig.
    let mut extra_args: Vec<String> = vec![
        ardop_ui.cmd_port.to_string(),
        ardop_ui.capture_device.clone(),
        ardop_ui.playback_device.clone(),
    ];
    if let Some(ref ptt) = ardop_ui.ptt_serial_path {
        // Prepend -p PTT before the positional args.
        extra_args.insert(0, ptt.clone());
        extra_args.insert(0, "-p".into());
    }
    let cfg = ArdopConfig {
        binary: std::path::PathBuf::from(&ardop_ui.binary),
        extra_args,
        cmd_port: ardop_ui.cmd_port,
        data_port: ardop_ui.cmd_port + 1,
        audio_device_path: None,
    };

    // Mark spawning.
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    session.set_status(snap);

    let mut transport = make_transport(cfg, target).map_err(|e| {
        let mut s = ModemStatus::stopped();
        s.state = ModemState::Error;
        s.last_error = Some(e.clone());
        session.set_status(s);
        e
    })?;

    // Init + ARQ connect.
    transport.init(&InitConfig::default())
        .map_err(|e| format!("init failed: {e:?}"))?;
    let _info = transport.connect_arq(target, CONNECT_REPEAT, CONNECT_DEADLINE)
        .map_err(|e| format!("ARQ connect failed: {e:?}"))?;

    // Install the live transport handle in the session.
    session.install_transport(transport);

    // Bump status to connected-irs (the broadcaster in Task 3.4 will replace
    // this with real-time data; this is the initial-state snapshot).
    let mut s = session.status_snapshot();
    s.state = ModemState::ConnectedIrs;
    s.peer = Some(target.to_string());
    session.set_status(s);

    Ok(())
}

#[tauri::command]
pub fn modem_ardop_connect(
    session: State<'_, Arc<ModemSession>>,
    target: String,
    consent_token: String,
) -> Result<(), String> {
    let ardop_ui = config_get_ardop();
    if ardop_ui.capture_device.is_empty() || ardop_ui.playback_device.is_empty() {
        return Err("ARDOP audio devices not configured — open Settings → ARDOP first".into());
    }
    modem_ardop_connect_inner(&session, &target, &consent_token, &ardop_ui)
}
```

> **Notes:** (a) `InitConfig::default()` — if there's no `Default` impl, construct one with mycall + gridsquare read from `config::load()` (the identity config). The implementer must check the actual `InitConfig` shape at `src-tauri/src/winlink/modem/ardop/session.rs:139`. (b) Once Task 3.3 lands, the test for `Task 3.2`'s `modem_ardop_disconnect_inner` needs to be expanded to actually call `transport.disconnect(...)` and `transport.shutdown(...)` (via `take_transport` → call methods → drop). Loop back and update.

- [ ] **Step 4: Register the command in `lib.rs`**

Add `crate::modem_commands::modem_ardop_connect` to the `invoke_handler!` list.

- [ ] **Step 5: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_commands::tests -- --nocapture
```
Expected: token-missing rejection passes; happy-path with stub transport passes.

- [ ] **Step 6: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_commands.rs src-tauri/src/modem_status.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(backend): modem_ardop_connect with RADIO-1 token gate + ArdopTransport spawn (tuxlink-4ek)"
```

### Task 3.4 — `ModemStatusBroadcaster` (background poll + emit `modem:status`)

**Files:**
- Modify: `src-tauri/src/modem_status.rs`
- Modify: `src-tauri/src/lib.rs` (spawn at app startup)
- Test: inline test using a recording emitter trait

- [ ] **Step 1: Write the failing test**

Append to `modem_status.rs` tests:
```rust
#[test]
fn broadcaster_emits_initial_stopped_snapshot() {
    use std::sync::Arc;
    let session = Arc::new(ModemSession::new());
    let mut recorded: Vec<ModemStatus> = Vec::new();
    let emit = |s: ModemStatus| recorded.push(s);
    let one_tick = ModemStatusBroadcaster::tick_for_test(&session, &emit);
    assert!(one_tick.is_ok());
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].state, ModemState::Stopped);
}
```

- [ ] **Step 2: Implement the broadcaster**

In `src-tauri/src/modem_status.rs`:
```rust
use std::time::Duration;

pub const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub const STATUS_EVENT: &str = "modem:status";

pub struct ModemStatusBroadcaster;

impl ModemStatusBroadcaster {
    /// Run the broadcaster on a dedicated thread. Emits via the closure.
    /// In production the closure is `|s| app.emit(STATUS_EVENT, s).ok();`.
    pub fn spawn<F>(session: Arc<ModemSession>, emit: F)
    where
        F: Fn(ModemStatus) + Send + 'static,
    {
        std::thread::spawn(move || loop {
            let snap = session.status_snapshot();
            emit(snap);
            std::thread::sleep(STATUS_POLL_INTERVAL);
        });
    }

    /// Run a single tick — used by unit tests to avoid sleeping.
    #[cfg(test)]
    pub fn tick_for_test<F>(session: &Arc<ModemSession>, emit: &F) -> std::io::Result<()>
    where
        F: Fn(ModemStatus),
    {
        emit(session.status_snapshot());
        Ok(())
    }
}
```

> **Note (future):** the broadcaster currently just emits the cached snapshot. The richer flow (poll the ardopcf cmd-socket for live S/N, throughput, etc.) is filed as a follow-up bd issue once the dock ships and we can see what jitter looks like in practice. For now the snapshot is updated by `modem_ardop_connect` (Task 3.3) and the future cmd-socket reader, both writing through `session.set_status(...)`.

- [ ] **Step 3: Spawn at app startup in `lib.rs`**

In the `tauri::Builder::default()` chain, after `.manage(Arc::new(ModemSession::new()))`:
```rust
.setup(|app| {
    let session = app.state::<Arc<ModemSession>>().inner().clone();
    let app_handle = app.handle().clone();
    ModemStatusBroadcaster::spawn(session, move |s| {
        let _ = app_handle.emit(STATUS_EVENT, s);
    });
    Ok(())
})
```

> **Note:** if `.setup` is already used by another bootstrap step, fold this in. Imports needed: `use tauri::Manager;` and `use crate::modem_status::{ModemStatusBroadcaster, ModemSession, STATUS_EVENT};`.

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem_status::tests -- --nocapture
```
Expected: 5 tests pass (2 from 1.1 + 2 from 3.1 + 1 new).

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src-tauri/src/modem_status.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(backend): ModemStatusBroadcaster background thread + modem:status emit (tuxlink-4ek)"
```

---

## Phase 4 — Frontend `ArdopDock` component + AppShell conditional 4-col grid

### Task 4.1 — `ArdopDock` (stopped state — Connect form only)

**Files:**
- Create: `src/modem/ArdopDock.tsx`
- Create: `src/modem/ArdopDock.css`
- Create: `src/modem/ArdopDock.test.tsx`

- [ ] **Step 1: Write the failing test (stopped state)**

`src/modem/ArdopDock.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ArdopDock } from './ArdopDock';
import { STOPPED } from './types';

vi.mock('./useModemStatus', () => ({
  useModemStatus: () => ({ status: STOPPED, loading: false }),
}));

describe('<ArdopDock> stopped', () => {
  it('renders the Connect form when status.state === stopped', () => {
    render(<ArdopDock />);
    expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
    expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /connect/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run — should fail (`ArdopDock` doesn't exist)**

```bash
pnpm -C "$WT" vitest run src/modem/ArdopDock.test.tsx
```
Expected: FAIL.

- [ ] **Step 3: Implement the stopped state**

`src/modem/ArdopDock.tsx`:
```tsx
import { useState } from 'react';
import { useModemStatus } from './useModemStatus';
import './ArdopDock.css';

export function ArdopDock() {
  const { status } = useModemStatus();
  const [target, setTarget] = useState('');

  return (
    <aside className="ardop-dock" data-testid="ardop-dock-root">
      <header className="ardop-dock-h">
        <span className="ardop-dock-state-dot" data-state={status.state} />
        <span className="ardop-dock-name">MODEM · ARDOP HF</span>
        <span className="ardop-dock-sub">ardopcf · :8515</span>
      </header>

      {status.state === 'stopped' && (
        <section className="ardop-dock-section">
          <div className="ardop-dock-section-h">Target station</div>
          <label className="ardop-dock-field">
            Target callsign
            <input
              className="ardop-dock-input"
              data-testid="ardop-target"
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="W7RMS-10"
            />
          </label>
          <button
            className="ardop-dock-btn ardop-dock-btn-primary"
            disabled={target.trim() === ''}
            // onClick wired in Task 6.2 (consent modal flow)
          >
            Connect
          </button>
        </section>
      )}
    </aside>
  );
}
```

- [ ] **Step 4: Add minimal CSS**

`src/modem/ArdopDock.css` — copy the structural tokens used in the mockup (`docs/superpowers/specs/2026-05-30-ardop-hf-ui-dock-active.png`). Key tokens:
```css
.ardop-dock { background: var(--surface); border-left: 1px solid var(--border-strong); width: 290px; overflow: auto; }
.ardop-dock-h { padding: 8px 12px; border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px; font-size: 11px; }
.ardop-dock-name { font-weight: 600; }
.ardop-dock-sub { color: var(--text-faint); margin-left: auto; font-family: var(--mono); font-size: 10px; }
.ardop-dock-state-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-faint); }
.ardop-dock-state-dot[data-state="stopped"] { background: var(--text-faint); }
.ardop-dock-state-dot[data-state="connected-irs"], .ardop-dock-state-dot[data-state="connected-iss"] { background: var(--success); box-shadow: 0 0 4px var(--success); }
.ardop-dock-section { padding: 10px 12px; border-bottom: 1px solid var(--border-soft); }
.ardop-dock-section-h { font-size: 9px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 8px; }
.ardop-dock-field { display: flex; flex-direction: column; gap: 3px; font-size: 10.5px; color: var(--text-faint); margin-bottom: 8px; }
.ardop-dock-input { background: #0a1014; border: 1px solid var(--border); border-radius: 3px; color: var(--text); font-family: var(--mono); font-size: 12.5px; padding: 4px 7px; height: 26px; }
.ardop-dock-btn { font-size: 12px; padding: 5px 12px; height: 26px; border-radius: 3px; border: 1px solid var(--border-strong); background: var(--surface-2); color: var(--text); cursor: pointer; }
.ardop-dock-btn-primary { background: var(--accent); border-color: var(--accent); color: #1a0e02; font-weight: 600; }
.ardop-dock-btn[disabled] { opacity: 0.5; cursor: default; }
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
pnpm -C "$WT" vitest run src/modem/ArdopDock.test.tsx
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git -C "$WT" add src/modem/ArdopDock.tsx src/modem/ArdopDock.css src/modem/ArdopDock.test.tsx
git -C "$WT" commit -m "feat(modem): ArdopDock stopped-state render (Connect form) (tuxlink-4ek)"
```

### Task 4.2 — `ArdopDock` running state (live meters, state grid, mono status block)

**Files:**
- Modify: `src/modem/ArdopDock.tsx`
- Modify: `src/modem/ArdopDock.css`
- Modify: `src/modem/ArdopDock.test.tsx`

- [ ] **Step 1: Add the failing tests**

Append to `ArdopDock.test.tsx`:
```tsx
import { afterEach } from 'vitest';
const useModemStatus = vi.fn();
vi.mock('./useModemStatus', () => ({ useModemStatus: () => useModemStatus() }));

afterEach(() => useModemStatus.mockReset());

describe('<ArdopDock> running', () => {
  it('renders meters + peer + state grid when status.state === connected-irs', () => {
    useModemStatus.mockReturnValue({
      status: {
        state: 'connected-irs',
        peer: 'W7RMS-10',
        mode: '4FSK 500',
        widthHz: 500,
        pttBackend: 'rts',
        snDb: 8.4, vuDbfs: -18.0, throughputBps: 540,
        bytesRx: 4128, bytesTx: 982, uptimeSec: 222,
        arqFlags: { busy: true, rx: true, tx: false },
        lastError: null,
      },
      loading: false,
    });
    render(<ArdopDock />);
    expect(screen.getByText(/W7RMS-10/)).toBeInTheDocument();
    expect(screen.getByText(/\+8\.4 dB/)).toBeInTheDocument();
    expect(screen.getByText(/540 bps/)).toBeInTheDocument();
    // ARQ state grid
    expect(screen.getByTestId('arq-cell-CON')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-IRS')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-BUSY')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-TX')).toHaveAttribute('data-on', 'false');
  });

  it('does NOT render the Connect form when running', () => {
    useModemStatus.mockReturnValue({ status: { /* connected-irs */ } /* same as above */, loading: false });
    render(<ArdopDock />);
    expect(screen.queryByRole('button', { name: /connect/i })).not.toBeInTheDocument();
  });
});
```

> **Note:** trim the second test's status payload to a reusable helper — both tests share the same fixture.

- [ ] **Step 2: Run — should fail**

Expected: missing data-testids, missing rendered text.

- [ ] **Step 3: Implement the running state**

Add to `ArdopDock.tsx`:
```tsx
const ARQ_CELLS = ['DISC', 'CON', 'IDLE', 'ISS', 'IRS', 'BUSY', 'RX', 'TX', 'DREQ'] as const;
type ArqCell = (typeof ARQ_CELLS)[number];

function isCellOn(cell: ArqCell, s: ModemStatus): boolean {
  switch (cell) {
    case 'DISC':  return s.state === 'stopped' || s.state === 'idle' || s.state === 'disconnecting';
    case 'CON':   return s.state === 'connected-irs' || s.state === 'connected-iss';
    case 'IDLE':  return s.state === 'idle';
    case 'ISS':   return s.state === 'connected-iss';
    case 'IRS':   return s.state === 'connected-irs';
    case 'BUSY':  return s.arqFlags.busy;
    case 'RX':    return s.arqFlags.rx;
    case 'TX':    return s.arqFlags.tx;
    case 'DREQ':  return s.state === 'connecting';
  }
}

// Add to the JSX, AFTER the stopped-state block:
{status.state !== 'stopped' && (
  <>
    <section className="ardop-dock-section">
      <div className="ardop-dock-section-h">ARQ state</div>
      <div className="ardop-arq-grid">
        {ARQ_CELLS.map((cell) => (
          <div
            key={cell}
            className="ardop-arq-cell"
            data-testid={`arq-cell-${cell}`}
            data-on={isCellOn(cell, status)}
          >
            {cell}
          </div>
        ))}
      </div>
    </section>

    <section className="ardop-dock-section">
      <div className="ardop-dock-section-h">Live</div>
      {status.snDb !== null && (
        <Meter label="S/N" value={`${status.snDb > 0 ? '+' : ''}${status.snDb.toFixed(1)} dB`} />
      )}
      {status.vuDbfs !== null && (
        <Meter label="VU input" value={`${status.vuDbfs.toFixed(0)} dBFS`} />
      )}
      {status.throughputBps !== null && (
        <Meter label="Throughput" value={`${status.throughputBps} bps`} warn />
      )}
    </section>

    <section className="ardop-dock-section">
      <pre className="ardop-mono-stat">
{`Peer   ${status.peer ?? '—'}
Mode   ${status.mode ?? '—'}
Width  ${status.widthHz !== null ? `${status.widthHz} Hz` : '—'}
PTT    ${status.pttBackend ?? '—'}
RX     ${status.bytesRx} B  ·  TX ${status.bytesTx} B
Up     ${fmtUptime(status.uptimeSec)}`}
      </pre>
    </section>
  </>
)}

// Helpers (place above the component or in a sibling file):
function Meter({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div className={`ardop-meter${warn ? ' warn' : ''}`}>
      <span className="ardop-meter-k">{label}</span>
      <span className="ardop-meter-v">{value}</span>
    </div>
  );
}

function fmtUptime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return m === 0 ? `${s}s` : `${m}m ${s}s`;
}
```

- [ ] **Step 4: Extend `ArdopDock.css`**

```css
.ardop-arq-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px; }
.ardop-arq-cell { font-family: var(--mono); font-size: 10.5px; padding: 5px 0; text-align: center; border: 1px solid var(--border-strong); border-radius: 3px; color: var(--text-faint); }
.ardop-arq-cell[data-on="true"] { background: rgba(93, 214, 160, 0.14); border-color: var(--success); color: var(--success); }
.ardop-meter { display: flex; justify-content: space-between; align-items: baseline; padding: 4px 0; }
.ardop-meter-k { font-size: 9.5px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-faint); }
.ardop-meter-v { font-size: 13px; font-family: var(--mono); color: var(--text); font-weight: 600; }
.ardop-meter.warn .ardop-meter-v { color: var(--accent-2); }
.ardop-mono-stat { font-family: var(--mono); font-size: 10.5px; color: var(--text-dim); padding: 8px 10px; background: var(--bg); border: 1px solid var(--border); border-radius: 3px; line-height: 1.7; margin: 0; white-space: pre; }
```

- [ ] **Step 5: Run tests**

```bash
pnpm -C "$WT" vitest run src/modem/ArdopDock.test.tsx
```
Expected: all dock tests pass.

- [ ] **Step 6: Commit**

```bash
git -C "$WT" add src/modem/ArdopDock.tsx src/modem/ArdopDock.css src/modem/ArdopDock.test.tsx
git -C "$WT" commit -m "feat(modem): ArdopDock running state — ARQ grid + meters + mono status block (tuxlink-4ek)"
```

### Task 4.3 — AppShell conditional 4-col grid (dock appears when modem !== stopped)

**Files:**
- Modify: `src/shell/AppShell.tsx`
- Modify: `src/shell/AppShell.css`
- Create: `src/connections/ArdopHfStub.tsx`
- Modify: `src/shell/AppShell.test.tsx` (or add a new file)

- [ ] **Step 1: Write the failing test**

In `src/shell/AppShell.test.tsx` (or a new file `AppShell.modemDock.test.tsx`):
```tsx
import { render, screen } from '@testing-library/react';
import { useModemStatus } from '../modem/useModemStatus';
import { AppShell } from './AppShell';

vi.mock('../modem/useModemStatus');

it('does not render the dock when modem is stopped', () => {
  (useModemStatus as any).mockReturnValue({ status: { state: 'stopped' }, loading: false });
  render(<AppShell />);
  expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
});

it('renders the dock + applies the 4-col grid class when modem is running', () => {
  (useModemStatus as any).mockReturnValue({
    status: { state: 'connected-irs', peer: 'W7RMS-10', arqFlags: { busy: false, rx: false, tx: false }, /* ... */ },
    loading: false,
  });
  render(<AppShell />);
  expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
  expect(screen.getByTestId('shell-panes')).toHaveClass('panes--with-dock');
});
```

- [ ] **Step 2: Run — should fail (no conditional rendering, no class swap)**

- [ ] **Step 3: Wire the dock + grid swap in `AppShell.tsx`**

In `AppShell.tsx`:
```tsx
import { useModemStatus } from '../modem/useModemStatus';
import { ArdopDock } from '../modem/ArdopDock';

// Inside the component, after status loads:
const { status: modemStatus } = useModemStatus();
const dockVisible = modemStatus.state !== 'stopped';

// In the .panes div, conditionally add the class:
<div
  className={`panes${dockVisible ? ' panes--with-dock' : ''}`}
  data-testid="shell-panes"
>
  {/* ... existing FolderSidebar / MessageList / reading-pane dispatch ... */}
  {dockVisible && <ArdopDock />}
</div>
```

- [ ] **Step 4: Add the grid variant in `AppShell.css`**

```css
.layout-b .panes--with-dock {
  grid-template-columns: 200px 340px 1fr 290px;
}
```

(Keeps the existing `.layout-b .panes { grid-template-columns: 200px 380px 1fr; }` for the dock-off state.)

- [ ] **Step 5: Add the `ArdopHfStub` for the reading-pane slot**

`src/connections/ArdopHfStub.tsx`:
```tsx
export function ArdopHfStub() {
  return (
    <div className="reading-pane ardop-hf-stub" data-testid="ardop-hf-stub">
      <p style={{ color: 'var(--text-faint)', padding: '14px 16px' }}>
        ARDOP HF is configured. Use the <strong>modem dock on the right</strong> to dial a target station.
      </p>
    </div>
  );
}
```

In `AppShell.tsx`'s dispatch (around line 230, alongside `if (sessionType === 'cms' && protocol === 'telnet')`):
```tsx
if (sessionType === 'cms' && protocol === 'ardop-hf') {
  return <ArdopHfStub />;
}
```

- [ ] **Step 6: Run tests**

```bash
pnpm -C "$WT" vitest run src/shell/AppShell.test.tsx src/shell/AppShell.modemDock.test.tsx
```
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git -C "$WT" add src/shell/AppShell.tsx src/shell/AppShell.css src/connections/ArdopHfStub.tsx src/shell/AppShell.test.tsx
git -C "$WT" commit -m "feat(shell): conditional 4-col grid + ArdopDock mount + ARDOP HF reading-pane stub (tuxlink-4ek)"
```

---

## Phase 5 — Settings panel: ARDOP section

### Task 5.1 — `SettingsPanel` ARDOP section (form + persist)

**Files:**
- Modify: `src/shell/SettingsPanel.tsx`
- Modify: `src/shell/SettingsPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

Append to `SettingsPanel.test.tsx`:
```tsx
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

it('renders the ARDOP HF section with binary/capture/playback/PTT/cmd-port fields', async () => {
  (invoke as any).mockResolvedValue({
    binary: 'ardopcf', captureDevice: 'plughw:1,0', playbackDevice: 'plughw:1,0',
    pttSerialPath: null, cmdPort: 8515,
  });
  render(<SettingsPanel onClose={() => {}} />);
  expect(await screen.findByLabelText(/ardopcf binary/i)).toBeInTheDocument();
  expect(screen.getByLabelText(/capture device/i)).toBeInTheDocument();
  expect(screen.getByLabelText(/playback device/i)).toBeInTheDocument();
  expect(screen.getByLabelText(/ptt serial/i)).toBeInTheDocument();
  expect(screen.getByLabelText(/cmd port/i)).toBeInTheDocument();
});

it('persists via config_set_ardop on blur', async () => {
  (invoke as any).mockImplementation((cmd: string) => {
    if (cmd === 'config_get_ardop') return Promise.resolve({
      binary: 'ardopcf', captureDevice: '', playbackDevice: '',
      pttSerialPath: null, cmdPort: 8515,
    });
    return Promise.resolve(undefined);
  });
  render(<SettingsPanel onClose={() => {}} />);
  const capture = await screen.findByLabelText(/capture device/i);
  fireEvent.change(capture, { target: { value: 'plughw:2,0' } });
  fireEvent.blur(capture);
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith('config_set_ardop', expect.objectContaining({
      value: expect.objectContaining({ captureDevice: 'plughw:2,0' }),
    }));
  });
});
```

- [ ] **Step 2: Run — fails (no section exists)**

- [ ] **Step 3: Add the section to `SettingsPanel.tsx`**

Append a new `<section>` modeled on the existing GPS section pattern:
```tsx
// Inside the SettingsPanel component, after the GPS section:
const [ardop, setArdop] = useState<ArdopUiConfig>({
  binary: 'ardopcf', captureDevice: '', playbackDevice: '',
  pttSerialPath: null, cmdPort: 8515,
});
useEffect(() => {
  void invoke<ArdopUiConfig>('config_get_ardop').then(setArdop).catch(() => {});
}, []);
const persistArdop = (next: ArdopUiConfig) => {
  setArdop(next);
  void invoke('config_set_ardop', { value: next }).catch(() => {});
};

// In JSX:
<section className="settings-section">
  <h3 className="settings-section-h">ARDOP HF</h3>
  <label className="settings-field">
    ardopcf binary
    <input
      type="text"
      value={ardop.binary}
      onChange={(e) => setArdop({ ...ardop, binary: e.target.value })}
      onBlur={() => persistArdop(ardop)}
    />
  </label>
  <label className="settings-field">
    Capture device (ALSA)
    <input
      type="text"
      value={ardop.captureDevice}
      placeholder="plughw:1,0"
      onChange={(e) => setArdop({ ...ardop, captureDevice: e.target.value })}
      onBlur={() => persistArdop(ardop)}
    />
  </label>
  <label className="settings-field">
    Playback device (ALSA)
    <input
      type="text"
      value={ardop.playbackDevice}
      placeholder="plughw:1,0"
      onChange={(e) => setArdop({ ...ardop, playbackDevice: e.target.value })}
      onBlur={() => persistArdop(ardop)}
    />
  </label>
  <label className="settings-field">
    PTT serial path (optional — leave blank for VOX)
    <input
      type="text"
      value={ardop.pttSerialPath ?? ''}
      placeholder="/dev/ttyUSB0"
      onChange={(e) => setArdop({ ...ardop, pttSerialPath: e.target.value || null })}
      onBlur={() => persistArdop(ardop)}
    />
  </label>
  <label className="settings-field">
    Cmd port
    <input
      type="number"
      value={ardop.cmdPort}
      onChange={(e) => setArdop({ ...ardop, cmdPort: parseInt(e.target.value, 10) || 8515 })}
      onBlur={() => persistArdop(ardop)}
    />
  </label>
</section>
```

Add the TS type at the top of the file:
```ts
interface ArdopUiConfig {
  binary: string;
  captureDevice: string;
  playbackDevice: string;
  pttSerialPath: string | null;
  cmdPort: number;
}
```

- [ ] **Step 4: Run tests**

```bash
pnpm -C "$WT" vitest run src/shell/SettingsPanel.test.tsx
```
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git -C "$WT" add src/shell/SettingsPanel.tsx src/shell/SettingsPanel.test.tsx
git -C "$WT" commit -m "feat(settings): ARDOP HF section — binary/capture/playback/PTT/cmd-port (tuxlink-4ek)"
```

---

## Phase 6 — RADIO-1 consent modal + per-session token flow

### Task 6.1 — `useConsent` hook (owns in-session token)

**Files:**
- Create: `src/modem/useConsent.ts`
- Create: `src/modem/useConsent.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConsent } from './useConsent';

describe('useConsent', () => {
  it('starts with no token; granted() returns the token after grant', () => {
    const { result } = renderHook(() => useConsent());
    expect(result.current.token).toBeNull();
    act(() => { result.current.grant('abc123'); });
    expect(result.current.token).toBe('abc123');
  });

  it('clear() wipes the token', () => {
    const { result } = renderHook(() => useConsent());
    act(() => { result.current.grant('abc123'); });
    act(() => { result.current.clear(); });
    expect(result.current.token).toBeNull();
  });
});
```

- [ ] **Step 2: Implement**

`src/modem/useConsent.ts`:
```ts
import { useState, useCallback } from 'react';

export function useConsent() {
  const [token, setToken] = useState<string | null>(null);
  const grant = useCallback((t: string) => setToken(t), []);
  const clear = useCallback(() => setToken(null), []);
  return { token, grant, clear };
}
```

> **Note:** in real use, the hook is hoisted to AppShell-level so the token survives modal close + Connect call. A future refinement may move it into a React context for cross-component access; for v1 the dock owns the consent state.

- [ ] **Step 3: Run + commit**

```bash
pnpm -C "$WT" vitest run src/modem/useConsent.test.ts
git -C "$WT" add src/modem/useConsent.ts src/modem/useConsent.test.ts
git -C "$WT" commit -m "feat(modem): useConsent hook owning the in-session RADIO-1 token (tuxlink-4ek)"
```

### Task 6.2 — Consent modal + wire Connect button → mint token → `modem_ardop_connect`

**Files:**
- Create: `src/modem/ConsentModal.tsx`
- Create: `src/modem/ConsentModal.test.tsx`
- Modify: `src/modem/ArdopDock.tsx` (wire onClick)

- [ ] **Step 1: Write the failing test for the modal**

`src/modem/ConsentModal.test.tsx`:
```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { ConsentModal } from './ConsentModal';

it('renders the RADIO-1 warning + Cancel/Connect buttons', () => {
  render(<ConsentModal target="W7RMS-10" onCancel={() => {}} onConfirm={() => {}} />);
  expect(screen.getByText(/About to transmit on amateur radio/i)).toBeInTheDocument();
  expect(screen.getByText(/W7RMS-10/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /^connect$/i })).toBeInTheDocument();
});

it('Connect button is disabled until the acknowledgement checkbox is ticked', () => {
  const onConfirm = vi.fn();
  render(<ConsentModal target="W7RMS-10" onCancel={() => {}} onConfirm={onConfirm} />);
  const connect = screen.getByRole('button', { name: /^connect$/i });
  expect(connect).toBeDisabled();
  fireEvent.click(screen.getByRole('checkbox'));
  expect(connect).not.toBeDisabled();
  fireEvent.click(connect);
  expect(onConfirm).toHaveBeenCalled();
});
```

- [ ] **Step 2: Implement the modal**

`src/modem/ConsentModal.tsx`:
```tsx
import { useState } from 'react';

export interface ConsentModalProps {
  target: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConsentModal({ target, onCancel, onConfirm }: ConsentModalProps) {
  const [ack, setAck] = useState(false);
  return (
    <div className="ardop-consent-overlay" role="dialog" aria-modal="true">
      <div className="ardop-consent-modal">
        <h3>About to transmit on amateur radio</h3>
        <p>Target: <strong>{target}</strong>. Estimated airtime: ~2–8 minutes typical (depends on traffic). Frequency under operator control via your rig + ardopcf.</p>
        <label className="ardop-consent-ack">
          <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
          I confirm I am the licensee or authorized to operate under this callsign and authorize this transmission.
        </label>
        <div className="ardop-consent-actions">
          <button onClick={onCancel}>Cancel</button>
          <button disabled={!ack} onClick={onConfirm}>Connect</button>
        </div>
      </div>
    </div>
  );
}
```

(Add minimal CSS in `ArdopDock.css` for `.ardop-consent-overlay` / `.ardop-consent-modal` — fixed-position overlay, centered card.)

- [ ] **Step 3: Wire the Connect button in `ArdopDock.tsx`**

```tsx
import { invoke } from '@tauri-apps/api/core';
import { useConsent } from './useConsent';
import { ConsentModal } from './ConsentModal';

// Inside ArdopDock:
const consent = useConsent();
const [showConsent, setShowConsent] = useState(false);
const [connecting, setConnecting] = useState(false);

const onConnectClick = () => {
  if (consent.token) {
    // Already authorized this session — go straight to connect.
    doConnect(consent.token);
  } else {
    setShowConsent(true);
  }
};

const doConnect = async (tok: string) => {
  setConnecting(true);
  try {
    await invoke('modem_ardop_connect', { target, consentToken: tok });
  } catch (e) {
    // Backend errors surface in the next modem:status event with state="error".
    console.error('connect failed', e);
  } finally {
    setConnecting(false);
  }
};

const onConsentConfirm = () => {
  // Mint a token on the backend, store it, kick off connect.
  // We synthesize the token client-side here and the backend mints the matching
  // one via mintConsentToken before connect — see Note below.
  const tok = Math.random().toString(36).slice(2, 18);
  consent.grant(tok);
  setShowConsent(false);
  doConnect(tok);
};

// In the JSX, after the Connect button:
{showConsent && (
  <ConsentModal target={target} onCancel={() => setShowConsent(false)} onConfirm={onConsentConfirm} />
)}
```

> **Critical token semantics note:** the backend MUST mint the token, not trust a frontend-generated one — otherwise the consent gate is pure theater. The flow has to be: (a) modal confirm → frontend calls `modem_mint_consent` → backend mints + remembers → returns the token → frontend stores → frontend calls `modem_ardop_connect` with the token. The implementer should add a `modem_mint_consent` Tauri command in `modem_commands.rs` that calls `session.mint_consent_token()` and returns the string. Update this Task to:
> ```ts
> const onConsentConfirm = async () => {
>   const tok = await invoke<string>('modem_mint_consent');
>   consent.grant(tok);
>   setShowConsent(false);
>   doConnect(tok);
> };
> ```
> AND add `modem_mint_consent` to the backend + `invoke_handler!`. The current Task 3.1 minted the token via direct API; expose it as a Tauri command in this Task.

- [ ] **Step 4: Add `modem_mint_consent` backend command**

In `modem_commands.rs`:
```rust
#[tauri::command]
pub fn modem_mint_consent(session: State<'_, Arc<ModemSession>>) -> String {
    session.mint_consent_token()
}
```
Register in `lib.rs`.

Add a test:
```rust
#[test]
fn mint_then_connect_with_matching_token_succeeds() {
    let session = std::sync::Arc::new(ModemSession::new());
    let token = session.mint_consent_token();
    let result = modem_ardop_connect_inner_with_factory(
        &session, "W7RMS-10", &token, &test_ardop_ui_config(),
        |_cfg, _t| Ok(stub_transport()),
    );
    assert!(result.is_ok());
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib modem -- --nocapture
pnpm -C "$WT" vitest run src/modem/
```
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git -C "$WT" add src/modem/ConsentModal.tsx src/modem/ConsentModal.test.tsx src/modem/ArdopDock.tsx src/modem/ArdopDock.css src-tauri/src/modem_commands.rs src-tauri/src/lib.rs
git -C "$WT" commit -m "feat(modem): RADIO-1 consent modal + modem_mint_consent backend command (tuxlink-4ek)"
```

---

## Phase 7 — Integration tests + Codex adversarial review

### Task 7.1 — End-to-end integration test (frontend → mocked Tauri → backend stub transport)

**Files:**
- Create: `src/modem/ArdopDock.integration.test.tsx`

- [ ] **Step 1: Write a hand-rolled integration that drives the dock through stopped → consent → connecting → connected**

```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArdopDock } from './ArdopDock';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

it('stopped → mint consent → connect → dock shows connected', async () => {
  let emitListener: ((e: { payload: any }) => void) | null = null;
  (listen as any).mockImplementation((_e: string, cb: any) => {
    emitListener = cb;
    return Promise.resolve(() => {});
  });
  (invoke as any).mockImplementation((cmd: string) => {
    if (cmd === 'modem_get_status') return Promise.resolve({ state: 'stopped', /* ... */ });
    if (cmd === 'modem_mint_consent') return Promise.resolve('test-token-123');
    if (cmd === 'modem_ardop_connect') {
      // simulate backend emitting connected status after connect resolves
      setTimeout(() => emitListener!({ payload: { state: 'connected-irs', peer: 'W7RMS-10', /* ... */ } }), 0);
      return Promise.resolve(undefined);
    }
    return Promise.resolve(undefined);
  });

  render(<ArdopDock />);
  await waitFor(() => expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument());

  fireEvent.change(screen.getByTestId('ardop-target'), { target: { value: 'W7RMS-10' } });
  fireEvent.click(screen.getByRole('button', { name: /connect/i }));
  // Consent modal appears.
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(screen.getByRole('button', { name: /^connect$/i }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith('modem_ardop_connect', {
    target: 'W7RMS-10', consentToken: 'test-token-123',
  }));
  await waitFor(() => expect(screen.queryByText(/W7RMS-10/)).toBeInTheDocument());
});
```

- [ ] **Step 2: Run**

Expected: passes (any failures here mean the dock-modal-connect wire isn't quite right; fix until green).

- [ ] **Step 3: Commit**

```bash
git -C "$WT" add src/modem/ArdopDock.integration.test.tsx
git -C "$WT" commit -m "test(modem): end-to-end dock integration — stopped→consent→connect→connected (tuxlink-4ek)"
```

### Task 7.2 — Run the full test suite

- [ ] **Step 1: Frontend**

```bash
pnpm -C "$WT" vitest run
```
Expected: all green.

- [ ] **Step 2: Backend**

```bash
cargo test --manifest-path "$WT/src-tauri/Cargo.toml" --lib
```
Expected: all green.

- [ ] **Step 3: Lint / type-check**

```bash
pnpm -C "$WT" exec tsc --noEmit
cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" --lib -- -D warnings
```
Expected: no errors. Fix and re-run if anything fails.

### Task 7.3 — Codex adversarial review (cross-provider)

Per project policy (see `feedback_no_carveout_on_cross_provider_adrev` in memory), Codex adrev is mandatory before merge for anything with a safety-critical (RADIO-1) component. Run on the full diff.

- [ ] **Step 1: Capture the diff against `main`**

```bash
cd "$WT"
git diff main..HEAD > /tmp/ardop-ui-diff.patch
wc -l /tmp/ardop-ui-diff.patch
```

- [ ] **Step 2: Round 1 — RADIO-1 + consent flow attack angle**

```bash
npx --yes @openai/codex review --base main \
  "Review for RADIO-1 / safety regressions. The Connect button mints a consent token via modem_mint_consent and passes it to modem_ardop_connect. Look for: (1) ways for a TX path to fire without a valid consent token; (2) ways the token persists across stops/restarts (it must not); (3) any code path that bypasses the consent gate; (4) ways the backend trusts frontend-supplied data inappropriately. Find P0 issues." \
  2>&1 | tee dev/adversarial/2026-05-30-ardop-ui-radio1-codex.md
```
> Note: `dev/adversarial/` is `.gitignore`d. Don't commit the transcript; summarize findings + dispositions in the PR body.

- [ ] **Step 3: Round 2 — concurrency / `ModemSession` mutex correctness**

```bash
npx --yes @openai/codex review --base main \
  "Review ModemSession's Mutex<ModemSessionInner> usage. Look for: (1) lock-order inversions; (2) panics inside .lock().unwrap() that could poison the mutex; (3) any path that holds the lock across an I/O call (deadlock risk); (4) Send/Sync correctness of Box<dyn ModemTransport>." \
  2>&1 | tee dev/adversarial/2026-05-30-ardop-ui-concurrency-codex.md
```

- [ ] **Step 4: Round 3 — error handling + recoverability**

```bash
npx --yes @openai/codex review --base main \
  "Review error handling. Look for: (1) modem_ardop_connect failure leaving the session in a state where Connect can never succeed without app restart; (2) ardopcf spawn failures that leak the process; (3) places where Result<...,String> swallows error context the operator would need; (4) the broadcaster panicking and stopping all status updates silently." \
  2>&1 | tee dev/adversarial/2026-05-30-ardop-ui-errors-codex.md
```

- [ ] **Step 5: Apply Codex findings**

For each P0/P1 finding from rounds 1-3, either fix in code (commit per finding with `fix(modem): <finding> (Codex round N)`) or explicitly disposition in the PR body as "deferred to follow-up bd issue tuxlink-XXX".

- [ ] **Step 6: Final test run + clippy + push**

```bash
pnpm -C "$WT" vitest run && cargo test --manifest-path "$WT/src-tauri/Cargo.toml" --lib && cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" --lib -- -D warnings
git -C "$WT" push
```

### Task 7.4 — Operator smoke prep

This task does NOT run the smoke (RADIO-1 forbids agent-initiated transmission). It prepares the smoke instructions the operator will follow.

- [ ] **Step 1: Write smoke instructions to the PR body**

In the PR body (next task), include a section:
```markdown
## Operator on-air smoke (RADIO-1 — operator only)

Prereqs: ardopcf installed on PATH; rig keyed via PTT serial; HF antenna; lab gateway target.

1. Build: `pnpm -C worktrees/bd-tuxlink-4ek-ardop-ui tauri dev` (port :1420; kill any other Vite first).
2. Complete the wizard.
3. Open Settings → ARDOP HF; fill capture/playback/PTT. Save.
4. Sidebar → Winlink (CMS) → ARDOP HF. The right-hand dock appears.
5. In the dock, type the lab target callsign. Hit Connect.
6. RADIO-1 modal appears. Tick the box. Confirm.
7. Watch the dock: state transitions Spawning → Initializing → Connecting → ConnectedIrs.
8. Disconnect via the (forthcoming) View → Show Modem Console → Disconnect, OR by closing the app.
9. Confirm ardopcf process exits cleanly (`pgrep ardopcf` returns nothing).
```

### Task 7.5 — Open the implementation PR

- [ ] **Step 1: Push (if not already)**

```bash
git -C "$WT" push
```

- [ ] **Step 2: Open the PR**

```bash
cd "$WT"
gh pr create --base main --head bd-tuxlink-4ek/ardop-ui \
  --title "[<MONIKER>] feat(modem): ARDOP HF UI — right-hand dock + Settings + RADIO-1 consent (tuxlink-4ek)" \
  --body "$(cat <<'EOF'
Implements docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md (spec PR #147 — same branch).

## Summary
- Right-hand modem dock (~290px) appears when ardopcf runs; reading-pane shrinks but mailbox + reading pane stay visible.
- Dial UX lives in the dock itself: Connect form when stopped, live meters when running.
- Operator-specific config (binary, ALSA capture/playback, optional PTT, cmd port) in Settings → ARDOP HF.
- RADIO-1 consent: per-session modal on first Connect; backend rejects connect attempts without a valid mint-time token.
- Generic ModemStatus shape so VARA / Dire Wolf can plug in later.

## Codex adversarial review
- Round 1 (RADIO-1 + consent) — findings + dispositions: ...
- Round 2 (concurrency / mutex) — findings + dispositions: ...
- Round 3 (error handling) — findings + dispositions: ...

## Operator on-air smoke
(See the "Operator on-air smoke" section above — operator-only per RADIO-1.)

## Out of scope (filed as follow-ups)
- Full Modem Console (charts, advanced controls) — new bd issue once dock ships.
- ALSA device enumeration via `arecord -l` — operator currently enters strings freeform.
- VARA HF wiring into the same dock — when the VARA backend lands.

Closes tuxlink-4ek.

Agent: <MONIKER>
EOF
)"
```

---

## Self-review

**Spec coverage check:** every spec section maps to a task —

- "ARDOP-HF as new protocol under Winlink (CMS)" → Task 1.4 ✓
- "Right-hand modem dock (~290 px)" → Tasks 4.1, 4.2, 4.3 ✓
- "Dial UX in the dock itself" → Tasks 4.1 (stopped form), 6.2 (Connect wire) ✓
- "Operator-specific config in Settings → ARDOP" → Tasks 2.1, 2.2, 5.1 ✓
- "Full Modem Console deferred" → explicit out-of-scope ✓
- "RADIO-1 consent: per-session modal on first Connect" → Tasks 3.3 (token gate), 6.1 (hook), 6.2 (modal + wire) ✓
- "Generic ModemStatusFeed shape" → Tasks 1.1, 1.2 ✓
- "Backend Pat code strip — NOT this spec's concern" → explicitly not in plan ✓
- All 12 entries in the spec's "New / changed files" table are claimed by a task in this plan ✓
- The 5 Tauri commands and 1 event in the spec are all implemented (Tasks 2.2, 3.2, 3.3, 3.4, 6.2) ✓

**Placeholder scan:** no "TBD" / "implement later" / "fill in details" left. Two `<MONIKER>` placeholders in commit-message templates are deliberate — the executor substitutes their session moniker.

**Type consistency:** `ModemStatus` field names match across Rust (`#[serde(rename_all = "camelCase")]`), TS type, and tests. `ModemState` variants — Rust uses `ConnectedIrs`/`ConnectedIss` enum variants which serialize to `connected-irs`/`connected-iss` on the wire; TS string literals match. `ArdopUiConfig` field names match across backend struct, TS type, and Tauri command args. `consentToken` is the wire name (camelCase) for what the Rust function signature names `consent_token`.

**Spec gap I should call out before execution:** the spec doesn't explicitly mention `modem_mint_consent` as a command — it's implicit in the consent flow. Task 6.2's note makes the mint-on-backend rule explicit (security-critical: a frontend-generated token would make the gate theater).
