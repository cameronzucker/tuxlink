// src/radio/modes/ArdopRadioPanel.tsx
//
// Spec §5.3 — ARDOP Winlink panel. Replaces the legacy ArdopDock +
// ArdopHfStub pair (P4.6 deletes both). Composes RadioPanel chrome
// + Connect form + Live + Signal + Session log + Actions.
//
// Live data: useModemStatus subscribes to the backend's 4 Hz
// `modem:status` event stream. The S/N + throughput sparklines pull
// from rolling 60-sample buffers (`useSampleHistory`) that tick once
// per second off the latest status snapshot. Quality + recent-frame
// state come directly from ModemStatus (PINGACK-derived; tuxlink-1637,
// P4.3) and from a derived state-driven frame history.
//
// Operator click on Connect is the Part 97 consent (per memory
// no-tuxlink-added-safeguards + operator decision bd tuxlink-8gq3).
// The RADIO-1 per-invocation consent modal was a tuxlink-added
// safeguard that has been dropped — preserved comment for rationale.
//
// Open WebGUI: ardopcf's built-in WebGUI listens on `cmd_port - 1`
// per its USAGE doc. We read the live cmd_port from config rather
// than hardcoding 8514 so the link tracks operator overrides. Guard
// mirrors the backend's build_ardop_extra_args check: cmd_port < 2
// yields an unbindable webgui_port, so we surface an error rather
// than open a dead URL.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { readLastTarget, writeLastTarget } from '../../connections/connectDispatch';
import { RadioPanel, type RadioPanelState } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { SignalSection } from '../sections/SignalSection';
import { Sparkline } from '../charts/Sparkline';
import { useSampleHistory } from '../useSampleHistory';
import { useModemStatus } from '../../modem/useModemStatus';
import type { ModemState, ModemStatus } from '../../modem/types';
import type { ArdopFrameType } from '../charts/FrameRibbon';
import { friendlyAudioOptions, type AudioOption } from './audioDevices';
import { AllowedStationsEditor } from '../sections/AllowedStationsEditor';
import { ListenArmButton } from '../sections/ListenArmButton';
import { useListenerState } from '../sections/useListenerState';
import { useActiveIdentity } from '../../shell/useIdentities';
import { FavoritesTabs } from '../../favorites/FavoritesTabs';
import { useFavorites } from '../../favorites/useFavorites';
import { listenGatewayPrefill } from '../../favorites/prefillEvent';
import { tsLocal } from '../../favorites/ts-local';
import type { FavoriteDial } from '../../favorites/types';
import type { RadioPanelMode } from '../types';
import './ArdopRadioPanel.css';
import '../sections/ListenSection.css';

type ArdopPanelMode = Extract<RadioPanelMode, { kind: 'ardop-hf' }>;

const DEFAULT_ARDOP_MODE: ArdopPanelMode = { kind: 'ardop-hf', intent: 'cms' };

export interface ArdopRadioPanelProps {
  mode?: ArdopPanelMode;
  onClose: () => void;
  /** tuxlink-6jpf: open the station finder ("Find a gateway") from the panel. */
  onFindGateway?: () => void;
}

// ARQ state cells — same set the legacy ArdopDock surfaced; kept here
// because the new panel still shows the same 9-cell state strip.
const ARQ_CELLS = ['DISC', 'CON', 'IDLE', 'ISS', 'IRS', 'BUSY', 'RX', 'TX', 'DREQ'] as const;
type ArqCell = (typeof ARQ_CELLS)[number];

function isArqCellOn(cell: ArqCell, s: ModemStatus): boolean {
  switch (cell) {
    case 'DISC':
      return s.state === 'stopped' || s.state === 'idle' || s.state === 'disconnecting';
    case 'CON':
      return s.state === 'connected-irs' || s.state === 'connected-iss';
    case 'IDLE':
      return s.state === 'idle';
    case 'ISS':
      return s.state === 'connected-iss';
    case 'IRS':
      return s.state === 'connected-irs';
    case 'BUSY':
      return s.arqFlags.busy;
    case 'RX':
      return s.arqFlags.rx;
    case 'TX':
      return s.arqFlags.tx;
    case 'DREQ':
      return s.state === 'connecting';
  }
}

/**
 * Map the modem state machine into the RadioPanel chrome's `state` prop.
 * The chrome's state set is a coarser palette (connecting / connected /
 * disconnecting / error / disconnected) than the modem's 9-state machine.
 */
function mapModemStateToPanelState(modemState: ModemState): RadioPanelState {
  switch (modemState) {
    case 'stopped':
    case 'idle':
      return 'disconnected';
    case 'spawning':
    case 'initializing':
    case 'connecting':
      return 'connecting';
    case 'connected-irs':
    case 'connected-iss':
      return 'connected';
    case 'disconnecting':
      return 'disconnecting';
    case 'error':
      return 'error';
  }
}

/**
 * Derive a coarse ArdopFrameType from a ModemStatus snapshot. ardopcf does
 * not directly emit per-frame subprotocol-type events on the cmd socket
 * today; we approximate from the state machine + ARQ flags so the ribbon
 * still gives an at-a-glance read on recent on-air activity. A real
 * per-frame event stream is a future enhancement (see spec §5.3 follow-up).
 */
function frameTypeFromStatus(s: ModemStatus): ArdopFrameType {
  if (s.state === 'connecting') return 'CON';
  if (s.arqFlags.tx || s.arqFlags.rx) return 'DATA';
  if (s.state === 'connected-irs' || s.state === 'connected-iss') return 'ACK';
  if (s.state === 'error') return 'REJ';
  return 'IDLE';
}

/**
 * Rolling buffer of derived frame types. Captures one frame-type sample
 * per `intervalMs` tick (default 1000 ms) so the ribbon corresponds to
 * "last N seconds of activity," matching the S/N sparkline's cadence.
 *
 * Hold the latest status in a ref so the interval reads the freshest
 * snapshot without restarting the timer on every render.
 */
