/**
 * GridEdit — inline-edit cell for the ribbon Grid value + MANUAL/GPS source chip.
 *
 * Click the grid cell → inline input; Enter validates + commits; Esc cancels.
 * Renders a MANUAL/GPS source chip beside the value reflecting the LIVE source
 * (Gps when a fresh fix is producing the displayed grid; Manual when falling
 * back to the manually-set grid). tuxlink-pjih: the prior "GPS ready — tap to
 * switch" affordance was removed; under the new arbiter semantics GPS-fresh
 * ALWAYS wins the displayed grid, so the explicit "switch to GPS" step is
 * structurally unreachable.
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §Task 8
 * bd issue: tuxlink-686, tuxlink-pjih
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
  source: PositionSource;                           // 'Manual' | 'Gps' (LIVE source per tuxlink-pjih)
  gpsReady: boolean;                                // a usable fix exists (sourced in Task 11)
  onCommit: (grid: string) => void | Promise<void>; // receives the NORMALIZED grid
}

export function GridEdit({ grid, source, gpsReady, onCommit }: GridEditProps) {
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
          placeholder="e.g. DM33xx"
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
      {/* Source chip. GPS gets a `locked` modifier when a fix is live so it
          reads as ACTIVE (green) rather than greyed-out-as-if-disabled (tuxlink-39b). */}
      <span
        className={`dash-source-chip ${source === 'Manual' ? 'manual' : 'gps'}${
          source === 'Gps' && gpsReady ? ' locked' : ''
        }`}
        data-testid="source-chip"
        aria-label={`Position source: ${source}`}
      >
        {source === 'Manual' ? 'MANUAL' : 'GPS'}
      </span>
    </div>
  );
}
