/**
 * StatusBar — bottom status bar (Mock B `.statusbar`).
 *
 * Mock B: ● <connection> · <N> unread                    v0.0.1 · Pat 1.0.0
 * The rich operator info (callsign/grid/GPS) lives in the DashboardRibbon up
 * top; this bar carries connection-ready state + unread count + versions.
 * Toggleable via View → Toggle Status Bar.
 */

import type { StatusTone } from './useStatus';
import { DEV_FIXTURE, DEV_CONNECTION_STATUS } from '../mailbox/devFixture';
import { formatPacketStatusBar, type PacketUiState } from '../packet/packetStatus';
import './StatusBar.css';

const APP_VERSION = 'v0.0.1';
const PAT_VERSION = 'Pat 1.0.0';

export interface StatusBarProps {
  /** When false, the status bar is hidden (returns null — zero height). */
  show: boolean;
  /** Unread count (Inbox) shown as "N unread". */
  unread: number;
  /** Connection state (label + dot tone). */
  state: { label: string; tone: StatusTone };
  /** Packet transport state; when active, overrides the CMS state label. */
  packet?: PacketUiState;
}

export function StatusBar({ show, unread, state, packet }: StatusBarProps) {
  if (!show) return null;

  // Packet override (when active, overrides CMS state label).
  const packetState = packet ? formatPacketStatusBar(packet) : null;
  // Mock B shows "Telnet ready" with a good dot; the dev fixture matches that,
  // the real app derives from the live connection state (or packet state).
  const connLabel = DEV_FIXTURE ? DEV_CONNECTION_STATUS : (packetState?.label ?? state.label);
  const connTone = DEV_FIXTURE ? 'good' : (packetState?.tone ?? state.tone);

  return (
    <div className="statusbar" data-testid="status-bar" role="status" aria-live="polite">
      <div className="status-item" data-testid="status-bar-state">
        <span className={`status-dot ${connTone}`} data-testid="status-bar-dot" aria-hidden="true" />
        {connLabel}
      </div>
      <span className="status-divider" aria-hidden="true">
        ·
      </span>
      <div className="status-item" data-testid="status-bar-unread">
        {unread} unread
      </div>
      <div className="status-right" data-testid="status-bar-version">
        {APP_VERSION} · {PAT_VERSION}
      </div>
    </div>
  );
}
