// LocationSettings — the Settings-chrome wrapper around GpsSourcePicker
// (tuxlink-9xy1 slice 1). Loads the operator's current grid + position source
// from config and persists changes via the shared useLocationConfig hook:
//   - picking a GPS source  → position_set_source (live arbiter switch, no restart)
//   - picking manual         → position_set_source('Manual')
//   - editing the grid       → config_set_grid (validated; pins Manual)
//
// Rendered inside the inline Settings overlay so GPS setup help is reachable
// where operators already go for GPS/privacy — the surface that was missing.
// The wizard's Location step (StepLocation) shares the same hook + picker.

import { GpsSourcePicker } from './GpsSourcePicker';
import { useLocationConfig } from './useLocationConfig';

export function LocationSettings() {
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
