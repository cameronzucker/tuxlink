// GpsSourcePicker — the shared GPS setup-assistance component, rendered in BOTH
// the Settings → Location panel and the first-run wizard's Location step
// (tuxlink-9xy1 slice 1). It detects working GPS sources, diagnoses the common
// Linux blockers (dialout group, ModemManager) with copy-pasteable fix commands,
// and always offers manual grid entry as a first-class fallback.
//
// The component is presentational + self-detecting: it runs the probes itself and
// renders cards, but the *selection* (which source / what grid) is controlled by
// the parent so each chrome persists it its own way (wizard_persist_gps vs the
// Settings config writers).

import { useCallback, useEffect, useState } from 'react';
import { validateGrid } from '../wizard/validators';
import {
  runGpsDetection,
  classifyGpsSources,
  pkexecAvailable,
  runGpsFix,
  triageFixAction,
  type GpsClassification,
  type GpsFixOutcome,
} from './gpsProbes';
import { LocationMap } from './LocationMap';
import './GpsSourcePicker.css';

export interface GpsSourcePickerProps {
  /** Persisted manual grid (controlled by the parent chrome). */
  grid: string;
  onGridChange: (grid: string) => void;
  /** Selected source id: `'manual'` | `'gpsd'` | `'serial:/dev/ttyACM0'`. */
  selectedSource: string;
  onSelectSource: (id: string) => void;
  // Live arbiter status (tuxlink-yy1m) — supplied by each chrome from
  // useLocationConfig; drives the confirm map pin + the acquiring/fixed readout.
  /** A fresh GPS fix exists and GPS is on. */
  gpsReady: boolean;
  /** Raw live-fix coords for the precise map pin, or null when no fresh fix. */
  fixLatLon: { lat: number; lon: number } | null;
  /** Effective local-display grid from the arbiter (live fix when source=Gps). */
  uiGrid: string;
}

type Status = 'loading' | 'ready' | 'error';

