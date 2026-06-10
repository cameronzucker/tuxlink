/**
 * GridEdit — inline-edit cell for the ribbon Grid value + source segmented control.
 *
 * Click the grid cell → inline input; Enter/blur validates + commits; Esc cancels.
 *
 * Source surface (tuxlink-z5pz, 2026-06-02): a radio-group segmented control
 * with two `<button role="radio">` segments rendered side-by-side — the GPS
 * segment and the MANUAL segment. The selected segment is marked
 * `aria-checked="true"`; clicking the already-selected segment is a no-op.
 * Clicking the OTHER segment fires the source switch:
 *   - GPS segment when source=Manual → onUseGps (calls position_set_source('Gps'))
 *   - MANUAL segment when source=Gps → onUseManual + enterEdit() (opens the
 *     grid input; commit fires the T4 config_set_grid path that atomically
 *     pins cfg.privacy.position_source = Manual)
 *
 * Sticky-Manual is preserved at the config boundary — a fresh GPS fix does
 * NOT flip the source back to Gps (spec §4.1). The displayed grid is the
 * LOCAL DISPLAY locator (ui_grid, tuxlink-va1i) — under LocalUiOnly + source=Gps
 * + fresh fix this can differ from what's transmitted on-air.
 *
 * Lineage:
 *   - T11 (passive <span data-testid="gps-ready-status">) → folded into the
 *     GPS segment as a ' ●' text suffix when source=Manual && gpsReady.
 *   - T12 (conditional <button aria-pressed={false}> when Manual, <span
 *     role="status"> when Gps) → superseded by the radiogroup pattern. The
 *     aria-pressed framing is moot; the segments use aria-checked per the
 *     WAI-ARIA radio-group convention.
 *
 * Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §2.1, §2.2, §2.4, §4.1, §4.4
 * bd issue: tuxlink-686, tuxlink-c79g, tuxlink-va1i, tuxlink-z5pz
 */

import { useState } from 'react';
import type { PositionSource } from './useStatus';
import { validateGrid, normalizeGrid } from '../wizard/validators';
import { GridPickerOverlay } from './GridPickerOverlay';

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
  onUseGps: () => void;                             // fires on GPS-segment click when source = Manual (spec §4.1)
  onUseManual: () => void;                          // fires on MANUAL-segment click when source = Gps (spec §4.1 amended, tuxlink-z5pz)
}

