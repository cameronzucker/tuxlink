/**
 * StatusBar — bottom app chrome (Mock D), the minimum-viable visibility surface.
 *
 * tuxlink-yd4 (2026-05-20): Mock D defers the dashboard ribbon entirely; the
 * callsign / grid / connection state live HERE instead. Markup matches the
 * mock's `.statusbar`:
 *
 *   ● <state>   ·   <callsign> · <grid>                         v0.0.1
 *
 * Pure presentation — AppShell owns the single `useStatusData` poll (so the
 * window title can reuse the callsign) and passes the derived values in. Left:
 * a tone-colored dot + short state word, then callsign · grid. Right: version.
 *
 * Toggleable via View → Toggle Status Bar (menu:view:status_bar) — AppShell
 * owns `show`; when false the component returns null (zero height).
 */

import type { StatusBarData } from './useStatus';
import './StatusBar.css';

/** App version shown at the right of the status bar (matches the mock). */
const APP_VERSION = 'v0.0.1';

export interface StatusBarProps {
  /** When false, the status bar is hidden (returns null — zero height). */
  show: boolean;
  /** Derived status values from `useStatusData` (owned by AppShell). */
  data: StatusBarData;
}

export function StatusBar({ show, data }: StatusBarProps) {
  // Spec: "toggleable via View→Toggle Status Bar"; hidden = zero height.
  if (!show) return null;

  const { callsign, grid, gridTooltip, state } = data;
  // Station label: "W4PHS · EM75xx", dropping whichever part is absent.
  const station = [callsign, grid].filter(Boolean).join(' · ');

  return (
    <div className="statusbar" data-testid="status-bar" role="status" aria-live="polite">
      <div className="status-item" data-testid="status-bar-state">
        <span className={`status-dot ${state.tone}`} data-testid="status-bar-dot" aria-hidden="true" />
        {state.label}
      </div>

      {station && <span className="status-divider" aria-hidden="true">·</span>}
      {station && (
        <div
          className="status-item"
          data-testid="status-bar-station"
          title={gridTooltip ?? undefined}
        >
          {station}
        </div>
      )}

      <div className="status-right" data-testid="status-bar-version">
        {APP_VERSION}
      </div>
    </div>
  );
}
