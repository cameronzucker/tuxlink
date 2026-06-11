// src/radio/modes/TelnetPostOfficeRadioPanel.test.tsx
//
// Tests for TelnetPostOfficeRadioPanel (tuxlink-6c9y, Task B3) — the NEW
// Post Office radio panel. Structurally mirrors TelnetP2pRadioPanel.test.tsx:
//   - Same vi.mock structure for @tauri-apps/api/core + event.
//   - Fresh QueryClient per test so useMailbox('outbox') + useQueryClient resolve.
//   - defaultInvokeImpl + beforeEach reset pattern.
//
// The panel has two modes (props.mode):
//   - 'local'   → Telnet RMS Post Office (logs in as <base>-L; host:port only)
//   - 'network' → Network Post Office     (logs in as full callsign; +favorites)
//
// Tauri commands under test:
//   config_read()                                   → { callsign, grid }
//   mailbox_list({ folder: 'outbox' })              → MessageMeta[]  (Outbox source)
//   telnet_post_office_connect({ req: {...} })      → { sent_count, received_count, relay_state }
//     ^ Phase-C backend command (C1); relay_state is a kebab-case RelayStateDto value.
//   network_po_favorites_get()                      → RelayFavorite[]  (network only)
//   network_po_favorites_add(favorite)              → RelayFavorite[]
//   network_po_favorites_remove(host, port)         → RelayFavorite[]

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TelnetPostOfficeRadioPanel } from './TelnetPostOfficeRadioPanel';

// Two Outbox drafts the panel lists as a checklist. Keyed on `id` (MID).
const OUTBOX_FIXTURE = [
  {
    id: 'OUT-1',
    from: 'N0CALL@winlink.org',
    to: ['W7AUX@winlink.org'],
    subject: 'Memphis ARES sitrep',
    date: '2026-06-08T12:00:00.000Z',
    unread: false,
    bodySize: 612,
    hasAttachments: false,
  },
  {
    id: 'OUT-2',
    from: 'N0CALL@winlink.org',
    to: ['MEMPHIS-ARES@winlink.org'],
    subject: 'Net check-in',
    date: '2026-06-08T11:00:00.000Z',
    unread: false,
    bodySize: 388,
    hasAttachments: false,
  },
];

function renderPanel(props: {
  mode?: 'local' | 'network';
  onClose?: () => void;
} = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <TelnetPostOfficeRadioPanel
        mode={props.mode ?? 'local'}
        onClose={props.onClose ?? (() => {})}
      />
    </QueryClientProvider>,
  );
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// Default invoke implementation — applied per-test in beforeEach so an
// override in one test cannot leak into the next.
const defaultInvokeImpl = async (cmd: string) => {
  if (cmd === 'config_read') {
    return { callsign: 'N7CPZ-10', grid: 'CN87' };
  }
  if (cmd === 'session_log_snapshot') {
    return [];
  }
  if (cmd === 'mailbox_list') {
    return OUTBOX_FIXTURE;
  }
  if (cmd === 'network_po_favorites_get') {
    return [];
  }
  return undefined;
};