export function GridEdit({ grid, source, gpsReady, onCommit, onUseGps, onUseManual }: GridEditProps) {
  const [editing, setEditing] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [showPicker, setShowPicker] = useState(false);

  function enterEdit() {
    setInputValue(grid ?? '');
    setError(null);
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setError(null);
  }

  function finishEdit(invalidInput: 'show-error' | 'revert') {
    const trimmed = inputValue.trim();
    // Empty → treat as cancel; backend rejects empty grids anyway.
    if (!trimmed) {
      cancelEdit();
      return;
    }

    const validationError = validateGrid(trimmed);
    if (validationError) {
      if (invalidInput === 'show-error') {
        setError(validationError);
      } else {
        cancelEdit();
      }
      return;
    }

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

  // Commit a grid chosen on the map (triage #18). The overlay returns an
  // already-normalized, on-map-derived grid; commit it via the same onCommit
  // path the typed-entry flow uses (config_set_grid → sticky-Manual).
  function commitPickedGrid(grid: string) {
    setShowPicker(false);
    const result = onCommit(grid);
    if (result instanceof Promise) {
      result
        .then(() => {
          setEditing(false);
          setError(null);
        })
        .catch((err: unknown) => {
          setError(extractErrorMessage(err));
          // Stay in edit mode so the operator can retry / type instead.
        });
    } else {
      setEditing(false);
      setError(null);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      cancelEdit();
      return;
    }
    if (e.key === 'Enter') {
      finishEdit('show-error');
    }
  }

  function handleManualSegmentClick() {
    // Per spec §2.1 (tuxlink-z5pz amendment): clicking the MANUAL segment when
    // source = Gps ALWAYS enters edit mode (regardless of whether manual_grid
    // is set). The operator can then type their grid; Enter commits via the
    // T4 config_set_grid path; Escape cancels and source remains Gps.
    onUseManual();
    enterEdit();
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
          // Don't revert-on-blur while the map picker is open — opening it
          // moves focus off the input, which must not cancel the edit.
          onBlur={() => {
            if (!showPicker) finishEdit('revert');
          }}
        />
        {/* Triage #18: set the locator by dropping a pin on the offline map,
            for operators without Geographica / a geocoder. */}
        <button
          type="button"
          className="dash-grid-pick-map"
          data-testid="grid-pick-on-map"
          // Use onMouseDown so the click registers before the input's onBlur.
          onMouseDown={(e) => {
            e.preventDefault();
            setShowPicker(true);
          }}
        >
          ▸ Pick on map…
        </button>
        {error && (
          <div className="dash-grid-error" data-testid="grid-error" role="alert">
            {error}
          </div>
        )}
        {showPicker && (
          <GridPickerOverlay
            initialGrid={inputValue.trim() || grid}
            onConfirm={commitPickedGrid}
            onCancel={() => setShowPicker(false)}
          />
        )}
      </div>
    );
  }

  // State 4 / State 5 (tuxlink-c79g T13, spec §2.3 + §2.4): when source = Gps
  // and no fresh fix is live, the GPS segment is selected-but-DIMMED, the grid
  // value carries an interpunct `· ` prefix (if a manual_grid fallback is
  // present), and the `Set manually` button is rendered so the operator can
  // escape to inline-edit.
  const showSetManually = source === 'Gps' && !gpsReady;
  const interpunctPrefix = showSetManually && grid ? '· ' : '';

  // Per spec §2.1 + §2.2: GPS segment label includes a ' ●' suffix when
  // source = Manual && gpsReady (the in-segment ready indicator that replaces
  // the T11 standalone gps-ready-status span). When source = Gps or gpsReady
  // is false, the label is just 'GPS'. The '●' glyph is wrapped in
  // aria-hidden="true" so screen readers don't announce "black circle"; the
  // semantic "fresh fix available" cue rides through aria-label on the
  // segment instead.
  const showGpsReadyDot = source === 'Manual' && gpsReady;
  const gpsAriaLabel = showGpsReadyDot ? 'GPS — fresh fix available' : 'GPS';

  // Per spec §2.4: the State 4/5 visual is encoded as
  // .dash-source-segment.gps.selected:not(.gps-ready). The implementation
  // also tags the segment with an explicit `dimmed` className for class-list
  // discoverability + symmetry with the prior chip.dimmed test assertion.
  const gpsClassName = [
    'dash-source-segment',
    'gps',
    source === 'Gps' ? 'selected' : '',
    gpsReady ? 'gps-ready' : '',
    source === 'Gps' && !gpsReady ? 'dimmed' : '',
  ].filter(Boolean).join(' ');

  const manualClassName = [
    'dash-source-segment',
    'manual',
    source === 'Manual' ? 'selected' : '',
  ].filter(Boolean).join(' ');

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
      {/* Source segmented control (tuxlink-z5pz, spec §2.1). Replaces the T12
          conditional <button>-or-<span> chip. Both segments are always
          rendered + always clickable; the selected segment has
          aria-checked="true"; clicking the already-selected segment is a
          no-op. The in-segment ' ●' GPS-ready indicator (spec §2.2) folds
          the T11 standalone gps-ready-status span into the GPS segment's
          text content when source=Manual && gpsReady. */}
      <div
        className="dash-source-segments"
        role="radiogroup"
        aria-label="Position source"
      >
        <button
          type="button"
          role="radio"
          aria-checked={source === 'Gps'}
          aria-label={gpsAriaLabel}
          data-testid="source-segment-gps"
          className={gpsClassName}
          onClick={source === 'Gps' ? undefined : onUseGps}
        >
          GPS{showGpsReadyDot ? <span aria-hidden="true"> ●</span> : null}
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={source === 'Manual'}
          data-testid="source-segment-manual"
          className={manualClassName}
          onClick={source === 'Manual' ? undefined : handleManualSegmentClick}
        >
          MANUAL
        </button>
      </div>
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
