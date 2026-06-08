import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MessageMeta } from '../mailbox/types';
import { saveDraft } from '../compose/useDraft';

// Vite-native raw-import of AppShell.css for the tuxlink-8rng chrome-width
// assertions below. Uses the same pattern as src/forms/innerhtml-ban.test.ts:
// `import.meta.glob` with `eager + ?raw + default` returns the CSS as a string
// at module-evaluation time, so no @types/node / node:fs dependency is needed
// and `pnpm tsc --noEmit` stays clean. Pitfall TEST-1
// (docs/pitfalls/implementation-pitfalls.md) forbids node:fs in tests.
const APP_SHELL_CSS_MODULES = import.meta.glob('./AppShell.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const appShellCss = APP_SHELL_CSS_MODULES['./AppShell.css'];

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
    // position_status: PositionStatusDto — no GPS fix, empty grids (null-state).
    // Without this stub, react-query's queryFn receives `undefined` and emits
    // "Query data cannot be undefined" warnings on every poll tick. The
    // positionQuery.data ?? null guard in useStatusData already maps this to
    // null downstream; the stub silences the contract violation (tuxlink-hnkn).
    if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
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

// tuxlink-gp8b: outbox fixture (a single queued draft) so the sidebar
// queue-depth badge has something to render. Distinct from inbox/sent so a
// count-mixup regression would fail the assertion.
const outboxMsgs: MessageMeta[] = [
  {
    id: 'OUTBOX1',
    subject: 'Queued draft',
    from: 'W4PHS@winlink.org',
    to: ['KK4XYZ@winlink.org'],
    date: '2026-06-02T10:00:00Z',
    unread: false,
    bodySize: 80,
    hasAttachments: false,
  },
];

vi.mock('../mailbox/useMailbox', () => ({
  useMailboxChangeEvents: () => {},
  useMailbox: (folder: string) => ({
    messages:
      folder === 'inbox'
        ? inboxMsgs
        : folder === 'sent'
        ? sentMsgs
        : folder === 'outbox'
        ? outboxMsgs
        : [],
    isLoading: false,
    isError: false,
    error: null,
  }),
  isBackendFolder: (f: string) => f === 'inbox' || f === 'outbox' || f === 'sent',
  isUserFolderSlug: (s: string) => /^[a-z0-9-]+$/.test(s) && !s.startsWith('-') && !s.endsWith('-'),
}));

// tuxlink-f62f: AppShell calls useUserFolders to populate the sidebar's
// Folders section. Tests don't exercise that path; an empty list keeps the
// section rendering its empty-hint without firing the real Tauri command.
vi.mock('../mailbox/useUserFolders', () => ({
  useUserFolders: () => ({ folders: [], isLoading: false, isError: false, error: null }),
  useCreateUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDeleteUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRenameUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  USER_FOLDERS_QUERY_KEY: ['userFolders'],
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

  // tuxlink-gp8b: PR #219 wired the Outbox folder entry into the sidebar but
  // never extended the `counts` map AppShell passes to FolderSidebar — so the
  // status bar's "1 to send" segment and the sidebar drew from the same
  // outbox.messages.length but only one rendered. This pins the queue-depth
  // badge so a future counts-object regression fails fast at the test layer
  // instead of waiting for an operator to notice the mismatch with the status
  // bar.
  it('sidebar Outbox shows queue depth matching the status bar (tuxlink-gp8b)', () => {
    renderShell();
    // The single queued outbox draft must surface as a `1` badge — same value
    // the status bar's "1 to send" derives from.
    expect(screen.getByTestId('folder-count-outbox')).toHaveTextContent('1');
    expect(screen.getByTestId('status-bar-outbox')).toHaveTextContent('1 to send');
  });

  it('Drafts lists local saved drafts and reopens a selected compose draft', async () => {
    saveDraft({
      draftId: 'draft-shell',
      to: 'KK4XYZ@winlink.org',
      subject: 'Saved local draft',
      body: 'Return to this before the net.',
      requestAck: false,
    });

    renderShell();
    expect(screen.getByTestId('folder-drafts')).not.toBeDisabled();
    expect(screen.getByTestId('folder-count-drafts')).toHaveTextContent('1');

    fireEvent.click(screen.getByTestId('folder-drafts'));
    const row = await screen.findByTestId('message-row-draft-shell');
    expect(row).toHaveTextContent('Saved local draft');
    expect(row).toHaveTextContent('Return to this before the net.');

    vi.mocked(invoke).mockClear();
    fireEvent.click(row);
    expect(invoke).toHaveBeenCalledWith('compose_window_open', { draftId: 'draft-shell' });
  });

  it('selecting a row updates ONLY the reader and does not remount the shell', async () => {
    renderShell();
    const shellBefore = screen.getByTestId('app-shell-root');
    const sidebarBefore = screen.getByTestId('folder-sidebar');

    fireEvent.click(screen.getByTestId('message-row-INBOX1'));

    // tuxlink-djnl: MessageView is now React.lazy. While the chunk is in
    // flight the Suspense fallback is the same MessageViewEmpty visible at
    // "no selection" — so don't assert its absence; instead wait for the
    // loaded state (or the error state, whichever the mock backend resolves).
    await screen.findByTestId('message-view-loaded', undefined, { timeout: 10000 });
    expect(screen.getByTestId('app-shell-root')).toBe(shellBefore);
    expect(screen.getByTestId('folder-sidebar')).toBe(sidebarBefore);
    expect(screen.getByTestId('virtuoso-mock')).toBeInTheDocument();
  });

  it('selecting a different folder resets the message selection and swaps the list', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    // Wait for the lazy MessageView chunk to resolve before asserting the
    // folder-switch behavior — same race as above (tuxlink-djnl).
    await screen.findByTestId('message-view-loaded', undefined, { timeout: 10000 });

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

  it('View → Toggle Mailbox Bar hides and shows the status bar', () => {
    renderShell();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
    // tuxlink-qxqj: the menu label is now "Toggle Mailbox Bar" — the bar's
    // content is mailbox queue state, not transport status. The action id
    // (menu:view:status_bar) and data-testid (status-bar) stay the same.
    clickMenu('View', /Toggle Mailbox Bar/);
    expect(screen.queryByTestId('status-bar')).toBeNull();
    clickMenu('View', /Toggle Mailbox Bar/);
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

  // tuxlink-a2gd: production mount path — the lazy overlay actually opens from the menu.
  // config_read is mocked to null above; the panel's null-grid is swallowed, so it still renders.
  it('Message → Find a Gateway opens the Catalog Builder (production mount path)', async () => {
    renderShell();
    clickMenu('Message', /find a gateway/i);
    expect(
      await screen.findByRole('dialog', { name: /find a gateway/i }, { timeout: 10000 }),
    ).toBeInTheDocument();
  });

  // Option (b): with a message selected, Message → Reply opens a reply window.
  // openReplyWindow seeds a draft then opens a compose window via
  // compose_window_open. The message_read mock resolves so useMessage's
  // openMessage is defined and the reply handler is not a no-op.
  it('Message → Reply opens a reply window for the selected message', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    // Wait for the lazy MessageView chunk + useMessage resolve. tuxlink-djnl
    // bumped the timeout from the default 1s — under the full parallel
    // suite the dynamic import can race the original 1s window.
    await screen.findByTestId('message-view-loaded', undefined, { timeout: 10000 });
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
    // tuxlink-twym: bump timeout — radio panels are now React.lazy and the
    // dynamic-import resolve can race the default 1s waitFor on Pi-class CI.
    const panel = await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
    expect(panel).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent(/Packet/);
    // Reading pane stays on the message view (no Packet form there anymore).
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  // tuxlink-u4ky: selecting a different folder must NOT dismiss the radio
  // panel. Pre-fix, onSelectFolder cleared selectedConnection along with
  // selectedMessage — leaking pre-P2-era reading-pane-contention behavior
  // that the post-P2 design comment on onSelectConnection explicitly
  // disavowed. Operator smoke walk 2026-06-05 flagged this as "switching
  // folders closes the radio modem dock — not intended behavior."
  it('selecting a folder preserves the active radio panel (selectedConnection is independent)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
    fireEvent.click(screen.getByTestId('folder-sent'));
    // Panel stays mounted across folder navigation — the operator can
    // browse mail without losing their connection panel context.
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent(/Packet/);
  });

  it('closing Packet keeps Packet as the ribbon transport intent and Connect does not start Telnet', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

    fireEvent.click(screen.getByTestId('radio-panel-close'));
    await waitFor(() => expect(screen.queryByTestId('radio-panel-root')).toBeNull());
    expect(screen.getByTestId('ribbon-connection')).toHaveTextContent('Packet 1200 · not connected');

    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByTestId('connect-button'));

    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === 'cms_connect')).toBe(false);
    expect(await screen.findByTestId('radio-panel-title', undefined, { timeout: 10000 }))
      .toHaveTextContent(/Packet/);
  });

  it('closing an ARDOP panel keeps ARDOP as the ribbon transport intent (item 38 gap — radio, not just packet)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-ardop-hf'));
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

    fireEvent.click(screen.getByTestId('radio-panel-close'));
    await waitFor(() => expect(screen.queryByTestId('radio-panel-root')).toBeNull());
    // The ribbon must still name ARDOP, not revert to the generic config label.
    expect(screen.getByTestId('ribbon-connection')).toHaveTextContent('ARDOP HF · not connected');

    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByTestId('connect-button'));
    // And Connect must not start a Telnet/CMS session for the radio intent.
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === 'cms_connect')).toBe(false);
  });

  it('renders the TelnetRadioPanel when cms+telnet is selected (P2: panel moved to right-hand radio panel)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    // Telnet UI now lives in the right radio panel (data-testid=radio-panel-root)
    // with the Telnet Winlink title; the reading pane shows the MessageView fallback.
    // tuxlink-twym: bump timeout — radio panels are now React.lazy and the
    // dynamic-import resolve can race the default 1s waitFor on Pi-class CI.
    const panel = await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
    expect(panel).toBeInTheDocument();
    expect(await screen.findByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
  });

  it('renders the TelnetP2pRadioPanel when p2p+telnet is selected (tuxlink-0pnb client-dial)', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-p2p'));
    fireEvent.click(screen.getByTestId('proto-p2p-telnet'));
    // p2p+telnet shares the radio-panel-root mount with cms+telnet but the
    // title swaps to "Telnet P2P" via the intent-aware panelTitle().
    // tuxlink-twym: bump timeout — radio panels are now React.lazy and the
    // dynamic-import resolve can race the default 1s waitFor on Pi-class CI.
    const panel = await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
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
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
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
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });
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
      if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
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