describe('<TelnetPostOfficeRadioPanel>', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  // ── Panel title (intent) ──────────────────────────────────────────────────

  it('local mode renders the "Telnet Post Office" panel title', () => {
    renderPanel({ mode: 'local' });
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Post Office');
  });

  it('network mode renders the "Telnet Network Post Office" panel title', () => {
    renderPanel({ mode: 'network' });
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent(
      'Telnet Network Post Office',
    );
  });

  // ── Outbox checklist + selection ──────────────────────────────────────────

  it('lists Outbox drafts from mailbox_list as a checklist', async () => {
    renderPanel({ mode: 'local' });
    expect(await screen.findByTestId('po-outbox-row-OUT-1')).toBeInTheDocument();
    expect(screen.getByTestId('po-outbox-row-OUT-2')).toBeInTheDocument();
    // Subject renders inside the row's label (alongside recipient + size).
    expect(screen.getByTestId('po-outbox-row-OUT-1')).toHaveTextContent('Memphis ARES sitrep');
  });

  it('select-all checks every Outbox row; select-none clears them', async () => {
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    const cb1 = screen.getByTestId('po-outbox-check-OUT-1') as HTMLInputElement;
    const cb2 = screen.getByTestId('po-outbox-check-OUT-2') as HTMLInputElement;
    expect(cb1.checked).toBe(false);
    expect(cb2.checked).toBe(false);

    fireEvent.click(screen.getByTestId('po-select-all'));
    expect(cb1.checked).toBe(true);
    expect(cb2.checked).toBe(true);

    fireEvent.click(screen.getByTestId('po-select-none'));
    expect(cb1.checked).toBe(false);
    expect(cb2.checked).toBe(false);
  });

  it('toggling a single row checkbox flips just that row', async () => {
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    const cb1 = screen.getByTestId('po-outbox-check-OUT-1') as HTMLInputElement;
    fireEvent.click(cb1);
    expect(cb1.checked).toBe(true);
    const cb2 = screen.getByTestId('po-outbox-check-OUT-2') as HTMLInputElement;
    expect(cb2.checked).toBe(false);
  });

  // ── Connect button label + enabled-state ──────────────────────────────────

  it('N=0 → Connect button is labelled "Connect" and stays ENABLED (receive-only)', async () => {
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    const connect = screen.getByTestId('po-connect-btn') as HTMLButtonElement;
    expect(connect).toHaveTextContent(/^Connect$/);
    expect(connect.disabled).toBe(false);
  });

  it('N>0 → Connect button label becomes "Connect & send {N}"', async () => {
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-outbox-check-OUT-1'));
    const connect = screen.getByTestId('po-connect-btn') as HTMLButtonElement;
    expect(connect).toHaveTextContent('Connect & send 1');
    fireEvent.click(screen.getByTestId('po-outbox-check-OUT-2'));
    expect(connect).toHaveTextContent('Connect & send 2');
  });

  // ── Network PO send-flow (tuxlink-b6ad): no checklist, drains like CMS ──────

  it('network mode does NOT render the Send-from-Outbox checklist', async () => {
    renderPanel({ mode: 'network' });
    await screen.findByTestId('po-network-send-note');
    expect(screen.queryByTestId('po-outbox-section')).toBeNull();
    expect(screen.queryByTestId('po-select-all')).toBeNull();
    expect(screen.queryByTestId('po-outbox-row-OUT-1')).toBeNull();
  });

  it('network mode Connect button is plain "Connect" (no per-message send count)', async () => {
    renderPanel({ mode: 'network' });
    await screen.findByTestId('po-network-send-note');
    expect(screen.getByTestId('po-connect-btn')).toHaveTextContent(/^Connect$/);
  });

  it('local mode still renders the Send-from-Outbox checklist (leakage guard kept)', async () => {
    renderPanel({ mode: 'local' });
    expect(await screen.findByTestId('po-outbox-section')).toBeInTheDocument();
    expect(screen.queryByTestId('po-network-send-note')).toBeNull();
  });

  // ── Connect action / invoke contract ──────────────────────────────────────

  it('Connect (local) fires telnet_post_office_connect with the { req } wrapper shape', async () => {
    const core = await import('@tauri-apps/api/core');
    let observedReq: Record<string, unknown> | null = null;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: unknown) => {
        if (cmd === 'telnet_post_office_connect') {
          observedReq = (args as { req: Record<string, unknown> }).req;
          return { sent_count: 1, received_count: 0, relay_state: 'not-relay' };
        }
        return defaultInvokeImpl(cmd);
      },
    );
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    // host defaults to 127.0.0.1:8772 (design §4.3).
    fireEvent.click(screen.getByTestId('po-outbox-check-OUT-1'));
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => expect(observedReq).not.toBeNull());
    expect(observedReq).toEqual({
      mode: 'local',
      host: '127.0.0.1',
      port: 8772,
      my_callsign: 'N7CPZ-10',
      locator: 'CN87',
      selected_mids: ['OUT-1'],
    });
  });

  it('Connect at N=0 sends an empty selected_mids array (receive-only)', async () => {
    const core = await import('@tauri-apps/api/core');
    let observedReq: Record<string, unknown> | null = null;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: unknown) => {
        if (cmd === 'telnet_post_office_connect') {
          observedReq = (args as { req: Record<string, unknown> }).req;
          return { sent_count: 0, received_count: 2, relay_state: 'not-relay' };
        }
        return defaultInvokeImpl(cmd);
      },
    );
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => expect(observedReq).not.toBeNull());
    expect(observedReq!.selected_mids).toEqual([]);
    expect(observedReq!.mode).toBe('local');
  });

  it('Connect (network) sends mode="network" in the req', async () => {
    const core = await import('@tauri-apps/api/core');
    let observedReq: Record<string, unknown> | null = null;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: unknown) => {
        if (cmd === 'telnet_post_office_connect') {
          observedReq = (args as { req: Record<string, unknown> }).req;
          return { sent_count: 0, received_count: 0, relay_state: 'not-relay' };
        }
        return defaultInvokeImpl(cmd);
      },
    );
    renderPanel({ mode: 'network' });
    // Network PO has no Outbox checklist (tuxlink-b6ad) — wait on the send note.
    await screen.findByTestId('po-network-send-note');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => expect(observedReq).not.toBeNull());
    expect(observedReq!.mode).toBe('network');
  });

  // ── Connect-error banner ──────────────────────────────────────────────────
  //
  // The Phase-C backend command telnet_post_office_connect is NOT yet wired, so
  // EVERY real Connect rejects until C1 lands. The error must reach the operator
  // via the inline po-error banner (mirrors the P2P panel's p2p-error path).

  it('Connect rejection surfaces the error string in the po-error banner', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        throw new Error('connect command not implemented');
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-error')).toHaveTextContent(
        'connect command not implemented',
      );
    });
  });

  // ── §5.9 relay-state banner ───────────────────────────────────────────────
  //
  // The relay-state banner (data-testid="po-relay-banner") appears after a
  // successful connect whenever relay_state is NOT 'not-relay'. It shows a
  // human-readable label from the RELAY_STATE_LABELS map. For 'not-relay' (an
  // ordinary CMS endpoint) no banner is rendered.

  it('relay_state "radio-network" → po-relay-banner renders the correct label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 0, received_count: 1, relay_state: 'radio-network' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-relay-banner')).toBeInTheDocument();
    });
    expect(screen.getByTestId('po-relay-banner')).toHaveTextContent('Radio network hub');
  });

  it('relay_state "local-database" → po-relay-banner renders the correct label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 0, received_count: 1, relay_state: 'local-database' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-relay-banner')).toBeInTheDocument();
    });
    expect(screen.getByTestId('po-relay-banner')).toHaveTextContent(
      'Local post office (holds mail locally)',
    );
  });

  it('relay_state "radio-network-and-internet" → po-relay-banner renders the correct label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 0, received_count: 0, relay_state: 'radio-network-and-internet' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    // Network PO has no Outbox checklist (tuxlink-b6ad) — wait on the send note.
    await screen.findByTestId('po-network-send-note');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-relay-banner')).toBeInTheDocument();
    });
    expect(screen.getByTestId('po-relay-banner')).toHaveTextContent(
      'Radio network + internet relay',
    );
  });

  it('relay_state "no-cms-connection-available" → po-relay-banner renders the correct label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 0, received_count: 0, relay_state: 'no-cms-connection-available' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-relay-banner')).toBeInTheDocument();
    });
    expect(screen.getByTestId('po-relay-banner')).toHaveTextContent(
      'Relay reachable; CMS uplink down',
    );
  });

  it('relay_state "not-relay" → po-relay-banner is NOT rendered', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 1, received_count: 0, relay_state: 'not-relay' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    // Wait for the result to appear (sent/received line).
    await waitFor(() => {
      expect(screen.getByTestId('po-result')).toBeInTheDocument();
    });
    // Banner must NOT be present for a non-relay endpoint.
    expect(screen.queryByTestId('po-relay-banner')).toBeNull();
  });

  it('po-relay-banner is absent before any connect (no result yet)', () => {
    renderPanel({ mode: 'local' });
    expect(screen.queryByTestId('po-relay-banner')).toBeNull();
  });

  // ── Login indicator ───────────────────────────────────────────────────────

  it('local mode login indicator shows <base>-L (strips SSID + DTN suffix)', async () => {
    // config callsign is N7CPZ-10 → base N7CPZ → login N7CPZ-L.
    renderPanel({ mode: 'local' });
    await waitFor(() => {
      expect(screen.getByTestId('po-login-indicator')).toHaveTextContent('N7CPZ-L');
    });
  });

  it('network mode login indicator shows the SSID-stripped base callsign (no -L)', async () => {
    // config callsign N7CPZ-10 → backend base_callsign_for_post_office(.., false)
    // strips the SSID to N7CPZ (no -L). The indicator must match what is sent.
    renderPanel({ mode: 'network' });
    await waitFor(() => {
      expect(screen.getByTestId('po-login-indicator')).toHaveTextContent('N7CPZ');
    });
    // base callsign only: neither the SSID (-10) nor the local -L suffix
    expect(screen.getByTestId('po-login-indicator')).not.toHaveTextContent('-10');
    expect(screen.getByTestId('po-login-indicator')).not.toHaveTextContent('-L');
  });

  it('local mode login indicator shows "-L" for an empty callsign (matches the unguarded backend)', async () => {
    // config_read returns no callsign → trimmed base is '' → backend
    // base_callsign_for_post_office('', true) = format!("{base}-L") = "-L".
    // The indicator must render that, NOT the '—' placeholder (the dropped guard).
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: '', grid: '' };
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    expect(screen.getByTestId('po-login-indicator')).toHaveTextContent('-L');
    // The em-dash placeholder must NOT be shown when the backend would send '-L'.
    expect(screen.getByTestId('po-login-indicator')).not.toHaveTextContent('—');
  });

  // ── No-consent assertion (RADIO-1: pure TCP, zero transmit) ────────────────

  it('Connect fires telnet_post_office_connect WITHOUT a consent modal or modem_mint_consent', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    fireEvent.click(screen.getByTestId('po-connect-btn'));
    // No modal should appear.
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'telnet_post_office_connect',
        expect.objectContaining({ req: expect.any(Object) }),
      );
    });
    // modem_mint_consent must NOT be called — Post Office never keys a TX.
    expect(invokeMock).not.toHaveBeenCalledWith('modem_mint_consent');
    expect(invokeMock).not.toHaveBeenCalledWith(
      'modem_mint_consent',
      expect.anything(),
    );
  });

  // ── host:port input ───────────────────────────────────────────────────────

  it('host input defaults to 127.0.0.1 and port to 8772 (design §4.3)', async () => {
    renderPanel({ mode: 'local' });
    const host = (await screen.findByTestId('po-host-input')) as HTMLInputElement;
    const port = screen.getByTestId('po-port-input') as HTMLInputElement;
    expect(host.value).toBe('127.0.0.1');
    expect(port.value).toBe('8772');
  });

  // ── Favorites (network mode only) ─────────────────────────────────────────

  it('local mode does NOT render a favorites section', async () => {
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');
    expect(screen.queryByTestId('po-favorites-section')).toBeNull();
  });

  it('network mode loads favorites via network_po_favorites_get on mount', async () => {
    const core = await import('@tauri-apps/api/core');
    renderPanel({ mode: 'network' });
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('network_po_favorites_get');
    });
  });

  it('network mode renders favorites returned by the backend', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'network_po_favorites_get') {
        return [{ callsign: 'W7RELAY', label: 'Mesh relay', host: 'relay.local', port: 8772 }];
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    expect(await screen.findByTestId('po-favorite-relay.local:8772')).toBeInTheDocument();
    expect(screen.getByText(/Mesh relay/)).toBeInTheDocument();
  });

  it('clicking a favorite fills host:port (and callsign)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'network_po_favorites_get') {
        return [{ callsign: 'W7RELAY', label: 'Mesh relay', host: 'relay.local', port: 9000 }];
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    const fav = await screen.findByTestId('po-favorite-relay.local:9000');
    fireEvent.click(fav);
    const host = screen.getByTestId('po-host-input') as HTMLInputElement;
    const port = screen.getByTestId('po-port-input') as HTMLInputElement;
    expect(host.value).toBe('relay.local');
    expect(port.value).toBe('9000');
  });

  it('adding a favorite fires network_po_favorites_add with the RelayFavorite shape', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'network_po_favorites_add') {
        return [(args as { favorite: unknown }).favorite];
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    await screen.findByTestId('po-favorites-section');
    // Fill the host:port (the favorite's endpoint), then add.
    fireEvent.change(screen.getByTestId('po-host-input'), {
      target: { value: 'relay.local' },
    });
    fireEvent.change(screen.getByTestId('po-port-input'), {
      target: { value: '9000' },
    });
    fireEvent.click(screen.getByTestId('po-favorite-add-btn'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'network_po_favorites_add',
        {
          favorite: expect.objectContaining({
            host: 'relay.local',
            port: 9000,
          }),
        },
      );
    });
  });

  it('a rejected favorites add (duplicate host:port) surfaces the error inline', async () => {
    // network_po_favorites_add is a pure config write — it emits NO
    // session_log:line events, so a UiError::Rejected (host:port already saved)
    // must be surfaced in the inline favorites error line, not silently dropped.
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'network_po_favorites_add') {
        throw new Error('Rejected: relay.local:8772 is already saved');
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    await screen.findByTestId('po-favorites-section');
    fireEvent.change(screen.getByTestId('po-host-input'), {
      target: { value: 'relay.local' },
    });
    fireEvent.click(screen.getByTestId('po-favorite-add-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('po-favorites-error')).toHaveTextContent(
        'relay.local:8772 is already saved',
      );
    });
  });

  it('removing a favorite fires network_po_favorites_remove with host + port', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'network_po_favorites_get') {
        return [{ callsign: 'W7RELAY', label: 'Mesh relay', host: 'relay.local', port: 8772 }];
      }
      if (cmd === 'network_po_favorites_remove') {
        return [];
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'network' });
    const removeBtn = await screen.findByTestId('po-favorite-remove-relay.local:8772');
    fireEvent.click(removeBtn);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'network_po_favorites_remove',
        { host: 'relay.local', port: 8772 },
      );
    });
  });

  // ── Partial-send survival (selection vs. shrinking Outbox) ─────────────────
  //
  // After a connect, sent drafts move Outbox→Sent and invalidateQueries refetches
  // a SMALLER Outbox. selectedMids is derived as outbox.filter(m => selected.has)
  // so a checked-but-now-vanished MID drops out automatically — no stale id can
  // linger in what the next Connect would send (design §4.7). The observable
  // proof is the Connect button's send-count, which is driven by selectedCount.

  it('drops vanished MIDs from the selection after the Outbox shrinks on connect', async () => {
    const core = await import('@tauri-apps/api/core');
    // First mailbox_list returns both drafts; after the connect-triggered
    // invalidation, the second returns only OUT-2 (OUT-1 was sent).
    let outboxCall = 0;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'mailbox_list') {
        outboxCall += 1;
        return outboxCall === 1 ? OUTBOX_FIXTURE : [OUTBOX_FIXTURE[1]];
      }
      if (cmd === 'telnet_post_office_connect') {
        return { sent_count: 1, received_count: 0, relay_state: 'not-relay' };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel({ mode: 'local' });
    await screen.findByTestId('po-outbox-row-OUT-1');

    // Select BOTH drafts → "Connect & send 2".
    fireEvent.click(screen.getByTestId('po-select-all'));
    const connect = screen.getByTestId('po-connect-btn') as HTMLButtonElement;
    expect(connect).toHaveTextContent('Connect & send 2');

    // Connect. OUT-1 leaves the Outbox; OUT-2 remains.
    fireEvent.click(connect);

    // The vanished OUT-1 row is gone, and the send-count recomputes to 1 — the
    // stale OUT-1 id no longer counts toward what the next Connect would send.
    await waitFor(() => {
      expect(screen.queryByTestId('po-outbox-row-OUT-1')).toBeNull();
    });
    expect(screen.getByTestId('po-outbox-row-OUT-2')).toBeInTheDocument();
    expect(connect).toHaveTextContent('Connect & send 1');
    // OUT-2 is still checked (its row survived the refetch).
    expect(
      (screen.getByTestId('po-outbox-check-OUT-2') as HTMLInputElement).checked,
    ).toBe(true);
  });

  // ── Session log + config + close ──────────────────────────────────────────

  it('renders the Session log section', () => {
    renderPanel({ mode: 'local' });
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('reads my_callsign + locator from config_read on mount', async () => {
    const core = await import('@tauri-apps/api/core');
    renderPanel({ mode: 'local' });
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('config_read');
    });
  });

  it('close button calls onClose', () => {
    const onClose = vi.fn();
    renderPanel({ mode: 'local', onClose });
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});

