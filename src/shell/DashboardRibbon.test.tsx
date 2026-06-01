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

// ---------------------------------------------------------------------------
// Operator smoke 2026-05-31: SSID propagates to callsign + is settable inline
// ---------------------------------------------------------------------------

describe('DashboardRibbon — SSID propagation + inline edit', () => {
  it('renders bare callsign when no ssid is supplied', () => {
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} />);
    expect(screen.getByTestId('ribbon-callsign')).toHaveTextContent('N7CPZ');
    expect(screen.queryByTestId('ribbon-ssid-select')).toBeNull();
  });

  it('renders callsign with -SSID suffix when ssid is supplied (no edit handler)', () => {
    // When onSsidChange is not provided we render the plain text span — the
    // dropdown only mounts in editable mode. The displayed value is the
    // effective call (base-SSID).
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} ssid={7} />);
    expect(screen.getByTestId('ribbon-callsign')).toHaveTextContent('N7CPZ-7');
    expect(screen.queryByTestId('ribbon-ssid-select')).toBeNull();
  });

  it('exposes a bare callsign chip + adjacent -N picker when onSsidChange is provided (tuxlink-i63g)', () => {
    // Operator smoke 2026-05-31 round 4 (tuxlink-i63g): the round-3 "one
    // select with `<base>-<N>` options" approach was rejected. Two
    // surfaces are correct: a callsign chip showing the BARE callsign
    // (no `-N`) and an adjacent picker whose options are JUST `-N`. The
    // chip never carries the SSID suffix in the editable branch — that
    // would re-introduce the two-SSID-surface bug.
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} ssid={3} onSsidChange={() => {}} />);
    const sel = screen.getByTestId('ribbon-ssid-select') as HTMLSelectElement;
    expect(sel.value).toBe('3');
    expect(Array.from(sel.options).map((o) => o.value)).toEqual(
      ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '10', '11', '12', '13', '14', '15'],
    );
    // Callsign chip is rendered alongside the picker, bare (no `-3`).
    expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent(/^N7CPZ$/);
    // Confirm the `<base>-<N>` formatted call is NOT present anywhere as
    // text within the callsign cell — that was the round-3 regression.
    expect(screen.queryByText('N7CPZ-3')).toBeNull();
  });

  it('each picker option label is just `-N` (no callsign prefix) (tuxlink-i63g)', () => {
    // Operator smoke 2026-05-31 round 4 (tuxlink-i63g): option labels
    // must be JUST the SSID (`-0`..`-15`), not the full call. The bare
    // form keeps the picker narrow enough that the WebKitGTK popup
    // scrollbar gutter does not visually clip the second digit of `-10`
    // through `-15`.
    render(<DashboardRibbon data={makeData({ callsign: 'W7CPZ' })} ssid={0} onSsidChange={() => {}} />);
    const sel = screen.getByTestId('ribbon-ssid-select') as HTMLSelectElement;
    const labels = Array.from(sel.options).map((o) => o.textContent);
    expect(labels).toEqual([
      '-0', '-1', '-2', '-3', '-4', '-5', '-6', '-7',
      '-8', '-9', '-10', '-11', '-12', '-13', '-14', '-15',
    ]);
  });

  it('does not render the SSID select when callsign is empty (pre-wizard)', () => {
    // Matches the prior "no dangling dash" behavior: don't render an empty
    // or broken select before the operator has set a callsign.
    render(<DashboardRibbon data={makeData({ callsign: '' })} ssid={0} onSsidChange={() => {}} />);
    expect(screen.queryByTestId('ribbon-ssid-select')).toBeNull();
  });

  it('fires onSsidChange when the operator selects a new SSID', () => {
    const onSsidChange = vi.fn();
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} ssid={0} onSsidChange={onSsidChange} />);
    fireEvent.change(screen.getByTestId('ribbon-ssid-select'), { target: { value: '10' } });
    expect(onSsidChange).toHaveBeenCalledWith(10);
  });
});
