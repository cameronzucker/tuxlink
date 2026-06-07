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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  render as rtlRender,
  screen,
  fireEvent,
  waitFor,
  type RenderOptions,
} from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DashboardRibbon } from './DashboardRibbon';
import type { StatusBarData, StatusTone } from './useStatus';
import type { PacketUiState } from '../packet/packetStatus';

// Task 14 (tuxlink-c79g): the optimistic-update tests below exercise the
// invoke('config_set_grid') and invoke('position_set_source') write paths and
// assert that queryClient.invalidateQueries({ queryKey: ['config_read'] }) is
// called after each. The earlier tests don't drive write paths so they didn't
// need a tauri-core mock; the Task 14 tests add one here.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
import { invoke } from '@tauri-apps/api/core';

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

// Task 14 wired `useQueryClient` into DashboardRibbon, so EVERY test now needs
// a QueryClientProvider in scope. Wrap the testing-library render so each test
// gets its own fresh QueryClient (no cross-test cache leakage). Tests that
// need a handle on the client (the optimistic-update tests below) bypass this
// helper and instantiate the client + spy directly.
function render(ui: React.ReactElement, options?: RenderOptions) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return rtlRender(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>, options);
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
// tuxlink-9osg: UTC/local clock derives local timezone from live/config grid
// ---------------------------------------------------------------------------

describe('<DashboardRibbon> — grid-derived local clock (tuxlink-9osg)', () => {
  it('uses the ribbon grid to choose the local timezone', () => {
    render(<DashboardRibbon data={makeData({ grid: 'DM33' })} />);
    const el = screen.getByTestId('ribbon-time');
    expect(el).toHaveAttribute('data-time-source', 'grid');
    expect(el).toHaveAttribute('title', expect.stringContaining('DM33'));
    expect(el).toHaveAttribute('title', expect.stringContaining('America/Phoenix'));
  });

  it('falls back to the device timezone when no grid is available', () => {
    render(<DashboardRibbon data={makeData({ grid: null })} />);
    const el = screen.getByTestId('ribbon-time');
    expect(el).toHaveAttribute('data-time-source', 'device');
    expect(el).toHaveAttribute('title', expect.stringContaining('No grid'));
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

// ---------------------------------------------------------------------------
// Task 14 (tuxlink-c79g): optimistic config_read refresh after grid + source
// writes. Per spec §4.3 + Codex P1 #4, both onCommit (config_set_grid) and
// onUseGps (position_set_source) MUST call queryClient.invalidateQueries({
// queryKey: ['config_read'] }) after the invoke resolves so the source chip's
// `source` value flips within one render cycle instead of waiting up to 5s
// for the next config poll.
// ---------------------------------------------------------------------------

describe('DashboardRibbon — optimistic config_read refresh (tuxlink-c79g T14)', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  function renderWithClient(ui: React.ReactElement) {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
    const utils = render(
      <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
    );
    return { ...utils, queryClient, invalidateSpy };
  }

  it('invalidates [config_read] within one render cycle after config_set_grid resolves', async () => {
    // Start in Manual + no GPS fix so the grid value button is rendered (State 1).
    const data = makeData({ position_source: 'Manual', grid: 'DN31', gpsReady: false });
    const { invalidateSpy } = renderWithClient(<DashboardRibbon data={data} />);

    // Enter inline edit, type a new grid, press Enter to commit.
    fireEvent.click(screen.getByTestId('grid-value-display'));
    const input = screen.getByTestId('grid-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'EM75' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    // The wrapped onCommit awaits invoke('config_set_grid', ...) then calls
    // invalidateQueries. Wait for the assertion to satisfy the async edge.
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('config_set_grid', { grid: 'EM75' });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['config_read'] });
    });
  });

  it('invalidates [config_read] within one render cycle after position_set_source resolves', async () => {
    // Source = Manual + gpsReady so the GPS segment is the click target that
    // fires onUseGps → invoke('position_set_source', { source: 'Gps' }).
    // tuxlink-z5pz: testid renamed from source-chip to source-segment-gps.
    const data = makeData({ position_source: 'Manual', grid: 'DN31', gpsReady: true });
    const { invalidateSpy } = renderWithClient(<DashboardRibbon data={data} />);

    fireEvent.click(screen.getByTestId('source-segment-gps'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('position_set_source', { source: 'Gps' });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['config_read'] });
    });
  });
});
