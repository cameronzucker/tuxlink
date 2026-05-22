/**
 * DashboardRibbon — connection string transport accuracy tests (tuxlink-989).
 *
 * The ribbon must NEVER show a hardcoded "telnet ready" suffix. It must
 * reflect the actual configured or active transport. Bug was confirmed during
 * the tuxlink-22l live smoke (operator N0CALL, CmsSsl config → showed
 * "Idle · telnet ready").
 *
 * DEV_FIXTURE is false under vitest, so the component renders from `data`.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DashboardRibbon } from './DashboardRibbon';
import type { StatusBarData, StatusTone } from './useStatus';
import type { PacketUiState } from '../packet/packetStatus';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeData(overrides: Partial<StatusBarData> = {}): StatusBarData {
  return {
    callsign: 'N0CALL',
    grid: 'DN31',
    gridTooltip: null,
    state: { label: 'Idle', tone: 'idle' as StatusTone },
    connection: 'Idle · CMS-SSL',
    position_source: 'Gps',
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

// ---------------------------------------------------------------------------
// tuxlink-9z2: Abort control appears while connecting and cancels the connect
// ---------------------------------------------------------------------------

describe('<DashboardRibbon> — abort control (tuxlink-9z2)', () => {
  it('shows an Abort button while connecting and calls onAbort when clicked', () => {
    const onAbort = vi.fn();
    render(
      <DashboardRibbon data={makeData()} onConnect={() => {}} onAbort={onAbort} connecting />,
    );
    fireEvent.click(screen.getByTestId('abort-button'));
    expect(onAbort).toHaveBeenCalledTimes(1);
  });

  it('does not render an Abort button when not connecting', () => {
    render(<DashboardRibbon data={makeData()} onConnect={() => {}} onAbort={() => {}} />);
    expect(screen.queryByTestId('abort-button')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Task 12: packet transport indicator in the ribbon Connection item
// ---------------------------------------------------------------------------

describe('DashboardRibbon — packet connection', () => {
  const data: StatusBarData = {
    callsign: 'N7CPZ', grid: 'CN85', gridTooltip: null,
    state: { label: 'Idle', tone: 'idle' as StatusTone }, connection: 'Idle · CMS-SSL',
    position_source: 'Gps', // required since the tuxlink-686 merge (was missing → tsc-only error)
  };
  const packet: PacketUiState = {
    active: true, listening: true, connected: false, effectiveCall: 'N7CPZ-7', linkLabel: 'KISS-TCP Dire Wolf',
  };

  it('shows the packet connection label when packet is active', () => {
    render(<DashboardRibbon data={data} packet={packet} />);
    expect(screen.getByTestId('ribbon-connection')).toHaveTextContent('Listening · Packet 1200');
  });
  it('falls back to the CMS connection string when packet is inactive', () => {
    render(<DashboardRibbon data={data} packet={{ ...packet, active: false }} />);
    expect(screen.getByTestId('ribbon-connection')).toHaveTextContent('Idle · CMS-SSL');
  });
});
