// Self-contained "Refresh off-air" control (Task 15, wwv offair spec). Owns
// its own useWwvOffair() hook instance — no props threaded through the large
// presentational StationFinderControls — and mounts directly into the
// station-finder's reserved actions row. Kicks a background snapshot read on
// mount so a prior off-air (or SWPC/RF) capture's provenance shows up
// immediately without requiring an operator click first; that read hits
// Tauri's `invoke`, which throws outside a real Tauri webview (e.g. `pnpm
// vitest run` / a plain browser tab), so the effect swallows the rejection —
// a failed background prefetch must never crash the host station finder.

import { useEffect } from 'react';
import { useWwvOffair } from './useWwvOffair';

export function WwvOffairControl() {
  const { status, snapshot, arm, refreshSnapshot } = useWwvOffair();

  useEffect(() => {
    void refreshSnapshot().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const capturing = status === 'capturing';
  // Narrowed together so TS carries the non-null `indices` through the JSX
  // below without a non-null assertion.
  const offairIndices =
    snapshot != null && snapshot.source === 'rf-wwv-voice' && snapshot.indices != null ? snapshot.indices : null;

  return (
    <>
      <button
        type="button"
        className="station-finder__refresh-offair"
        disabled={capturing}
        onClick={() => void arm(Date.now())}
      >
        {capturing ? 'Capturing…' : 'Refresh off-air'}
      </button>
      {offairIndices && snapshot && (
        <span
          className="station-finder__offair"
          data-testid="wwv-offair-provenance"
          title={`off-air WWV ${new Date(snapshot.updated_at_ms).toISOString()}`}
        >
          off-air WWV · SFI <b>{offairIndices.sfi}</b>
          {offairIndices.k_index != null && (
            <>
              {' '}
              · K <b>{offairIndices.k_index}</b>
            </>
          )}
        </span>
      )}
      {status === 'nocopy' && (
        <span className="station-finder__offair-note" data-testid="wwv-offair-nocopy">
          couldn't copy — retry next cycle
        </span>
      )}
      {status === 'error' && (
        <span className="station-finder__offair-note" data-testid="wwv-offair-error">
          off-air refresh failed
        </span>
      )}
    </>
  );
}
