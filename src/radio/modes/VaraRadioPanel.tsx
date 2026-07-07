// src/radio/modes/VaraRadioPanel.tsx
//
// Phase 2 (bd-tuxlink-dfmf) — VARA HF / VARA FM panel. Conservative scope:
// open/close the TCP transport to the operator's VARA instance, surface the
// connect/error state, edit the persisted VaraUiConfig. No RF connect-to-peer
// yet — that path needs the session state machine + RADIO-1 consent flow,
// both Phase 3 deliverables.
//
// Mode awareness: the panel renders the same controls for `vara-hf` and
// `vara-fm` — the operator picks the variant via which VARA instance they
// point tuxlink at (different cmd_port). The mode prop drives only the
// panel header title.
//
// Pi-availability (tuxlink-xfo): on aarch64 hosts (Pi 5), Wine cannot run
// VARA — the panel reads `platform_info.vara_supported` and renders a
// disabled-with-banner state so the operator understands why the controls
// are unusable. The Start button is disabled regardless of the form state.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ChangeEvent, KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { readLastTarget, writeLastTarget } from '../../connections/connectDispatch';
import { Button, Select, Field } from '../../controls';
import { RadioPanel, type RadioPanelState } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { useVaraConfig } from '../useVaraConfig';
import type { VaraUiConfig } from '../useVaraConfig';
import type { RadioPanelMode } from '../types';
import { AllowedStationsEditor } from '../sections/AllowedStationsEditor';
import { ListenArmButton } from '../sections/ListenArmButton';
import { useListenerState } from '../sections/useListenerState';
import { useActiveIdentity } from '../../shell/useIdentities';
import { FavoritesTabs } from '../../favorites/FavoritesTabs';
import { useFavorites } from '../../favorites/useFavorites';
import { listenGatewayPrefill } from '../../favorites/prefillEvent';
import { tsLocal } from '../../favorites/ts-local';
import type { FavoriteDial } from '../../favorites/types';
import { RigControlSection } from './RigControlSection';
import { VaraProvision } from '../VaraProvision';
import {
  parseFreqInputToHz,
  dialFreqToMhzString,
  dialsToQsyCandidates,
} from './freq';
import './VaraRadioPanel.css';
import '../sections/ListenSection.css';

/** Mirror of Rust's `modem_status::TransportOwner` (shared arbiter enum —
 *  `src-tauri/src/modem_status.rs`). camelCase per its own
 *  `#[serde(rename_all = "camelCase")]` derive. Lists every current
 *  variant; a future addition to the Rust enum needs a matching addition
 *  here (the panel only branches on the two "listener owns it" values
 *  explicitly, so an unhandled future variant just falls through as
 *  "not listener-armed" rather than a type error). */
type TransportOwnerDto =
  | 'none'
  | 'listenerArmed'
  | 'listenerInbound'
  | 'outboundPending'
  | 'outbound'
  | 'heartbeat';

/** Mirror of Rust's `commands::VaraStatus`. camelCase per the Rust
 *  `#[serde(rename_all = "camelCase")]` on the struct. */
interface VaraStatusDto {
  // tuxlink-6urh2: 'socket-lost' is the heartbeat-detected cmd-port-drop
  // state (Rust `VaraState::SocketLost`, wire form via
  // `#[serde(rename = "socket-lost")]`). Distinct from 'error' (the
  // Start-time TCP-connect failure) — SocketLost means the transport WAS
  // open and the peer went away underneath it.
  state: 'closed' | 'connecting' | 'open' | 'error' | 'socket-lost';
  lastError: string | null;
  boundHost: string | null;
  boundCmdPort: number | null;
  // tuxlink-6urh2 v2 (Codex P1 #1b): the backend's `take_transport()`
  // (called when the listener consumer arms) sets the CACHED `state` to
  // `'closed'` for the whole armed/exchange window — see
  // `VaraSession::take_transport`'s doc in commands.rs. `listenerArmed` /
  // `transportOwner` are the arbiter's LIVE overlay (from
  // `VaraSession::snapshot()`) that stay accurate through that window.
  // Optional because older backends (pre-tuxlink-0ye6 Task 3.0 wire-in)
  // and defensive test fixtures may omit them — treated as "not armed" /
  // "none" when absent.
  listenerArmed?: boolean;
  transportOwner?: TransportOwnerDto;
}

/** Mirror of Rust's `commands::PlatformInfo`. */
interface PlatformInfoDto {
  arch: string;
  os: string;
  varaSupported: boolean;
}

export interface VaraRadioPanelProps {
  mode: RadioPanelMode;
  onClose: () => void;
  /** tuxlink-6jpf: open the station finder ("Find a gateway") from the panel. */
  onFindGateway?: () => void;
}

/** Documented bandwidth presets. The selector lets the operator pick one of
 *  these and persists `bandwidth_hz`. Empty (string "") = "leave at VARA's
 *  default" — the start command skips the BW setter in that case. */
