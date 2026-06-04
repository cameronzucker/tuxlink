import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FormComposeProps } from '../forms/forms';
import './PositionFormV2.css';

interface PositionFix {
  grid: string | null;
  source: string;
  fresh: boolean;
}

/** Compose-side Position Report form — pre-fills grid from PositionArbiter.
 *
 * Conforms to FormComposeProps so it can be registered in the form registry.
 * Calls onSubmit({ formId, grid, remark }) which is a valid Record<string, string>.
 */
export function PositionFormV2({ onSubmit, onCancel }: FormComposeProps) {
  const [fix, setFix] = useState<PositionFix | null>(null);
  const [grid, setGrid] = useState('');
  const [remark, setRemark] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<PositionFix>('position_current_fix')
      .then((f) => {
        setFix(f);
        if (f.grid) setGrid(f.grid);
      })
      .catch((e) => setError(String(e)));
  }, []);

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
        {fix && (
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
        aria-label="Maidenhead grid"
      />

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
          onClick={() => onSubmit({ formId: 'Position_Report', grid, remark })}
          disabled={!grid}
        >
          Send
        </button>
      </div>
    </div>
  );
}
