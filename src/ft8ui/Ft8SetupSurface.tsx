// src/ft8ui/Ft8SetupSurface.tsx
//
// Setup / degraded surface for the FT-8 listener (plan tuxlink-b026z.4 Task
// C9a — DEVICE-PICKER half; Task C9b adds Step 2 — rig control / Test CAT /
// the `Start listening on <band> →` CTA).
//
// Spec: docs/superpowers/specs/2026-07-11-station-intel-l3-panel-design.md
// §FirstRun ("Setup / degraded surfaces") + §States ("Setup-surface arms by
// blocked reason").
//
// Task C9b notes:
//   - Step 2 renders the SAME shared `RigControlSection` (its third render
//     site, after ARDOP and VARA) with `storageKeyPrefix="ft8"` so its
//     collapse state is independent of the other two panels. It writes the
//     one `Config.rig` — "one radio, one config" (§FirstRun).
//   - Edit-flush contract: RigControlSection persists the CAT-serial/baud/
//     rigctld-binary fields on blur, so a value the operator just typed but
//     hasn't blurred yet is NOT on the backend. Test CAT calls the section's
//     `commitNow()` (via a forwarded ref) FIRST and awaits it before
//     `ft8_cat_probe`, so a just-typed-but-unblurred field never produces a
//     false "radio not responding".
//   - The CTA is disabled-with-reason for every blocker reachable while this
//     component is mounted (it only mounts for a needs-setup-class blocked
//     reason — see the mounting contract below): `wsjtx-absent`,
//     `capture-wedged` (defensive — not currently reachable through this
//     component's mounting contract, but BlockedReasonDto includes it, and a
//     silently-enabled CTA on an unexpected reason would violate "never a
//     silent re-render"), `unsupported-sample-rate` (the currently-configured
//     device is unusable — a different one must be picked via a Step-1 row,
//     same as needs-device-selection), and "no device resolved" for
//     needs-device-selection/device-absent. Picking a Step-1 device row
//     already performs the full meter/start handover + `ft8_listener_start`
//     (C9a), so by the time the CTA could be clicked with a FRESH pick this
//     surface has typically already unmounted via `onStarted`; the CTA's own
//     click handler stays a plain `ft8_listener_start()` (no device rows are
//     ever rendered — and hence no meter poll ever in flight — in the one
//     arm where the CTA can legitimately be enabled).
//
// Mounting contract: the caller (LiveBandStrip / StationFinderPanel, Task
// D1) mounts this component ONLY when `deriveUiState(snapshot).state ===
// 'needs-setup'` — i.e. `snapshot.service` is `{ axis: 'blocked', reason }`
// with a needs-setup-class reason. This component reads `snapshot.service`
// directly (not the derived 9-member `Ft8UiState`) because it needs the raw
// `reason` to pick an arm; B2's `deriveUiState` collapses every needs-setup
// reason to one state on purpose (§States row 6) — that collapse is correct
// for the STRIP header, but the setup surface itself needs the fidelity.
//
// Arms (§States "Setup-surface arms by blocked reason"):
//   - `wsjtx-absent`            → package-install copy FIRST, always — jt9 is
//     a binary that ships with the wsjt-x package, so a missing decoder is
//     never a device/plug-in problem. Device guidance renders only beneath
//     the package copy, and only when no device is already configured.
//   - `unsupported-sample-rate` → the snapshot omits `availableDevices` in
//     this state (L2 presence rule), so this arm fetches via
//     `ft8_list_devices`. Never renders the zero-devices plug-in copy.
//   - `needs-device-selection` / `device-absent` (no device configured)
//     → the device-row picker, sourced from `snapshot.availableDevices`.
//   - zero devices enumerated → plug-in guidance + Refresh. Renders ONLY
//     when enumeration genuinely completed empty, never as a loading
//     fallback (a null/absent list is "still loading", not "empty").

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../controls';
import { RigControlSection } from '../radio/modes/RigControlSection';
import type { RigControlSectionHandle } from '../radio/modes/RigControlSection';
import type {
  AudioDeviceChoice,
  BlockedReasonDto,
  CatProbeDto,
  Ft8CmdError,
  Ft8Snapshot,
  MeterDto,
  StableAudioId,
} from './ft8Types';
import './Ft8SetupSurface.css';

