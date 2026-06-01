import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';

// ---------------------------------------------------------------------------
// Tauri IPC mocks. The Mock B shell mounts the HTML chrome (TitleBar + MenuBar
// + ResizeHandles), the dashboard ribbon + status bar (useStatusData →
// config_read/backend_status), the sidebar, the list, the reader (useMessage →
// message_read), and the human session log (session_log_snapshot + listen).
// Stub the IPC so the shell mounts in jsdom. Menu actions are now driven
// in-process through the rendered <MenuBar> (no `listen('menu')` channel).
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'modem_get_status') {
      // useModemStatus' initial snapshot — STOPPED keeps the dock unmounted,
      // which preserves the existing 3-col Mock B topology these tests assert.
      return {
        state: 'stopped',
        peer: null, mode: null, widthHz: null, pttBackend: null,
        snDb: null, vuDbfs: null, throughputBps: null,
        bytesRx: 0, bytesTx: 0, uptimeSec: 0,
        arqFlags: { busy: false, rx: false, tx: false },
        lastError: null,
      };
    }
    if (cmd === 'message_read') {
      return {
        id: 'INBOX1',
        subject: 's',
        from: 'f',
        to: [],
        cc: [],
        date: '2026-05-19T00:00:00Z',
        body: 'b',
        attachments: [],
        isForm: false,
        routing: null,
      };
    }
    if (cmd === 'packet_config_get') {
      return {
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
    }
    // Search IPC stubs (Task 17 — find-messages wiring)
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    return undefined;
  }),
}));

// SessionLog still subscribes to its own event channel; the mock keeps the
// shell mounting under jsdom. The menu no longer uses an event channel.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// TitleBar + ResizeHandles now mount in the shell and call window controls.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    setTitle: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
  }),
  ResizeDirection: { North:'North',South:'South',East:'East',West:'West',NorthEast:'NorthEast',NorthWest:'NorthWest',SouthEast:'SouthEast',SouthWest:'SouthWest' },
}));

vi.mock('react-virtuoso', () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: MessageMeta[];
    itemContent: (i: number, m: MessageMeta) => unknown;
  }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (
        <div key={m.id}>{itemContent(i, m) as ReactNode}</div>
      ))}
    </div>
  ),
}));

const inboxMsgs: MessageMeta[] = [
  {
    id: 'INBOX1',
    subject: 'Inbox subject',
    from: 'KK4XYZ@winlink.org',
    to: [],
    date: '2026-05-19T14:00:00Z',
    unread: true,
    bodySize: 100,
    hasAttachments: false,
  },
];
const sentMsgs: MessageMeta[] = [
  {
    id: 'SENT1',
    subject: 'Sent subject',
    from: 'W4PHS@winlink.org',
    to: ['KK4XYZ@winlink.org'],
    date: '2026-05-19T13:00:00Z',
    unread: false,
    bodySize: 200,
    hasAttachments: true,
  },
];

vi.mock('../mailbox/useMailbox', () => ({
  useMailbox: (folder: MailboxFolder) => ({
    messages: folder === 'inbox' ? inboxMsgs : folder === 'sent' ? sentMsgs : [],
    isLoading: false,
    isError: false,
    error: null,
  }),
  isBackendFolder: (f: MailboxFolder) => f === 'inbox' || f === 'outbox' || f === 'sent',
}));

import { invoke } from '@tauri-apps/api/core';
import { AppShell } from './AppShell';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

