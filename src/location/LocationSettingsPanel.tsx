// LocationSettingsPanel — the dedicated Settings-chrome surface for GPS/position
// (tuxlink-2sl6). The old inline fieldset crammed a map feature into the ~520px
// stacked settings modal: tiny map, full-width fields, cards clipped on the right,
// long scroll. This is the right SHAPE instead — a bounded, wide two-pane modal
// (map hero + controls rail), the Settings sibling of the full-screen wizard step,
// reusing the same GpsSourcePicker (so both surfaces stay in sync).
//
// Opened from Settings → "Location & GPS" → Open setup. Esc / backdrop / Done close.

import { useEffect } from 'react';
import { GpsSourcePicker } from './GpsSourcePicker';
import { useLocationConfig } from './useLocationConfig';
import './LocationSettingsPanel.css';

export interface LocationSettingsPanelProps {
  open: boolean;
  onClose: () => void;
}

export function LocationSettingsPanel({ open, onClose }: LocationSettingsPanelProps) {
  const { grid, selectedSource, error, onGridChange, onSelectSource, gpsReady, fixLat, fixLon, uiGrid } =
    useLocationConfig();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="tux-location-backdrop" data-testid="location-backdrop" onClick={onClose}>
      <div
        className="tux-location-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Location and GPS"
        data-testid="location-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-location-head">
          <h2 className="tux-location-title">Location &amp; GPS</h2>
          <button
            type="button"
            className="tux-location-close"
            data-testid="location-close"
            aria-label="Close location settings"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        {error && (
          <p className="tux-location-error" role="alert" data-testid="location-error">
            {error}
          </p>
        )}

        <div className="tux-location-body">
          <GpsSourcePicker
            grid={grid}
            onGridChange={onGridChange}
            selectedSource={selectedSource}
            onSelectSource={onSelectSource}
            gpsReady={gpsReady}
            fixLatLon={fixLat != null && fixLon != null ? { lat: fixLat, lon: fixLon } : null}
            uiGrid={uiGrid}
          />
        </div>

        <div className="tux-location-foot">
          <button type="button" className="tux-location-done" data-testid="location-done" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
