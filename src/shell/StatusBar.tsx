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

import { DEV_FIXTURE } from '../mailbox/devFixture';
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

export function StatusBar({ show, unread, outboxQueued }: StatusBarProps) {
  if (!show) return null;

  // Dev fixture hard-codes a recognizable queue depth for the Mock-B
  // screenshot baseline; the real app derives from the live mailbox.
  const queued = DEV_FIXTURE ? 2 : outboxQueued;

  return (
    <div className="statusbar" data-testid="status-bar" role="status" aria-live="polite">
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
}