function useFrameHistory(
  status: ModemStatus,
  length: number,
  intervalMs: number = 1000,
): ArdopFrameType[] {
  const [frames, setFrames] = useState<ArdopFrameType[]>(() =>
    new Array(length).fill('IDLE' as ArdopFrameType),
  );
  const latest = useRef<ModemStatus>(status);
  latest.current = status;

  useEffect(() => {
    const id = setInterval(() => {
      setFrames((prev) => [...prev.slice(1), frameTypeFromStatus(latest.current)]);
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return frames;
}

function Meter({
  label,
  value,
  warn,
}: {
  label: string;
  value: string;
  warn?: boolean;
}) {
  return (
    <div className={`ardop-meter${warn ? ' warn' : ''}`} data-testid={`ardop-meter-${label.toLowerCase().replace(/[^a-z0-9]/g, '-')}`}>
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

// ARQ bandwidth options — wire shape expected by ardopcf's `ARQBW`. `null`
// renders as empty <option value=""> meaning "Auto (ardopcf default)".
// Mirrors SettingsPanel.tsx — restored to the Connect section per Codex P1
// 2026-05-31 so the operator doesn't have to leave the radio panel.
/** Render the friendly device options into a native <select>, with a disabled
 *  separator before the virtual (Loopback / HDMI) block so real radio
 *  interfaces lead and the never-a-radio devices are visually demoted. The raw
 *  ALSA id trails the friendly name instead of leading it (tuxlink-ebtbv). */
function renderDeviceOptions(options: AudioOption[]) {
  const firstVirtualIdx = options.findIndex((o) => o.isVirtual);
  return options.flatMap((o, i) => {
    const opt = (
      <option key={o.value} value={o.value}>
        {o.primary} — {o.secondary}
      </option>
    );
    if (i === firstVirtualIdx && firstVirtualIdx > 0) {
      return [
        <option key="__virtual_sep" disabled>
          ──────── virtual (loopback / HDMI) ────────
        </option>,
        opt,
      ];
    }
    return [opt];
  });
}

const ARQ_BANDWIDTH_OPTIONS: { value: number | null; label: string }[] = [
  { value: null, label: 'Auto (ardopcf default)' },
  { value: 200, label: '200 Hz (most robust)' },
  { value: 500, label: '500 Hz (marginal HF)' },
  { value: 1000, label: '1000 Hz' },
  { value: 2000, label: '2000 Hz (best throughput)' },
];

/**
 * Frontend mirror of Rust's `config::ArdopUiConfig`. Keys are snake_case
 * because the Rust struct lacks `#[serde(rename_all = "camelCase")]`. The
 * Radio section below edits a subset (capture_device / playback_device /
 * ptt_serial_path). Other fields are loaded + written back untouched so a
 * partial write doesn't clobber bandwidth_hz / cmd_port / binary.
 *
 * Mirrors the same shape SettingsPanel.tsx uses (the original editor that
 * the operator-flagged smoke wanted available inline in the radio panel
 * for parity with AX.25's ModemLinkSection).
 */
/** ALSA device returned by `ardop_list_audio_devices` (tuxlink-y7x7). The
 *  picker sorts hardware entries first so the operator's top-of-list pick
 *  is the kind ardopcf actually wants. */
interface AlsaDeviceDto {
  name: string;
  description: string;
  isHardware: boolean;
}

/** Bundled capture + playback lists from one `ardop_list_audio_devices`
 *  invocation (tuxlink-y7x7). */
interface AlsaDevicesDto {
  captures: AlsaDeviceDto[];
  playbacks: AlsaDeviceDto[];
}

/** Serial device from `packet_list_serial_devices`. Reused here for PTT
 *  enumeration — the same backend command that drives the AX.25 USB
 *  picker (tuxlink-mqu3). */
interface SerialDeviceDto {
  path: string;
  kind: 'usb' | 'bluetooth' | 'uart';
  label: string;
}

/** How tuxlink keys the radio for ARDOP TX (tuxlink-wu0k). Mirrors Rust's
 *  `config::PttMethod` (serde snake_case). CAT command is for radios like the
 *  FT-710 that key ONLY by CAT (TX1;/TX0;) and need the serial port CLOSED
 *  during audio — tuxlink owns a close-serial bridge for that path. */
type PttMethod = 'vox' | 'serial_rts' | 'cat_command';

interface ArdopFullConfig {
  binary: string;
  capture_device: string;
  playback_device: string;
  /** How tuxlink keys the radio. Default 'vox'. */
  ptt_method: PttMethod;
  ptt_serial_path: string | null;
  /** Serial device for CAT PTT (consulted only when ptt_method='cat_command'). */
  cat_serial_path: string | null;
  /** CAT serial baud. Default 38400 (FT-710 Enhanced port). */
  cat_baud: number;
  /** CAT key command, e.g. 'TX1;'. */
  cat_key_cmd: string;
  /** CAT unkey command, e.g. 'TX0;'. */
  cat_unkey_cmd: string;
  /** Loopback TCP port the close-serial CAT-PTT bridge listens on. Default 4532. */
  cat_bridge_port: number;
  cmd_port: number;
  bandwidth_hz: number | null;
  /** Optional WebGUI port pin. null → derive from `cmd_port - 1` (the
   *  ardopcf convention). Set when an operator runs an ardopcf build
   *  that binds the WebGUI somewhere non-standard; the spawn passes
   *  `-G <webgui_port>` from the SAME source so the URL never drifts
   *  from where ardopcf is actually listening (operator smoke
   *  2026-05-31 round 3 — "Open WebGUI returns connection refused"). */
  webgui_port: number | null;
  /** How long the inbound listener stays armed, in MINUTES, before it
   *  self-expires. `0` (the default) means NO EXPIRY — armed until the
   *  operator disarms it (WLE-parity; 2026-06-16 operator decision). A
   *  positive value arms for that many minutes (tuxlink-5g5d). */
  listen_ttl_minutes: number;
  /** Hamlib rig model ID for rigctld-based QSY / VFO control. null = no
   *  rigctld integration. Set to the hamlib model number matching the
   *  transceiver (e.g. 1049 for IC-7300). */
  rig_hamlib_model: number | null;
  /** Host where rigctld is listening. Default '127.0.0.1'. */
  rigctld_host: string;
  /** TCP port rigctld is listening on. Default 4532. */
  rigctld_port: number;
  /** rigctld binary name or path used when tuxlink spawns it. Default 'rigctld'. */
  rigctld_binary: string;
  /** When true, tuxlink closes the CAT serial port before passing audio to
   *  ardopcf and re-opens it after TX (required for single-port radios). */
  close_serial_sequencing: boolean;
  /** When true, tuxlink polls the VFO frequency from rigctld in real time. */
  live_vfo_poll: boolean;
  /** When true, tuxlink attempts an automatic QSY to the gateway frequency
   *  before initiating a connect. */
  qsy_on_fail: boolean;
}

/** Mirror of Rust's `ArdopUiConfig::resolved_webgui_port`. Single source
 *  of truth for "where does the WebGUI bind?" — the spawn flag and the
 *  Open-WebGUI button URL MUST agree, so both go through this helper. */
function resolveWebguiPort(cfg: Pick<ArdopFullConfig, 'cmd_port' | 'webgui_port'>): number | null {
  if (cfg.webgui_port !== null && cfg.webgui_port !== undefined) {
    return cfg.webgui_port;
  }
  if (cfg.cmd_port >= 2) {
    return cfg.cmd_port - 1;
  }
  return null;
}

export function ArdopRadioPanel({
  mode = DEFAULT_ARDOP_MODE,
  onClose,
  onFindGateway,
}: ArdopRadioPanelProps) {
  const { status } = useModemStatus();
  const [target, setTarget] = useState('');
  // tuxlink-ypz3 (3a): restore the persisted ARDOP target on mount so switching
  // modes (panel remounts) doesn't blank the previously-dialed station. Mirrors
  // the localStorage key the ribbon Connect (connectDispatch) reads. Keyed on
  // mode.kind ('ardop-hf') for uniformity with the VARA panel. Passive: seeds
  // the input only — never auto-connects (RADIO-1: Send/Receive is the consent
  // click).
  useEffect(() => {
    setTarget(readLastTarget(mode.kind));
  }, [mode.kind]);
  // ARQ bandwidth (restored 2026-05-31 — Codex P1). Loaded from
  // config_get_ardop on mount; persisted via config_set_ardop on change.
  // null = "leave at ardopcf default."
  const [bandwidth, setBandwidth] = useState<number | null>(null);
  // Full ARDOP config — operator smoke 2026-05-31 added a Radio section
  // here so audio devices + PTT serial path are editable inline (parity
  // with AX.25's ModemLinkSection on the Packet panel). Loaded on mount,
  // persisted via config_set_ardop on each blur.
  const [ardopConfig, setArdopConfig] = useState<ArdopFullConfig | null>(null);
  const [captureInput, setCaptureInput] = useState<string>('');
  const [playbackInput, setPlaybackInput] = useState<string>('');
  const [pttSerialInput, setPttSerialInput] = useState<string>('');
  // tuxlink-wu0k: PTT method + CAT-command fields. CAT command keys radios
  // like the FT-710 that key ONLY by CAT (TX1;/TX0;) and need the serial port
  // CLOSED during audio — tuxlink spawns a close-serial bridge for that path.
  const [pttMethod, setPttMethod] = useState<PttMethod>('vox');
  const [catSerialInput, setCatSerialInput] = useState<string>('');
  const [catBaudInput, setCatBaudInput] = useState<string>('38400');
  const [catKeyInput, setCatKeyInput] = useState<string>('TX1;');
  const [catUnkeyInput, setCatUnkeyInput] = useState<string>('TX0;');
  // tuxlink-0kew: collapse the Radio-configuration group to reclaim panel
  // real estate once it's set. Default open (discoverable on first use); the
  // operator's collapse choice persists across sessions via localStorage.
  const [radioCfgOpen, setRadioCfgOpen] = useState<boolean>(() => {
    try {
      return localStorage.getItem('tuxlink.ardop.radioCfgOpen') !== '0';
    } catch {
      return true;
    }
  });
  // cmd_port + binary inputs (tuxlink-jmfm Task 3). PR #185 commit 4c88618
  // added Capture / Playback / PTT / WebGUI; Task 2 of the radio-panel-width
  // plan deleted the Settings ARDOP fieldset, so cmd_port + binary needed an
  // inline-edit surface in the panel to remain reachable.
  const [cmdPortInput, setCmdPortInput] = useState<string>('');
  const [binaryInput, setBinaryInput] = useState<string>('');
  // WebGUI port override (operator smoke 2026-05-31 round 3). Stored as a
  // string in the input so the operator can type freely; commits to the
  // backend on blur after a non-empty numeric parse. Empty input → null
  // (revert to "derive from cmd_port - 1" — the default).
  const [webguiPortInput, setWebguiPortInput] = useState<string>('');
  // tuxlink-5g5d: inbound-listener arm-window TTL in MINUTES. Empty or 0 means
  // NO EXPIRY (the WLE-parity default; 2026-06-16 operator decision) — armed
  // until the operator disarms it. Stored as a string so the operator can type
  // freely; commits on blur after a non-negative integer parse.
  const [listenTtlInput, setListenTtlInput] = useState<string>('');
  // tuxlink-y7x7: device-enumeration state for the Radio section pickers.
  // Captures + playbacks come from `ardop_list_audio_devices` (which shells
  // to `arecord -L` / `aplay -L`); PTT comes from `packet_list_serial_devices`
  // (reusing the AX.25 USB picker's enumeration — same `/dev/ttyUSB*`
  // candidates apply to ardopcf's `-k <serial-path>` PTT keying). Empty
  // lists are normal (no USB audio interface plugged in / no USB-serial
  // CAT cable attached) — the manual-fallback input is the escape hatch.
  const [captureDevices, setCaptureDevices] = useState<AlsaDeviceDto[]>([]);
  const [playbackDevices, setPlaybackDevices] = useState<AlsaDeviceDto[]>([]);
  const [pttDevices, setPttDevices] = useState<SerialDeviceDto[]>([]);
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const [exchanging, setExchanging] = useState(false);

  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Favorites integration (Task B6-ARDOP). RADIO-1: a favorite's Connect is
  // PRE-FILL ONLY — it sets `target` via `handlePrefill` and NEVER invokes a
  // connect/exchange command. The operator's later Start + Send/Receive clicks
  // are the Part 97 consent gates. recordAttempt logs the HONEST on-air outcome
  // (reached on the connected-* link transition; failed in the b2f catch).
  const { recordAttempt } = useFavorites('ardop-hf');
  // The favorite whose Connect was last clicked. Carries its metadata into the
  // connection record IFF it matches the live peer. Cleared on a manual target
  // edit (a hand-typed target is not the prefilled favorite).
  const pendingDialRef = useRef<FavoriteDial | null>(null);
  const handlePrefill = useCallback((dial: FavoriteDial) => {
    setTarget(dial.gateway);
    pendingDialRef.current = dial;
    // tuxlink-vu97: persist so the ribbon Connect can dial this target with the
    // pane closed.
    writeLastTarget('ardop-hf', dial.gateway);
  }, []);

  useEffect(
    () => listenGatewayPrefill('ardop-hf', handlePrefill),
    [handlePrefill],
  );
  // Build the dial for a connection record from the LIVE peer. If the prefilled
  // favorite matches the peer callsign, carry its metadata (freq/band/grid/
  // transport) into the record; otherwise record a minimal dial (manual connect).
  const buildRecordDial = (): FavoriteDial | null => {
    const gw = status.peer?.trim();
    if (!gw) return null;
    const pend = pendingDialRef.current;
    if (pend && pend.gateway.trim().toUpperCase() === gw.toUpperCase()) {
      return { ...pend, mode: 'ardop-hf', gateway: gw };
    }
    return { mode: 'ardop-hf', gateway: gw };
  };

  // ARDOP listener arms + allowlist plumbing (spec §1.3). ARDOP has no
  // station-password layer (per ardop-p2p.md divergence 2) so the
  // panel does NOT render the password expander. The set-allow-all
  // arg-key is `allow_all` (snake_case to match the backend handler).
  const activeIdentity = useActiveIdentity();
  const ardopListener = useListenerState({
    activeIdentityLabel: activeIdentity.data?.address_as ?? null,
    commands: {
      listen: 'ardop_listen',
      setListen: 'ardop_set_listen',
      allowedGet: 'ardop_allowed_stations_get',
      allowedAddCallsign: 'ardop_allowed_stations_add',
      allowedAddCallsignArgKey: 'callsign',
      allowedRemoveCallsign: 'ardop_allowed_stations_remove',
      allowedRemoveCallsignArgKey: 'callsign',
      allowedSetAllowAll: 'ardop_allowed_stations_set_allow_all',
      // Tauri auto-camelCases Rust arg `allow_all: bool` → JS wire key `allowAll`.
      // Codex review 2026-06-03 [P2] (tuxlink-7vea): the prior `allow_all` key
      // here meant Tauri delivered no value to the Rust handler and the operator
      // couldn't toggle.
      allowedSetAllowAllArgKey: 'allowAll',
    },
  });

  // Rolling 60-sample buffers (1 Hz tick) for the S/N + throughput
  // sparklines. The hook reads the latest reading out of a ref every
  // tick, so the buffer always reflects the freshest modem snapshot.
  const snrHistory = useSampleHistory(status.snDb, 60);
  const throughputHistory = useSampleHistory(status.throughputBps, 60);
  const frameHistory = useFrameHistory(status, 60);

  // Load ARDOP config on mount: bandwidth (existing) + audio/PTT (added
  // 2026-05-31 for the Radio section).
  useEffect(() => {
    let cancelled = false;
    invoke<ArdopFullConfig>('config_get_ardop')
      .then((c) => {
        if (cancelled || !c) return;
        if (typeof c.bandwidth_hz !== 'undefined') {
          setBandwidth(c.bandwidth_hz);
        }
        setArdopConfig(c);
        setCaptureInput(c.capture_device ?? '');
        setPlaybackInput(c.playback_device ?? '');
        setPttSerialInput(c.ptt_serial_path ?? '');
        // tuxlink-wu0k: PTT method + CAT fields. Older configs may lack
        // ptt_method (the backend migrates it on read), so default to 'vox'.
        setPttMethod(c.ptt_method ?? 'vox');
        setCatSerialInput(c.cat_serial_path ?? '');
        setCatBaudInput(String(c.cat_baud ?? 38400));
        setCatKeyInput(c.cat_key_cmd ?? 'TX1;');
        setCatUnkeyInput(c.cat_unkey_cmd ?? 'TX0;');
        setCmdPortInput(String(c.cmd_port));
        setBinaryInput(c.binary ?? '');
        // Display the resolved port (override OR cmd_port-1) as a hint so
        // the operator sees what URL the button will open. Empty → no
        // override pinned yet, so the input shows the derived default as
        // a placeholder rather than a value.
        setWebguiPortInput(c.webgui_port !== null && c.webgui_port !== undefined ? String(c.webgui_port) : '');
        // tuxlink-5g5d: blank input represents "no expiry" (stored as 0).
        setListenTtlInput(c.listen_ttl_minutes > 0 ? String(c.listen_ttl_minutes) : '');
      })
      .catch(() => {
        /* pre-wizard / config absent — keep null default */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** Persist an updated ARDOP config slice. Mirrors SettingsPanel's
   *  merge-then-write pattern so two writers can't clobber each other
   *  (the bandwidth selector + this Radio section both edit subsets). */
  const persistArdop = (patch: Partial<ArdopFullConfig>) => {
    if (!ardopConfig) return;
    const next = { ...ardopConfig, ...patch };
    setArdopConfig(next);
    void invoke('config_set_ardop', { value: next }).catch(() => {
      /* persist errors surface in the session log via the backend */
    });
  };

  const commitCapture = () => {
    const trimmed = captureInput.trim();
    persistArdop({ capture_device: trimmed });
  };
  const commitPlayback = () => {
    const trimmed = playbackInput.trim();
    persistArdop({ playback_device: trimmed });
  };
  const commitPttSerial = () => {
    const trimmed = pttSerialInput.trim();
    // Empty string → null on the wire (means "use VOX").
    persistArdop({ ptt_serial_path: trimmed === '' ? null : trimmed });
  };
  // tuxlink-wu0k: CAT-PTT field commits. The method selector persists eagerly
  // on change; the text fields persist on blur (matching the panel's idiom).
  const onPttMethodChange = (next: PttMethod) => {
    setPttMethod(next);
    persistArdop({ ptt_method: next });
  };
  const commitCatSerial = () => {
    const trimmed = catSerialInput.trim();
    persistArdop({ cat_serial_path: trimmed === '' ? null : trimmed });
  };
  const commitCatBaud = () => {
    const n = Number(catBaudInput.trim());
    // Reject NaN / non-integer / non-positive; keep the prior persisted value.
    if (!Number.isInteger(n) || n <= 0) {
      setCatBaudInput(String(ardopConfig?.cat_baud ?? 38400));
      return;
    }
    persistArdop({ cat_baud: n });
  };
  const commitCatKey = () => {
    const trimmed = catKeyInput.trim();
    // Empty → keep the default rather than persisting an unkeyable empty string.
    if (trimmed === '') {
      setCatKeyInput(ardopConfig?.cat_key_cmd ?? 'TX1;');
      return;
    }
    persistArdop({ cat_key_cmd: trimmed });
  };
  const commitCatUnkey = () => {
    const trimmed = catUnkeyInput.trim();
    if (trimmed === '') {
      setCatUnkeyInput(ardopConfig?.cat_unkey_cmd ?? 'TX0;');
      return;
    }
    persistArdop({ cat_unkey_cmd: trimmed });
  };
  const commitWebguiPort = () => {
    const trimmed = webguiPortInput.trim();
    if (trimmed === '') {
      // Empty → clear override; backend reverts to "derive from cmd_port - 1".
      persistArdop({ webgui_port: null });
      return;
    }
    const n = Number(trimmed);
    // Reject NaN, non-integer, and out-of-u16-range values. On reject, also
    // resync the input to whatever's persisted so the operator's bad value
    // doesn't linger in the field.
    if (!Number.isInteger(n) || n < 1 || n > 65535) {
      setWebguiPortInput(
        ardopConfig?.webgui_port !== null && ardopConfig?.webgui_port !== undefined
          ? String(ardopConfig.webgui_port)
          : '',
      );
      setConnectError(`Invalid WebGUI port "${trimmed}" — must be 1..65535. Reverted.`);
      return;
    }
    persistArdop({ webgui_port: n });
  };
  // cmd_port commit: strict parse (Number + Number.isInteger; rejects
  // "8515abc" where parseInt would have silently accepted 8515), reject
  // out-of-u16-range, skip no-op writes. Mirrors commitWebguiPort exactly
  // so the two sibling port-input handlers behave identically — the prior
  // asymmetry (parseInt + no upper bound on cmd_port; Number + 65535 on
  // webgui_port) was a code smell.
  const commitCmdPort = () => {
    if (!ardopConfig) return;
    const trimmed = cmdPortInput.trim();
    const n = Number(trimmed);
    if (!Number.isInteger(n) || n < 1 || n > 65535) {
      // Revert the input to whatever's persisted so the operator's bad
      // value doesn't linger in the field.
      setCmdPortInput(String(ardopConfig.cmd_port));
      setConnectError(`Invalid cmd_port "${trimmed}" — must be 1..65535. Reverted.`);
      return;
    }
    if (n === ardopConfig.cmd_port) return;
    persistArdop({ cmd_port: n });
  };
  // binary commit: empty trimmed reverts the input to the persisted value
  // (matches commitCmdPort's revert-on-invalid pattern). Without this, an
  // operator who clears the field and tabs out sees the input go visually
  // empty, ardopConfig.binary unchanged, then the useEffect resync on next
  // mount overwrites the empty input back to the persisted value — looks
  // like "my edit silently vanished." The revert surfaces the truth
  // immediately and writes a connectError explaining why.
  const commitBinary = () => {
    if (!ardopConfig) return;
    const trimmed = binaryInput.trim();
    if (trimmed === '') {
      setBinaryInput(ardopConfig.binary);
      setConnectError('Binary path cannot be empty — reverted to persisted value.');
      return;
    }
    if (trimmed === ardopConfig.binary) return;
    persistArdop({ binary: trimmed });
  };

  // tuxlink-5g5d: arm-window TTL in minutes. Empty or 0 → NO EXPIRY (the
  // WLE-parity default). Reject negatives / non-integers, reverting the field
  // to the persisted value (mirrors commitCmdPort's revert-on-invalid pattern).
  const commitListenTtl = () => {
    if (!ardopConfig) return;
    const trimmed = listenTtlInput.trim();
    const minutes = trimmed === '' ? 0 : Number(trimmed);
    if (!Number.isInteger(minutes) || minutes < 0) {
      setListenTtlInput(
        ardopConfig.listen_ttl_minutes > 0 ? String(ardopConfig.listen_ttl_minutes) : '',
      );
      setConnectError(
        `Invalid listener duration "${trimmed}" — enter a whole number of minutes (0 or blank = no expiry). Reverted.`,
      );
      return;
    }
    if (minutes === ardopConfig.listen_ttl_minutes) return;
    persistArdop({ listen_ttl_minutes: minutes });
  };

  const onBandwidthChange = (e: ChangeEvent<HTMLSelectElement>) => {
    // value="" represents the "Auto" option → null. Otherwise parse as int.
    const raw = e.target.value;
    const next = raw === '' ? null : parseInt(raw, 10);
    setBandwidth(next);
    // Persist via merge: read current ardop config, splice bandwidth, write
    // back. Mirrors SettingsPanel.tsx's pattern so two writers can't clobber
    // each other's fields.
    void (async () => {
      try {
        const current = await invoke<Record<string, unknown>>('config_get_ardop');
        await invoke('config_set_ardop', {
          value: { ...current, bandwidth_hz: next },
        });
      } catch {
        // Persist errors surface via the session log; UI keeps the new value.
      }
    })();
  };

  const isStopped = status.state === 'stopped';
  const isExchangeReady =
    status.state === 'connected-irs' || status.state === 'connected-iss';

  // Record `reached` ONCE per connection, on the actual on-air ARQ link
  // transition (C3) — NOT when `modem_ardop_connect` resolves (that's the local
  // ardopcf spawn, not the gateway link). The guard ref fires the record only on
  // the first connected-* tick with a peer, then resets when the link drops so a
  // subsequent connection re-records.
  const isConnected =
    status.state === 'connected-irs' || status.state === 'connected-iss';
  // Initialize from the mount-time connection state: if the panel mounts INTO an
  // already-connected session (e.g. a remount mid-session), treat it as already
  // recorded so we don't log a `reached` for a connection that happened earlier.
  // A real STOPPED→connected transition during this mount still records once.
  const recordedConnRef = useRef(isConnected);
  useEffect(() => {
    if (isConnected && status.peer && !recordedConnRef.current) {
      recordedConnRef.current = true;
      const dial = buildRecordDial();
      if (dial) void recordAttempt(dial, 'reached', tsLocal());
    }
    if (!isConnected) recordedConnRef.current = false;
    // buildRecordDial / recordAttempt are stable enough for this transition
    // guard; keying on isConnected + peer is the intended fire condition.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isConnected, status.peer]);

  // tuxlink-y7nq: filter ALSA enumerations to hardware-only for the dropdown.
  // `arecord -L` / `aplay -L` return a dozen plugin / converter chains
  // (lavrate, samplerate, speex, upmix, vdownmix, jack, oss, null, sysdefault,
  // usbstream HDMI passthroughs, etc.) that aren't useful for ARDOP. The
  // backend already classifies `plughw:CARD=…` / `hw:CARD=…` entries as
  // `isHardware`; here we use that as a FILTER (not just a sort key as the
  // initial tuxlink-y7x7 ship did). Manual-fallback input below the dropdown
  // covers the rare advanced case (someone explicitly wants `pulse`/`default`).
  const captureOptions = friendlyAudioOptions(captureDevices);
  const playbackOptions = friendlyAudioOptions(playbackDevices);
  const captureHasReal = captureOptions.some((o) => !o.isVirtual);
  const playbackHasReal = playbackOptions.some((o) => !o.isVirtual);

  // tuxlink-y7x7: load device lists when the Radio section becomes editable
  // (Stopped state). Refresh buttons re-invoke via these same callbacks.
  // Soft-failure posture: any rejection (ardopcf binary absent on CI,
  // bluetoothctl missing, /dev unreadable) clears the list and the picker
  // shows a "no devices found — refresh after plugging in" hint rather
  // than an error.
  const loadAudioDevices = useCallback(() => {
    void invoke<AlsaDevicesDto>('ardop_list_audio_devices')
      .then((dto) => {
        setCaptureDevices(dto?.captures ?? []);
        setPlaybackDevices(dto?.playbacks ?? []);
      })
      .catch(() => {
        setCaptureDevices([]);
        setPlaybackDevices([]);
      });
  }, []);
  const loadPttDevices = useCallback(() => {
    void invoke<SerialDeviceDto[]>('packet_list_serial_devices')
      .then((list) => setPttDevices(list ?? []))
      .catch(() => setPttDevices([]));
  }, []);
  useEffect(() => {
    if (!isStopped) return;
    loadAudioDevices();
    loadPttDevices();
  }, [isStopped, loadAudioDevices, loadPttDevices]);
  // Effective B2F target: ONLY the backend-reported peer authorizes a TX
  // Operator-click is the Part 97 consent gate. effectiveTarget is always the
  // backend-reported peer, not the stopped-state `target` input, so TX only
  // fires when the modem has an active connection.
  const effectiveTarget: string | null = status.peer?.trim() ?? null;

  const doConnect = async () => {
    setConnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_connect', {
        target: target.trim(),
      });
    } catch (e) {
      // tuxlink-nnjz: modem errors are surfaced in the session log (the backend
      // emits an Error line on failure) — not in an inline panel element wedged
      // next to the buttons. Keep a dev-console trace; the operator-facing
      // report is the log row right there in the panel.
      console.debug('ARDOP connect failed (surfaced in session log):', e);
    } finally {
      setConnecting(false);
    }
  };

  const onStartClick = () => {
    setConnectError(null);
    void doConnect();
  };

  const onSendReceiveClick = async () => {
    if (!isExchangeReady || effectiveTarget === null) return;
    setExchanging(true);
    setConnectError(null);
    try {
      // tuxlink-nnws: derive the routing intent from the sidebar-selected
      // RadioPanelMode, matching VARA's intent-aware panel path.
      // `transportKind: 'ardop'` is the panel's mode.kind.
      await invoke('modem_ardop_b2f_exchange', {
        target: effectiveTarget,
        intent: mode.intent,
        transportKind: 'ardop',
      });
    } catch (e) {
      // tuxlink-nnjz: the send/receive failure is surfaced in the session log
      // by the backend (Error line), not an inline panel element. The gateway
      // `failed` record below is a separate, deliberate empirical fact.
      console.debug('ARDOP send/receive failed (surfaced in session log):', e);
      // Record a gateway `failed` (C3) — in the CATCH, never the FINALLY, so a
      // pre-air busy-guard rejection or the local connect never logs a spurious
      // gateway failure. The guard at the top (isExchangeReady / effectiveTarget
      // null) returns before any record path, so only a real exchange attempt
      // that threw reaches here.
      // Note: a session that already reached connected-* logged a `reached`; a later
      // exchange failure logs an additional `failed`. Both are intentional, distinct
      // empirical facts (link reached vs. message exchange failed) — not a double-count.
      const dial = buildRecordDial();
      if (dial) void recordAttempt(dial, 'failed', tsLocal());
    } finally {
      setExchanging(false);
    }
  };

  const onStopClick = async () => {
    setDisconnecting(true);
    setConnectError(null);
    try {
      await invoke('modem_ardop_disconnect');
    } catch (e) {
      // tuxlink-nnjz: surfaced in the session log by the backend, not inline.
      console.debug('ARDOP disconnect failed (surfaced in session log):', e);
    } finally {
      setDisconnecting(false);
    }
  };

  const onOpenWebGuiClick = async () => {
    setConnectError(null);
    try {
      // Re-read the live config so the URL reflects any in-flight Settings
      // change. Use the resolved-port helper (mirror of Rust's
      // `ArdopUiConfig::resolved_webgui_port`) so this site agrees with the
      // spawn's `-G` flag by construction. Operator smoke 2026-05-31 round 3
      // — "Open WebGUI opens but connection refused" — was rooted in the
      // possibility of those two sources drifting; routing both through the
      // same helper rules that bug class out.
      const ardop = await invoke<ArdopFullConfig>('config_get_ardop');
      const webguiPort = resolveWebguiPort(ardop);
      if (webguiPort === null) {
        setConnectError(
          `Cannot open WebGUI: cmd_port=${ardop.cmd_port} too low and no explicit webgui_port override set`,
        );
        return;
      }
      // The button is only rendered while ardopcf is running (`!isStopped`),
      // so by the time we get here ardopcf SHOULD be bound to webguiPort.
      // If the operator still gets "connection refused," the most likely
      // causes are: (a) the ardopcf binary on PATH doesn't honor `-G`
      // (older build), (b) ardopcf bound but crashed during init, or
      // (c) the operator pinned `webgui_port` to a wrong value. Surface
      // a useful error template covering those.
      await shellOpen(`http://localhost:${webguiPort}/`);
    } catch (e) {
      setConnectError(
        `Failed to open WebGUI: ${e}. If the URL opens but reports "connection refused," ardopcf may not have bound the WebGUI — check that the binary on PATH supports the -G flag and that webgui_port matches its actual bind port.`,
      );
    }
  };

  const onTargetChange = (e: ChangeEvent<HTMLInputElement>) => {
    setTarget(e.target.value);
    // A hand-typed target is not the prefilled favorite — drop the association
    // so the connection record doesn't carry stale favorite metadata.
    pendingDialRef.current = null;
    // tuxlink-vu97: persist the configured target so the ribbon Connect button
    // can fire ARDOP's full send/receive with this pane closed.
    writeLastTarget('ardop-hf', e.target.value);
  };

  const headerSub = `${status.peer ?? '—'} · ${status.widthHz ? `${status.widthHz} Hz` : '—'}`;

  return (
    <RadioPanel
      mode={mode}
      state={mapModemStateToPanelState(status.state)}
      sub={headerSub}
      onClose={onClose}
      onFindGateway={onFindGateway}
    >
      {/* Connect surface — Favorites / Recent / Manual (Task B6-ARDOP). The
          hand-entry Target + Bandwidth fields are the Manual tab's content;
          a favorite's Connect PRE-FILLS the target via handlePrefill and never
          transmits (RADIO-1). Only rendered in the stopped state — favorites/
          connect only make sense before ardopcf is running. The Start /
          Send-Receive / Stop action buttons stay OUTSIDE this surface, below. */}
      {isStopped && (
        <FavoritesTabs
          mode="ardop-hf"
          onPrefill={handlePrefill}
          manualContent={
            <section className="radio-panel-sec">
              <h5>Connect</h5>
              <label className="radio-panel-input-row">
                <span>Target</span>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid="ardop-target-input"
                  value={target}
                  onChange={onTargetChange}
                  placeholder="W7RMS-10"
                  spellCheck={false}
                  autoCapitalize="characters"
                  autoCorrect="off"
                />
              </label>
              <label className="radio-panel-input-row">
                <span>Bandwidth</span>
                <select
                  className="radio-panel-input"
                  data-testid="ardop-bandwidth-select"
                  value={bandwidth ?? ''}
                  onChange={onBandwidthChange}
                >
                  {ARQ_BANDWIDTH_OPTIONS.map((opt) => (
                    <option key={String(opt.value)} value={opt.value ?? ''}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </label>
            </section>
          }
        />
      )}

      {/* Radio (audio devices + PTT serial path). Operator smoke 2026-05-31:
          AX.25 has the ModemLinkSection for TNC link selection; ARDOP needed
          parallel pickers for audio + PTT so the operator doesn't have to
          jump to the Settings panel. Editable in the stopped state — ardopcf
          consumes these at spawn time, so changing them mid-session has no
          effect until restart. We surface them in stopped state only to
          avoid implying live-changeability. (Bandwidth follows the same
          pattern in the Connect section above.) */}
      {isStopped && (
        <section className="radio-panel-sec" data-testid="ardop-radio-section">
          <details
            className="expander"
            open={radioCfgOpen}
            onToggle={(e) => {
              const open = e.currentTarget.open;
              setRadioCfgOpen(open);
              try {
                localStorage.setItem('tuxlink.ardop.radioCfgOpen', open ? '1' : '0');
              } catch {
                /* localStorage unavailable — in-memory toggle still works */
              }
            }}
            data-testid="ardop-config-expander"
          >
            <summary className="expander-summary">Radio</summary>
          {/* tuxlink-y7x7: Capture / Playback / PTT now load real device
              lists from the backend and render dropdown + Refresh + manual
              fallback (same pattern as ModemLinkSection's AX.25 picker).
              The previous text-input ghosts (placeholder="plughw:1,0") were
              auto-fill theatre — empty fields silently failed at ardopcf
              spawn because no `-c`/`-p` was ever passed. Pickers list
              hardware devices first so the operator's natural pick is the
              kind ardopcf actually wants. */}
          <label className="radio-panel-input-row">
            <span>Capture</span>
            <select
              className="radio-panel-input"
              data-testid="ardop-capture-select"
              value={captureOptions.some((o) => o.value === captureInput) ? captureInput : ''}
              onChange={(e) => {
                const next = e.target.value;
                setCaptureInput(next);
                persistArdop({ capture_device: next });
              }}
            >
              <option value="" disabled>
                {captureHasReal
                  ? 'Choose capture device…'
                  : 'No USB audio interfaces found — plug one in and Refresh'}
              </option>
              {renderDeviceOptions(captureOptions)}
            </select>
            <button
              type="button"
              className="radio-panel-btn-sm"
              data-testid="ardop-capture-refresh"
              onClick={loadAudioDevices}
              aria-label="Refresh capture device list"
            >
              ↻
            </button>
          </label>
          <label className="radio-panel-input-row">
            <span>Manual</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="ardop-capture-input"
              value={captureInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="plughw:CARD=… (unlisted)"
              onChange={(e) => setCaptureInput(e.target.value)}
              onBlur={commitCapture}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Playback</span>
            <select
              className="radio-panel-input"
              data-testid="ardop-playback-select"
              value={playbackOptions.some((o) => o.value === playbackInput) ? playbackInput : ''}
              onChange={(e) => {
                const next = e.target.value;
                setPlaybackInput(next);
                persistArdop({ playback_device: next });
              }}
            >
              <option value="" disabled>
                {playbackHasReal
                  ? 'Choose playback device…'
                  : 'No USB audio interfaces found — plug one in and Refresh'}
              </option>
              {renderDeviceOptions(playbackOptions)}
            </select>
            <button
              type="button"
              className="radio-panel-btn-sm"
              data-testid="ardop-playback-refresh"
              onClick={loadAudioDevices}
              aria-label="Refresh playback device list"
            >
              ↻
            </button>
          </label>
          <label className="radio-panel-input-row">
            <span>Manual</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="ardop-playback-input"
              value={playbackInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="plughw:CARD=… (unlisted)"
              onChange={(e) => setPlaybackInput(e.target.value)}
              onBlur={commitPlayback}
            />
          </label>
          {/* tuxlink-wu0k: PTT method selector. VOX = no PTT line; Serial RTS =
              ardopcf toggles RTS on a serial port; CAT command = key the radio
              by a CAT command (TX1;/TX0;) through tuxlink's close-serial bridge
              (for radios like the FT-710 that key ONLY by CAT and whose codec
              resets if the serial port is held open during audio). */}
          <label className="radio-panel-input-row">
            <span>PTT method</span>
            <select
              className="radio-panel-input"
              data-testid="ardop-ptt-method-select"
              value={pttMethod}
              onChange={(e) => onPttMethodChange(e.target.value as PttMethod)}
            >
              <option value="vox">VOX (no PTT line)</option>
              <option value="serial_rts">Serial RTS</option>
              <option value="cat_command">CAT command (TX1;/TX0;)</option>
            </select>
          </label>
          {pttMethod === 'serial_rts' && (
            <>
              <label className="radio-panel-input-row">
                <span>PTT serial</span>
                <select
                  className="radio-panel-input"
                  data-testid="ardop-ptt-select"
                  // A persisted path not in the enumerated list falls back to
                  // the empty option so the dropdown isn't lying.
                  value={pttDevices.some((d) => d.path === pttSerialInput) ? pttSerialInput : ''}
                  onChange={(e) => {
                    const next = e.target.value;
                    setPttSerialInput(next);
                    persistArdop({ ptt_serial_path: next === '' ? null : next });
                  }}
                >
                  <option value="">Choose serial port…</option>
                  {pttDevices
                    .filter((d) => d.kind === 'usb')
                    .map((d) => (
                      <option key={d.path} value={d.path}>
                        {d.path} — {d.label}
                      </option>
                    ))}
                </select>
                <button
                  type="button"
                  className="radio-panel-btn-sm"
                  data-testid="ardop-ptt-refresh"
                  onClick={loadPttDevices}
                  aria-label="Refresh PTT serial device list"
                >
                  ↻
                </button>
              </label>
              <label className="radio-panel-input-row">
                <span>Manual</span>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid="ardop-ptt-input"
                  value={pttSerialInput}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="/dev/ttyUSB0 (unlisted)"
                  onChange={(e) => setPttSerialInput(e.target.value)}
                  onBlur={commitPttSerial}
                />
              </label>
            </>
          )}
          {pttMethod === 'cat_command' && (
            <>
              <label className="radio-panel-input-row">
                <span>CAT serial</span>
                <select
                  className="radio-panel-input"
                  data-testid="ardop-cat-serial-select"
                  value={pttDevices.some((d) => d.path === catSerialInput) ? catSerialInput : ''}
                  onChange={(e) => {
                    const next = e.target.value;
                    setCatSerialInput(next);
                    persistArdop({ cat_serial_path: next === '' ? null : next });
                  }}
                >
                  <option value="">Choose CAT serial port…</option>
                  {pttDevices
                    .filter((d) => d.kind === 'usb')
                    .map((d) => (
                      <option key={d.path} value={d.path}>
                        {d.path} — {d.label}
                      </option>
                    ))}
                </select>
                <button
                  type="button"
                  className="radio-panel-btn-sm"
                  data-testid="ardop-cat-serial-refresh"
                  onClick={loadPttDevices}
                  aria-label="Refresh CAT serial device list"
                >
                  ↻
                </button>
              </label>
              <label className="radio-panel-input-row">
                <span>Manual</span>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid="ardop-cat-serial-input"
                  value={catSerialInput}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="/dev/ttyUSB0 (CAT/Enhanced port)"
                  onChange={(e) => setCatSerialInput(e.target.value)}
                  onBlur={commitCatSerial}
                />
              </label>
              <label className="radio-panel-input-row">
                <span>CAT baud</span>
                <input
                  type="text"
                  inputMode="numeric"
                  className="radio-panel-input"
                  data-testid="ardop-cat-baud-input"
                  value={catBaudInput}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="38400"
                  onChange={(e) => setCatBaudInput(e.target.value)}
                  onBlur={commitCatBaud}
                />
              </label>
              <label className="radio-panel-input-row">
                <span>Key cmd</span>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid="ardop-cat-key-input"
                  value={catKeyInput}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="TX1;"
                  onChange={(e) => setCatKeyInput(e.target.value)}
                  onBlur={commitCatKey}
                />
              </label>
              <label className="radio-panel-input-row">
                <span>Unkey cmd</span>
                <input
                  type="text"
                  className="radio-panel-input"
                  data-testid="ardop-cat-unkey-input"
                  value={catUnkeyInput}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="TX0;"
                  onChange={(e) => setCatUnkeyInput(e.target.value)}
                  onBlur={commitCatUnkey}
                />
              </label>
            </>
          )}
          {/* WebGUI port — operator smoke 2026-05-31 round 3. Defaults to
              `cmd_port - 1` (the ardopcf convention). Override when running
              an ardopcf build that binds the WebGUI somewhere else. Empty
              input clears the override. The spawn passes `-G <webgui_port>`
              from the SAME source as this field so the "Open WebGUI" button
              never drifts from where ardopcf is actually listening. */}
          <label className="radio-panel-input-row">
            <span>WebGUI</span>
            <input
              type="text"
              inputMode="numeric"
              className="radio-panel-input"
              data-testid="ardop-webgui-port-input"
              value={webguiPortInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder={
                ardopConfig
                  ? `${Math.max(1, (ardopConfig.cmd_port || 8515) - 1)} (auto = cmd_port - 1)`
                  : '8514 (auto = cmd_port - 1)'
              }
              onChange={(e) => setWebguiPortInput(e.target.value)}
              onBlur={commitWebguiPort}
            />
          </label>
          {/* cmd_port + binary (tuxlink-jmfm Task 3). The Settings ARDOP
              fieldset was deleted in Task 2; without these rows the
              operator would have no UI surface to edit either control. */}
          <label className="radio-panel-input-row">
            <span>Cmd port</span>
            <input
              type="text"
              inputMode="numeric"
              className="radio-panel-input"
              data-testid="ardop-cmd-port-input"
              value={cmdPortInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="8515 (ardopcf default)"
              onChange={(e) => setCmdPortInput(e.target.value)}
              onBlur={commitCmdPort}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Binary</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="ardop-binary-input"
              value={binaryInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="ardopcf"
              onChange={(e) => setBinaryInput(e.target.value)}
              onBlur={commitBinary}
            />
          </label>
          </details>
        </section>
      )}

      {!isStopped && (
        <section className="radio-panel-sec">
          <h5>ARQ state</h5>
          <div className="ardop-arq-grid">
            {ARQ_CELLS.map((cell) => (
              <div
                key={cell}
                className="ardop-arq-cell"
                data-testid={`arq-cell-${cell}`}
                data-on={isArqCellOn(cell, status)}
              >
                {cell}
              </div>
            ))}
          </div>
        </section>
      )}

      {!isStopped && (
        <section className="radio-panel-sec">
          <h5>Live</h5>
          {status.snDb !== null && (
            <Meter
              label="S/N"
              value={`${status.snDb > 0 ? '+' : ''}${status.snDb.toFixed(1)} dB`}
            />
          )}
          {status.vuDbfs !== null && (
            <Meter label="VU" value={`${status.vuDbfs.toFixed(0)} dBFS`} />
          )}
          {status.throughputBps !== null && (
            <Meter label="Throughput" value={`${status.throughputBps} bps`} warn />
          )}
          <Sparkline samples={throughputHistory} height={28} />
          <pre className="radio-panel-mono ardop-stats">
{`Peer   ${status.peer ?? '—'}
Mode   ${status.mode ?? '—'}
Width  ${status.widthHz !== null ? `${status.widthHz} Hz` : '—'}
PTT    ${status.pttBackend ?? '—'}
RX     ${status.bytesRx} B  ·  TX ${status.bytesTx} B
Up     ${fmtUptime(status.uptimeSec)}`}
          </pre>
        </section>
      )}

      <SignalSection
        quality={status.quality}
        snrSamples={snrHistory}
        recentFrames={frameHistory}
        snrCurrent={status.snDb}
      />

      {/* Listen (Accept Inbound) — spec §1.3. ARDOP modem TCP host/port
          already lives in the Radio section above, so this section does
          NOT carry a Listener-setup expander. ARDOP also has no
          station-password layer (ardop-p2p.md divergence 2), so no
          Station Password expander either — the allowlist is the only
          application-layer gate. */}
      <section className="radio-panel-sec" data-testid="ardop-listen-section">
        <h5>Listen (Accept Inbound)</h5>
        <ListenArmButton
          armed={ardopListener.armed}
          minutesRemaining={ardopListener.minutesRemaining}
          boundIdentity={ardopListener.boundIdentityLabel}
          busy={ardopListener.busy}
          helpText="Accepts inbound ARDOP P2P sessions until disarmed or the TTL expires. The modem must be running before arming the listener."
          onArm={ardopListener.arm}
          onDisarm={ardopListener.disarm}
          testIdPrefix="ardop-listen"
        />
        {/* tuxlink-5g5d: operator-configurable arm duration. Blank/0 = no
            expiry (the default). Disabled while armed — the window is fixed at
            arm time. */}
        <label
          className="radio-panel-radio-help"
          data-testid="ardop-listen-ttl-field"
          style={{ display: 'block', marginTop: '0.5rem' }}
        >
          Arm duration (minutes){' '}
          <input
            type="number"
            min={0}
            inputMode="numeric"
            value={listenTtlInput}
            placeholder="no expiry"
            onChange={(e) => setListenTtlInput(e.target.value)}
            onBlur={commitListenTtl}
            disabled={ardopListener.armed}
            data-testid="ardop-listen-ttl-input"
            style={{ width: '6rem' }}
          />
          <span style={{ display: 'block', opacity: 0.8 }}>
            Blank or 0 keeps the listener armed until you disarm it. Enter a number
            of minutes to auto-expire.
          </span>
        </label>
        {ardopListener.error && (
          <p
            className="radio-panel-radio-help"
            data-testid="ardop-listen-error"
            style={{ color: 'var(--error, #f87171)' }}
          >
            {ardopListener.error}
          </p>
        )}
        <details className="expander" data-testid="ardop-allowed-expander">
          <summary className="expander-summary">
            Allowed stations
            <span className="expander-count" data-testid="ardop-allowed-count">
              {ardopListener.allowed.allowAll
                ? 'allow any'
                : ardopListener.allowed.callsigns.length === 0
                ? 'restrict to none'
                : `${ardopListener.allowed.callsigns.length} callsign${ardopListener.allowed.callsigns.length === 1 ? '' : 's'}`}
            </span>
          </summary>
          <AllowedStationsEditor
            allowAll={ardopListener.allowed.allowAll}
            callsigns={ardopListener.allowed.callsigns}
            helpText="ARDOP has no station-password layer — the callsign allowlist is the only application-layer gate."
            onSetAllowAll={ardopListener.setAllowAll}
            onAddCallsign={ardopListener.addCallsign}
            onRemoveCallsign={ardopListener.removeCallsign}
            testIdPrefix="ardop-allowed"
          />
        </details>
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      <section className="radio-panel-sec radio-panel-act">
        {isStopped && (
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-primary"
            data-testid="ardop-start-btn"
            disabled={target.trim() === '' || connecting}
            onClick={onStartClick}
          >
            {connecting ? 'Connecting…' : 'Start'}
          </button>
        )}
        {!isStopped && (
          <>
            <button
              type="button"
              className="radio-panel-btn radio-panel-btn-primary"
              data-testid="ardop-send-receive-btn"
              disabled={!isExchangeReady || exchanging || effectiveTarget === null}
              onClick={onSendReceiveClick}
            >
              {exchanging ? 'Exchanging…' : 'Send/Receive'}
            </button>
            <button
              type="button"
              className="radio-panel-btn radio-panel-btn-bad"
              data-testid="ardop-stop-btn"
              disabled={disconnecting}
              onClick={onStopClick}
            >
              {disconnecting ? 'Stopping…' : 'Stop'}
            </button>
          </>
        )}
        <button
          type="button"
          className="radio-panel-btn"
          data-testid="ardop-open-webgui-btn"
          onClick={onOpenWebGuiClick}
          disabled={isStopped}
          title={
            isStopped
              ? 'Start ARDOP first — ardopcf must be running for its WebGUI to be reachable'
              : "Open ardopcf's built-in Spectrum/Waterfall view in browser"
          }
        >
          Open WebGUI
        </button>
        {connectError !== null && (
          <p className="radio-panel-error" role="alert">{connectError}</p>
        )}
      </section>

    </RadioPanel>
  );
}
