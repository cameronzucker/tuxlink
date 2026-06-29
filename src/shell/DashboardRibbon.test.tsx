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
import type { ActiveIdentityDto, IdentityListDto } from './identityTypes';
import { EGRESS_STATUS_DISARMED } from '../security/egressTypes';

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

  it('renders Connection inside a stable slot with an ellipsizable label span (tuxlink-a8x6)', () => {
    const longLabel = 'Error: RMS gateway refused authentication after an unusually long diagnostic string';
    render(
      <DashboardRibbon
        data={makeData({
          state: { label: 'Error', tone: 'error' },
          connection: longLabel,
        })}
      />,
    );
    const el = screen.getByTestId('ribbon-connection');
    expect(el.closest('.dash-item')).toHaveClass('dash-item--connection');
    expect(el).toHaveAttribute('title', longLabel);
    const text = el.querySelector('.dash-connection-text');
    expect(text).not.toBeNull();
    expect(text).toHaveTextContent(longLabel);
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
// bd-tuxlink-y8tf: the SSID select was removed from the ribbon callsign chip —
// SSID is now set per-transport in the AX.25 / APRS panes. The ribbon shows the
// bare callsign only and never an SSID picker.
describe('DashboardRibbon — no SSID on the callsign chip (bd-tuxlink-y8tf)', () => {
  it('renders the bare callsign and never an SSID select', () => {
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} />);
    expect(screen.getByTestId('ribbon-callsign')).toHaveTextContent('N7CPZ');
    expect(screen.queryByTestId('ribbon-ssid-select')).toBeNull();
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

// ---------------------------------------------------------------------------
// tuxlink-pmp5: the inline "On connect" control (Review | Download all). Renders
// only when onReviewInboundChange is supplied, reflects reviewInbound (null/true
// ⇒ Review active — the default; false ⇒ Download all), and persists the choice
// through the handler. Moved here out of the GPS/Privacy settings modal.
// ---------------------------------------------------------------------------

describe('DashboardRibbon — On connect review/download-all control (tuxlink-pmp5)', () => {
  it('is not rendered when onReviewInboundChange is omitted', () => {
    render(<DashboardRibbon data={makeData()} />);
    expect(screen.queryByTestId('ribbon-review-inbound')).not.toBeInTheDocument();
  });

  it('renders Review active by default (reviewInbound null = not yet loaded)', () => {
    render(<DashboardRibbon data={makeData()} reviewInbound={null} onReviewInboundChange={vi.fn()} />);
    expect(screen.getByTestId('review-inbound-review')).toHaveClass('active');
    expect(screen.getByTestId('review-inbound-review')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('review-inbound-download-all')).not.toHaveClass('active');
  });

  it('renders Review active when reviewInbound is true', () => {
    render(<DashboardRibbon data={makeData()} reviewInbound onReviewInboundChange={vi.fn()} />);
    expect(screen.getByTestId('review-inbound-review')).toHaveClass('active');
    expect(screen.getByTestId('review-inbound-download-all')).not.toHaveClass('active');
  });

  it('renders Download all active when reviewInbound is false', () => {
    render(<DashboardRibbon data={makeData()} reviewInbound={false} onReviewInboundChange={vi.fn()} />);
    expect(screen.getByTestId('review-inbound-download-all')).toHaveClass('active');
    expect(screen.getByTestId('review-inbound-download-all')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('review-inbound-review')).not.toHaveClass('active');
  });

  it('calls onReviewInboundChange(false) when Download all is clicked', () => {
    const onChange = vi.fn();
    render(<DashboardRibbon data={makeData()} reviewInbound onReviewInboundChange={onChange} />);
    fireEvent.click(screen.getByTestId('review-inbound-download-all'));
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it('calls onReviewInboundChange(true) when Review is clicked', () => {
    const onChange = vi.fn();
    render(<DashboardRibbon data={makeData()} reviewInbound={false} onReviewInboundChange={onChange} />);
    fireEvent.click(screen.getByTestId('review-inbound-review'));
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

// ---------------------------------------------------------------------------
// APRS status-strip control (entry ①) in the ribbon
// ---------------------------------------------------------------------------

describe('DashboardRibbon — APRS status control (plan 2, T4)', () => {
  it('renders the APRS status control and opens chat on click', () => {
    const onOpen = vi.fn();
    render(<DashboardRibbon data={makeData()} aprs={{ listening: true, unread: 1, onOpen }} />);
    const btn = screen.getByTestId('dash-aprs-control');
    // The "APRS" label is in the dash-label sibling, not inside the button —
    // check the containing dash-item instead.
    expect(btn.closest('.dash-aprs')).toHaveTextContent(/APRS/i);
    expect(screen.getByTestId('dash-aprs-unread')).toHaveTextContent('1');
    fireEvent.click(btn);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('does not render the APRS control when aprs prop is absent', () => {
    render(<DashboardRibbon data={makeData()} />);
    expect(screen.queryByTestId('dash-aprs-control')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Task 10 (tuxlink-noa0): the IdentitySwitcher mounts in the callsign slot when
// onSwitchIdentity (+ identities/activeIdentity) are supplied. When they are
// NOT, the ribbon keeps the legacy bare callsign-row markup (back-compat) —
// covered by every prop-free test above (which still pass unchanged).
// ---------------------------------------------------------------------------

describe('DashboardRibbon — IdentitySwitcher integration (Task 10, tuxlink-noa0)', () => {
  const ACTIVE: ActiveIdentityDto = {
    mycall: 'W7XYZ',
    address_as: 'W7XYZ',
    is_tactical: false,
  };
  const IDENTITIES: IdentityListDto = {
    full: [
      { callsign: 'W7XYZ', label: null, has_cms_account: true, cms_registered: true, needs_auth: false },
      { callsign: 'W1ABC', label: 'Club', has_cms_account: true, cms_registered: true, needs_auth: true },
    ],
    tactical: [{ label: 'EOC-3', parent: 'W1ABC', cms_badge: 'registered' }],
    last_selected: 'W7XYZ',
  };

  it('renders the IdentitySwitcher trigger when onSwitchIdentity is provided', () => {
    render(
      <DashboardRibbon
        data={makeData({ callsign: 'W7XYZ' })}
        identities={IDENTITIES}
        activeIdentity={ACTIVE}
        onSwitchIdentity={vi.fn()}
      />,
    );
    // The switcher owns the ribbon-callsign container (no SSID select — y8tf).
    expect(screen.getByTestId('ribbon-callsign')).toBeInTheDocument();
    expect(screen.getByTestId('identity-switcher-trigger')).toBeInTheDocument();
    expect(screen.queryByTestId('ribbon-ssid-select')).not.toBeInTheDocument();
    // Closed: dropdown absent.
    expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
    // No duplicate testids — exactly one ribbon-callsign in the document.
    expect(screen.getAllByTestId('ribbon-callsign')).toHaveLength(1);
  });

  it('opens the identity list when the trigger is clicked', () => {
    render(
      <DashboardRibbon
        data={makeData({ callsign: 'W7XYZ' })}
        identities={IDENTITIES}
        activeIdentity={ACTIVE}
        onSwitchIdentity={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('identity-switcher-trigger'));
    expect(screen.getByTestId('identity-switcher-list')).toBeInTheDocument();
    expect(screen.getByTestId('identity-row-full-W7XYZ')).toBeInTheDocument();
    expect(screen.getByTestId('identity-row-full-W1ABC')).toBeInTheDocument();
  });

  it('falls back to the legacy callsign markup when onSwitchIdentity is omitted', () => {
    // No switcher props → the bare callsign-row path renders the plain callsign.
    render(<DashboardRibbon data={makeData({ callsign: 'N7CPZ' })} />);
    expect(screen.queryByTestId('identity-switcher-trigger')).toBeNull();
    expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent(/^N7CPZ$/);
    expect(screen.queryByTestId('ribbon-ssid-select')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Merged Elmer × Agent-send control (one ribbon slot). Display-only: shows
// arm/taint state and opens the Elmer drawer on click. Arm/disarm/re-arm were
// relocated to the drawer header — the ribbon NO LONGER renders the egress
// popover (that's covered by ElmerPane.test.tsx now). Supersedes the separate
// launcher + arm chip (tuxlink-yfezs) that overflowed Connect at 1080p.
// ---------------------------------------------------------------------------

describe('<DashboardRibbon> — merged Elmer × Agent-send chip', () => {
  const ARMED = { armed: true, armedRemainingSecs: 300, tainted: false };
  const TAINTED = { armed: false, armedRemainingSecs: 0, tainted: true };
  const armEgress = (status: typeof ARMED) => ({ status, onArm: vi.fn(), onDisarm: vi.fn() });

  it('renders the launcher and calls onOpenElmer on click', () => {
    const onOpenElmer = vi.fn();
    render(<DashboardRibbon data={makeData()} onOpenElmer={onOpenElmer} />);
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));
    expect(onOpenElmer).toHaveBeenCalledTimes(1);
  });

  it('does not render the control when onOpenElmer is omitted', () => {
    render(<DashboardRibbon data={makeData()} egress={armEgress(ARMED)} />);
    expect(screen.queryByTestId('ribbon-elmer-launcher')).toBeNull();
  });

  it('reflects the open state via aria-pressed', () => {
    render(<DashboardRibbon data={makeData()} onOpenElmer={vi.fn()} elmerOpen />);
    expect(screen.getByTestId('ribbon-elmer-launcher')).toHaveAttribute('aria-pressed', 'true');
  });

  it('disarmed → reads "Elmer", no countdown', () => {
    render(<DashboardRibbon data={makeData()} onOpenElmer={vi.fn()} egress={armEgress(EGRESS_STATUS_DISARMED)} />);
    const chip = screen.getByTestId('ribbon-elmer-launcher');
    expect(chip).toHaveAttribute('data-mode', 'disarmed');
    expect(chip).toHaveTextContent('Elmer');
    expect(screen.queryByTestId('ribbon-elmer-countdown')).toBeNull();
  });

  it('armed → transforms to "Agent send" + a live countdown', () => {
    render(<DashboardRibbon data={makeData()} onOpenElmer={vi.fn()} egress={armEgress(ARMED)} />);
    const chip = screen.getByTestId('ribbon-elmer-launcher');
    expect(chip).toHaveAttribute('data-mode', 'armed');
    expect(chip).toHaveTextContent('Agent send');
    expect(screen.getByTestId('ribbon-elmer-countdown')).toBeInTheDocument();
  });

  it('tainted → reads LOCKED', () => {
    render(<DashboardRibbon data={makeData()} onOpenElmer={vi.fn()} egress={armEgress(TAINTED)} />);
    const chip = screen.getByTestId('ribbon-elmer-launcher');
    expect(chip).toHaveAttribute('data-mode', 'locked');
    expect(chip).toHaveTextContent('LOCKED');
  });

  it('still opens Elmer on click while armed (click ≠ arm/disarm)', () => {
    const onOpenElmer = vi.fn();
    render(<DashboardRibbon data={makeData()} onOpenElmer={onOpenElmer} egress={armEgress(ARMED)} />);
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));
    expect(onOpenElmer).toHaveBeenCalledTimes(1);
  });

  it('no longer renders the arm popover/presets in the ribbon (moved to the drawer)', () => {
    render(<DashboardRibbon data={makeData()} onOpenElmer={vi.fn()} egress={armEgress(EGRESS_STATUS_DISARMED)} />);
    expect(screen.queryByTestId('egress-chip')).toBeNull();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
  });
});