// ── Network PO relay-favorite edit-in-place (tuxlink-oi1g) ────────────────────
describe('<TelnetPostOfficeRadioPanel> relay-favorite edit-in-place (oi1g)', () => {
  // Sibling describe — does NOT inherit the main block's beforeEach, so reset the
  // shared invoke mock here or call counts leak between these tests.
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
  });

  const FAV = { callsign: 'W7RELAY', label: 'PDX relay', host: '10.0.0.5', port: 8772 };

  const withOneFavorite = async (cmd: string, args?: unknown) => {
    if (cmd === 'config_read') return { callsign: 'N7CPZ-10', grid: 'CN87' };
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'mailbox_list') return OUTBOX_FIXTURE;
    if (cmd === 'network_po_favorites_get') return [FAV];
    if (cmd === 'network_po_favorites_set') {
      return (args as { favorites: unknown[] }).favorites;
    }
    return undefined;
  };

  it('edits a relay favorite in place via network_po_favorites_set (no remove+re-add)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(withOneFavorite);
    renderPanel({ mode: 'network' });

    fireEvent.click(await screen.findByTestId('po-favorite-edit-10.0.0.5:8772'));
    fireEvent.change(screen.getByTestId('po-favorite-edit-label-10.0.0.5:8772'), {
      target: { value: 'Portland relay' },
    });
    fireEvent.click(screen.getByTestId('po-favorite-edit-save-10.0.0.5:8772'));

    await waitFor(() => {
      const calls = invokeSpy.mock.calls.filter(([c]) => c === 'network_po_favorites_set');
      expect(calls).toHaveLength(1);
      const list = (calls[0][1] as { favorites: Array<Record<string, unknown>> }).favorites;
      expect(list).toHaveLength(1);
      expect(list[0].label).toBe('Portland relay');
      expect(list[0].callsign).toBe('W7RELAY'); // untouched field preserved
      expect(list[0].host).toBe('10.0.0.5');
      expect(list[0].port).toBe(8772);
    });
    // remove was NOT used to effect the edit.
    expect(invokeSpy.mock.calls.some(([c]) => c === 'network_po_favorites_remove')).toBe(false);
  });

  it('Cancel closes the edit form without calling network_po_favorites_set', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(withOneFavorite);
    renderPanel({ mode: 'network' });

    fireEvent.click(await screen.findByTestId('po-favorite-edit-10.0.0.5:8772'));
    fireEvent.click(screen.getByTestId('po-favorite-edit-cancel-10.0.0.5:8772'));
    expect(screen.queryByTestId('po-favorite-edit-label-10.0.0.5:8772')).toBeNull();
    expect(invokeSpy.mock.calls.some(([c]) => c === 'network_po_favorites_set')).toBe(false);
  });
});

