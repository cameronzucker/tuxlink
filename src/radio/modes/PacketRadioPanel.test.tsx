// src/radio/modes/PacketRadioPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import type { FavoriteDial } from '../../favorites/types';
import { emitGatewayPrefill } from '../../favorites/prefillEvent';
import { writeLastTarget } from '../../connections/connectDispatch';
import { PacketRadioPanel } from './PacketRadioPanel';

// The panel now mounts FavoritesTabs/useFavorites (react-query), so every
// render must be wrapped in a QueryClientProvider or the queries throw
// "No QueryClient set". retry:false keeps a rejected favorites read from
// retrying through the test. (Mirrors the ARDOP test's renderPanel.)
const renderPanel = (ui: ReactElement) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
};

// The hand-entry To input + relays UI + path preview now live in the
// FavoritesTabs "Manual" tab (Task B6-PACKET). Radix Tabs.Trigger switches
// on mouseDown (button 0) under jsdom, not click. Tests that need the To
// input / relays call this to switch to the Manual tab first. (The Start
// button stays OUTSIDE the tabs, so it remains directly reachable.)
const switchToManualTab = async () => {
  const manual = await screen.findByRole('tab', { name: 'Manual' });
  fireEvent.mouseDown(manual, { button: 0 });
};

// Tauri IPC mocks. `invoke` returns command-specific defaults; `listen`
// resolves to a no-op unlisten so useSessionLog cleanup runs cleanly
// (matches TelnetRadioPanel.test idiom; we don't dispatch synthetic log
// events in this suite, so no handler-capture is needed).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

const DEFAULT_CONFIG = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'Tcp',
  tcpHost: '127.0.0.1',
  tcpPort: 8001,
  serialDevice: null,
  serialBaud: null,
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};

// Default invoke implementation — applied per-test in beforeEach so a test
// that overrides via mockImplementation cannot leak into the next test.
// ALL custom mockImplementations fall through to this so the mounted
// FavoritesTabs/useFavorites queries (favorites_read / favorites_recents /
// position_current_fix / favorite_tod_hint) resolve to benign shapes — a
// query that returned undefined would emit react-query's "Query data cannot
// be undefined" console error.
const defaultInvokeImpl = async (cmd: string, _args?: unknown) => {
  if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
  if (cmd === 'session_log_snapshot') return [];
  // Listener defaults (tuxlink-7vea backend default flip).
  if (cmd === 'packet_allowed_stations_get') {
    return { allow_all: true, callsigns: [] };
  }
  // Favorites surface (Task B6-PACKET). The mounted FavoritesTabs/useFavorites
  // issue these reads; return empty/benign shapes so the queries RESOLVE
  // (rejecting or returning undefined would noisily fail in jsdom). Tests that
  // need a clickable favorite override favorites_read / favorites_recents.
  if (cmd === 'favorites_read') {
    return { schema_version: 1, favorites: [], log: [] };
  }
  if (cmd === 'favorites_recents') return [];
  if (cmd === 'position_current_fix') return { grid: null };
  if (cmd === 'favorite_tod_hint') return null;
  return undefined;
};

