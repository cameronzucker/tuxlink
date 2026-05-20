/**
 * DashboardRibbon — top app chrome, always visible, ~40px height.
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.6
 * bd issue: tuxlink-hvv
 *
 * Displays (left to right):
 *   Callsign · Grid · GPS status · UTC+local time · Connection state (with transport)
 *
 * The ribbon is wired into AppShell's "ribbon" CSS grid region in the
 * orchestrator integration commit (spec §4.3). This file is standalone-
 * buildable and unit-tested against mocked IPC.
 *
 * DESIGN NOTE: The ribbon does NOT edit AppShell.tsx or lib.rs — those
 * changes land in the integration commit (spec §4.3).
 *
 * IPC surface (mocked in tests; registered in integration commit):
 *   - invoke('config_read') → ConfigViewDto
 *   - invoke('backend_status') → StatusDto | null (null when AppBackend is None)
 */

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  formatCallsign,
  formatConnectionState,
  formatGpsStatus,
  formatGrid,
  type ConfigViewDto,
  type StatusDto,
} from './useStatus';
import './DashboardRibbon.css';

// ============================================================================
// Internal UTC + local clock hook
// ============================================================================

function useClock() {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  const utc = now.toISOString().substring(11, 16) + 'z';
  const local = now.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });

  return { utc, local };
}

// ============================================================================
// Main component
// ============================================================================

export function DashboardRibbon() {
  const [config, setConfig] = useState<ConfigViewDto | null>(null);
  const [status, setStatus] = useState<StatusDto | null>(null);
  const { utc, local } = useClock();

  // Load config once on mount (5s refetch per spec §5.6).
  useEffect(() => {
    let mounted = true;
    const load = () => {
      invoke<ConfigViewDto>('config_read')
        .then((c) => { if (mounted) setConfig(c); })
        .catch(() => { /* config absent / pre-wizard: ribbon shows empty */ });
    };
    load();
    const id = setInterval(load, 5000);
    return () => { mounted = false; clearInterval(id); };
  }, []);

  // Poll backend_status every 2s when the backend may be alive.
  useEffect(() => {
    let mounted = true;
    const load = () => {
      invoke<StatusDto | null>('backend_status')
        .then((s) => { if (mounted) setStatus(s ?? null); })
        .catch(() => { if (mounted) setStatus(null); });
    };
    load();
    const id = setInterval(load, 2000);
    return () => { mounted = false; clearInterval(id); };
  }, []);

  // Derived display values using pure formatters.
  const callsign = config
    ? formatCallsign({
        connect_to_cms: config.connect_to_cms,
        callsign: config.callsign,
        identifier: config.identifier,
      })
    : '';

  const gridResult = config
    ? formatGrid({ grid: config.grid, precision: config.position_precision })
    : { broadcast: null, tooltip: null };

  const gpsLabel = config ? formatGpsStatus(config.gps_state) : '';

  const connectionLabel = config
    ? formatConnectionState(status, config.transport)
    : 'Loading…';

  return (
    <div className="dashboard-ribbon" data-testid="dashboard-ribbon" role="banner">
      {callsign && (
        <span className="ribbon-callsign" data-testid="ribbon-callsign" title="Your callsign">
          {callsign}
        </span>
      )}

      {gridResult.broadcast && (
        <span
          className="ribbon-grid"
          data-testid="ribbon-grid"
          title={gridResult.tooltip ?? gridResult.broadcast}
        >
          {gridResult.broadcast}
        </span>
      )}

      {gpsLabel && (
        <span className="ribbon-gps" data-testid="ribbon-gps">
          {gpsLabel}
        </span>
      )}

      <span className="ribbon-time" data-testid="ribbon-time">
        <span className="ribbon-time-utc" title="UTC">{utc}</span>
        <span className="ribbon-time-sep"> / </span>
        <span className="ribbon-time-local" title="Local">{local}</span>
      </span>

      <span
        className="ribbon-connection"
        data-testid="ribbon-connection"
        title="Connection state"
      >
        {connectionLabel}
      </span>
    </div>
  );
}
