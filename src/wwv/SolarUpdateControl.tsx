// Self-contained "Update propagation data" control (tuxlink-ot71 completion —
// the internet counterpart to WwvOffairControl's "Refresh off-air", mounted
// beside it in the station-finder actions row). Owns its own request state;
// no props threaded through the large presentational StationFinderControls,
// same shape as WwvOffairControl. Fetches NOAA SWPC's smoothed-SSN forecast +
// live WWV indices and persists them via `updateSolarFromInternet`
// (src-tauri/src/propagation/commands.rs `propagation_update_solar`) into the
// same ssn-forecast.json / solar-snapshot.json the offline predictor and
// WwvOffairControl's snapshot both read — the backend re-reads the forecast
// fresh on every predict call, so a subsequent prediction picks up the new
// SSN without an app restart or any explicit wiring back into this panel.

import { useState } from 'react';
import { updateSolarFromInternet } from './wwvApi';

export function SolarUpdateControl() {
  const [status, setStatus] = useState<'idle' | 'updating' | 'error'>('idle');

  const handleClick = async () => {
    setStatus('updating');
    try {
      await updateSolarFromInternet();
      setStatus('idle');
    } catch {
      setStatus('error');
    }
  };

  return (
    <>
      <button
        type="button"
        className="station-finder__refresh"
        data-testid="solar-update-button"
        disabled={status === 'updating'}
        onClick={handleClick}
      >
        {status === 'updating' ? 'Updating…' : 'Update propagation data'}
      </button>
      {status === 'error' && (
        <span className="station-finder__stale" data-testid="solar-update-error">
          propagation update failed
        </span>
      )}
    </>
  );
}
