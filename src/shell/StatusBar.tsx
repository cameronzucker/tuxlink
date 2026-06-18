/**
 * StatusBar — bottom mailbox-state bar (tuxlink-qxqj).
 *
 * Pre-tuxlink-qxqj this bar carried the connection state (dot + label) + the
 * Inbox unread count + the app version. The connection state duplicated the
 * DashboardRibbon's connection chip — same data, same packet-override logic,
 * different fonts. The operator flagged it: peripheral-vision real-estate
 * shouldn't be spent on the same data twice.
 *
 * Post-redesign the bar carries **mailbox queue state** — what's waiting on
 * the operator's attention — instead of transport state:
 *   - Outbox queue depth (how many drafts are queued to send)
 *   - Inbox unread count (the existing unique value)
 *   - App version (right-anchored — the existing unique value)
 *
 * Connection / transport state lives in the DashboardRibbon at the top of
 * the window where the operator-facing identity (callsign, grid, time) also
 * lives. The two surfaces no longer overlap.
 *
 * The menu label changes from "Toggle Status Bar" to "Toggle Mailbox Bar"
 * (menuModel) — the bar's job is mailbox state, not transport status.
 *
 * Component name (StatusBar), CSS class (.statusbar), data-testid
 * (status-bar) all stay — the rename is in operator-visible copy only, so
 * downstream tests and tooling don't churn.
 */

import { memo } from 'react';
import { DEV_FIXTURE } from '../mailbox/devFixture';
import { useActiveDownload } from '../map/useActiveDownload';
import './StatusBar.css';

// Injected at build time from version.txt (release-please's canonical bump
// target). See vite.config.ts. Prefixed with "v" for display.
const APP_VERSION = `v${__APP_VERSION__}`;

export interface StatusBarProps {
  /** When false, the bar is hidden (returns null — zero height). */
  show: boolean;
  /** Inbox unread count. */
  unread: number;
  /** Number of messages queued in the Outbox waiting for the next CMS connect.
   *  When 0 the segment is hidden (peripheral vision shouldn't carry zero-
   *  state noise). The dev fixture forces a value for screenshot reproducibility. */
  outboxQueued: number;
}

// tuxlink-djnl: React.memo so 2s status polls / shell-level renders don't
// re-render the status bar when its inputs are unchanged (primitive props
// shallow-compare cleanly).
export const StatusBar = memo(function StatusBar({ show, unread, outboxQueued }: StatusBarProps) {
  // tuxlink-8g28: ambient offline-map download progress. Subscribed here (not via
  // an AppShell prop) so the indicator is app-level and stays visible after the
  // operator leaves the Offline-maps panel — the panel's own row owns rate/eta;
  // this is just "a map download is running, NN%". Hook runs before the `!show`
  // early return (rules of hooks). Returns null when nothing is downloading.
  const download = useActiveDownload();

  if (!show) return null;

  // Dev fixture hard-codes a recognizable queue depth for the Mock-B
  // screenshot baseline; the real app derives from the live mailbox.
  const queued = DEV_FIXTURE ? 2 : outboxQueued;

  return (
    <div className="statusbar" data-testid="status-bar" role="status" aria-live="polite">
      {download && (
        <>
          <div className="status-item status-item-download" data-testid="status-bar-download">
            {download.finishing
              ? 'Downloading map…'
              : `Downloading map ${Math.round(download.percent * 100)}%`}
          </div>
          <span className="status-divider" aria-hidden="true">·</span>
        </>
      )}
      {queued > 0 && (
        <>
          <div className="status-item" data-testid="status-bar-outbox">
            {queued} to send
          </div>
          <span className="status-divider" aria-hidden="true">·</span>
        </>
      )}
      <div className="status-item" data-testid="status-bar-unread">
        {unread} unread
      </div>
      <div className="status-right" data-testid="status-bar-version">
        {APP_VERSION}
      </div>
    </div>
  );
});