describe('<PacketRadioPanel>', () => {
  beforeEach(async () => {
    // tuxlink-ypz3 (3a): the panel now restores its target from
    // localStorage['tuxlink.lastTarget.packet'] on mount, and prefill tests
    // write that key — clear it so a persisted target can't leak across tests.
    localStorage.clear();
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  it('tuxlink-ypz3 (3a): restores the persisted target on mount', async () => {
    // Simulate a prior session that dialed N0CALL-7 (ribbon Connect / panel edit
    // both persist here). A fresh mount must repopulate the visible input rather
    // than blanking it — the "previously called station cleared on mode switch"
    // regression.
    writeLastTarget('packet', 'N0CALL-7');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await switchToManualTab();
    const input = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe('N0CALL-7'));
  });

  it('renders the Packet Winlink panel title for intent=cms', () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Packet Winlink');
  });

  it('renders the Packet P2P panel title for intent=p2p', () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Packet P2P');
  });

  it('renders the ModemLinkSection', async () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
    });
  });

  it('renders the SessionLog section', () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('shows Listen action for intent=p2p', async () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('packet-listen-btn')).toBeInTheDocument();
    });
  });

  it('hides Listen action for intent=cms (cms-gateway is connect-only)', async () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('packet-listen-btn')).not.toBeInTheDocument();
  });

  it('shows effective callsign (base-SSID) from config_get', async () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('packet-effective-call')).toHaveTextContent('N7CPZ-7');
    });
  });

  it('first Stop press fires a GRACEFUL cms_disconnect (DISC to remote — tuxlink-avu9)', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    const stop = await screen.findByTestId('packet-stop-btn');
    fireEvent.click(stop);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('cms_disconnect');
    });
    // First press must NOT have rudely hard-aborted.
    expect(invoke).not.toHaveBeenCalledWith('cms_abort');
    expect(screen.getByTestId('packet-stop-btn')).toHaveTextContent('Force stop');
  });

  it('second Stop press escalates to a hard cms_abort (force-kill — tuxlink-avu9)', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    const stop = await screen.findByTestId('packet-stop-btn');
    fireEvent.click(stop); // graceful
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('cms_disconnect'));
    fireEvent.click(await screen.findByTestId('packet-stop-btn')); // force
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('cms_abort');
    });
  });

  it('exposes Stop in p2p intent too (a session can always be halted)', async () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    expect(await screen.findByTestId('packet-stop-btn')).toBeInTheDocument();
  });

  it('clicking Connect fires packet_connect with the typed call sign and empty path', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // The To input now lives in the Manual tab.
    await switchToManualTab();
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('packet-target-input'), { target: { value: 'W7RPT' } });
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('packet_connect', { call: 'W7RPT', path: [] });
    });
  });

  it('clicking Connect with a relay path fires packet_connect with that path', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await switchToManualTab();
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    fireEvent.change(screen.getByTestId('packet-target-input'), { target: { value: 'W7RPT' } });
    fireEvent.click(screen.getByTestId('packet-add-relay'));
    fireEvent.change(screen.getByTestId('packet-relay-0'), { target: { value: 'W7XYZ-1' } });
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('packet_connect', {
        call: 'W7RPT',
        path: ['W7XYZ-1'],
      });
    });
  });

  it('clicking Connect with empty target does NOT fire packet_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await switchToManualTab();
    await waitFor(() => expect(screen.getByTestId('packet-target-input')).toBeInTheDocument());
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByTestId('packet-start-btn'));
    // Sift: no call to packet_connect among any invocations.
    const calls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c: unknown[]) => c[0] === 'packet_connect',
    );
    expect(calls).toHaveLength(0);
  });

  it('clicking Listen (intent=p2p) fires packet_listen', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('packet-listen-btn')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('packet-listen-btn'));
    expect(invoke).toHaveBeenCalledWith('packet_listen');
  });

  it('changing SSID persists the new config via packet_config_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // The SSID handler short-circuits when `config` is null; wait for the
    // mock packet_config_get response to land in component state BEFORE
    // firing the change. The select renders the loaded DEFAULT_CONFIG.ssid
    // (0) so we wait for that value to appear. Without this wait the test
    // races on CI (passes locally on the Pi where the microtask queue
    // drains faster).
    await waitFor(() => {
      // DEFAULT_CONFIG.ssid === 7; the select renders 0 before the mock
      // resolves, then 7 once setConfig fires. Waiting for 7 ensures
      // `config` is non-null when fireEvent.change runs.
      const sel = screen.getByTestId('packet-ssid-select') as HTMLSelectElement;
      expect(sel.value).toBe('7');
    });
    fireEvent.change(screen.getByTestId('packet-ssid-select'), { target: { value: '10' } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'packet_config_set',
        expect.objectContaining({ dto: expect.objectContaining({ ssid: 10 }) }),
      );
    });
  });

  it('switching modem segment to USB without a device does NOT persist an incomplete link (tuxlink-614x)', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // Wait for config to load into component state before firing the click.
    await waitFor(() => {
      const sel = screen.getByTestId('packet-ssid-select') as HTMLSelectElement;
      expect(sel.value).toBe('7');
    });
    await waitFor(() => expect(screen.getByTestId('modem-seg-usb')).toBeInTheDocument());
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    // The segment switches locally so the device picker appears, but persisting a
    // null-device Serial link would be rejected by the backend and the
    // usePacketConfig rollback would snap the segment back — so NO packet_config_set
    // fires until a device is actually chosen (tuxlink-614x).
    expect(screen.getByTestId('modem-seg-usb')).toHaveAttribute('aria-pressed', 'true');
    expect(invoke).not.toHaveBeenCalledWith('packet_config_set', expect.anything());
  });

  // ── Listener allowed-stations editor (tuxlink-7vea) ──────────────────────
  //
  // The packet Listen section now carries an "Allowed stations" expander
  // for callsign curation (spec §1.3). AX.25 has no IP layer so the IP row
  // is hidden — only the callsign chip-row + allow-any toggle are present.

  it('renders the allowed-stations expander for intent=p2p', async () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    expect(
      await screen.findByTestId('packet-allowed-expander'),
    ).toBeInTheDocument();
  });

  it('does NOT render allowed-stations expander for intent=cms', async () => {
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('packet-allowed-expander')).not.toBeInTheDocument();
  });

  it('allowed-stations count chip shows allow-any default', async () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-count')).toHaveTextContent(/allow any/),
    );
  });

  it('Allow-any-peer toggle fires packet_allowed_stations_set_allow_all', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-allow-all-toggle')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-allow-all-toggle'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'packet_allowed_stations_set_allow_all',
        { allowAll: false },
      );
    });
  });

  it('adding a callsign fires packet_allowed_stations_add', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-callsign-add-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-callsign-add-btn'));
    const input = await screen.findByTestId('packet-allowed-callsign-add-input');
    fireEvent.change(input, { target: { value: 'w7aux' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'packet_allowed_stations_add',
        { callsign: 'W7AUX' },
      );
    });
  });

  it('removing a callsign fires packet_allowed_stations_remove', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'packet_allowed_stations_get') {
        return { allow_all: false, callsigns: ['W7AUX'] };
      }
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-callsign-remove-W7AUX')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-callsign-remove-W7AUX'));
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith(
        'packet_allowed_stations_remove',
        { callsign: 'W7AUX' },
      );
    });
  });

  it('Packet allowed-stations editor does NOT render an IP row', async () => {
    renderPanel(<PacketRadioPanel intent="p2p" baseCall="N7CPZ" onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('packet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('packet-allowed-callsign-row')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('packet-allowed-ip-row')).not.toBeInTheDocument();
  });

  it('falls back to defaults when packet_config_get rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'packet_config_get') throw new Error('NotConfigured');
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // A rejected config is the pre-wizard / unconfigured case → the panel
    // defaults to the Managed connection path (the recommended accessibility
    // route), so the managed section renders rather than the BYO modem link.
    await waitFor(() => {
      expect(screen.getByTestId('managed-modem-section')).toBeInTheDocument();
    });
  });

  // ── Favorites integration (Task B6-PACKET) ───────────────────────────────
  //
  // RADIO-1 + H4 + M4. A favorite's Connect PRE-FILLS the target only (never
  // transmits). Packet's `packet_connect` is a BLOCKING connect→B2F, so the
  // honest signal is the resolve/reject of that single call: `reached` is
  // recorded on resolve, `failed` is recorded in the CATCH (never finally). The
  // record timestamp carries a UTC offset (M4 / H1).

  const findRecordCalls = (invokeMock: ReturnType<typeof vi.fn>) =>
    invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_record_attempt');

  describe('Favorites integration (B6-PACKET)', () => {
    it('records reached when packet_connect resolves', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'packet_connect') return null; // resolves = on-air reach
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      await switchToManualTab();
      const target = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
      fireEvent.change(target, { target: { value: 'W7RPT' } });
      fireEvent.click(screen.getByTestId('packet-start-btn'));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [
          string,
          { dial: FavoriteDial; outcome: string },
        ];
        expect(args.outcome).toBe('reached');
        expect(args.dial.gateway).toBe('W7RPT');
        expect(args.dial.mode).toBe('packet');
      });
    });

    it('records failed when packet_connect rejects (H4 — failure now observable)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'packet_connect') throw new Error('no answer');
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      await switchToManualTab();
      const target = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
      fireEvent.change(target, { target: { value: 'W7RPT' } });
      fireEvent.click(screen.getByTestId('packet-start-btn'));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [
          string,
          { dial: FavoriteDial; outcome: string },
        ];
        expect(args.outcome).toBe('failed');
        expect(args.dial.gateway).toBe('W7RPT');
        expect(args.dial.mode).toBe('packet');
      });
    });

    it('empty target records nothing (pre-air guard precedes the record path)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      await switchToManualTab();
      await waitFor(() =>
        expect(screen.getByTestId('packet-target-input')).toBeInTheDocument(),
      );
      invokeMock.mockClear();
      fireEvent.click(screen.getByTestId('packet-start-btn'));
      await new Promise((r) => setTimeout(r, 20));
      expect(findRecordCalls(invokeMock)).toHaveLength(0);
      // The pre-air guard also means no connect fired.
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'packet_connect')).toBe(false);
    });

    it('CONSENT NON-BYPASS (M13): a favorite Connect pre-fills only, never transmits', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Route a starred packet favorite so the Favorites tab has a row.
      const fav = {
        id: 'fav-1',
        mode: 'packet' as const,
        gateway: 'W7RPT-1',
        band: '2m',
        starred: true,
        created_at: '2026-06-08T00:00:00-07:00',
        updated_at: '2026-06-08T00:00:00-07:00',
      };
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'favorites_read') {
          return { schema_version: 1, favorites: [fav], log: [] };
        }
        if (cmd === 'favorites_recents') return [];
        if (cmd === 'packet_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);

      // Default tab is Favorites; the favorite's Connect appears there.
      const connectBtn = await screen.findByTestId('favorite-connect-fav-1');
      fireEvent.click(connectBtn);
      // Let any (forbidden) async invoke settle.
      await new Promise((r) => setTimeout(r, 20));

      // RADIO-1: the prefill must NOT have fired packet_connect.
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'packet_connect'),
      ).toBe(false);

      // Prefill worked: the Manual tab's To input now holds the gateway.
      await switchToManualTab();
      const target = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
      expect(target.value).toBe('W7RPT-1');

      // Consent gate intact: clicking Start NOW invokes packet_connect.
      fireEvent.click(screen.getByTestId('packet-start-btn'));
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'packet_connect',
          expect.objectContaining({ call: 'W7RPT-1' }),
        );
      });
    });

    it('station-picker prefill event fills the packet target without transmitting', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);

      act(() => {
        emitGatewayPrefill({
          mode: 'packet',
          gateway: 'W6ABC-10',
          freq: '14.105',
          grid: 'CN87',
        });
      });

      await switchToManualTab();
      const target = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
      expect(target.value).toBe('W6ABC-10');
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'packet_connect'),
      ).toBe(false);
    });

    it('records an offset-bearing ts_local (M4) — not a UTC Z timestamp', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'packet_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      await switchToManualTab();
      const target = (await screen.findByTestId('packet-target-input')) as HTMLInputElement;
      fireEvent.change(target, { target: { value: 'W7RPT' } });
      fireEvent.click(screen.getByTestId('packet-start-btn'));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls.length).toBeGreaterThanOrEqual(1);
        const [, args] = calls[0] as [string, { tsLocal: string }];
        // camelCase wire key + offset-bearing (±HH:MM), never Z.
        expect(typeof args.tsLocal).toBe('string');
        expect(args.tsLocal).toMatch(/[+-]\d{2}:\d{2}$/);
        expect(args.tsLocal.endsWith('Z')).toBe(false);
      });
    });
  });

  // ── Managed modem (tuxlink-yq3l P7) ──────────────────────────────────────
  //
  // The managed connection path is the accessibility payoff: the operator picks
  // a sound card + PTT from dropdowns (packet_list_audio_devices) and never
  // authors a Dire Wolf .conf. Selecting a card + PTT persists
  // linkKind:'Managed' + managedAudioDevice (the stableId) + managedPtt.
  describe('Managed modem (P7)', () => {
    // Two fake managed devices: a DigiRig (serial-RTS PTT) and a DRA-100
    // (CM108-HID PTT) — the wire shapes mirror the Rust ManagedAudioDeviceDto +
    // internally-tagged PttChoice.
    const FAKE_DEVICES = [
      {
        humanName: 'C-Media USB Audio Device (DigiRig)',
        alsaPlughw: 'plughw:CARD=Device,DEV=0',
        stableId: { kind: 'byIdSymlink', value: 'usb-C-Media_DigiRig_Audio-00' },
        pttCandidates: [{ kind: 'serialRts', tty: '/dev/ttyUSB0' }],
      },
      {
        humanName: 'C-Media USB Audio Device (DRA-100)',
        alsaPlughw: 'plughw:CARD=DRA,DEV=0',
        stableId: { kind: 'byIdSymlink', value: 'usb-C-Media_DRA-100_CM119A-01' },
        pttCandidates: [
          { kind: 'cm108Hid', hidrawPath: '/dev/hidraw3' },
          { kind: 'serialRts', tty: '/dev/ttyUSB1' },
        ],
      },
    ];

    // A config with no link yet (fresh/unconfigured) → the panel defaults to
    // Managed. Reuses DEFAULT_CONFIG's params with linkKind nulled.
    const UNCONFIGURED = { ...DEFAULT_CONFIG, linkKind: null };

    const withDevices = (
      configForGet: unknown,
      extra?: (cmd: string, args?: unknown) => unknown,
    ) => async (cmd: string, args?: unknown) => {
      if (cmd === 'packet_config_get') return configForGet;
      if (cmd === 'packet_list_audio_devices') return FAKE_DEVICES;
      const e = extra?.(cmd, args);
      if (e !== undefined) return e;
      return defaultInvokeImpl(cmd, args);
    };

    it('defaults a fresh (unconfigured) panel to Managed', async () => {
      const core = await import('@tauri-apps/api/core');
      (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
        withDevices(UNCONFIGURED),
      );
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      // Managed section shows; BYO modem-link section does not.
      expect(await screen.findByTestId('managed-modem-section')).toBeInTheDocument();
      expect(screen.queryByTestId('modem-link-section')).not.toBeInTheDocument();
      // The Managed toggle is the active one.
      expect(screen.getByTestId('packet-conn-managed')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });

    it('does NOT clobber an existing Tcp config — shows BYO selected', async () => {
      // DEFAULT_CONFIG has linkKind:'Tcp' → BYO must be the active mode.
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      expect(await screen.findByTestId('modem-link-section')).toBeInTheDocument();
      expect(screen.queryByTestId('managed-modem-section')).not.toBeInTheDocument();
      expect(screen.getByTestId('packet-conn-byo')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });

    it('selecting a device + its default PTT persists linkKind:Managed + stableId + managedPtt', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(withDevices(UNCONFIGURED));
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      // Wait for the device dropdown to populate from packet_list_audio_devices.
      const select = (await screen.findByTestId(
        'managed-device-select',
      )) as HTMLSelectElement;
      // Pick the DRA-100 (CM108 HID is its ranked-first PTT).
      fireEvent.change(select, {
        target: { value: 'byIdSymlink:usb-C-Media_DRA-100_CM119A-01' },
      });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'packet_config_set',
          expect.objectContaining({
            dto: expect.objectContaining({
              linkKind: 'Managed',
              managedAudioDevice: {
                kind: 'byIdSymlink',
                value: 'usb-C-Media_DRA-100_CM119A-01',
              },
              // The ranked-first candidate is the default on device-select.
              managedPtt: { kind: 'cm108Hid', hidrawPath: '/dev/hidraw3' },
            }),
          }),
        );
      });
    });

    it('overriding the PTT persists the chosen managedPtt for the selected device', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Start with the DRA-100 already persisted (its default PTT is the HID).
      const PRECONFIGURED = {
        ...DEFAULT_CONFIG,
        linkKind: 'Managed',
        managedAudioDevice: { kind: 'byIdSymlink', value: 'usb-C-Media_DRA-100_CM119A-01' },
        managedPtt: { kind: 'cm108Hid', hidrawPath: '/dev/hidraw3' },
      };
      invokeMock.mockImplementation(withDevices(PRECONFIGURED));
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      const pttSelect = (await screen.findByTestId(
        'managed-ptt-select',
      )) as HTMLSelectElement;
      invokeMock.mockClear();
      // Override to the serial-RTS alternative on the DRA-100.
      fireEvent.change(pttSelect, { target: { value: 'serialRts:/dev/ttyUSB1' } });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'packet_config_set',
          expect.objectContaining({
            dto: expect.objectContaining({
              linkKind: 'Managed',
              managedPtt: { kind: 'serialRts', tty: '/dev/ttyUSB1' },
            }),
          }),
        );
      });
    });

    it('renders the empty-list affordance + Refresh when no sound card is detected', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Managed mode (unconfigured) but packet_list_audio_devices returns [].
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'packet_config_get') return UNCONFIGURED;
        if (cmd === 'packet_list_audio_devices') return [];
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      expect(await screen.findByTestId('managed-no-devices')).toBeInTheDocument();
      // The Refresh affordance is present (no dead-end).
      expect(screen.getByTestId('managed-refresh')).toBeInTheDocument();
      // No device dropdown when the list is empty.
      expect(screen.queryByTestId('managed-device-select')).not.toBeInTheDocument();
    });

    it('Refresh re-calls packet_list_audio_devices and recovers from an empty first read', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      let firstCall = true;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'packet_config_get') return UNCONFIGURED;
        if (cmd === 'packet_list_audio_devices') {
          // First read: empty (nothing plugged in). Subsequent: devices present.
          if (firstCall) {
            firstCall = false;
            return [];
          }
          return FAKE_DEVICES;
        }
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      // First render: empty-list affordance.
      const refresh = await screen.findByTestId('managed-refresh');
      fireEvent.click(refresh);
      // After Refresh the device dropdown appears.
      expect(await screen.findByTestId('managed-device-select')).toBeInTheDocument();
    });

    it('shows the effective callsign read-only in the managed section', async () => {
      const core = await import('@tauri-apps/api/core');
      (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
        withDevices(UNCONFIGURED),
      );
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      const call = (await screen.findByTestId(
        'managed-effective-call',
      )) as HTMLInputElement;
      // baseCall N7CPZ + DEFAULT_CONFIG.ssid 7 → N7CPZ-7, read-only.
      // The input mounts with the ssid-0 default (config?.ssid ?? 0) and updates
      // to ssid 7 once packet_config_get resolves, so wait for the settled value
      // rather than reading synchronously — otherwise the assertion races the
      // async config load (flaked on faster CI runners: N7CPZ-0 vs N7CPZ-7).
      await waitFor(() => expect(call.value).toBe('N7CPZ-7'));
      expect(call).toHaveAttribute('readonly');
    });

    it('toggling BYO → Managed shows the managed picker (operator can switch)', async () => {
      const core = await import('@tauri-apps/api/core');
      (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
        // DEFAULT_CONFIG (Tcp) → starts on BYO; provide devices for the switch.
        withDevices(DEFAULT_CONFIG),
      );
      renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
      expect(await screen.findByTestId('modem-link-section')).toBeInTheDocument();
      fireEvent.click(screen.getByTestId('packet-conn-managed'));
      expect(await screen.findByTestId('managed-modem-section')).toBeInTheDocument();
      expect(screen.queryByTestId('modem-link-section')).not.toBeInTheDocument();
    });

    it('re-seeds its config when a same-window packet-config change is broadcast (B3)', async () => {
      // tuxlink-hoi1 B3: the panel held a private snapshot loaded once and never
      // re-synced, so a link/SSID change made elsewhere (the APRS strip) left it
      // stale — a later panel write then clobbered the link from the frozen
      // snapshot. It must re-seed from the same-window broadcast.
      renderPanel(<PacketRadioPanel intent="p2p" baseCall="W7ABC" onClose={() => {}} />);
      const select = (await screen.findByTestId('packet-ssid-select')) as HTMLSelectElement;
      await waitFor(() => expect(select.value).toBe('7'));
      act(() => {
        window.dispatchEvent(
          new CustomEvent('tuxlink:packet-config:change', {
            detail: { ...DEFAULT_CONFIG, ssid: 3 },
          }),
        );
      });
      await waitFor(() => expect(select.value).toBe('3'));
    });
  });
});