// ---------------------------------------------------------------------------
// Radio-panel chrome width — tuxlink-8rng
//
// Operator 2026-06-01 surfaced ARDOP-panel content clipping at the 360 px
// width; a prior CSS clamp (commit cc82bf4) only partially fixed it.
// tuxlink-8rng widened the radio-panel column to 400 px and originally
// shrank the mailbox column 380 → 300 to fund 80 px of the panel. Operator
// pushback (tuxlink-40u8, 2026-06-03): compressing the mailbox column AND
// the reading pane at the same time feels unpolished. The mailbox now
// stays at 380 px and the panel's full 400 px comes from the 1fr
// reading-pane. These tests pin the grid-template-columns declaration so
// the layout doesn't quietly walk back. Rule applies to both the 4-col
// (`panes--with-dock`) and 5-col (`.panes--with-legacy-dock`) variants.
// ---------------------------------------------------------------------------
describe('AppShell.css radio-panel chrome width (tuxlink-8rng + tuxlink-40u8)', () => {
  it('declares the radio-panel column at 400px in .panes--with-dock', () => {
    expect(appShellCss).toMatch(
      /\.layout-b \.panes--with-dock\s*\{[^}]*200px\s+380px\s+1fr\s+400px/,
    );
  });

  it('declares the radio-panel column at 400px in .panes--with-legacy-dock', () => {
    expect(appShellCss).toMatch(
      /\.layout-b \.panes--with-dock\.panes--with-legacy-dock\s*\{[^}]*200px\s+380px\s+1fr\s+400px\s+290px/,
    );
  });
});

