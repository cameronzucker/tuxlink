/**
 * GridPickerOverlay — an in-app overlay (dimmed backdrop + centered, width-
 * CONSTRAINED panel; never an OS window) that lets the operator set their
 * Maidenhead grid by dropping a pin on the offline map, instead of typing it.
 *
 * Triage #18: without Geographica or a third-party geocoder, an operator has no
 * way to derive even a 2-char Maidenhead. `GridMapPicker` (pin mode) already
 * computes the grid from a clicked point; this overlay simply surfaces it from
 * the grid field (`GridEdit`) and returns the chosen grid on confirm.
 *
 * The panel is capped (not full-bleed) per the no-stretched-full-width rule; the
 * map viewport is bounded-height so Leaflet renders under WebKitGTK.
 */

import { useState } from 'react';
import { createPortal } from 'react-dom';
import { GridPicker } from '../map/GridPicker';
import { normalizeGrid } from '../wizard/validators';
import './GridPickerOverlay.css';

export interface GridPickerOverlayProps {
  /** Grid to seed the pin from (the field's current value), if any. */
  initialGrid?: string | null;
  /** Confirm — receives the NORMALIZED chosen grid. */
  onConfirm: (grid: string) => void;
  /** Cancel — close without changing the grid. */
  onCancel: () => void;
}

export function GridPickerOverlay({ initialGrid, onConfirm, onCancel }: GridPickerOverlayProps) {
  const [picked, setPicked] = useState<string>(() =>
    initialGrid ? normalizeGrid(initialGrid.trim()) : '',
  );

  const body = (
    <div
      className="grid-picker-overlay__backdrop"
      data-testid="grid-picker-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Pick location on map"
      onMouseDown={(e) => {
        // Click on the dimmed backdrop (not the panel) cancels.
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="grid-picker-overlay__panel">
        <div className="grid-picker-overlay__head">
          <h2 className="grid-picker-overlay__title">Pick location on map</h2>
          <button
            type="button"
            className="grid-picker-overlay__close"
            aria-label="Cancel"
            onClick={onCancel}
          >
            ✕
          </button>
        </div>

        <p className="grid-picker-overlay__hint">
          Click the map to drop a pin. The grid square under the pin becomes the locator.
        </p>

        <div className="grid-picker-overlay__map">
          <GridPicker
            mode="pin"
            grid={picked || undefined}
            onGridChange={(g) => setPicked(normalizeGrid(g))}
          />
        </div>

        <div className="grid-picker-overlay__foot">
          <span className="grid-picker-overlay__readout" data-testid="grid-picker-readout">
            {picked ? `Locator: ${picked}` : 'Click the map to choose a locator'}
          </span>
          <span className="grid-picker-overlay__actions">
            <button
              type="button"
              className="grid-picker-overlay__cancel"
              onClick={onCancel}
            >
              Cancel
            </button>
            <button
              type="button"
              className="grid-picker-overlay__confirm"
              data-testid="grid-picker-confirm"
              disabled={!picked}
              onClick={() => picked && onConfirm(picked)}
            >
              Use this location
            </button>
          </span>
        </div>
      </div>
    </div>
  );

  // Portal to body so the overlay centers over the whole app, not clipped
  // inside the ribbon. `document.body` exists in the webview + jsdom.
  return createPortal(body, document.body);
}
