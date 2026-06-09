// GribRequestPanel — inline overlay for Saildocs GRIB requests (tuxlink-vrpk).
//
// Opens from Message → GRIB File Request. Operator fills in region +
// grid + forecast times + parameters; on Send, composes a message to
// query@saildocs.com with the Saildocs body syntax and queues it in the
// outbox. Response arrives as a regular Private message with a GRIB-1
// binary attachment that the operator saves locally and opens in an
// external viewer (zyGrib / OpenCPN / Expedition). WLE parity — no
// in-app GRIB rendering.

import { useState } from 'react';
import {
  ALL_GRIB_PARAMETERS,
  DEFAULT_GRIB_REQUEST,
  parseForecastTimes,
  type GribDirection,
  type GribMode,
  type GribParameter,
  type GribRequest,
} from './types';
import { sendGribRequest } from './useGrib';
import { GridMapPicker } from '../map/GridMapPicker';
import { signedBboxToGribRegion } from '../map/gribRegion';
import './GribRequestPanel.css';

export interface GribRequestPanelProps {
  onClose: () => void;
}

type SendState =
  | { kind: 'idle' }
  | { kind: 'sending' }
  | { kind: 'success'; mid: string }
  | { kind: 'error'; message: string };

const PARAM_LABELS: Record<GribParameter, string> = {
  PRMSL: 'PRMSL (pressure)',
  WIND: 'WIND',
  HGT: 'HGT (geopotential height)',
  SEATMP: 'SEATMP (sea-surface temp)',
  AIRTMP: 'AIRTMP (air temp)',
  WAVES: 'WAVES',
};