describe('<AppShell> — Mock B topology', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    vi.mocked(invoke).mockClear();
  });

  // radio-panel-shell P1.6: the bottom session-log strip was removed — the log
  // moves into the radio panel as a per-mode section in P2-P4.
  it('renders the Mock B regions: dashboard ribbon, sidebar, panes, status bar', () => {
    renderShell();
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
    expect(screen.getByTestId('dashboard-ribbon')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toBeInTheDocument();
    expect(screen.getByTestId('rows-pane')).toBeInTheDocument();
    expect(screen.queryByTestId('session-log-root')).not.toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
  });

  it('does NOT render a tab strip (Mock B uses the sidebar for folder nav)', () => {
    renderShell();
    expect(screen.queryByTestId('tab-strip')).toBeNull();
  });

  it('sidebar shows Inbox active + counts (Inbox unread, Sent total)', () => {
    renderShell();
    expect(screen.getByTestId('folder-inbox')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('1'); // 1 unread
    expect(screen.getByTestId('folder-count-sent')).toHaveTextContent('1'); // 1 total
  });

  it('selecting a row updates ONLY the reader and does not remount the shell', () => {
    renderShell();
    const shellBefore = screen.getByTestId('app-shell-root');
    const sidebarBefore = screen.getByTestId('folder-sidebar');

    fireEvent.click(screen.getByTestId('message-row-INBOX1'));

    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();
    expect(screen.getByTestId('app-shell-root')).toBe(shellBefore);
    expect(screen.getByTestId('folder-sidebar')).toBe(sidebarBefore);
    expect(screen.getByTestId('virtuoso-mock')).toBeInTheDocument();
  });

  it('selecting a different folder resets the message selection and swaps the list', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sent')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();
  });

  // Drive a menu action through the rendered <MenuBar>: open the top menu, then
  // click the leaf item (mirrors MenuBar.test.tsx's interaction model). Scoped
  // to the menubar so item labels (e.g. "Reply") don't collide with the
  // reading-pane action buttons ("Reply (Ctrl+R)").
  function clickMenu(top: string, item: RegExp) {
    const menubar = screen.getByRole('menubar');
    fireEvent.click(within(menubar).getByRole('button', { name: top }));
    fireEvent.click(within(menubar).getByRole('button', { name: item }));
  }

  // radio-panel-shell P1.6: the View → Toggle Session Log menu item was
  // removed when the bottom session-log strip went away. The menu no longer
  // offers it; the log will reappear inside the radio panel in P2-P4.
  it('does not offer a View → Toggle Session Log menu item', () => {
    renderShell();
    const menubar = screen.getByRole('menubar');
    fireEvent.click(within(menubar).getByRole('button', { name: 'View' }));
    expect(
      within(menubar).queryByRole('button', { name: /Toggle Session Log/ }),
    ).toBeNull();
  });

  it('View → Toggle Status Bar hides and shows the status bar', () => {
    renderShell();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
    clickMenu('View', /Toggle Status Bar/);
    expect(screen.queryByTestId('status-bar')).toBeNull();
    clickMenu('View', /Toggle Status Bar/);
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  it('the Mailbox menu switches folders', () => {
    renderShell();
    clickMenu('Mailbox', /^Sent$/);
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();
  });

  it('Message → New Message opens a compose window', () => {
    renderShell();
    clickMenu('Message', /New Message/);
    expect(invoke).toHaveBeenCalledWith(
      'compose_window_open',
      expect.objectContaining({ draftId: expect.any(String) }),
    );
  });

  // Option (b): with a message selected, Message → Reply opens a reply window.
  // openReplyWindow seeds a draft then opens a compose window via
  // compose_window_open. The message_read mock resolves so useMessage's
  // openMessage is defined and the reply handler is not a no-op.
  it('Message → Reply opens a reply window for the selected message', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    // Wait for useMessage to resolve the selected message (message_read mock).
    await screen.findByTestId('message-view-loaded');
    vi.mocked(invoke).mockClear();
    // The Reply menu item's accessible name is "ReplyCtrl+R" (label + accel
    // span, no separating space) — anchored regex picks it over "Reply All".
    clickMenu('Message', /^ReplyCtrl/);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'compose_window_open',
        expect.objectContaining({ draftId: expect.any(String) }),
      ),
    );
  });

  it('selecting the CMS Packet connection mounts the PacketRadioPanel (P3: panel moved to right-hand radio panel)', async () => {
    renderShell();
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    // P3: Packet UI lives in the right radio panel. The reading pane
    // falls back to the message view (same pattern as Telnet (P2) and
    // ARDOP (P4)).
    const panel = await screen.findByTestId('radio-panel-root');
    expect(panel).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent(/Packet/);
    // Reading pane stays on the message view (no Packet form there anymore).
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  it('selecting a folder dismisses the PacketRadioPanel (selectedConnection clears)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    await screen.findByTestId('radio-panel-root');
    fireEvent.click(screen.getByTestId('folder-sent'));
    // Folder switch clears selectedConnection (intentional — onSelectFolder
    // resets the reading-pane context). Panel unmounts unless a modem is
    // active. In this test there's no active modem, so the panel goes away.
    expect(screen.queryByTestId('radio-panel-root')).toBeNull();
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
  });

  it('renders the TelnetRadioPanel when cms+telnet is selected (P2: panel moved to right-hand radio panel)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    // Telnet UI now lives in the right radio panel (data-testid=radio-panel-root)
    // with the Telnet Winlink title; the reading pane shows the MessageView fallback.
    const panel = await screen.findByTestId('radio-panel-root');
    expect(panel).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
  });

  it('renders the TelnetP2pRadioPanel when p2p+telnet is selected (tuxlink-0pnb client-dial)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-p2p'));
    fireEvent.click(screen.getByTestId('proto-p2p-telnet'));
    // p2p+telnet shares the radio-panel-root mount with cms+telnet but the
    // title swaps to "Telnet P2P" via the intent-aware panelTitle().
    const panel = await screen.findByTestId('radio-panel-root');
    expect(panel).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent('Telnet P2P');
  });

  it('keeps the radio panel open when the operator clicks a message (2026-05-31 decoupling fix)', async () => {
    // Operator-flagged bug: clicking a message while the Telnet panel was open
    // unmounted the panel because onSelectMessage cleared selectedConnection.
    // The post-P2 reading pane is decoupled from selectedConnection for Telnet,
    // so the two states must be independent.
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    await screen.findByTestId('radio-panel-root');
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    // Panel must still be present; the click on the message no longer clears
    // selectedConnection.
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
  });

  it('keeps an open message in the reading pane when the operator re-clicks a connection', async () => {
    // The sister of the above: onSelectConnection used to clear selectedMessage.
    // Now they're independent. Selecting Telnet again (after a message is open)
    // must not erase the selected message.
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    // selectedMessage is set; reading pane shows MessageView for INBOX1.
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    await screen.findByTestId('radio-panel-root');
    // The message row stays highlighted (selectedMessage was preserved).
    const messageRow = screen.getByTestId('message-row-INBOX1');
    expect(messageRow).toHaveAttribute('aria-selected', 'true');
  });
  it('disables unbuilt protocol rows (radio-only+telnet)', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-radio-only'));
    expect(screen.getByTestId('proto-radio-only-telnet')).toBeDisabled();
  });
});

