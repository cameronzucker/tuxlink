// StepLocation.tsx — wizard cluster Location step (tuxlink-9xy1 slice 1).
//
// The first-run counterpart to Settings → Location: it runs GPS detection, helps
// the operator past the common Linux blockers (dialout group, ModemManager) with
// copy-pasteable fix commands, and always offers manual grid entry. This is the
// "beautiful guided setup including GPS" the mocks promised — a real win over WLE,
// which leaves Linux GPS as an undocumented first-run wall.
//
// Every identity path (CMS-verified, CMS-skipped, offline) threads through here
// before `complete` (see wizardReducer: SUBMIT_CREDENTIALS_SUCCESS / SKIP_CMS_VERIFY
// / SUBMIT_OFFLINE_SUCCESS all route to 'location'). Persistence reuses the shared
// useLocationConfig hook → config_set_grid / position_set_source, identical to the
// Settings chrome. The grid/source are written as the operator interacts, so the
// "Continue" button only advances the step (ADVANCE_FROM_LOCATION → complete).
//
// Non-blocking by design: grid is optional everywhere in onboarding, so Continue is
// always available. A non-empty-but-invalid grid surfaces an inline error (via the
// picker) and simply isn't persisted; the operator can refine it later in Settings.

import { useWizard } from './wizardContext';
import { GpsSourcePicker } from '../location/GpsSourcePicker';
import { useLocationConfig } from '../location/useLocationConfig';

export function StepLocation() {
  const { dispatch } = useWizard();
  const { grid, selectedSource, error, onGridChange, onSelectSource, gpsReady, fixLat, fixLon, uiGrid } =
    useLocationConfig();

  return (
    <div
      className="wizard-step wizard-step-location wizard-step-location--fullscreen"
      data-testid="wizard-step-location"
    >
      <div className="wizard-location__rail">
        <h1>Set up your location</h1>
        <p>
          This is where Tuxlink thinks your station is. Confirm it on the map, or set it
          yourself by clicking the map or typing a grid. Your grid powers propagation
          predictions and your on-air position report. Optional — change it any time
          under <strong>Tools → Settings → Location</strong>.
        </p>

        {error && (
          <div role="alert" className="wizard-error-banner" data-testid="wizard-location-error">
            {error}
          </div>
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

        <div className="wizard-submit-row">
          <button
            type="button"
            className="wizard-btn-secondary"
            data-testid="wizard-location-skip"
            onClick={() => dispatch({ type: 'ADVANCE_FROM_LOCATION' })}
          >
            Set later
          </button>
          <button
            type="button"
            data-testid="wizard-location-continue"
            onClick={() => dispatch({ type: 'ADVANCE_FROM_LOCATION' })}
          >
            Looks right — Continue
          </button>
        </div>
      </div>
    </div>
  );
}