// ── PO edit-in-place validation (Codex 2026-06-10 P2 ×2) ─────────────────────
describe('<TelnetPostOfficeRadioPanel> relay-favorite edit validation (oi1g)', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
  });

  const TWO = [
    { callsign: 'W7AAA', label: 'A', host: '10.0.0.5', port: 8772 },
    { callsign: 'W7BBB', label: 'B', host: '10.0.0.9', port: 8772 },
  ];
  const withTwo = (extra: Record<string, unknown> = {}) => async (cmd: string, args?: unknown) => {
    if (cmd === 'config_read') return { callsign: 'N7CPZ-10', grid: 'CN87' };
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'mailbox_list') return OUTBOX_FIXTURE;
    if (cmd === 'network_po_favorites_get') return TWO;
    if (cmd in extra) return extra[cmd];
    if (cmd === 'network_po_favorites_set') return (args as { favorites: unknown[] }).favorites;
    return undefined;
  };

  it('does NOT persist an edit that blanks the host (no network_po_favorites_set)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(withTwo());
    renderPanel({ mode: 'network' });
    fireEvent.click(await screen.findByTestId('po-favorite-edit-10.0.0.5:8772'));
    fireEvent.change(screen.getByTestId('po-favorite-edit-host-10.0.0.5:8772'), { target: { value: '   ' } });
    fireEvent.click(screen.getByTestId('po-favorite-edit-save-10.0.0.5:8772'));
    await new Promise((r) => setTimeout(r, 20));
    expect(invokeSpy.mock.calls.some(([c]) => c === 'network_po_favorites_set')).toBe(false);
  });

  it('does NOT persist an edit whose new host:port collides with another favorite', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(withTwo());
    renderPanel({ mode: 'network' });
    // Edit A (10.0.0.5:8772) to take B's endpoint (10.0.0.9:8772) → collision.
    fireEvent.click(await screen.findByTestId('po-favorite-edit-10.0.0.5:8772'));
    fireEvent.change(screen.getByTestId('po-favorite-edit-host-10.0.0.5:8772'), { target: { value: '10.0.0.9' } });
    fireEvent.click(screen.getByTestId('po-favorite-edit-save-10.0.0.5:8772'));
    await new Promise((r) => setTimeout(r, 20));
    expect(invokeSpy.mock.calls.some(([c]) => c === 'network_po_favorites_set')).toBe(false);
  });
});

