// src/ft8ui/Ft8StripSetup.tsx
//
// Compact in-strip FT-8 setup: one dropdown + live meter + Start, replacing
// the row-per-device full-panel setup surface with an OS-convention single
// <select>. Task 1 of the Station Intelligence operational-usability series
// (mounted in place of the full-panel takeover by Task 2; the old surface is
// deleted by Task 3).
//
// The blocked arms (zero devices / wsjtx-absent / unsupported-sample-rate)
// render compact single-line notices, reusing the same data-testids the old
// surface used so existing operator muscle-memory / any future scripted QA
// keeps working.
//
// RigControlSection mechanics (import path, ref/commitNow(), the Test CAT
// button + ft8_cat_probe) are mirrored exactly from the old full-panel
// surface's Step 2: that surface was the authoritative source for how the
// shared rig-control component is wired, not a fresh invention here.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../controls';
import { RigControlSection } from '../radio/modes/RigControlSection';
import type { RigControlSectionHandle } from '../radio/modes/RigControlSection';
import { useDeviceMeterPoll } from './useDeviceMeterPoll';
import type { AudioDeviceChoice, CatProbeDto, Ft8CmdError, Ft8Snapshot } from './ft8Types';
import './Ft8StripSetup.css';

export interface Ft8StripSetupProps {
  /** The current listener snapshot. */
  snapshot: Ft8Snapshot;
  /** Fired after the Start action's `ft8_listener_start` succeeds. */
  onStarted?: () => void;
  /** Manual "Retry" affordance for the wsjtx-absent arm. */
  onRetry?: () => void;
}

function isFt8CmdError(e: unknown): e is Ft8CmdError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'detail' in e;
}

function cmdErrorMessage(e: unknown): string {
  if (isFt8CmdError(e)) return e.detail;
  if (e instanceof Error) return e.message;
  return 'Something went wrong, try again.';
}

/** Test-CAT failure copy, keyed on `Ft8CmdError.kind`, mirrors the old
 *  full-panel surface's `catProbeErrorCopy`. `ft8_cat_probe`'s real kinds:
 *  modem-busy | rig-not-configured | probe-timeout | internal-error. */
function catProbeErrorCopy(e: Ft8CmdError): string {
  switch (e.kind) {
    case 'modem-busy':
      return 'Radio busy with another mode session, disconnect it first.';
    case 'rig-not-configured':
      return 'No radio configured yet, set one up below, then Test CAT again.';
    case 'probe-timeout':
      return "Radio didn't respond, check the CAT cable and port.";
    default:
      return 'Radio check failed, try again.';
  }
}

/** `14.074000 MHz`: mirrors the old full-panel surface's `formatDialMHz`. */
function formatDialMHz(dialHz: number): string {
  return `${(dialHz / 1_000_000).toFixed(6)} MHz`;
}

function MeterBar({
  rmsDbfs,
  state,
  error,
}: {
  rmsDbfs: number | null;
  state: string | null;
  error: Ft8CmdError | null;
}) {
  if (error) {
    return (
      <span className="ft8ss__meter ft8ss__meter--error" data-testid="ft8-setup-meter-error">
        meter unavailable{error.detail ? `: ${error.detail}` : `: ${error.kind}`}
      </span>
    );
  }
  if (state === 'in-use') {
    return (
      <span className="ft8ss__meter ft8ss__meter--inuse" data-testid="ft8-setup-meter-inuse">
        in use by another app
      </span>
    );
  }
  if (rmsDbfs == null) {
    return <span className="ft8ss__meter ft8ss__meter--loading">reading level…</span>;
  }
  const pct = Math.max(0, Math.min(100, ((rmsDbfs + 60) / 60) * 100));
  return (
    <span className="ft8ss__meter" data-testid="ft8-setup-meter-live">
      <span className="ft8ss__meter-bar">
        <span className="ft8ss__meter-fill" style={{ width: `${pct}%` }} />
      </span>
      <span className="ft8ss__meter-db">{Number.isFinite(rmsDbfs) ? `${rmsDbfs.toFixed(0)} dBFS` : '-'}</span>
    </span>
  );
}

