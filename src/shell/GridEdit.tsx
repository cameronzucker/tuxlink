/**
 * GridEdit — inline-edit cell for the ribbon Grid value + MANUAL/GPS source chip.
 *
 * Click the grid cell → inline input; Enter validates + commits; Esc cancels.
 * Renders a MANUAL/GPS source chip beside the value. When source is Manual and
 * a GPS fix is available (gpsReady), renders a "GPS ready" affordance to switch.
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §Task 8
 * bd issue: tuxlink-686
 */

import { useState } from 'react';
import type { PositionSource } from './useStatus';
import { validateGrid, normalizeGrid } from '../wizard/validators';

function extractErrorMessage(err: unknown): string {
  if (err && typeof err === 'object' && 'detail' in err) {
    const d = (err as { detail?: unknown }).detail;
    if (typeof d === 'string' && d) return d;
  }
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === 'string' && err) return err;
  return 'Could not save the grid.';
}

export interface GridEditProps {
  grid: string | null;                              // current broadcast grid (data.grid)
  source: PositionSource;                           // 'Manual' | 'Gps'
  gpsReady: boolean;                                // a usable fix exists (sourced in Task 11)
  onCommit: (grid: string) => void | Promise<void>; // receives the NORMALIZED grid
  onUseGps: () => void;                             // switch source to GPS
}

export function GridEdit({ grid, source, gpsReady, onCommit, onUseGps }: GridEditProps) {
  const [editing, setEditing] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const [error, setError] = useState<string | null>(null);

  function enterEdit() {
    setInputValue(grid ?? '');
    setError(null);
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setError(null);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      cancelEdit();
      return;
    }
    if (e.key === 'Enter') {
      const trimmed = inputValue.trim();
      // Empty → treat as cancel; backend rejects empty grids anyway
      if (!trimmed) {
        cancelEdit();
        return;
      }
      const validationError = validateGrid(trimmed);
      if (validationError) {
        setError(validationError);
        return;
      }
      // Valid and non-empty — normalize and commit
      const normalized = normalizeGrid(trimmed);
      const result = onCommit(normalized);
      if (result instanceof Promise) {
        result.then(() => {
          setEditing(false);
          setError(null);
        }).catch((err: unknown) => {
          setError(extractErrorMessage(err));
          // Stay in edit mode so the operator can correct
        });
      } else {
        setEditing(false);
        setError(null);
      }
    }
  }

  if (editing) {
    return (
      <div className="dash-grid-edit-container">
        <input
          className="dash-value dash-grid-input"
          data-testid="grid-input"
          aria-label="Grid locator"
          value={inputValue}
          autoFocus
          onChange={(e) => {
            setInputValue(e.target.value);
            setError(null);
          }}
          onKeyDown={handleKeyDown}
        />
        {error && (
          <div className="dash-grid-error" data-testid="grid-error" role="alert">
            {error}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="dash-grid-display">
      {/* The clickable grid value — data-testid matches the old ribbon-grid testid */}
      <button
        type="button"
        className="dash-value dash-grid-value-btn"
        data-testid="ribbon-grid"
        onClick={enterEdit}
        title="Click to edit grid"
      >
        {grid ?? '—'}
      </button>
      {/* Source chip */}
      <span
        className={`dash-source-chip ${source === 'Manual' ? 'manual' : 'gps'}`}
        aria-label={`Position source: ${source}`}
      >
        {source === 'Manual' ? 'MANUAL' : 'GPS'}
      </span>
      {/* GPS-ready affordance: visible when Manual and a fix exists */}
      {source === 'Manual' && gpsReady && (
        <button
          type="button"
          className="dash-use-gps"
          data-testid="use-gps"
          onClick={onUseGps}
          title="A GPS fix is available — click to switch to GPS"
        >
          <span aria-hidden="true">● </span>GPS ready — tap to switch
        </button>
      )}
    </div>
  );
}