// ── tuxlink-1w7t: AREDN mesh Post Office discovery ──────────────────────────
describe('<TelnetPostOfficeRadioPanel> AREDN mesh discovery (1w7t)', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  const MESH_FIXTURE = [
    { name: 'W7ABC-10 Winlink Post Office', ip: '10.5.3.2', port: 8772, link: 'http://10.5.3.2:8772/', reachable: true, rtt_ms: 12 },
    { name: 'N7XYZ Post Office', ip: '10.5.9.1', port: 8772, link: 'http://10.5.9.1:8772/', reachable: false, rtt_ms: null },
  ];

  const withMesh = async (impl: (cmd: string, args?: unknown) => Promise<unknown>) => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      const base = await defaultInvokeImpl(cmd);
      if (base !== undefined) return base;
      return impl(cmd, args);
    });
  };

  it('local mode does NOT render the mesh discovery section', () => {
    renderPanel({ mode: 'local' });
    expect(screen.queryByTestId('po-mesh-discovery')).not.toBeInTheDocument();
  });

  it('network mode renders the mesh discovery section (replacing the old "omitted" note)', async () => {
    renderPanel({ mode: 'network' });
    expect(await screen.findByTestId('po-mesh-discovery')).toBeInTheDocument();
    expect(screen.getByTestId('po-mesh-discover-btn')).toBeInTheDocument();
  });

  it('Discover invokes mesh_discover_post_offices and renders reachable + down rows', async () => {
    await withMesh(async (cmd) => (cmd === 'mesh_discover_post_offices' ? MESH_FIXTURE : undefined));
    renderPanel({ mode: 'network' });
    fireEvent.click(await screen.findByTestId('po-mesh-discover-btn'));
    expect(await screen.findByTestId('po-mesh-row-10.5.3.2:8772')).toBeInTheDocument();
    expect(screen.getByTestId('po-mesh-reach-10.5.3.2:8772')).toHaveTextContent('●');
    expect(screen.getByTestId('po-mesh-reach-10.5.9.1:8772')).toHaveTextContent('○');
  });

  it('"Use" on a discovered relay loads its numeric IP + port into the connect form', async () => {
    await withMesh(async (cmd) => (cmd === 'mesh_discover_post_offices' ? MESH_FIXTURE : undefined));
    renderPanel({ mode: 'network' });
    fireEvent.click(await screen.findByTestId('po-mesh-discover-btn'));
    fireEvent.click(await screen.findByTestId('po-mesh-use-10.5.3.2:8772'));
    expect((screen.getByTestId('po-host-input') as HTMLInputElement).value).toBe('10.5.3.2');
    expect((screen.getByTestId('po-port-input') as HTMLInputElement).value).toBe('8772');
  });

  it('empty discovery result → "No Post Offices advertised" empty state', async () => {
    await withMesh(async (cmd) => (cmd === 'mesh_discover_post_offices' ? [] : undefined));
    renderPanel({ mode: 'network' });
    fireEvent.click(await screen.findByTestId('po-mesh-discover-btn'));
    expect(await screen.findByTestId('po-mesh-empty')).toBeInTheDocument();
  });

  it('a DNS failure renders the off-mesh error message', async () => {
    await withMesh(async (cmd) => {
      if (cmd === 'mesh_discover_post_offices') throw 'error sending request: dns error: failed to lookup address';
      return undefined;
    });
    renderPanel({ mode: 'network' });
    fireEvent.click(await screen.findByTestId('po-mesh-discover-btn'));
    const err = await screen.findByTestId('po-mesh-error');
    expect(err).toHaveTextContent(/Not on an AREDN mesh/i);
  });

  it('editing the mesh node host persists it via config_set_aredn_master_node_host on blur', async () => {
    const core = await import('@tauri-apps/api/core');
    await withMesh(async () => undefined);
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel({ mode: 'network' });
    const input = await screen.findByTestId('po-mesh-host-input');
    fireEvent.change(input, { target: { value: 'master.local.mesh' } });
    fireEvent.blur(input);
    await waitFor(() =>
      expect(
        invokeSpy.mock.calls.some(
          ([c, a]) =>
            c === 'config_set_aredn_master_node_host' &&
            (a as { host?: string })?.host === 'master.local.mesh',
        ),
      ).toBe(true),
    );
  });
});