const BANDWIDTH_OPTIONS: { value: number | ''; label: string }[] = [
  { value: '', label: 'Auto (VARA default)' },
  { value: 500, label: '500 Hz (narrow HF)' },
  { value: 2300, label: '2300 Hz (HF Standard)' },
  { value: 2750, label: '2750 Hz (HF Tactical)' },
];

function mapVaraStateToPanelState(s: VaraStatusDto['state']): RadioPanelState {
  switch (s) {
    case 'closed':
      return 'disconnected';
    case 'connecting':
      return 'connecting';
    case 'open':
      return 'connected';
    case 'error':
      return 'error';
    // tuxlink-6urh2: the heartbeat-detected drop is a distinct wire state
    // from the Start-time connect failure, but the panel header only has
    // connected/connecting/disconnected/error — 'error' is the right
    // bucket (it renders the same way an operator needs to notice + act).
    case 'socket-lost':
      return 'error';
    default:
      return 'error';
  }
}

export function VaraRadioPanel({ mode, onClose, onFindGateway }: VaraRadioPanelProps) {
  const { config, setConfig } = useVaraConfig();
  const [status, setStatus] = useState<VaraStatusDto>({
    state: 'closed',
    lastError: null,
    boundHost: null,
    boundCmdPort: null,
  });
  const [platform, setPlatform] = useState<PlatformInfoDto | null>(null);
  // VARA provisioning (tuxlink-w7212): the anytime / post-upgrade entry point to
  // set up VARA HF under WINE. engineAvailable gates the affordance to builds
  // that bundle the setup engine; showProvision toggles the inline flow.
  const [engineAvailable, setEngineAvailable] = useState(false);
  const [showProvision, setShowProvision] = useState(false);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  // tuxlink-p6iq: set when a "use this gateway" action (Find-a-Station "Use →"
  // or a favorite Connect) wants the transport opened so the operator lands
  // connectable instead of on disabled Send/Receive. Consumed by the auto-open
  // effect once the live transport state is known (race-safe vs the mount poll).
  const [autoOpenPending, setAutoOpenPending] = useState(false);
  // True once the mount `vara_status` poll has resolved. The auto-open effect
  // waits for this so it never acts on the indistinguishable INITIAL 'closed'
  // (Codex p6iq [P1]).
  const [initialStatusLoaded, setInitialStatusLoaded] = useState(false);
  // Live status ref so the stable openSession callback reads the current state
  // without being re-created on every render.
  const statusRef = useRef<VaraStatusDto>(status);
  statusRef.current = status;
  // tuxlink-6urh2: ref so the recurring status-poll effect (below) can read
  // the CURRENT busy flag from its setInterval closure without re-creating
  // the interval on every render. (exchangingRef is declared further down,
  // right after `exchanging`'s own useState — it must come after that
  // declaration in source order.)
  const busyRef = useRef(false);
  busyRef.current = busy;
  // Synchronous open-in-flight guard — set true BEFORE the first await so a
  // manual Start click and the auto-open effect firing in the same tick cannot
  // both issue vara_open_session (a render-synced ref can't: it only updates at
  // render). Codex p6iq [P2].
  const openInFlightRef = useRef(false);
  // An open has been INITIATED — the one-shot mount status poll must not clobber
  // it with a stale mount-time snapshot if it resolves late. Codex p6iq [P1].
  const openInitiatedRef = useRef(false);

  // Local input mirrors so the operator can type freely; commit on blur.
  const [hostInput, setHostInput] = useState<string>('');
  const [cmdPortInput, setCmdPortInput] = useState<string>('');
  const [dataPortInput, setDataPortInput] = useState<string>('');

  // Dial surface (tuxlink-xglf) — mirrors ARDOP/Packet. `target` is the RMS
  // gateway callsign; `exchanging` drives the Send/Receive in-flight label and
  // re-entrancy guard. VARA's b2f is a SINGLE blocking connect→B2F→disconnect
  // (modem_vara_b2f_exchange) requiring a prior open session, so `reached` is
  // recorded on the call's resolve and `failed` in its catch (Packet semantics).
  const [target, setTarget] = useState<string>('');
  const [exchanging, setExchanging] = useState(false);
  // tuxlink-6urh2: mirrors busyRef — read by the status-poll effect.
  const exchangingRef = useRef(false);
  exchangingRef.current = exchanging;
  // tuxlink-8fkkk A3: operator-entered frequency in MHz for CAT-based QSY before
  // the VARA connect. "7.102" → 7102000 Hz. Empty/invalid → null (backend skips
  // the pre-audio retune). Mirrors ARDOP; uses the shared parse/normalize helper.
  const [freqMhz, setFreqMhz] = useState<string>('');
  const freqHz = useMemo<number | null>(() => parseFreqInputToHz(freqMhz), [freqMhz]);
  // recordAttempt logs the HONEST on-air outcome. mode.kind is literally
  // 'vara-hf' / 'vara-fm' — the same strings as the favorites RadioMode union.
  const { recordAttempt } = useFavorites(mode.kind);
  // The favorite whose Connect was last clicked. Carries its metadata into the
  // connection record IFF its gateway matches the dial target. Cleared on a
  // manual target edit (a hand-typed target is not the prefilled favorite).
  const pendingDialRef = useRef<FavoriteDial | null>(null);
  // tuxlink-8fkkk Task B: the ranked QSY-on-fail candidate list from the last
  // Find-a-Station "Use →". Sent as `qsyCandidates` on Send/Receive when it has
  // more than one entry; a single/empty list falls back to the legacy single dial.
  const pendingCandidatesRef = useRef<FavoriteDial[]>([]);
  // RADIO-1: a favorite's Connect is PRE-FILL ONLY — it sets `target` and NEVER
  // invokes the exchange. The operator's later Send/Receive click is the Part 97
  // consent gate.
  const handlePrefill = useCallback(
    (dial: FavoriteDial, candidates?: FavoriteDial[]) => {
      setTarget(dial.gateway);
      pendingDialRef.current = dial;
      pendingCandidatesRef.current = candidates ?? [];
      // tuxlink-vu97: persist so the ribbon Connect can dial this target (pane closed).
      writeLastTarget(mode.kind, dial.gateway);
      // tuxlink-8fkkk A3/C4: normalize the dial's freq metadata to a MHz string
      // (handles both Find-a-Station MHz and saved-favorite kHz) and populate the
      // field so Send/Receive CAT-tunes the rig. CLEAR the field when the dial
      // carries no parseable freq so a stale freq never tunes the wrong frequency.
      setFreqMhz(dialFreqToMhzString(dial) ?? '');
      // tuxlink-p6iq: a "use this gateway" action lands the operator CONNECTABLE.
      // Request an auto-open of the transport — opening the cmd/data sockets does
      // NOT transmit (the Start button says as much), so this preserves the
      // prefill-never-transmits rule: Send/Receive stays the explicit consent click.
      setAutoOpenPending(true);
    },
    [mode.kind],
  );

  // tuxlink-ypz3 (3a): restore the per-mode persisted target so switching modes
  // (or reopening the pane) doesn't blank the previously-dialed station. The
  // ribbon Connect already reads this same localStorage key (connectDispatch);
  // the panel now mirrors it on the visible input. Keyed on mode.kind so a
  // vara-hf↔vara-fm switch (same component instance — AppShell applies no key,
  // so React reuses it instead of remounting) ALSO re-restores from the right
  // key. Passive: seeds the string only, never auto-opens the transport or
  // transmits (RADIO-1: Send/Receive stays the explicit consent click).
  useEffect(() => {
    setTarget(readLastTarget(mode.kind));
  }, [mode.kind]);

  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // VARA listener arms + allowlist plumbing (tuxlink-9ls2). VARA matches
  // ARDOP's posture: no station-password layer (peers don't challenge per
  // the clean-sheet decision), allowlist is the only application-layer
  // gate. The TTL defaults to the hook's 1h (no get-config Tauri command
  // for VARA listener yet — operator-tunable TTL is a follow-up).
  const activeIdentity = useActiveIdentity();
  const varaListener = useListenerState({
    activeIdentityLabel: activeIdentity.data?.address_as ?? null,
    commands: {
      listen: 'vara_listen',
      setListen: 'vara_set_listen',
      allowedGet: 'vara_allowed_stations_get',
      allowedAddCallsign: 'vara_allowed_stations_add',
      allowedAddCallsignArgKey: 'callsign',
      allowedRemoveCallsign: 'vara_allowed_stations_remove',
      allowedRemoveCallsignArgKey: 'callsign',
      allowedSetAllowAll: 'vara_allowed_stations_set_allow_all',
      // Tauri auto-camelCases Rust arg `allow_all: bool` → wire key `allowAll`.
      // Mirrors the ARDOP fix at ArdopRadioPanel.tsx (Codex review
      // 2026-06-03 [P2] tuxlink-7vea) — the prior snake_case key meant
      // Tauri delivered no value to the Rust handler.
      allowedSetAllowAllArgKey: 'allowAll',
    },
  });

  const varaAllowedSummary = (() => {
    if (varaListener.allowed.allowAll) return 'allow any';
    const c = varaListener.allowed.callsigns.length;
    if (c === 0) return 'restrict to none';
    return `${c} callsign${c === 1 ? '' : 's'}`;
  })();

  // Hydrate inputs from the loaded config. Re-runs when config changes (e.g.
  // a peer hook persisted an update via the same-window CustomEvent).
  useEffect(() => {
    setHostInput(config.host);
    setCmdPortInput(String(config.cmd_port));
    setDataPortInput(String(config.data_port));
  }, [config.host, config.cmd_port, config.data_port]);

  // Load platform info once on mount for the Pi-availability gating.
  useEffect(() => {
    let cancelled = false;
    invoke<PlatformInfoDto>('platform_info')
      .then((p) => {
        if (!cancelled) setPlatform(p);
      })
      .catch(() => {
        // platform_info has no failure path in practice (cfg!-based, no
        // I/O). If it's missing for some reason (older backend in dev),
        // err on the side of permissive — leave platform=null, which
        // does NOT disable the controls.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Load the initial status on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<VaraStatusDto>('vara_status')
      .then((s) => {
        if (cancelled) return;
        // Don't clobber an open that the operator/auto-flow already kicked off
        // while this poll was in flight (Codex p6iq [P1]).
        if (!openInitiatedRef.current && s) setStatus(s);
        setInitialStatusLoaded(true);
      })
      .catch(() => {
        // status defaults to closed; mark loaded so the auto-open gate releases.
        if (!cancelled) setInitialStatusLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // tuxlink-6urh2: recurring status poll. Before this effect existed the
  // panel NEVER re-read `vara_status` after the mount-time snapshot except
  // in the Start/Stop/Send-Receive call sites' own `finally` blocks — a
  // backend-driven transition with no frontend call in flight (the VARA
  // drop-detection heartbeat stamping `SocketLost` while the operator is
  // just sitting on an idle-open session) was invisible until the operator
  // happened to click something. Polling makes that transition observable.
  //
  // Skip applying a tick's result while:
  //  - `openInFlightRef` — a Start click's own `vara_open_session` call is
  //    in flight; that call's `finally` already re-syncs status, and a
  //    poll resolving in the same window could show a stale pre-open
  //    snapshot.
  //  - `busyRef` — a Start/Stop click is in flight (shared `busy` flag);
  //    same reasoning.
  //  - `exchangingRef` — an outbound Send/Receive is running.
  //    `VaraSession::take_transport` (the dial's claim on the transport)
  //    transiently sets `status.state = Closed` for the WHOLE exchange
  //    window, not just a connect-timing race — applying a poll tick here
  //    would flip the panel back to the Start button mid-exchange. The
  //    `onSendReceive` `finally` block already re-syncs status once the
  //    exchange (and the transport install-back) completes.
  //
  // Deliberately NOT reusing `openInitiatedRef` for this gate (unlike the
  // one-shot mount poll above): that ref is set true on the FIRST open and
  // never reset, so gating a RECURRING poll on it would permanently
  // disable polling after the operator's first Start click — defeating
  // the point of observing a heartbeat-driven SocketLost that can occur
  // long after that first open. The three transient refs above cover the
  // actual clobber hazard without that side effect.
  useEffect(() => {
    let cancelled = false;
    const id = setInterval(() => {
      if (cancelled) return;
      if (openInFlightRef.current || busyRef.current || exchangingRef.current) return;
      invoke<VaraStatusDto>('vara_status')
        .then((s) => {
          if (!cancelled && s) setStatus(s);
        })
        .catch(() => {
          // Transient poll failure — keep the prior status and try again
          // on the next tick.
        });
    }, 2500);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const platformBlocked = platform !== null && !platform.varaSupported;

  // Is the VARA setup engine bundled in this build? Gates the "Set up VARA HF…"
  // affordance (tuxlink-w7212).
  useEffect(() => {
    let cancelled = false;
    void invoke<boolean>('vara_engine_available')
      .then((ok) => {
        if (!cancelled) setEngineAvailable(ok);
      })
      .catch(() => {
        if (!cancelled) setEngineAvailable(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const commitHost = () => {
    const trimmed = hostInput.trim();
    if (!trimmed) {
      setHostInput(config.host); // revert
      setActionError('Host cannot be empty — reverted.');
      return;
    }
    if (trimmed === config.host) return;
    setConfig({ ...config, host: trimmed });
  };

  const commitPort = (
    raw: string,
    field: 'cmd_port' | 'data_port',
    setInput: (s: string) => void,
  ) => {
    const trimmed = raw.trim();
    const n = Number(trimmed);
    if (!Number.isInteger(n) || n < 1 || n > 65535) {
      setInput(String(config[field]));
      setActionError(`Invalid ${field.replace('_', ' ')} "${trimmed}" — must be 1..65535. Reverted.`);
      return;
    }
    if (n === config[field]) return;
    setConfig({ ...config, [field]: n });
  };

  const onBandwidthChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const raw = e.target.value;
    const next: VaraUiConfig = {
      ...config,
      bandwidth_hz: raw === '' ? null : parseInt(raw, 10),
    };
    setConfig(next);
  };

  const onPortKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  // Open the VARA transport. Shared by the explicit Start click and the
  // auto-open-on-use path (tuxlink-p6iq). Self-guards via refs so a concurrent
  // call (e.g. the auto-open effect racing a manual click) is a no-op rather
  // than a double-open. tuxlink-poh6: NO platformBlocked guard here — the
  // operator CAN open a session against a remote VARA host from a Pi.
  const openSession = useCallback(async () => {
    if (openInFlightRef.current) return;
    const st = statusRef.current.state;
    if (st === 'open' || st === 'connecting') return;
    // Claim in-flight + initiated SYNCHRONOUSLY, before any await, so a racing
    // caller in the same tick is a no-op (Codex p6iq [P1]/[P2]).
    openInFlightRef.current = true;
    openInitiatedRef.current = true;
    setBusy(true);
    setActionError(null);
    try {
      // tuxlink-o0c8: thread the sidebar-selected intent (cms / p2p / radio-only)
      // from mode.intent — mirroring ARDOP (tuxlink-nnws). transportKind is the
      // panel's mode.kind ('vara-hf' vs 'vara-fm') so the backend records the
      // operator-meaningful discriminator on session state.
      const next = await invoke<VaraStatusDto>('vara_open_session', {
        intent: mode.intent,
        transportKind: mode.kind,
      });
      // Defensive: only adopt a real status object (the backend always returns
      // one; this guards a malformed response from clobbering state with undefined).
      if (next) setStatus(next);
    } catch (e) {
      setActionError(`Start failed: ${String(e)}`);
      // Refresh status so a backend-side Error state surfaces.
      try {
        const s = await invoke<VaraStatusDto>('vara_status');
        if (s) setStatus(s);
      } catch {
        /* keep prior status */
      }
    } finally {
      setBusy(false);
      openInFlightRef.current = false;
    }
  }, [mode.intent, mode.kind]);

  const onStartClick = () => {
    void openSession();
  };

  // tuxlink-p6iq: consume an auto-open request. Gate on `initialStatusLoaded` so
  // we never act on the indistinguishable INITIAL 'closed' — only once the mount
  // poll has reported the real backend state (Codex p6iq [P1]). Then open only
  // if genuinely closed/error: an already-open/connecting session just clears the
  // request, and openSession's synchronous guard prevents a double-open.
  useEffect(() => {
    if (!autoOpenPending || !initialStatusLoaded || busy) return;
    if (status.state === 'open' || status.state === 'connecting') {
      setAutoOpenPending(false);
      return;
    }
    setAutoOpenPending(false);
    void openSession();
  }, [autoOpenPending, initialStatusLoaded, busy, status.state, openSession]);

  const onStopClick = async () => {
    if (busy) return;
    setBusy(true);
    setActionError(null);
    try {
      const next = await invoke<VaraStatusDto>('vara_close_session');
      setStatus(next);
    } catch (e) {
      setActionError(`Stop failed: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  // Station-picker / favorite prefill → fills the target only (RADIO-1). Never
  // transmits. The subscription filters on mode.kind so a vara-fm prefill does
  // not land in a vara-hf pane and vice-versa.
  useEffect(
    () => listenGatewayPrefill(mode.kind, handlePrefill),
    [mode.kind, handlePrefill],
  );

  // Build the dial for a connection record. The gateway is the dial target. If
  // the prefilled favorite matches it (case-insensitive), carry its metadata
  // (band/grid/note) into the record; otherwise record a minimal manual dial.
  const buildRecordDial = (call: string): FavoriteDial => {
    const gw = call.trim();
    const pend = pendingDialRef.current;
    if (pend && pend.gateway.trim().toUpperCase() === gw.toUpperCase()) {
      return { ...pend, mode: mode.kind, gateway: gw };
    }
    return { mode: mode.kind, gateway: gw };
  };

  // Send/Receive — the on-air dial. modem_vara_b2f_exchange is a SINGLE blocking
  // connect→B2F→disconnect that REQUIRES an open session (vara_open_session
  // installed the transport). The pre-air guard returns BEFORE any record path,
  // so an empty-target / not-open / re-entrant click logs nothing. `reached` is
  // recorded on resolve, `failed` in the catch (NEVER the finally) so a pre-air
  // bail never logs a spurious gateway failure. Bounded airtime + working abort
  // live in the backend; the operator aborts an in-flight dial via Stop
  // (vara_close_session → abort_in_flight, bounded ~2s ABORT).
  const onSendReceive = async () => {
    const call = target.trim();
    if (!call || exchanging || status.state !== 'open') return;
    const dial = buildRecordDial(call);
    setExchanging(true);
    setActionError(null);
    try {
      // tuxlink-o0c8: route the exchange under the sidebar-selected intent
      // (mode.intent), mirroring ARDOP (tuxlink-nnws). transportKind is the
      // panel's mode.kind so the backend records the operator-meaningful HF/FM
      // discriminator on session state.
      // tuxlink-8fkkk A3/Task B: send the pre-audio tune frequency and (when a
      // Find-a-Station "Use →" supplied more than one ranked candidate) the
      // ordered `qsyCandidates` list the backend's qsy_on_fail walk visits in
      // turn. A non-empty list overrides the single `target`/`freqHz`; otherwise
      // the legacy single-dial path is unchanged.
      const candidates = pendingCandidatesRef.current;
      await invoke('modem_vara_b2f_exchange', {
        target: call,
        intent: mode.intent,
        transportKind: mode.kind,
        freqHz, // null when unset → backend skips the pre-audio CAT retune
        qsyCandidates:
          candidates.length > 1 ? dialsToQsyCandidates(candidates) : null,
      });
      void recordAttempt(dial, 'reached', tsLocal());
    } catch (e) {
      const msg = String(e);
      // tuxlink-n95sr: the send/receive failure is surfaced in the SESSION LOG
      // by the backend (emit_vara_log Error line, vara/commands.rs), matching
      // ARDOP (tuxlink-nnjz). Errors belong in the log, not a separate inline
      // action-error element below it — so do NOT setActionError here.
      console.debug('VARA send/receive failed (surfaced in session log):', e);
      // Codex 2026-06-10 P2 #2: a pre-air ownership failure (the transport was
      // never available — the backend's take_transport returned None, e.g. the
      // listener consumer holds it, or a stale-status race) NEVER transmitted,
      // so it is not an on-air outcome. Recording `failed` for it would pollute
      // the favorite's reach/fail history with a non-dial. Only a failure AFTER
      // the exchange went on-air is an honest `failed`. The backend signals the
      // pre-air bail with the "session not open" message.
      if (!/session not open/i.test(msg)) {
        void recordAttempt(dial, 'failed', tsLocal());
      }
    } finally {
      setExchanging(false);
      // The exchange returns the session to Open (within-session dial); re-read
      // status so any backend-side state change surfaces in the UI.
      try {
        const s = await invoke<VaraStatusDto>('vara_status');
        setStatus(s);
      } catch {
        /* keep prior status */
      }
    }
  };

  const headerSub = status.boundHost
    ? `${status.boundHost}:${status.boundCmdPort ?? '?'}`
    : `${hostInput || config.host}:${cmdPortInput || config.cmd_port}`;

  // tuxlink-6urh2 v2 (Codex P1 #1b): a listener-owned/armed transport must
  // read as OCCUPIED even though the backend's cached `status.state` reads
  // 'closed' during that window (see `VaraStatusDto.transportOwner`'s
  // doc). Without the `transportOwner`/`listenerArmed` fold-in, a
  // P2p/RadioOnly session that auto-armed its listener would show the
  // Start button again — inviting a double-open (`vara_open_session_inner`
  // now also rejects that at the backend per the reopen-guard fix, but the
  // UI should never offer the action in the first place).
  const listenerOwnsTransport =
    status.listenerArmed === true ||
    status.transportOwner === 'listenerArmed' ||
    status.transportOwner === 'listenerInbound';
  const isOpen =
    status.state === 'open' || status.state === 'connecting' || listenerOwnsTransport;

  return (
    <RadioPanel
      mode={mode}
      state={mapVaraStateToPanelState(status.state)}
      sub={headerSub}
      onClose={onClose}
      onFindGateway={onFindGateway}
    >
      {platformBlocked && (
        <p
          className="radio-panel-info radio-panel-info-compact"
          role="status"
          data-testid="vara-platform-banner"
          title={
            'VARA is x86 Windows software; Wine cannot run on this architecture ' +
            `(${platform?.arch}, ${platform?.os}). Point Host at a remote machine ` +
            'running VARA (x86 Windows, or x86 Linux + Wine); tuxlink connects to ' +
            'it over TCP.'
          }
        >
          VARA can&rsquo;t run on <code>{platform?.arch}</code> — point Host at a remote
          x86 VARA instance.
        </p>
      )}

      {!platformBlocked && engineAvailable && (
        <section className="radio-panel-sec" data-testid="vara-setup-section">
          {showProvision ? (
            <VaraProvision
              variant="panel"
              onComplete={() => setShowProvision(false)}
              onSkip={() => setShowProvision(false)}
            />
          ) : (
            <div className="radio-panel-input-row">
              <span>VARA HF setup</span>
              <Button
                type="button"
                data-testid="vara-setup-open"
                onClick={() => setShowProvision(true)}
              >
                Set up VARA HF…
              </Button>
            </div>
          )}
        </section>
      )}

      <section className="radio-panel-sec">
        <h5>VARA host</h5>
        <label className="radio-panel-input-row">
          <span>Host</span>
          <Field
            type="text"
            className="radio-panel-input"
            data-testid="vara-host-input"
            value={hostInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="127.0.0.1"
            disabled={isOpen}
            onChange={(e) => setHostInput(e.target.value)}
            onBlur={commitHost}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Cmd port</span>
          <Field
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="vara-cmd-port-input"
            value={cmdPortInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="8300"
            disabled={isOpen}
            onChange={(e) => setCmdPortInput(e.target.value)}
            onBlur={() => commitPort(cmdPortInput, 'cmd_port', setCmdPortInput)}
            onKeyDown={onPortKey}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Data port</span>
          <Field
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="vara-data-port-input"
            value={dataPortInput}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="8301"
            disabled={isOpen}
            onChange={(e) => setDataPortInput(e.target.value)}
            onBlur={() => commitPort(dataPortInput, 'data_port', setDataPortInput)}
            onKeyDown={onPortKey}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Bandwidth</span>
          <Select
            className="radio-panel-input"
            data-testid="vara-bandwidth-select"
            value={config.bandwidth_hz ?? ''}
            disabled={isOpen}
            onChange={onBandwidthChange}
          >
            {BANDWIDTH_OPTIONS.map((opt) => (
              <option key={String(opt.value)} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </Select>
        </label>
      </section>

      {/* tuxlink-8fkkk Task A1UI: Rig control — shared with ARDOP so the operator
          configures hamlib / CAT serial / QSY in one place for both modes. Reads
          and writes Config.rig (config_get_rig / config_set_rig). */}
      <section className="radio-panel-sec" data-testid="vara-rig-section">
        <RigControlSection storageKeyPrefix="vara" />
      </section>

      <section className="radio-panel-sec" data-testid="vara-status-section">
        <h5>Transport</h5>
        <p className="radio-panel-mono" data-testid="vara-state-display">
          {`State: ${status.state}`}
        </p>
        {status.lastError && (
          <p className="radio-panel-error" data-testid="vara-last-error">
            {status.lastError}
          </p>
        )}
        {/* tuxlink-6urh2: SocketLost is heartbeat-detected, not an operator
            action — the plain error string above doesn't say what to do
            about it. isOpen already excludes 'socket-lost' (see below), so
            the action row already reverts to the Start button; this note
            just tells the operator that. */}
        {status.state === 'socket-lost' && (
          <p className="radio-panel-radio-help" data-testid="vara-socket-lost-banner">
            The VARA connection dropped on its own. Press Start to reopen it.
          </p>
        )}
      </section>

      {/* Connect (tuxlink-xglf) — Favorites / Recent / Manual surface + the
          on-air Send/Receive. M7's VARA Manual-only exclusion was retired here:
          VARA HF dials RMS gateways like ARDOP, so favorites are meaningful. A
          favorite's Connect PRE-FILLS the target via handlePrefill and never
          transmits (RADIO-1); Send/Receive stays OUTSIDE the tabs so it is
          visible on every tab. Send/Receive requires an open session — it is
          disabled until Start (above) reports the transport Open. */}
      <section className="radio-panel-sec" data-testid="vara-connect-section">
        <h5>Connect</h5>
        <FavoritesTabs
          mode={mode.kind}
          onPrefill={handlePrefill}
          manualContent={
            <label className="radio-panel-input-row">
              <span>To</span>
              <Field
                type="text"
                className="radio-panel-input"
                data-testid="vara-target-input"
                // tuxlink-8c9f: the dial target is a PEER station's callsign for
                // P2P (mirrors WLE's "remote callsign" in Vara P2P), and an RMS
                // gateway for cms / radio-only (both connect to an RMS — radio-only
                // is gateway-routed, just over the radio-only network).
                placeholder={
                  mode.intent === 'p2p' ? 'peer station call sign' : 'RMS gateway call sign'
                }
                value={target}
                spellCheck={false}
                autoCapitalize="characters"
                autoCorrect="off"
                onChange={(e) => {
                  setTarget(e.target.value);
                  // A hand-typed target is not the prefilled favorite — drop the
                  // association so the record doesn't carry stale metadata.
                  pendingDialRef.current = null;
                  // tuxlink-8fkkk Task B: a manual target is a single dial —
                  // drop any ranked QSY candidates from a prior prefill.
                  pendingCandidatesRef.current = [];
                  // tuxlink-vu97: persist the configured target so the ribbon
                  // Connect can fire VARA's full send/receive with this pane closed.
                  writeLastTarget(mode.kind, e.target.value);
                }}
              />
            </label>
          }
        />
        {/* tuxlink-8fkkk A3: frequency + Tune affordance, mirroring ARDOP. The
            operator types the gateway frequency in MHz; the panel parses it to Hz
            and sends it on Send/Receive so the backend CAT-tunes the rig before
            the VARA connect. Tune… (ardop_tune_rig — mode-agnostic Tune-only,
            reads Config.rig) sets the rig WITHOUT dialing. Both paths are no-ops
            when the field is blank (freqHz === null). */}
        <div className="radio-panel-input-row">
          <label htmlFor="vara-freq">Frequency (MHz)</label>
          <Field
            id="vara-freq"
            data-testid="vara-freq"
            className="radio-panel-input radio-panel-mono"
            value={freqMhz}
            onChange={(e) => setFreqMhz(e.target.value)}
            placeholder="14.105"
            inputMode="decimal"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
          />
          <Button
            tone="neutral" emphasis="outline" size="xs"
            data-testid="vara-tune"
            disabled={freqHz === null}
            onClick={() => {
              if (freqHz !== null) void invoke('ardop_tune_rig', { freqHz });
            }}
          >
            Tune…
          </Button>
        </div>
        {/* tuxlink-n95sr #3: Send/Receive moved OUT of the Connect section into
            the action row below, mirroring ARDOP's control model exactly — the
            action row toggles Start ⇄ (Send/Receive + Stop), so Start and
            Send/Receive are never both visible. Operator decision: ARDOP and
            VARA must not diverge on connect-control UX. */}
        {/* tuxlink-p6iq: a VISIBLE closed-state hint (not just the button's hover
            title) so the manual-target path is never a silent dead-end. Find-a-
            Station "Use →" auto-opens the transport; an operator who instead
            opens the panel and types a target needs to know to press Start.
            tuxlink-6urh2 v2: gate on `!isOpen` (not the raw `status.state`
            check) so this hint doesn't contradict the action row during the
            listener-armed window, where `status.state` reads 'closed' but
            the panel is correctly showing Stop, not Start (see `isOpen`'s
            doc above). */}
        {!isOpen && (
          <p className="radio-panel-radio-help" data-testid="vara-transport-hint">
            Transport closed — press <strong>Start</strong> below to open the VARA
            session, then Send / Receive.
          </p>
        )}
      </section>

      {/* Listen (Accept Inbound) — VARA P2P listener arms + allowlist.
          Mirrors the Telnet/Packet/ARDOP Listen sections per spec
          2026-06-03-listener-ui-design.md §1.3, extended to VARA in
          tuxlink-9ls2. The arm button is disabled when the VARA transport
          is not Open because vara_listen refuses to arm without an open
          session — the operator must press Start above first. */}
      <section
        className="radio-panel-sec"
        data-testid="vara-listen-section"
      >
        <h5>Listen (Accept Inbound)</h5>

        <ListenArmButton
          armed={varaListener.armed}
          minutesRemaining={varaListener.minutesRemaining}
          boundIdentity={varaListener.boundIdentityLabel}
          // Separate concerns (tuxlink-tccc): `busy` is in-flight-call (drives
          // the transient "Arming…" / "Disarming…" label), `disabled` is the
          // precondition gate (transport must be Open). Folding both into
          // `busy` made the button say "Arming…" on mount even when nothing
          // was arming, because !isOpen is true at first render.
          busy={varaListener.busy}
          disabled={!isOpen}
          helpText={
            isOpen
              ? 'Sends LISTEN ON to the VARA modem and accepts inbound peer CONNECTED events until disarmed or the TTL expires. VARA has no station-password layer (peers do not challenge); the allowlist below is the gate.'
              : 'Start the VARA transport first (above) — the listener arm requires an open cmd socket so it can send LISTEN ON.'
          }
          onArm={varaListener.arm}
          onDisarm={varaListener.disarm}
          testIdPrefix="vara-listen"
        />
        {varaListener.error && (
          <p
            className="radio-panel-radio-help"
            data-testid="vara-listen-error"
            style={{ color: 'var(--error, #f87171)' }}
          >
            {varaListener.error}
          </p>
        )}

        {/* Allowed stations — callsign-only (VARA is RF; no IP layer). */}
        <details className="expander" data-testid="vara-allowed-expander">
          <summary className="expander-summary">
            Allowed stations
            <span className="expander-count" data-testid="vara-allowed-count">
              {varaAllowedSummary}
            </span>
          </summary>
          <AllowedStationsEditor
            allowAll={varaListener.allowed.allowAll}
            callsigns={varaListener.allowed.callsigns}
            helpText="Match logic: when Allow-any is OFF, only peers whose callsign matches the list are admitted. VARA is RF so there is no IP-pattern layer. No station-password layer either (peers do not challenge over VARA)."
            onSetAllowAll={varaListener.setAllowAll}
            onAddCallsign={varaListener.addCallsign}
            onRemoveCallsign={varaListener.removeCallsign}
            testIdPrefix="vara-allowed"
          />
        </details>
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      {/* tuxlink-n95sr #3: action row mirrors ARDOP's toggle EXACTLY. When the
          transport is closed, ONLY Start renders; once open, Start is replaced
          by Send/Receive + Stop. Start and Send/Receive are never both present
          — a state machine that toggles which buttons render, not a disabled
          toggle. Operator decision: no UX divergence between ARDOP and VARA. */}
      <section className="radio-panel-sec radio-panel-act">
        {!isOpen && (
          <Button
            tone="primary" emphasis="soft" size="md"
            data-testid="vara-start-btn"
            disabled={busy}
            onClick={onStartClick}
            title={
              isOpen
                ? 'Already open — Stop first to reconnect'
                : platformBlocked
                  ? 'VARA cannot run on this host (aarch64); point host at a remote VARA instance to use it from here'
                  : 'Open TCP transport to VARA (does not transmit)'
            }
          >
            {busy ? 'Starting…' : 'Start'}
          </Button>
        )}
        {isOpen && (
          <>
            <Button
              tone="primary" emphasis="soft" size="md"
              data-testid="vara-send-receive-btn"
              disabled={
                busy ||
                exchanging ||
                status.state !== 'open' ||
                target.trim() === '' ||
                varaListener.armed
              }
              onClick={onSendReceive}
              title={
                status.state !== 'open'
                  ? 'Open Session first — Send/Receive needs an open VARA transport (press Start)'
                  : varaListener.armed
                    ? 'Disarm the listener first — it owns the VARA transport while armed'
                    : target.trim() === ''
                      ? 'Enter a target RMS gateway call sign'
                      : 'Connect to the target and exchange Winlink mail (transmits)'
              }
            >
              {exchanging ? 'Exchanging…' : 'Send / Receive'}
            </Button>
            <Button
              tone="danger" emphasis="soft" size="md"
              data-testid="vara-stop-btn"
              disabled={busy}
              onClick={onStopClick}
            >
              {busy ? 'Stopping…' : 'Stop'}
            </Button>
          </>
        )}
        {actionError && (
          <p className="radio-panel-error" role="alert" data-testid="vara-action-error">
            {actionError}
          </p>
        )}
      </section>
    </RadioPanel>
  );
}
