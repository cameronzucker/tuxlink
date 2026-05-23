import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
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

  it('renders the Mock B regions: dashboard ribbon, sidebar, panes, session log, status bar', () => {
    renderShell();
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
    expect(screen.getByTestId('dashboard-ribbon')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toBeInTheDocument();
    expect(screen.getByTestId('rows-pane')).toBeInTheDocument();
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument(); // visible by default
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

  it('View → Toggle Session Log toggles the (default-visible) session log', () => {
    renderShell();
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
    clickMenu('View', /Toggle Session Log/);
    expect(screen.queryByTestId('session-log-root')).toBeNull();
    clickMenu('View', /Toggle Session Log/);
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
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

  it('selecting the CMS Packet connection swaps the reader for the packet panel', async () => {
    renderShell();
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    expect(await screen.findByTestId('packet-panel-root')).toBeInTheDocument();
    expect(screen.queryByTestId('message-view-empty')).toBeNull();
    // The full-width session log + status bar stay put.
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  it('selecting a folder clears the packet panel back to the reader', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    await screen.findByTestId('packet-panel-root');
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(screen.queryByTestId('packet-panel-root')).toBeNull();
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
  });

  it('renders the Telnet-CMS pane when cms+telnet is selected', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    expect(await screen.findByTestId('telnet-cms-panel-root')).toBeInTheDocument();
  });
  it('disables unbuilt protocol rows (radio-only+telnet)', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('sess-radio-only'));
    expect(screen.getByTestId('proto-radio-only-telnet')).toBeDisabled();
  });
});
