/**
 * PositionPickerOverlay — the expand-to-overlay Position location picker
 * (tuxlink-sdbd, Map-Picker v2 design §6).
 *
 * Replaces the cramped inline 240px map in the Position compose form with a
 * large in-app overlay (dimmed backdrop + centered panel; the SAME pattern as
 * GribRequestPanel / GridPickerOverlay — never an OS pop-up window, honoring the
 * inline-UI / no-window-clutter rule, with the Compose window as the settled
 * exception the overlay rides inside).
 *
 * Pin mode, no mode toggle: click drops a pin, the map pans on drag — click and
 * drag are distinct, so there is no box-vs-pan overload (§6). Reuses the
 * existing PositionMapWidget, which emits a FULL-precision (6-char) locator on
 * click; this overlay applies the operator's explicit precision choice on top.
 *
 * Precision selector (§6 + §8.6): segmented 4-char (default) | 6-char. 4-char is
 * the broadcast/APRS precision-reduction default. 6-char is GATED on
 * `sixCharAllowed` — disabled unless the view under the pin is backed by
 * validated real LAN tiles zoomed past SIX_CHAR_MIN_ZOOM. Because
 * PositionMapWidget passes no `tileSource` to BaseMap (the frozen C11 contract),
 * its substrate is pinned at the raster-native zoom, so 6-char stays gated until
 * the map gains a tile layer + zoom controls (the a1cc / dyop substrate wiring).
 * This replaces the prior silent contradiction where PositionMapWidget
 * hard-emitted 6-char on a z2 raster that cannot back it (illusory precision).
 *
 * The shared §5 control cluster (zoom +/-, fit, jump-to, scale bar) is the
 * separate Pillar-2 `a1cc` unit and is intentionally NOT built here; this
 * overlay degrades gracefully to the bundled-raster substrate per design §9.
 */
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { PositionMapWidget } from './PositionMapWidget';
import {
  getTileSourceStatus,
  sixCharAllowed,
  type TileSourceStatus,
} from '../map/tileSource';
import './PositionPickerOverlay.css';

// (RASTER_VIEW_ZOOM constant removed in Task 8 — the overlay now tracks the
// live view zoom via onZoomChange forwarded from PositionMapWidget → BaseMap's
// zoomend bridge, so the 6-char gate reflects the operator's actual zoom.)

type Precision = 4 | 6;

export interface PositionPickerOverlayProps {
  /** Seed the pin from the form's current grid (may be empty). */
  initialGrid: string;
  /** The PositionArbiter's current fix, for "Reset to GPS fix" (omit/null hides it). */
  gpsGrid?: string | null;
  /** Confirm — receives the grid at the chosen precision. */
  onConfirm: (grid: string) => void;
  /** Cancel — close without changing the form's grid. */
  onCancel: () => void;
}

export function PositionPickerOverlay({
  initialGrid,
  gpsGrid,
  onConfirm,
  onCancel,
}: PositionPickerOverlayProps) {
  // Picked grid at FULL precision (PositionMapWidget emits 6-char on click);
  // the precision selector trims it at readout/confirm time.
  const [pickedFull, setPickedFull] = useState<string>(() => initialGrid.trim().toUpperCase());
  const [precision, setPrecision] = useState<Precision>(4);
  const [status, setStatus] = useState<TileSourceStatus | null>(null);
  // Live view zoom, updated via onZoomChange from PositionMapWidget's zoomend
  // bridge. Initial value matches PositionMapWidget's initialZoom fallback (1)
  // so the gate starts closed and only opens once the operator zooms in.
  const [viewZoom, setViewZoom] = useState<number>(1);

  useEffect(() => {
    let mounted = true;
    getTileSourceStatus()
      .then((s) => mounted && setStatus(s))
      .catch(() => mounted && setStatus(null));
    return () => {
      mounted = false;
    };
  }, []);

  const sixAllowed = status ? sixCharAllowed(status, { zoom: viewZoom }) : false;
  // If 6-char is selected but not (or no longer) allowed, fall back to 4-char.
  const effectivePrecision: Precision = precision === 6 && !sixAllowed ? 4 : precision;

  const atPrecision = (g: string) => (effectivePrecision === 4 ? g.slice(0, 4) : g);
  const readout = pickedFull ? atPrecision(pickedFull) : '';
  const canConfirm = readout.length >= 4;

  const resetGrid = gpsGrid ? gpsGrid.trim().toUpperCase() : '';

  const body = (
    <div
      className="position-picker-overlay__backdrop"
      data-testid="position-picker-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Pick your location"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="position-picker-overlay__panel">
        <div className="position-picker-overlay__head">
          <h2 className="position-picker-overlay__title">Pick your location</h2>
          <button
            type="button"
            className="position-picker-overlay__close"
            aria-label="Close"
            onClick={onCancel}
          >
            ✕
          </button>
        </div>

        <div className="position-picker-overlay__tools">
          <span className="position-picker-overlay__hint">
            Click to set · drag the pin to fine-tune · drag the map to pan
          </span>
          {resetGrid && (
            <button
              type="button"
              className="position-picker-overlay__reset"
              data-testid="position-picker-reset-gps"
              onClick={() => setPickedFull(resetGrid)}
            >
              ⌖ Reset to GPS fix
            </button>
          )}
        </div>

        <div className="position-picker-overlay__map">
          <PositionMapWidget
            grid={pickedFull}
            onGridChange={(g) => setPickedFull(g.toUpperCase())}
            onZoomChange={setViewZoom}
          />
        </div>

        <div className="position-picker-overlay__locrow">
          <div className="position-picker-overlay__loc">
            <span className="position-picker-overlay__readout" data-testid="position-picker-readout">
              {readout || 'Click the map to choose a location'}
            </span>
          </div>
          <div className="position-picker-overlay__precision" role="group" aria-label="Report precision">
            <span className="position-picker-overlay__precision-label">Report precision</span>
            <div className="position-picker-overlay__precision-seg">
              <button
                type="button"
                data-testid="precision-4char"
                className={effectivePrecision === 4 ? 'is-active' : ''}
                aria-pressed={effectivePrecision === 4}
                onClick={() => setPrecision(4)}
              >
                4-char
              </button>
              <button
                type="button"
                data-testid="precision-6char"
                className={effectivePrecision === 6 ? 'is-active' : ''}
                aria-pressed={effectivePrecision === 6}
                disabled={!sixAllowed}
                onClick={() => setPrecision(6)}
              >
                6-char
              </button>
            </div>
          </div>
        </div>

        {!sixAllowed && (
          <p className="position-picker-overlay__precision-hint" data-testid="precision-hint">
            6-char precision needs a LAN tile source and a closer zoom — configure one under
            Tools → Settings → Map tiles… Until then, reports use the 4-char grid square.
          </p>
        )}

        <div className="position-picker-overlay__foot">
          <button
            type="button"
            className="position-picker-overlay__cancel"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            type="button"
            className="position-picker-overlay__confirm"
            data-testid="position-picker-confirm"
            disabled={!canConfirm}
            onClick={() => canConfirm && onConfirm(readout)}
          >
            Use this location
          </button>
        </div>
      </div>
    </div>
  );

  // Portal to body so the overlay centers over the whole app (it is launched
  // from inside the Compose window's form).
  return createPortal(body, document.body);
}
