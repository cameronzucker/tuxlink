// src/aprs/StationsView.tsx
//
// The Station Data pop-out window's root (tuxlink-2phz), mounted by App.tsx when
// the webview loads "/stations". It is the same source-reactive panel as the
// in-dock tenant, just filling its own window: it subscribes to the live
// weather/telemetry events (broadcast to every window) and renders EnvPanel.
//
// History note: this window's per-channel rings start empty when it opens and
// fill as new frames arrive — the from-launch buffer lives in the MAIN window's
// shell-level hook, which a separate webview's JS context cannot share. Live
// values appear immediately; the graphs grow from open-time.

import { useEnvStations } from './useEnvStations';
import { EnvPanel } from './EnvPanel';
import './StationsView.css';

export function StationsView() {
  // Client role: seed from the main shell's snapshot on open so this window
  // shows the live roster immediately rather than an empty "no station data"
  // state until the next beacon (tuxlink-hzwc bug #4).
  const { stations } = useEnvStations({ snapshotRole: 'client' });
  return (
    <div className="stations-view" data-testid="stations-view">
      <EnvPanel stations={stations} />
    </div>
  );
}
