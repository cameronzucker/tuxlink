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
//   telnet_post_office_connect({ req: {...} })      → { sent_count, received_count }
//     ^ Phase-C backend command — NOT yet implemented; mocked here. B3↔C1 seam.
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

  // ── Connect action / invoke contract ──────────────────────────────────────

  it('Connect (local) fires telnet_post_office_connect with the { req } wrapper shape', async () => {
    const core = await import('@tauri-apps/api/core');
    let observedReq: Record<string, unknown> | null = null;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: unknown) => {
        if (cmd === 'telnet_post_office_connect') {
          observedReq = (args as { req: Record<string, unknown> }).req;
          return { sent_count: 1, received_count: 0 };
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
          return { sent_count: 0, received_count: 2 };
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
          return { sent_count: 0, received_count: 0 };
        }
        return defaultInvokeImpl(cmd);
      },
    );
    renderPanel({ mode: 'network' });
    await screen.findByTestId('po-outbox-row-OUT-1');
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

  // ── Login indicator ───────────────────────────────────────────────────────

  it('local mode login indicator shows <base>-L (strips SSID + DTN suffix)', async () => {
    // config callsign is N7CPZ-10 → base N7CPZ → login N7CPZ-L.
    renderPanel({ mode: 'local' });
    await waitFor(() => {
      expect(screen.getByTestId('po-login-indicator')).toHaveTextContent('N7CPZ-L');
    });
  });

  it('network mode login indicator shows the FULL callsign (no -L)', async () => {
    renderPanel({ mode: 'network' });
    await waitFor(() => {
      expect(screen.getByTestId('po-login-indicator')).toHaveTextContent('N7CPZ-10');
    });
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
        return { sent_count: 1, received_count: 0 };
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