export function GribRequestPanel({ onClose }: GribRequestPanelProps) {
  const [request, setRequest] = useState<GribRequest>(DEFAULT_GRIB_REQUEST);
  const [timesText, setTimesText] = useState(''); // empty → Saildocs default applies
  const [timesError, setTimesError] = useState<string | null>(null);
  const [sendState, setSendState] = useState<SendState>({ kind: 'idle' });

  const setLat = (which: 'lat0' | 'lat1', degrees: number, dir: GribDirection) =>
    setRequest((r) => ({ ...r, [which]: { degrees, dir } }));
  const setLon = (which: 'lon0' | 'lon1', degrees: number, dir: GribDirection) =>
    setRequest((r) => ({ ...r, [which]: { degrees, dir } }));

  const toggleParam = (p: GribParameter) =>
    setRequest((r) => ({
      ...r,
      params: r.params.includes(p) ? r.params.filter((x) => x !== p) : [...r.params, p],
    }));

  const setTimes = (raw: string) => {
    setTimesText(raw);
    const parsed = parseForecastTimes(raw);
    if (parsed.ok) {
      setTimesError(null);
      setRequest((r) => ({ ...r, times: parsed.value }));
    } else {
      setTimesError(parsed.error);
    }
  };

  const onSend = async () => {
    if (timesError) return;
    if (!request.subject.trim()) return;
    setSendState({ kind: 'sending' });
    try {
      const mid = await sendGribRequest(request);
      setSendState({ kind: 'success', mid });
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setSendState({ kind: 'error', message });
    }
  };

  return (
    <div className="grib-overlay" data-testid="grib-overlay" onClick={onClose}>
      <div className="grib-panel" data-testid="grib-panel" onClick={(e) => e.stopPropagation()}>
        <header className="grib-header">
          <div className="grib-title">
            <h2>GRIB File Request</h2>
            <p className="grib-subtitle">
              Request a weather GRIB file from Saildocs (3rd-party service). The response will
              arrive in your inbox as a Private message with a GRIB-1 attachment — save it and
              open in zyGrib, OpenCPN, or another GRIB viewer.
            </p>
          </div>
          <button
            type="button"
            className="grib-close"
            data-testid="grib-close"
            onClick={onClose}
            aria-label="Close GRIB request panel"
            title="Close"
          >
            ✕
          </button>
        </header>

        <div className="grib-body">
          <section className="grib-section">
            <h3>Region</h3>
            <div className="grib-region-map" data-testid="grib-region-map" style={{ height: 260 }}>
              <GridMapPicker
                mode="box"
                onBoxChange={(a, b) => {
                  const r = signedBboxToGribRegion(a, b);
                  setLat('lat0', r.lat0.degrees, r.lat0.dir);
                  setLat('lat1', r.lat1.degrees, r.lat1.dir);
                  setLon('lon0', r.lon0.degrees, r.lon0.dir);
                  setLon('lon1', r.lon1.degrees, r.lon1.dir);
                }}
              />
            </div>
            <p className="grib-hint">
              Drag a box on the map to set the region, or edit the fields below.
            </p>
            <div className="grib-region-row">
              <LatField label="Lat 0" value={request.lat0} onChange={(d, dir) => setLat('lat0', d, dir)} testId="grib-lat0" />
              <LatField label="Lat 1" value={request.lat1} onChange={(d, dir) => setLat('lat1', d, dir)} testId="grib-lat1" />
              <LonField label="Lon 0" value={request.lon0} onChange={(d, dir) => setLon('lon0', d, dir)} testId="grib-lon0" />
              <LonField label="Lon 1" value={request.lon1} onChange={(d, dir) => setLon('lon1', d, dir)} testId="grib-lon1" />
            </div>
          </section>

          <section className="grib-section">
            <h3>Grid spacing (degrees)</h3>
            <div className="grib-grid-row">
              <label className="grib-field">
                <span>dlat</span>
                <input
                  type="number"
                  min={1}
                  max={10}
                  step={1}
                  data-testid="grib-dlat"
                  value={request.grid[0]}
                  onChange={(e) =>
                    setRequest((r) => ({ ...r, grid: [clampPositive(e.target.value, r.grid[0]), r.grid[1]] }))
                  }
                />
              </label>
              <label className="grib-field">
                <span>dlon</span>
                <input
                  type="number"
                  min={1}
                  max={10}
                  step={1}
                  data-testid="grib-dlon"
                  value={request.grid[1]}
                  onChange={(e) =>
                    setRequest((r) => ({ ...r, grid: [r.grid[0], clampPositive(e.target.value, r.grid[1])] }))
                  }
                />
              </label>
              <span className="grib-hint">Saildocs default: 2,2. Smaller = more data.</span>
            </div>
          </section>

          <section className="grib-section">
            <h3>Forecast times (hours)</h3>
            <input
              type="text"
              className="grib-times-input"
              data-testid="grib-times"
              placeholder="e.g. 24,48,72 or 6,12..96 (empty = Saildocs default)"
              value={timesText}
              onChange={(e) => setTimes(e.target.value)}
            />
            {timesError && (
              <div className="grib-times-error" data-testid="grib-times-error">
                {timesError}
              </div>
            )}
          </section>

          <section className="grib-section">
            <h3>Parameters</h3>
            <div className="grib-params-row">
              {ALL_GRIB_PARAMETERS.map((p) => (
                <label key={p} className="grib-param-label">
                  <input
                    type="checkbox"
                    data-testid={`grib-param-${p}`}
                    checked={request.params.includes(p)}
                    onChange={() => toggleParam(p)}
                  />
                  {PARAM_LABELS[p]}
                </label>
              ))}
            </div>
            <div className="grib-hint">Empty = Saildocs default (PRESS, WIND).</div>
          </section>

          <section className="grib-section">
            <h3>Mode</h3>
            <div className="grib-mode-row">
              <label className="grib-mode-option">
                <input
                  type="radio"
                  name="grib-mode"
                  data-testid="grib-mode-send"
                  checked={request.mode === 'send'}
                  onChange={() => setRequest((r) => ({ ...r, mode: 'send' as GribMode }))}
                />
                Send (one-shot)
              </label>
              <label className="grib-mode-option">
                <input
                  type="radio"
                  name="grib-mode"
                  data-testid="grib-mode-sub"
                  checked={request.mode === 'sub'}
                  onChange={() => setRequest((r) => ({ ...r, mode: 'sub' as GribMode }))}
                />
                Subscribe (recurring)
              </label>
            </div>
            {request.mode === 'sub' && (
              <div className="grib-sub-row">
                <label className="grib-field">
                  <span>Days</span>
                  <input
                    type="number"
                    min={1}
                    max={365}
                    step={1}
                    data-testid="grib-sub-days"
                    value={request.sub_days ?? ''}
                    onChange={(e) =>
                      setRequest((r) => ({
                        ...r,
                        sub_days: e.target.value === '' ? null : Math.max(1, Math.floor(Number(e.target.value))),
                      }))
                    }
                    placeholder="optional"
                  />
                </label>
                <label className="grib-field">
                  <span>Time (HH:MM UTC)</span>
                  <input
                    type="text"
                    data-testid="grib-sub-time"
                    value={request.sub_time ?? ''}
                    onChange={(e) =>
                      setRequest((r) => ({ ...r, sub_time: e.target.value === '' ? null : e.target.value }))
                    }
                    placeholder="optional, e.g. 18:00"
                  />
                </label>
              </div>
            )}
          </section>

          <section className="grib-section">
            <h3>Subject</h3>
            <input
              type="text"
              className="grib-subject-input"
              data-testid="grib-subject"
              value={request.subject}
              onChange={(e) => setRequest((r) => ({ ...r, subject: e.target.value }))}
            />
            <div className="grib-hint">Saildocs ignores this — for your outbox/sent listing only.</div>
          </section>
        </div>

        <footer className="grib-footer">
          <div className="grib-actions">
            <button
              type="button"
              className="grib-cancel"
              data-testid="grib-cancel"
              onClick={onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="grib-send"
              data-testid="grib-send"
              onClick={onSend}
              disabled={
                sendState.kind === 'sending' || timesError !== null || !request.subject.trim()
              }
            >
              {sendState.kind === 'sending' ? 'Sending…' : 'Send Request'}
            </button>
          </div>
          {sendState.kind === 'success' && (
            <div className="grib-status grib-status-success" data-testid="grib-send-success">
              Queued GRIB request to outbox (MID {sendState.mid}). Connect to the CMS to send.
            </div>
          )}
          {sendState.kind === 'error' && (
            <div className="grib-status grib-status-error" data-testid="grib-send-error">
              Send failed: {sendState.message}
            </div>
          )}
        </footer>
      </div>
    </div>
  );
}

interface LatFieldProps {
  label: string;
  value: { degrees: number; dir: GribDirection };
  onChange: (degrees: number, dir: GribDirection) => void;
  testId: string;
}

function LatField({ label, value, onChange, testId }: LatFieldProps) {
  return (
    <label className="grib-field">
      <span>{label}</span>
      <div className="grib-field-row">
        <input
          type="number"
          min={0}
          max={90}
          step={1}
          data-testid={`${testId}-deg`}
          value={value.degrees}
          onChange={(e) => onChange(clampDegrees(e.target.value, 90, value.degrees), value.dir)}
        />
        <select
          data-testid={`${testId}-dir`}
          value={value.dir}
          onChange={(e) => onChange(value.degrees, e.target.value as GribDirection)}
        >
          <option value="N">N</option>
          <option value="S">S</option>
        </select>
      </div>
    </label>
  );
}

function LonField({ label, value, onChange, testId }: LatFieldProps) {
  return (
    <label className="grib-field">
      <span>{label}</span>
      <div className="grib-field-row">
        <input
          type="number"
          min={0}
          max={180}
          step={1}
          data-testid={`${testId}-deg`}
          value={value.degrees}
          onChange={(e) => onChange(clampDegrees(e.target.value, 180, value.degrees), value.dir)}
        />
        <select
          data-testid={`${testId}-dir`}
          value={value.dir}
          onChange={(e) => onChange(value.degrees, e.target.value as GribDirection)}
        >
          <option value="E">E</option>
          <option value="W">W</option>
        </select>
      </div>
    </label>
  );
}

function clampDegrees(raw: string, max: number, fallback: number): number {
  const n = Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return Math.max(0, Math.min(max, Math.floor(n)));
}

function clampPositive(raw: string, fallback: number): number {
  const n = Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return Math.max(1, Math.floor(n));
}
