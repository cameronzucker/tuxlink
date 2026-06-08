// src/radio/modes/PacketRadioPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import type { FavoriteDial } from '../../favorites/types';
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
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
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

  it('switching modem segment (TCP → USB) persists via packet_config_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<PacketRadioPanel intent="cms" baseCall="N7CPZ" onClose={() => {}} />);
    // Same race as the SSID test — wait for config to be loaded into
    // component state before firing the click (the handler short-circuits
    // when `config` is null).
    await waitFor(() => {
      const sel = screen.getByTestId('packet-ssid-select') as HTMLSelectElement;
      expect(sel.value).toBe('7');
    });
    await waitFor(() => expect(screen.getByTestId('modem-seg-usb')).toBeInTheDocument());
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'packet_config_set',
        expect.objectContaining({
          dto: expect.objectContaining({ linkKind: 'Serial' }),
        }),
      );
    });
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
    // Panel still renders the modem section using fallback defaults.
    await waitFor(() => {
      expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
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
});
