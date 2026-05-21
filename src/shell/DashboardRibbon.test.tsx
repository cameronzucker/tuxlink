/**
 * DashboardRibbon — connection string transport accuracy tests (tuxlink-989).
 *
 * The ribbon must NEVER show a hardcoded "telnet ready" suffix. It must
 * reflect the actual configured or active transport. Bug was confirmed during
 * the tuxlink-22l live smoke (operator N7CPZ, CmsSsl config → showed
 * "Idle · telnet ready").
 *
 * DEV_FIXTURE is false under vitest, so the component renders from `data`.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DashboardRibbon } from './DashboardRibbon';
import type { StatusBarData, StatusTone } from './useStatus';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeData(overrides: Partial<StatusBarData> = {}): StatusBarData {
  return {
    callsign: 'N7CPZ',
    grid: 'DN31',
    gridTooltip: null,
    state: { label: 'Idle', tone: 'idle' as StatusTone },
    connection: 'Idle · CMS-SSL',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// tuxlink-989: ribbon connection field must reflect the real transport
// ---------------------------------------------------------------------------

describe('<DashboardRibbon> — transport label accuracy (tuxlink-989)', () => {
  it('CmsSsl config: ribbon shows "CMS-SSL" not "telnet" when idle', () => {
    render(<DashboardRibbon data={makeData({ connection: 'Idle · CMS-SSL' })} />);
    const el = screen.getByTestId('ribbon-connection');
    expect(el.textContent?.toLowerCase()).not.toContain('telnet');
    expect(el.textContent).toContain('CMS-SSL');
  });

  it('Telnet config: ribbon shows "Telnet" when idle', () => {
    render(<DashboardRibbon data={makeData({ connection: 'Idle · Telnet' })} />);
    const el = screen.getByTestId('ribbon-connection');
    expect(el.textContent).toContain('Telnet');
  });

  it('CmsSsl config + Disconnected status: ribbon shows "CMS-SSL" not "telnet"', () => {
    render(
      <DashboardRibbon
        data={makeData({
          state: { label: 'Idle', tone: 'idle' },
          connection: 'Disconnected · CMS-SSL',
        })}
      />,
    );
    const el = screen.getByTestId('ribbon-connection');
    expect(el.textContent?.toLowerCase()).not.toContain('telnet');
    expect(el.textContent).toContain('CMS-SSL');
  });

  it('Connected CmsSsl: ribbon shows "Connected · CMS-SSL"', () => {
    render(
      <DashboardRibbon
        data={makeData({
          state: { label: 'Connected', tone: 'good' },
          connection: 'Connected · CMS-SSL',
        })}
      />,
    );
    const el = screen.getByTestId('ribbon-connection');
    expect(el.textContent).toContain('Connected');
    expect(el.textContent).toContain('CMS-SSL');
  });

  it('Error state: ribbon shows the error reason, not a transport suffix', () => {
    render(
      <DashboardRibbon
        data={makeData({
          state: { label: 'Error', tone: 'error' },
          connection: 'Error: connection refused',
        })}
      />,
    );
    const el = screen.getByTestId('ribbon-connection');
    expect(el.textContent).toContain('Error: connection refused');
  });
});
