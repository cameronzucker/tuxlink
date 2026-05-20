/**
 * DashboardRibbon tests — FIX 6: error reason surfaced in connection label.
 *
 * Spec: docs/superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md §2
 * Three-state requirement: "Backend error → explicit error + reason in the
 * ribbon status". The ribbon must show the reason and NEVER append
 * "· telnet ready" during an error state.
 *
 * DEV_FIXTURE is false under vitest (MODE=test), so the component renders
 * the data prop directly — not the dev fixture strings.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DashboardRibbon } from './DashboardRibbon';
import type { StatusBarData } from './useStatus';

// ---------------------------------------------------------------------------
// FIX 6 — [P1] Error reason surfaced; "· telnet ready" absent in error state
// ---------------------------------------------------------------------------

describe('DashboardRibbon — error state (FIX 6)', () => {
  it('shows the error reason in the connection label when state tone is error', () => {
    const data: StatusBarData = {
      callsign: 'W4PHS',
      grid: 'EM75',
      gridTooltip: null,
      state: { label: 'Error', tone: 'error' },
      errorReason: 'Pat failed: X',
    };
    render(<DashboardRibbon data={data} />);
    const conn = screen.getByTestId('ribbon-connection');
    expect(conn.textContent).toContain('Pat failed: X');
  });

  it('does NOT show "telnet ready" when state tone is error', () => {
    const data: StatusBarData = {
      callsign: 'W4PHS',
      grid: 'EM75',
      gridTooltip: null,
      state: { label: 'Error', tone: 'error' },
      errorReason: 'Pat binary unavailable: the bundled sidecar is a 0-byte dev stub',
    };
    render(<DashboardRibbon data={data} />);
    const conn = screen.getByTestId('ribbon-connection');
    expect(conn.textContent).not.toContain('telnet ready');
  });

  it('shows the full reason string in the connection label', () => {
    const data: StatusBarData = {
      callsign: 'W4PHS',
      grid: 'EM75',
      gridTooltip: null,
      state: { label: 'Error', tone: 'error' },
      errorReason: 'Pat binary unavailable: the bundled sidecar is a 0-byte dev stub',
    };
    render(<DashboardRibbon data={data} />);
    const conn = screen.getByTestId('ribbon-connection');
    expect(conn.textContent).toContain(
      'Pat binary unavailable: the bundled sidecar is a 0-byte dev stub',
    );
  });

  it('shows "· telnet ready" suffix for non-error states (idle)', () => {
    const data: StatusBarData = {
      callsign: 'W4PHS',
      grid: 'EM75',
      gridTooltip: null,
      state: { label: 'Idle', tone: 'idle' },
    };
    render(<DashboardRibbon data={data} />);
    const conn = screen.getByTestId('ribbon-connection');
    expect(conn.textContent).toContain('Idle');
    expect(conn.textContent).toContain('telnet ready');
  });

  it('shows "· telnet ready" suffix for Connected state', () => {
    const data: StatusBarData = {
      callsign: 'W4PHS',
      grid: 'EM75',
      gridTooltip: null,
      state: { label: 'Connected', tone: 'good' },
    };
    render(<DashboardRibbon data={data} />);
    const conn = screen.getByTestId('ribbon-connection');
    expect(conn.textContent).toContain('Connected');
    expect(conn.textContent).toContain('telnet ready');
  });
});