export function Ft8StripSetup({ snapshot, onStarted, onRetry }: Ft8StripSetupProps) {
  const [devices, setDevices] = useState<AudioDeviceChoice[] | null>(snapshot.availableDevices);
  const [busy, setBusy] = useState(false);
  const [catOpen, setCatOpen] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [loadingDevices, setLoadingDevices] = useState(false);

  const loadDevices = useCallback(async () => {
    setLoadingDevices(true);
    try {
      setDevices(await invoke<AudioDeviceChoice[]>('ft8_list_devices'));
    } catch {
      // keep the last list; the meter arm surfaces device-level errors
    } finally {
      setLoadingDevices(false);
    }
  }, []);
  useEffect(() => {
    void loadDevices();
    // Fetch once on mount; a manual Refresh click re-fetches explicitly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selected = useMemo(
    () => (devices ?? []).find((d) => d.humanName === snapshot.configuredDeviceName) ?? null,
    [devices, snapshot.configuredDeviceName],
  );
  // Meter follows the persisted pick; paused while a handover is in flight.
  const { meter, error: meterError, stopAndAwait } = useDeviceMeterPoll(
    selected?.stableId ?? { kind: 'cardIdHash', value: '' },
    !busy && selected != null,
  );

  const pick = useCallback(
    async (humanName: string) => {
      const device = (devices ?? []).find((d) => d.humanName === humanName);
      if (!device) return;
      setBusy(true);
      setStartError(null);
      try {
        await stopAndAwait(); // release the meter handle first
        await invoke('ft8_set_device', { stableId: device.stableId });
      } catch (e) {
        setStartError(cmdErrorMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [devices, stopAndAwait],
  );

  const start = useCallback(async () => {
    setBusy(true);
    setStartError(null);
    try {
      await invoke('ft8_listener_start');
      onStarted?.();
    } catch (e) {
      setStartError(cmdErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }, [onStarted]);

  // ---- Rig control (CAT) · Test CAT, mirrors the old full-panel surface's
  // Step 2 edit-flush contract exactly: commitNow() (forwarded ref) flushes any
  // just-typed-but-unblurred CAT field BEFORE ft8_cat_probe reads
  // Config.rig, so a value the operator just typed never false-fails. -----
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

  const reason = snapshot.service.axis === 'blocked' ? snapshot.service.reason : null;

  if (reason === 'wsjtx-absent') {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-wsjtx-absent">
        <span>FT-8 decoder missing: install the wsjt-x package, then</span>
        <Button
          tone="neutral"
          emphasis="outline"
          size="sm"
          data-testid="ft8-setup-retry"
          onClick={() => onRetry?.()}
        >
          Retry
        </Button>
      </div>
    );
  }

  if (reason === 'unsupported-sample-rate') {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-bad-rate">
        This input cannot capture at 12 kHz: pick a different device below.
      </div>
    );
  }

  if ((devices ?? []).length === 0) {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-zero-devices">
        <span>No capture-capable soundcard found: plug in the rig interface, then</span>
        <Button
          tone="neutral"
          emphasis="outline"
          size="sm"
          data-testid="ft8-setup-refresh"
          onClick={() => void loadDevices()}
          disabled={loadingDevices}
        >
          {loadingDevices ? 'Refreshing…' : 'Refresh'}
        </Button>
      </div>
    );
  }

  return (
    <div className="ft8ss" data-testid="ft8-strip-setup">
      <span className="ft8ss__label">audio input</span>
      <select
        className="ft8ss__select"
        data-testid="ft8-setup-device-select"
        value={selected?.humanName ?? ''}
        disabled={busy}
        onChange={(e) => void pick(e.target.value)}
      >
        {selected == null && (
          <option value="" disabled>
            Choose input…
          </option>
        )}
        {(devices ?? []).map((d) => (
          <option key={d.humanName} value={d.humanName}>
            {d.humanName}
          </option>
        ))}
      </select>
      <MeterBar rmsDbfs={meter?.rmsDbfs ?? null} state={meter?.state ?? null} error={meterError} />
      <Button
        tone="primary"
        emphasis="solid"
        size="sm"
        data-testid="ft8-setup-start"
        disabled={busy || selected == null}
        onClick={() => void start()}
      >
        {busy ? 'Starting…' : `Start listening on ${snapshot.band}`}
      </Button>
      <button
        type="button"
        className="ft8ss__cat-toggle"
        data-testid="ft8-setup-cat-toggle"
        onClick={() => setCatOpen((v) => !v)}
      >
        rig control (CAT) · optional {catOpen ? '⌃' : '⌄'}
      </button>
      {startError && (
        <span className="ft8ss__err" data-testid="ft8-setup-start-error" role="alert">
          {startError}
        </span>
      )}
      {catOpen && (
        <div className="ft8ss__cat" data-testid="ft8-setup-cat">
          <RigControlSection storageKeyPrefix="ft8" ref={rigControlRef} />
          <div className="ft8ss__cat-test">
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
              <span className="ft8ss__cat-ok" data-testid="ft8-setup-cat-success">
                radio responds, dial reads {formatDialMHz(catResult.dialHz)} ({catResult.band})
              </span>
            )}
            {catError && (
              <span className="ft8ss__err" data-testid="ft8-setup-cat-error" role="alert">
                {catProbeErrorCopy(catError)}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