/** ~2 Hz per §FirstRun Step 1 ("live level meter … poll ~2 Hz while the
 *  picker is visible"). */
const METER_POLL_MS = 500;

export interface Ft8SetupSurfaceProps {
  /** The current listener snapshot. The caller only mounts this component
   *  for a needs-setup-class blocked reason (§States row 6); an unexpected
   *  `service.axis` renders nothing (defensive — never crashes). */
  snapshot: Ft8Snapshot;
  /** Fired after a device row's "Use this device" handover completes
   *  (`ft8_set_device` + `ft8_listener_start` both resolved) so the parent
   *  can re-hydrate / dismiss the setup surface. Optional for tests. */
  onStarted?: () => void;
  /** Optional manual "Retry" affordance for the wsjtx-absent arm (re-checks
   *  for the jt9 binary by nudging the parent to re-hydrate the snapshot).
   *  Omitted in tests that don't care about the retry wiring. */
  onRetry?: () => void;
}

function stableIdKey(id: StableAudioId): string {
  return `${id.kind}:${id.value}`;
}

function isFt8CmdError(e: unknown): e is Ft8CmdError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'detail' in e;
}

function cmdErrorMessage(e: unknown): string {
  if (isFt8CmdError(e)) return e.detail;
  if (e instanceof Error) return e.message;
  return 'Something went wrong — try again.';
}

/** Test-CAT failure copy, keyed on `Ft8CmdError.kind` — never the free-text
 *  `detail` (§NewCommands "the UI copy branches on `kind`, never parses
 *  strings"). `ft8_cat_probe`'s real kinds: modem-busy | rig-not-configured |
 *  probe-timeout | internal-error (`device-in-use` is never a probe kind). */
function catProbeErrorCopy(e: Ft8CmdError): string {
  switch (e.kind) {
    case 'modem-busy':
      return 'Radio busy with another mode session — disconnect it first.';
    case 'rig-not-configured':
      return 'No radio configured yet — set one up below, then Test CAT again.';
    case 'probe-timeout':
      return "Radio didn't respond — check the CAT cable and port.";
    default:
      return 'Radio check failed — try again.';
  }
}

/** `14.074000 MHz` — plain, testable MHz formatting for the Test-CAT success
 *  line ("✓ radio responds — dial reads … (<band>)", §FirstRun Step 2). */
function formatDialMHz(dialHz: number): string {
  return `${(dialHz / 1_000_000).toFixed(6)} MHz`;
}

// ---------------------------------------------------------------------------
// Per-row live meter — polls `ft8_device_meter` at ~2 Hz while `enabled`.
// Exposes `stopAndAwait`, the race-safety handover primitive (§FirstRun
// "Meter/start handover"): stops future polls immediately AND awaits any
// poll already in flight, so the row's device handle is guaranteed released
// before the caller proceeds to `ft8_set_device` / `ft8_listener_start`.
// ---------------------------------------------------------------------------

interface DeviceMeterState {
  meter: MeterDto | null;
  error: Ft8CmdError | null;
  stopAndAwait: () => Promise<void>;
}