describe('AppShell.css print stylesheet (tuxlink-zdfj)', () => {
  const printCss = appShellCss.slice(appShellCss.indexOf('@media print'));

  it('hides app chrome and list columns for message-focused printing', () => {
    expect(printCss).toContain('@media print');
    for (const selector of [
      '.layout-b .tux-titlebar',
      '.layout-b .tux-menubar',
      '.layout-b .tux-resize',
      '.layout-b .ribbon-with-search',
      '.layout-b .search-zone',
      '.layout-b .dashboard',
      '.layout-b .sidebar',
      '.layout-b .rows-pane',
      '.layout-b .statusbar',
      '.layout-b .radio-panel',
      '.layout-b .reading-pane .actions',
      '.layout-b .reading-pane .msg-attachment-preview',
      '.layout-b .reading-pane .msg-attachment-save',
      '.tux-dropdown',
      '.message-list-sort-menu',
    ]) {
      expect(printCss).toContain(selector);
    }
    expect(printCss).toMatch(/display:\s*none\s*!important;/);
  });

  it('lets the message reader print full-width with an unsplit header block', () => {
    expect(printCss).toMatch(/\.layout-b\s*\{[\s\S]*height:\s*auto;[\s\S]*overflow:\s*visible;/);
    expect(printCss).toMatch(
      /\.layout-b \.panes,[\s\S]*\.layout-b \.panes--with-dock,[\s\S]*\.layout-b \.panes--with-dock\.panes--with-legacy-dock\s*\{[\s\S]*display:\s*block;[\s\S]*overflow:\s*visible;/,
    );
    expect(printCss).toMatch(/\.layout-b \.reading-pane\s*\{[^}]*width:\s*100%;/);
    expect(printCss).toMatch(/\.layout-b \.reading-pane\s*\{[^}]*max-width:\s*none;/);
    expect(printCss).toMatch(/\.layout-b \.reading-pane\s*\{[^}]*padding:\s*0;/);
    expect(printCss).toMatch(/\.layout-b \.reading-pane\s*\{[^}]*overflow:\s*visible;/);
    expect(printCss).toMatch(
      /\.layout-b \.reading-pane \.message-print-header\s*\{[\s\S]*break-inside:\s*avoid;[\s\S]*page-break-inside:\s*avoid;/,
    );
    expect(printCss).toMatch(
      /\.layout-b \.reading-pane \.msg-meta\s*\{[\s\S]*break-before:\s*avoid;[\s\S]*break-inside:\s*avoid;[\s\S]*page-break-inside:\s*avoid;/,
    );
  });
});
