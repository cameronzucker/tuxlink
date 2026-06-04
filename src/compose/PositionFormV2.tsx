import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FormComposeProps } from '../forms/forms';
import { gridToLatLon } from '../forms/position/maidenhead';
import './PositionFormV2.css';

interface PositionFix {
  grid: string | null;
  /** PascalCase from Debug derive: "Gps" | "Manual" */
  source: string;
  fresh: boolean;
}

/** Compose-side Position Report form — pre-fills grid from PositionArbiter.
 *
 * Conforms to FormComposeProps so it can be registered in the form registry.
 *
 * Wire-format contract:
 *   onSubmit emits { thetime, lat, lon, message } — the field IDs that
 *   POSITION_REPORT's template expects. The UI stores grid + remark internally
 *   and transforms to wire format at submit time via gridToLatLon().
 *
 * Draft contract:
 *   onChange emits { grid, message: remark } (UI shape) so autosave stores
 *   what the operator can directly edit. On mount, initialValues?.grid and
 *   initialValues?.message rehydrate the inputs without a reverse-Maidenhead.
 */
export function PositionFormV2({
  initialValues,
  onChange,
  onSubmit,
  onCancel,
}: FormComposeProps) {
  const [fix, setFix] = useState<PositionFix | null>(null);
  // Seed from draft if present; GPS pull fills in when no draft.
  const [grid, setGrid] = useState(initialValues?.grid ?? '');
  const [remark, setRemark] = useState(initialValues?.message ?? '');
  const [error, setError] = useState<string | null>(null);

  // Pull current fix from PositionArbiter. Only sets grid if there is no
  // draft initialValues.grid — drafts win over GPS pull.
  useEffect(() => {
    let mounted = true;
    invoke<PositionFix>('position_current_fix')
      .then((f) => {
        if (!mounted) return;
        setFix(f);
        // Only pre-fill grid from GPS if the draft didn't provide one.
        if (f.grid && !initialValues?.grid) setGrid(f.grid);
      })
      .catch((e) => {
        if (mounted) setError(String(e));
      });
    return () => { mounted = false; };
    // initialValues.grid is intentionally captured at mount — don't re-run
    // when the parent re-renders with a new reference.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Lift form state into draft autosave (UI shape: grid + message so drafts
  // rehydrate directly without a reverse-Maidenhead conversion).
  useEffect(() => {
    onChange?.({ grid, message: remark });
  }, [grid, remark, onChange]);

  const noFixAvailable = fix !== null && fix.grid === null;

  const onSubmitClick = () => {
    const ll = gridToLatLon(grid);
    if (!ll) {
      setError('Invalid grid — fix the Maidenhead grid before sending');
      return;
    }
    onSubmit({
      thetime: new Date().toISOString(),
      lat: ll.lat.toFixed(4),
      lon: ll.lon.toFixed(4),
      message: remark,
    });
  };

  if (error) {
    return (
      <div className="position-form-v2" role="alert">
        Position fix unavailable: {error}
      </div>
    );
  }

  return (
    <div className="position-form-v2" data-testid="position-form-v2">
      <div className="position-form-v2__header">
        <h2>Position Report</h2>
        {fix && fix.grid !== null && (
          <div className={`position-form-v2__fix-badge ${fix.fresh ? 'fresh' : 'stale'}`}>
            {fix.fresh ? 'Fresh' : 'Stale'} {fix.source.toUpperCase()} fix
          </div>
        )}
      </div>

      <label htmlFor="position-grid">Maidenhead grid</label>
      <input
        id="position-grid"
        type="text"
        value={grid}
        onChange={(e) => setGrid(e.target.value.toUpperCase())}
        placeholder="CN87us"
        autoFocus={noFixAvailable && !grid}
      />
      {noFixAvailable && (
        <p className="position-form-v2__no-fix-hint" role="note">
          No GPS fix — enter grid manually
        </p>
      )}

      {/* Map widget mount-point — Leaflet integration ships in a follow-up
          commit on this branch (operator decision 2026-06-04: Leaflet +
          offline tiles). This div is intentionally empty. */}
      <div className="position-form-v2__map" data-testid="position-map-mount">
        {/* Map widget mounts here when wired. */}
      </div>

      <label htmlFor="position-remark">Remark (optional)</label>
      <textarea
        id="position-remark"
        value={remark}
        onChange={(e) => setRemark(e.target.value)}
        rows={3}
      />

      <div className="position-form-v2__actions">
        <button type="button" onClick={onCancel}>Cancel</button>
        <button
          type="button"
          className="primary"
          onClick={onSubmitClick}
          disabled={!grid}
        >
          Send
        </button>
      </div>
    </div>
  );
}