function useDeviceMeterPoll(stableId: StableAudioId, enabled: boolean): DeviceMeterState {
  const [meter, setMeter] = useState<MeterDto | null>(null);
  const [error, setError] = useState<Ft8CmdError | null>(null);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const inFlightRef = useRef<Promise<void> | null>(null);
  const stoppedRef = useRef(false);
  const mountedRef = useRef(true);
  const idRef = useRef(stableId);
  idRef.current = stableId;

  const poll = useCallback(() => {
    if (stoppedRef.current) return;
    const id = idRef.current;
    const p = invoke<MeterDto>('ft8_device_meter', { stableId: id })
      .then((m) => {
        if (!mountedRef.current || stoppedRef.current) return;
        setMeter(m);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!mountedRef.current || stoppedRef.current) return;
        // ft8_device_meter's real error kinds: device-not-found |
        // device-reserved | internal-error (never device-in-use — a busy
        // device is the Ok state:'in-use' value, handled by the caller).
        setError(isFt8CmdError(e) ? e : { kind: 'internal-error', detail: cmdErrorMessage(e) });
      })
      .finally(() => {
        inFlightRef.current = null;
      });
    inFlightRef.current = p;
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    stoppedRef.current = false;
    if (!enabled) return undefined;

    poll(); // immediate first read, then ~2 Hz
    timerRef.current = setInterval(poll, METER_POLL_MS);

    return () => {
      mountedRef.current = false;
      stoppedRef.current = true;
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
    // `stableId` changes are handled via the `key`-remounted DeviceRow, not
    // an effect dependency here — idRef.current always tracks the latest.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, poll]);

  const stopAndAwait = useCallback(async () => {
    stoppedRef.current = true;
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (inFlightRef.current) {
      await inFlightRef.current;
    }
  }, []);

  return { meter, error, stopAndAwait };
}

// ---------------------------------------------------------------------------
// One device row: name · alsaHw · live meter · in-use badge · "use this
// device" action.
// ---------------------------------------------------------------------------

interface DeviceRowProps {
  device: AudioDeviceChoice;
  busy: boolean;
  onUse: (device: AudioDeviceChoice, stopAndAwait: () => Promise<void>) => void;
}

function MeterReadout({ meter, error }: { meter: MeterDto | null; error: Ft8CmdError | null }) {
  if (error) {
    // device-not-found / internal-error / device-reserved — all render a
    // muted, non-alarming line; polling continues (a reserved/transient
    // failure often clears on the next ~500ms tick). The WHY travels with it
    // (operator live-test 2026-07-12: a bare "meter unavailable" on every
    // device is undiagnosable in the field — rate mismatch, EBUSY from a
    // sound server, and a vanished device all looked identical).
    return (
      <span
        className="ft8-setup__meter ft8-setup__meter--error"
        data-testid="ft8-setup-meter-error"
        title={error.detail || undefined}
      >
        meter unavailable{error.detail ? ` — ${error.detail}` : ''}
      </span>
    );
  }
  if (!meter) {
    return <span className="ft8-setup__meter ft8-setup__meter--loading">reading level…</span>;
  }
  if (meter.state === 'error') {
    // The Ok-but-error MeterDto arm (open/negotiate/IO failure). Without this
    // arm it fell through to the level bar and rendered a bogus "-120 dBFS".
    return (
      <span
        className="ft8-setup__meter ft8-setup__meter--error"
        data-testid="ft8-setup-meter-error"
        title={meter.detail || undefined}
      >
        meter unavailable{meter.detail ? ` — ${meter.detail}` : ''}
      </span>
    );
  }
  if (meter.state === 'in-use') {
    // The unified signal for BOTH the live meter and the "used by ARDOP/
    // VARA" badge (§FirstRun): a busy device surfaces as the Ok
    // `MeterDto.state === 'in-use'` value, never a distinct error kind.
    return (
      <span className="ft8-setup__meter ft8-setup__meter--inuse" data-testid="ft8-setup-meter-inuse">
        in use by ARDOP/VARA
      </span>
    );
  }
  const pct = Math.max(0, Math.min(100, ((meter.rmsDbfs + 60) / 60) * 100));
  return (
    <span
      className={`ft8-setup__meter ft8-setup__meter--${meter.state}`}
      data-testid="ft8-setup-meter-live"
    >
      <span className="ft8-setup__meter-bar">
        <span className="ft8-setup__meter-fill" style={{ width: `${pct}%` }} />
      </span>
      <span className="ft8-setup__meter-db">{Number.isFinite(meter.rmsDbfs) ? `${meter.rmsDbfs.toFixed(0)} dBFS` : '—'}</span>
    </span>
  );
}

function DeviceRow({ device, busy, onUse }: DeviceRowProps) {
  const { meter, error, stopAndAwait } = useDeviceMeterPoll(device.stableId, true);
  const inUse = meter?.state === 'in-use';

  return (
    <div className="ft8-setup__device-row" data-testid={`ft8-setup-device-row-${stableIdKey(device.stableId)}`}>
      <div className="ft8-setup__device-info">
        <span className="ft8-setup__device-name">{device.humanName}</span>
        <span className="ft8-setup__device-hw">{device.alsaHw}</span>
      </div>
      <MeterReadout meter={meter} error={error} />
      <Button
        tone="primary"
        emphasis="outline"
        size="sm"
        data-testid={`ft8-setup-device-use-${stableIdKey(device.stableId)}`}
        disabled={busy || inUse}
        onClick={() => onUse(device, stopAndAwait)}
      >
        Use this device
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared "no devices — plug in + Refresh" arm body (§FirstRun: "Zero devices
// enumerated ⇒ plug-in guidance + Refresh (only then)").
// ---------------------------------------------------------------------------

function ZeroDevicesNotice({ onRefresh, loading }: { onRefresh: () => void; loading: boolean }) {
  return (
    <div className="ft8-setup__zero" data-testid="ft8-setup-arm-zero-devices">
      <p className="ft8-setup__body">
        No audio input devices found. Plug in your interface (DigiRig / DRA-100 / rig
        USB audio), then Refresh.
      </p>
      <Button tone="neutral" emphasis="outline" size="sm" data-testid="ft8-setup-refresh" onClick={onRefresh} disabled={loading}>
        {loading ? 'Refreshing…' : '↻ Refresh'}
      </Button>
    </div>
  );
}

function DeviceList({
  devices,
  busy,
  onUse,
}: {
  devices: AudioDeviceChoice[];
  busy: boolean;
  onUse: (device: AudioDeviceChoice, stopAndAwait: () => Promise<void>) => void;
}) {
  return (
    <div className="ft8-setup__device-list" data-testid="ft8-setup-device-list">
      {devices.map((d) => (
        <DeviceRow key={stableIdKey(d.stableId)} device={d} busy={busy} onUse={onUse} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Ft8SetupSurface
// ---------------------------------------------------------------------------

export function Ft8SetupSurface({ snapshot, onStarted, onRetry }: Ft8SetupSurfaceProps) {
  const reason: BlockedReasonDto | null =
    snapshot.service.axis === 'blocked' ? snapshot.service.reason : null;

  // The `unsupported-sample-rate` arm's device list is NOT on the snapshot
  // (L2 presence rule — §States) — it must be fetched via `ft8_list_devices`.
  // Every other arm's list has ALSO the option of a manual Refresh, which
  // re-fetches the same way and (once fetched) takes priority over the
  // snapshot's possibly-stale list.
  const [fetchedDevices, setFetchedDevices] = useState<AudioDeviceChoice[] | null>(null);
  const [fetching, setFetching] = useState(false);

  const loadDevices = useCallback(() => {
    setFetching(true);
    invoke<AudioDeviceChoice[]>('ft8_list_devices')
      .then((list) => setFetchedDevices(Array.isArray(list) ? list : []))
      .catch(() => setFetchedDevices([]))
      .finally(() => setFetching(false));
  }, []);

  const needsFetch = reason === 'unsupported-sample-rate';
  useEffect(() => {
    if (needsFetch) loadDevices();
    // Re-fetch whenever the arm switches into unsupported-sample-rate (a new
    // `reason` value); the snapshot-sourced arms rely on the snapshot itself
    // and only fetch on an explicit manual Refresh click.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needsFetch]);

  const devices: AudioDeviceChoice[] = fetchedDevices ?? snapshot.availableDevices ?? [];
  const enumerationSettled = fetchedDevices !== null || snapshot.availableDevices !== null;
  const zeroDevices = enumerationSettled && devices.length === 0;

  // ---- device-select → the race-safe handover (§FirstRun "Meter/start
  // handover"): stop this row's meter polling and await its in-flight read
  // BEFORE calling ft8_set_device/ft8_listener_start, so the row's device
  // handle is released before the listener tries to open it. -------------
  const [selecting, setSelecting] = useState(false);
  const [selectError, setSelectError] = useState<string | null>(null);

  const handleUse = useCallback(
    (device: AudioDeviceChoice, stopAndAwait: () => Promise<void>) => {
      setSelecting(true);
      setSelectError(null);
      void stopAndAwait() // stop metering + await the settle FIRST
        .then(() => invoke('ft8_set_device', { stableId: device.stableId }))
        .then(() => invoke('ft8_listener_start'))
        .then(() => {
          onStarted?.();
        })
        .catch((e: unknown) => {
          setSelectError(cmdErrorMessage(e));
        })
        .finally(() => {
          setSelecting(false);
        });
    },
    [onStarted],
  );

  // ---- Step 2 · Rig control (CAT) · Test CAT (§FirstRun Step 2) ---------
  // Edit-flush contract: RigControlSection persists CAT serial/baud/rigctld
  // binary on blur — commitNow() (via the forwarded ref) flushes whatever
  // the operator just typed but hasn't blurred yet, BEFORE the probe reads
  // Config.rig, so a just-typed-but-unblurred field never false-fails.
  const rigControlRef = useRef<RigControlSectionHandle>(null);
  const [catProbing, setCatProbing] = useState(false);
  const [catResult, setCatResult] = useState<CatProbeDto | null>(null);
  const [catError, setCatError] = useState<Ft8CmdError | null>(null);

  const handleTestCat = useCallback(() => {
    setCatProbing(true);
    setCatError(null);
    setCatResult(null);
    const flush = rigControlRef.current?.commitNow() ?? Promise.resolve();
    void flush
      .then(() => invoke<CatProbeDto>('ft8_cat_probe'))
      .then((res) => {
        setCatResult(res);
      })
      .catch((e: unknown) => {
        setCatError(isFt8CmdError(e) ? e : { kind: 'internal-error', detail: cmdErrorMessage(e) });
      })
      .finally(() => {
        setCatProbing(false);
      });
  }, []);

  // ---- CTA `Start listening on <band> →` (§FirstRun "CTA") --------------
  // Disabled-with-reason for EVERY blocker reachable while this surface is
  // mounted — never a silent re-render. Priority mirrors the arm order
  // above: a decoder that isn't installed or a device that can't be used at
  // all takes precedence over "no device chosen yet".
  const deviceResolved = snapshot.configuredDeviceName !== null;
  let ctaBlockReason: string | null = null;
  if (reason === 'wsjtx-absent') {
    ctaBlockReason = 'Install wsjt-x first.';
  } else if (reason === 'capture-wedged') {
    // Defensive: BlockedReasonDto includes this variant even though the
    // mounting contract (needs-setup only) doesn't currently route it here.
    ctaBlockReason = 'Restart Tuxlink.';
  } else if (reason === 'unsupported-sample-rate') {
    ctaBlockReason = 'Choose a supported audio input above.';
  } else if (!deviceResolved) {
    ctaBlockReason = 'Select an audio input above.';
  }

  const [ctaStarting, setCtaStarting] = useState(false);

  const handleStartCta = useCallback(() => {
    if (ctaBlockReason !== null) return; // never a silent no-op re-render
    setCtaStarting(true);
    setSelectError(null);
    void invoke('ft8_listener_start')
      .then(() => {
        onStarted?.();
      })
      .catch((e: unknown) => {
        setSelectError(cmdErrorMessage(e));
      })
      .finally(() => {
        setCtaStarting(false);
      });
  }, [ctaBlockReason, onStarted]);

  if (reason === null) {
    // Defensive: the caller should never mount this component outside a
    // needs-setup-class blocked reason. Render nothing rather than throw.
    return null;
  }

  return (
    <div className="ft8-setup" data-testid="ft8-setup-surface">
      <div className="ft8-setup__step-head">
        <h3 className="ft8-setup__step-title">Step 1 · Audio input</h3>
        <span className="ft8-setup__step-badge ft8-setup__step-badge--required">REQUIRED</span>
      </div>

      {selectError && (
        <p className="ft8-setup__select-error" data-testid="ft8-setup-select-error" role="alert">
          {selectError}
        </p>
      )}

      {reason === 'wsjtx-absent' ? (
        <div data-testid="ft8-setup-arm-wsjtx-absent">
          <p className="ft8-setup__body">
            FT-8 decoding needs the <strong>wsjt-x</strong> package — install the
            WSJT-X package via apt/your package manager (it provides the jt9
            decoder), then Retry.
          </p>
          <Button tone="neutral" emphasis="outline" size="sm" data-testid="ft8-setup-retry" onClick={() => onRetry?.()}>
            Retry
          </Button>
          {snapshot.configuredDeviceName !== null ? (
            <p className="ft8-setup__using" data-testid="ft8-setup-using-configured">
              Using <strong>{snapshot.configuredDeviceName}</strong> for audio input.
            </p>
          ) : zeroDevices ? (
            <ZeroDevicesNotice onRefresh={loadDevices} loading={fetching} />
          ) : devices.length > 0 ? (
            <DeviceList devices={devices} busy={selecting} onUse={handleUse} />
          ) : null}
        </div>
      ) : reason === 'unsupported-sample-rate' ? (
        <div data-testid="ft8-setup-arm-unsupported-sample-rate">
          <p className="ft8-setup__body">
            This input can&apos;t capture 48 kHz — choose a different card.
          </p>
          {/* §States: "Never render plug-in-a-device guidance here" — even an
              empty fetch result renders the plain device list (empty), not
              the zero-devices plug-in copy. */}
          <DeviceList devices={devices} busy={selecting} onUse={handleUse} />
        </div>
      ) : zeroDevices ? (
        // needs-device-selection / device-absent, enumeration completed empty.
        <ZeroDevicesNotice onRefresh={loadDevices} loading={fetching} />
      ) : (
        // needs-device-selection / device-absent, devices available (or still
        // loading — `devices` is [] pre-hydrate, which renders an empty list
        // rather than a false zero-devices claim per `enumerationSettled`).
        <div data-testid="ft8-setup-arm-device-selection">
          <p className="ft8-setup__body">Choose the audio input FT-8 should listen on.</p>
          <DeviceList devices={devices} busy={selecting} onUse={handleUse} />
        </div>
      )}

      {/* Step 2 · Rig control (CAT) · OPTIONAL·RECOMMENDED (§FirstRun). The
          SAME shared RigControlSection as the ARDOP/VARA panels — one radio,
          one config, set here and everywhere. */}
      <div className="ft8-setup__step-head" data-testid="ft8-setup-step2-head">
        <h3 className="ft8-setup__step-title">Step 2 · Rig control (CAT)</h3>
        <span className="ft8-setup__step-badge" data-testid="ft8-setup-step2-badge">
          OPTIONAL · RECOMMENDED
        </span>
      </div>
      <p className="ft8-setup__body">
        One radio, one config — set them here and they&apos;re set everywhere.
      </p>
      <RigControlSection storageKeyPrefix="ft8" ref={rigControlRef} />
      <div className="ft8-setup__cat-test" data-testid="ft8-setup-cat-test">
        <Button
          tone="neutral"
          emphasis="outline"
          size="sm"
          data-testid="ft8-setup-test-cat"
          disabled={catProbing}
          onClick={handleTestCat}
        >
          {catProbing ? 'Testing…' : 'Test CAT'}
        </Button>
        {catResult && (
          <p className="ft8-setup__using" data-testid="ft8-setup-cat-success">
            ✓ radio responds — dial reads {formatDialMHz(catResult.dialHz)} ({catResult.band})
          </p>
        )}
        {catError && (
          <p className="ft8-setup__select-error" data-testid="ft8-setup-cat-error" role="alert">
            {catProbeErrorCopy(catError)}
          </p>
        )}
      </div>

      {/* CTA `Start listening on <band> →` (§FirstRun "CTA") — disabled with
          a visible reason for every blocker; a click while blocked is a
          guarded no-op, never a silent re-render. */}
      <div className="ft8-setup__cta-row" data-testid="ft8-setup-cta-row">
        <Button
          tone="primary"
          emphasis="solid"
          data-testid="ft8-setup-start-cta"
          disabled={ctaBlockReason !== null || ctaStarting}
          onClick={handleStartCta}
        >
          {ctaStarting ? 'Starting…' : `Start listening on ${snapshot.band} →`}
        </Button>
        <p className="ft8-setup__body" data-testid="ft8-setup-cta-caption">
          starts the decoder on the selected card · nothing ever transmits
        </p>
        {ctaBlockReason !== null && (
          <p
            className="ft8-setup__select-error"
            data-testid="ft8-setup-cta-blocked-reason"
            role="status"
          >
            {ctaBlockReason}
          </p>
        )}
      </div>
    </div>
  );
}
