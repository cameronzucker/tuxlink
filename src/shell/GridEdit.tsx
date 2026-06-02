/**
 * GridEdit — inline-edit cell for the ribbon Grid value + MANUAL/GPS source chip.
 *
 * Click the grid cell → inline input; Enter validates + commits; Esc cancels.
 * Renders a MANUAL/GPS source chip beside the value reflecting the configured
 * source (Manual when the operator has pinned a grid; Gps when the arbiter is
 * sourcing from gpsd). Sticky-Manual is preserved at the config boundary — a
 * fresh GPS fix does NOT flip the chip back to Gps (spec §4.1).
 *
 * The displayed grid is the LOCAL DISPLAY locator (ui_grid from the backend's
 * two-helper split, tuxlink-va1i, spec §2.5 + §4.1) — under LocalUiOnly +
 * source=Gps + fresh fix this can differ from what's transmitted on-air
 * (broadcast_grid stays at the static config grid for privacy). The
 * pre-restoration framing — that the displayed grid always matches the on-air
 * locator and "GPS-fresh always wins" — has been superseded by the va1i
 * amendment.
 *
 * Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §2.5, §4.1
 * bd issue: tuxlink-686, tuxlink-c79g, tuxlink-va1i
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
  grid: string | null;                              // current LOCAL DISPLAY grid (ui_grid via data.grid, tuxlink-va1i)
  source: PositionSource;                           // 'Manual' | 'Gps' — configured source per spec §4.1 (sticky-Manual preserved)
  gpsReady: boolean;                                // a usable fix exists (sourced in Task 11)
  onCommit: (grid: string) => void | Promise<void>; // receives the NORMALIZED grid
  onUseGps: () => void;                             // RESTORED per spec §4.1 — fires when chip is clicked while source = Manual
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
          id="grid-input"
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

  // State 4 / State 5 (tuxlink-c79g T13, spec §2.3 + §2.4): when source = Gps
  // and no fresh fix is live, the source chip is DIMMED, the grid value carries
  // an interpunct `· ` prefix (if a manual_grid fallback is present), and the
  // `Set manually` button is rendered so the operator can escape to inline-edit.
  const showSetManually = source === 'Gps' && !gpsReady;
  const interpunctPrefix = showSetManually && grid ? '· ' : '';

  return (
    <div className="dash-grid-display">
      {/* The clickable grid value — data-testid renamed from `ribbon-grid` to
          `grid-value-display` per the T13 plan body (spec §2.3 wiring). */}
      <button
        type="button"
        className="dash-value dash-grid-value-btn"
        data-testid="grid-value-display"
        onClick={enterEdit}
        title="Click to edit grid"
      >
        {grid ? `${interpunctPrefix}${grid}` : '—'}
      </button>
      {/* Source chip. Per spec §2.1 + §4.2 (tuxlink-c79g T12): renders as a
          real <button> (keyboard-accessible, screen-reader-actionable) when
          source = Manual, and as a passive <span role="status"> when source =
          Gps. GPS gets a `locked` modifier when a fix is live so it reads as
          ACTIVE (green) rather than greyed-out-as-if-disabled (tuxlink-39b).
          T13 (spec §2.4): the GPS chip carries a `dimmed` modifier when no
          fresh fix is live, visually differentiating State 4/5 from State 3. */}
      {source === 'Manual' ? (
        <button
          type="button"
          className={`dash-source-chip manual${gpsReady ? ' gps-ready-glow' : ''}`}
          data-testid="source-chip"
          aria-label="Switch position source to GPS"
          aria-pressed={false}
          onClick={onUseGps}
        >
          MANUAL
        </button>
      ) : (
        <span
          className={`dash-source-chip gps${gpsReady ? ' locked' : ' dimmed'}`}
          data-testid="source-chip"
          role="status"
          aria-label={`Position source: GPS, ${gpsReady ? 'fresh fix' : 'no fix'}`}
        >
          GPS
        </span>
      )}
      {/* State 2 hint (tuxlink-c79g T11, spec §2.2 + §4.2): when source = Manual
          AND a fresh fix is available, render a PASSIVE "GPS ready" status text
          beside the chip. Pre-pjih had this as a clickable <button data-testid="use-gps">
          with "GPS ready — tap to switch" framing; the restoration is a passive
          <span> — the chip itself is the click surface for switching to GPS. */}
      {source === 'Manual' && gpsReady && (
        <span
          className="dash-gps-ready-status"
          data-testid="gps-ready-status"
          role="status"
        >
          ● GPS ready
        </span>
      )}
      {/* State 4 / State 5 affordance (tuxlink-c79g T13, spec §2.3 + §2.5):
          status text + Set manually button. The status text suffix
          "broadcasting fallback" appears only when a manual_grid is set (State
          4) — in State 5 (no manual_grid) the suffix is omitted per spec row
          3. The Set manually <button> is keyboard-accessible and focuses the
          grid input on click via enterEdit() + the input's autoFocus. */}
      {showSetManually && (
        <>
          <span
            className="dash-gps-no-fix-status"
            data-testid="gps-no-fix-status"
            role="status"
          >
            GPS no fix{grid ? ' · broadcasting fallback' : ''}
          </span>
          <button
            type="button"
            className="dash-set-manually"
            data-testid="set-manually-button"
            aria-controls="grid-input"
            onClick={enterEdit}
          >
            ▸ Set manually
          </button>
        </>
      )}
    </div>
  );
}