describe('AppShell — search → MessageList wiring (tuxlink-c7qz)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.mocked(invoke).mockClear();
  });

  it('renders search results in MessageList when search is active', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return null;
      if (cmd === 'backend_status') return null;
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'packet_config_get') return {
        ssid: 7, listenDefault: true, linkKind: 'Tcp', tcpHost: '127.0.0.1',
        tcpPort: 8001, serialDevice: null, serialBaud: null, txdelay: 30,
        persistence: 63, slotTime: 10, paclen: 128, maxframe: 4, t1Ms: 3000, n2Retries: 10,
      };
      if (cmd === 'modem_get_status') {
        // useModemStatus' initial snapshot — STOPPED keeps the ArdopDock unmounted
        // so this test only asserts the search → MessageList wiring.
        return {
          state: 'stopped',
          peer: null, mode: null, widthHz: null, pttBackend: null,
          snDb: null, vuDbfs: null, throughputBps: null,
          bytesRx: 0, bytesTx: 0, uptimeSec: 0,
          arqFlags: { busy: false, rx: false, tx: false },
          lastError: null,
        };
      }
      if (cmd === 'tauri_search_list_saved') return [];
      if (cmd === 'tauri_search_list_recent') return [];
      if (cmd === 'tauri_search_run') return {
        items: [
          {
            id: 'A', subject: 'DAMAGE report', from: 'KX5DD', to: ['N7CPZ'],
            date: '2024-05-20T10:13:00Z', unread: true, bodySize: 100,
            hasAttachments: false, folder: 'inbox',
          },
        ],
        totalMatches: 1, queryMs: 10, effectiveSpec: {},
      };
      return undefined;
    });
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false, gcTime: Infinity } },
    });
    render(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );
    // Type into the SearchBar → onSpecChange → search.setSpec
    act(() => {
      fireEvent.change(screen.getByTestId('searchbar-input'), { target: { value: 'damage' } });
    });
    // Advance past the 150ms debounce so `debounced` updates and query enables
    await act(async () => { vi.advanceTimersByTime(200); });
    // React Query fires tauri_search_run; results arrive and re-render shows them.
    // Assert via data-testid (MessageRow renders message-row-<id>) to avoid
    // any getByText/highlight-split ambiguity.
    await waitFor(() => expect(screen.getByTestId('message-row-A')).toBeInTheDocument(), { timeout: 2000 });
  });
});

describe('<AppShell> — find-messages wiring (Task 17)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    vi.mocked(invoke).mockClear();
  });

  it('renders the SearchBar in the ribbon', () => {
    renderShell();
    expect(screen.getByTestId('search-bar')).toBeInTheDocument();
  });

  it('does NOT render a separate ChipStrip row (filters inline in search bar)', () => {
    renderShell();
    expect(screen.queryByTestId('chip-strip')).not.toBeInTheDocument();
  });

  it('dashboard ribbon dash-items still render (right-clustered)', () => {
    renderShell();
    // DashboardRibbon renders "Callsign" and "Connection" as .dash-label elements.
    expect(screen.getByText('Callsign')).toBeInTheDocument();
    expect(screen.getByText('Connection')).toBeInTheDocument();
  });

  it('SearchBar appears before the panes in the DOM', () => {
    renderShell();
    const root = screen.getByTestId('app-shell-root');
    const searchBar = screen.getByTestId('search-bar');
    const panes = screen.getByTestId('shell-panes');
    expect(root).toContainElement(searchBar);
    expect(root).toContainElement(panes);
    // SearchBar DOM position must precede panes
    expect(
      searchBar.compareDocumentPosition(panes) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
