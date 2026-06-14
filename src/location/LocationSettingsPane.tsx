// LocationSettingsPane — the INLINE Location & GPS content for the Settings
// nav+pane layout (tuxlink-b95x). Renders the shared GpsSourcePicker (map hero +
// source/diagnostics/manual rail) directly inside the Settings content pane — no
// modal, no popup, no "Open" button. Replaces the LocationSettingsPanel
// modal-on-modal (the WLE nested-window anti-pattern). The wizard's StepLocation
// is the full-screen sibling; both share GpsSourcePicker + useLocationConfig.

import { GpsSourcePicker } from './GpsSourcePicker';
import { useLocationConfig } from './useLocationConfig';

export function LocationSettingsPane() {
  const { grid, selectedSource, error, onGridChange, onSelectSource, gpsReady, fixLat, fixLon, uiGrid } =
    useLocationConfig();

  return (
    <div className="location-settings" data-testid="location-settings">
      {error && (
        <p className="location-settings__error" role="alert" data-testid="location-settings-error">
          {error}
        </p>
      )}
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
  );
}
