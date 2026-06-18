// src/aprs/EnvPanel.tsx
//
// The source-reactive environmental panel body (tuxlink-2phz): a scrollable
// list of station cards, one per callsign heard emitting weather and/or
// telemetry, most-recently-heard first (fed by useEnvStations). Environmental
// data is sparse on the air, so an honest empty state is the common first view —
// it says nothing has been heard rather than implying a fault.

import './EnvPanel.css';
import { EnvStationCard } from './EnvStationCard';
import type { EnvStation } from './envStations';

export interface EnvPanelProps {
  stations: EnvStation[];
  /// Local epoch-ms reference for staleness + relative times. Injectable so the
  /// card's stale/age rendering is deterministic under test.
  now?: number;
}

export function EnvPanel({ stations, now = Date.now() }: EnvPanelProps) {
  if (stations.length === 0) {
    return (
      <div className="env-panel" data-testid="env-panel">
        <div className="env-empty" data-testid="env-empty">
          <p className="env-empty-title">No station data heard yet</p>
          <p className="env-empty-sub">
            Weather and telemetry stations appear here as their beacons are decoded off the
            channel. Nothing has been heard on this link so far.
          </p>
        </div>
      </div>
    );
  }
  return (
    <div className="env-panel" data-testid="env-panel">
      <div className="env-list">
        {stations.map((s) => (
          <EnvStationCard key={s.call} station={s} now={now} />
        ))}
      </div>
    </div>
  );
}