export function GpsSourcePicker({
  grid,
  onGridChange,
  selectedSource,
  onSelectSource,
  gpsReady,
  fixLatLon,
  uiGrid,
}: GpsSourcePickerProps) {
  const [status, setStatus] = useState<Status>('loading');
  const [classification, setClassification] = useState<GpsClassification>({
    sources: [],
    triage: [],
    noDevice: false,
  });
  const [openCommand, setOpenCommand] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  // "Fix it for me" (tuxlink-m9ej): pkexec availability gates the buttons; the
  // last fix result drives per-card feedback (e.g. the dialout re-login notice).
  const [pkexec, setPkexec] = useState(false);
  const [fixResult, setFixResult] = useState<{ kind: string; outcome: GpsFixOutcome } | null>(null);
  const [fixing, setFixing] = useState<string | null>(null);

  const rescan = useCallback(() => {
    let cancelled = false;
    setStatus('loading');
    runGpsDetection()
      .then((d) => {
        if (cancelled) return;
        setClassification(classifyGpsSources(d));
        setStatus('ready');
      })
      .catch(() => {
        if (!cancelled) setStatus('error');
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => rescan(), [rescan]);

  // Probe pkexec once so the fix buttons only enable where a system auth dialog
  // is possible (AppImage / minimal installs degrade to "Show command").
  useEffect(() => {
    let mounted = true;
    pkexecAvailable()
      .then((ok) => mounted && setPkexec(ok))
      .catch(() => mounted && setPkexec(false));
    return () => {
      mounted = false;
    };
  }, []);

  const runFix = async (kind: 'dialout' | 'modemmanager') => {
    setFixing(kind);
    setFixResult(null);
    try {
      const outcome = await runGpsFix(triageFixAction(kind));
      setFixResult({ kind, outcome });
      // Re-scan only for ModemManager: masking takes effect immediately, so the
      // card should disappear. dialout does NOT take effect until re-login, so a
      // rescan would re-render the still-"blocked" card under the success notice
      // (adrev P2) — leave it; the re-login notice tells the operator what's next.
      if (outcome === 'ok' && kind === 'modemmanager') rescan();
    } catch {
      setFixResult({ kind, outcome: 'failed' });
    } finally {
      setFixing(null);
    }
  };

  const gridError = validateGrid(grid);

  const copy = async (text: string, key: string) => {
    try {
      await navigator.clipboard.writeText(text.split('#')[0].trim());
      setCopied(key);
    } catch {
      // Clipboard unavailable (or denied) — the command is still visible to copy by hand.
      setCopied(null);
    }
  };

  return (
    <div className="gps-picker" data-testid="gps-picker">
      {/* Confirmation surface (tuxlink-yy1m): the offline map shows where Tuxlink
          thinks you are. Prefer the live arbiter grid (uiGrid) so a GPS fix is
          framed; fall back to the manual grid. Click/drag sets it by hand. The
          map + controls are separate containers so the wizard chrome can place
          them side-by-side (full-screen) while Settings stacks them. */}
      <div className="gps-picker__map">
        <LocationMap
          grid={uiGrid || grid}
          fixLatLon={fixLatLon}
          selectedSource={selectedSource}
          onGridChange={onGridChange}
        />
      </div>

      <div className="gps-picker__controls">
      {/* Live GPS readout — only meaningful when a GPS source is selected. */}
      {selectedSource !== 'manual' &&
        (gpsReady ? (
          <div className="gps-readout gps-readout--ok" data-testid="gps-readout-fixed" role="status">
            <span className="gps-readout__grid">{uiGrid || grid || '—'}</span>
            <span className="gps-readout__sub">GPS fix acquired</span>
          </div>
        ) : (
          <div className="gps-readout gps-readout--acq" data-testid="gps-readout-acquiring" role="status">
            Acquiring GPS fix…
          </div>
        ))}

      <div className="gps-picker__head">
        <span className="gps-picker__title">GPS source</span>
        <button
          type="button"
          className="gps-picker__rescan"
          data-testid="gps-picker-rescan"
          onClick={rescan}
          disabled={status === 'loading'}
        >
          {status === 'loading' ? 'Scanning…' : 'Rescan'}
        </button>
      </div>

      {status === 'error' && (
        <p className="gps-picker__error" role="alert" data-testid="gps-picker-error">
          Could not scan for GPS sources. You can still enter your grid manually below.
        </p>
      )}

      {/* Working sources */}
      {classification.sources.map((s) => {
        const selected = selectedSource === s.id;
        return (
          <div key={s.id} className={`gps-card gps-card--source${selected ? ' is-selected' : ''}`} data-testid={`gps-source-${s.id}`}>
            <div className="gps-card__body">
              <span className="gps-card__label">{s.label}</span>
              <span className="gps-card__detail">{s.detail}</span>
            </div>
            <button
              type="button"
              className="gps-card__use"
              data-testid={`gps-use-${s.id}`}
              aria-pressed={selected}
              onClick={() => onSelectSource(s.id)}
            >
              {selected ? 'In use' : 'Use this'}
            </button>
          </div>
        );
      })}

      {/* Blocked sources — triage with a fix command */}
      {classification.triage.map((t) => (
        <div key={t.kind} className="gps-card gps-card--triage" data-testid={`gps-triage-${t.kind}`}>
          <div className="gps-card__body">
            <span className="gps-card__label gps-card__label--warn">{t.title}</span>
            <span className="gps-card__detail">{t.problem}</span>
            {openCommand === t.kind && (
              <div className="gps-card__cmd">
                <code data-testid={`gps-command-${t.kind}`}>{t.command}</code>
                <button type="button" className="gps-card__copy" data-testid={`gps-copy-${t.kind}`} onClick={() => copy(t.command, t.kind)}>
                  {copied === t.kind ? 'Copied' : 'Copy'}
                </button>
              </div>
            )}
          </div>
          <div className="gps-card__actions">
            <button
              type="button"
              className="gps-card__show"
              data-testid={`gps-show-command-${t.kind}`}
              aria-expanded={openCommand === t.kind}
              onClick={() => setOpenCommand((c) => (c === t.kind ? null : t.kind))}
            >
              {openCommand === t.kind ? 'Hide command' : 'Show command'}
            </button>
            {/* tuxlink-m9ej: one-click fix via the pkexec helper. Enabled only
                where it's fixable AND pkexec exists; otherwise the operator uses
                "Show command" (AppImage / minimal installs). */}
            <button
              type="button"
              className="gps-card__fix"
              data-testid={`gps-fix-${t.kind}`}
              disabled={!pkexec || !t.fixable || fixing === t.kind}
              title={
                !pkexec
                  ? 'PolicyKit (pkexec) not available — use Show command'
                  : !t.fixable
                    ? 'Automatic fix unavailable — use Show command'
                    : undefined
              }
              onClick={() => runFix(t.kind)}
            >
              {fixing === t.kind ? 'Working…' : 'Fix it for me'}
            </button>
          </div>
          {fixResult?.kind === t.kind && (
            <div className="gps-card__fix-result" data-testid={`gps-fix-result-${t.kind}`} role="status">
              {fixResult.outcome === 'ok' && t.kind === 'dialout' && (
                <span data-testid="gps-relogin-notice">
                  Done. Log out and back in for this to take effect — that's a Linux rule we can't bypass.
                </span>
              )}
              {fixResult.outcome === 'ok' && t.kind === 'modemmanager' && (
                <span>Done — ModemManager masked. Plug in your GPS and Rescan.</span>
              )}
              {fixResult.outcome === 'auth_dismissed' && <span>Cancelled — no changes made.</span>}
              {fixResult.outcome === 'failed' && (
                <span className="gps-card__detail--warn">Couldn't apply the fix. Use “Show command” to run it by hand.</span>
              )}
              {fixResult.outcome === 'pkexec_missing' && (
                <span className="gps-card__detail--warn">PolicyKit unavailable. Use “Show command”.</span>
              )}
            </div>
          )}
        </div>
      ))}

      {/* No receiver detected yet (tuxlink-yy1m) — device-independent diagnostics
          above still render; this tells the operator to plug in + rescan. */}
      {classification.noDevice && (
        <div className="gps-card gps-card--nodevice" data-testid="gps-no-device">
          <div className="gps-card__body">
            <span className="gps-card__label">No GPS receiver detected yet</span>
            <span className="gps-card__detail">
              Plug in your USB or serial GPS, then press Rescan. A phone sharing its
              location over gpsd works too.
            </span>
          </div>
        </div>
      )}

      {/* Manual grid — always first-class (Mike's "I'll just type my grid" path) */}
      <div className={`gps-card gps-card--manual${selectedSource === 'manual' ? ' is-selected' : ''}`} data-testid="gps-source-manual">
        <div className="gps-card__body">
          <label className="gps-card__label" htmlFor="gps-manual-grid">Enter your grid manually</label>
          <input
            id="gps-manual-grid"
            className="gps-card__grid-input"
            data-testid="gps-manual-grid-input"
            type="text"
            inputMode="text"
            autoCapitalize="characters"
            placeholder="EM75 or EM75xx"
            value={grid}
            onChange={(e) => onGridChange(e.target.value.trim())}
            aria-invalid={gridError != null}
          />
          {gridError && (
            <span className="gps-card__detail gps-card__detail--warn" role="alert" data-testid="gps-grid-error">
              {gridError}
            </span>
          )}
          <span className="gps-card__detail">
            …or click the map above to drop your location, and drag the pin to fine-tune.
          </span>
          <span className="gps-card__detail">
            Broadcast precision is reduced to a 4-character grid by default; set finer precision under Privacy.
          </span>
        </div>
        <button
          type="button"
          className="gps-card__use"
          data-testid="gps-use-manual"
          aria-pressed={selectedSource === 'manual'}
          onClick={() => onSelectSource('manual')}
        >
          {selectedSource === 'manual' ? 'In use' : 'Use manual'}
        </button>
      </div>
      </div>
    </div>
  );
}
