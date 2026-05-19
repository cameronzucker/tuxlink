/**
 * StatusBar — bottom app chrome, ~24px height, toggleable via View → Toggle Status Bar.
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.6
 * bd issue: tuxlink-hvv
 *
 * Design intent: Mail.app-minimal, NOT Express cryptic-strip.
 * Left side: app activity indicator.
 * Right side: window info (folder + message count when applicable).
 *
 * The status bar is wired into AppShell's "statusbar" CSS grid region in the
 * orchestrator integration commit (spec §4.3). This file is standalone-buildable.
 *
 * Toggle: the parent (AppShell, eventually) passes `show` as a boolean prop.
 * When `show` is false, the component returns null (zero height, no layout impact).
 * The View → Toggle Status Bar menu item controls `show`; AppShell owns that state.
 */

import './StatusBar.css';

// ============================================================================
// Props
// ============================================================================

export interface StatusBarProps {
  /** When false, the status bar is hidden (returns null — zero height). */
  show: boolean;
  /** Activity message for the left side. Empty string = no indicator shown. */
  activity?: string;
  /** Informational text for the right side (e.g. "Inbox · 12 messages"). */
  windowInfo?: string;
}

// ============================================================================
// Component
// ============================================================================

export function StatusBar({ show, activity = '', windowInfo = '' }: StatusBarProps) {
  // Spec: "toggleable via View→Toggle Status Bar"; hidden = zero height, not invisible.
  if (!show) return null;

  return (
    <div className="status-bar" data-testid="status-bar" role="status" aria-live="polite">
      <span className="status-bar-left" data-testid="status-bar-activity">
        {activity || null}
      </span>
      <span className="status-bar-right" data-testid="status-bar-window-info">
        {windowInfo || null}
      </span>
    </div>
  );
}
