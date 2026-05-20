import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';

// ---------------------------------------------------------------------------
// Tauri IPC mocks. The Mock B shell mounts the dashboard ribbon + status bar
// (useStatusData → config_read/backend_status), the sidebar, the list, the
// reader (useMessage → message_read), and the human session log
// (session_log_snapshot + listen). Stub the IPC so the shell mounts in jsdom.
// The `menu` listener is captured (vi.hoisted) so tests can fire View events.
// ---------------------------------------------------------------------------
const h = vi.hoisted(() => ({ menuHandler: null as null | ((e: { payload: string }) => void) }));

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
    return undefined;
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: string }) => void) => {
    if (event === 'menu') h.menuHandler = cb;
    return () => {};
  }),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'main', setTitle: vi.fn(async () => {}) }),
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
    h.menuHandler = null;
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

  it('View → Session Log toggles the (default-visible) session log', async () => {
    renderShell();
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:session_log' });
    });
    expect(screen.queryByTestId('session-log-root')).toBeNull();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:session_log' });
    });
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();
  });

  it('View → Toggle Status Bar hides and shows the status bar', async () => {
    renderShell();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:status_bar' });
    });
    expect(screen.queryByTestId('status-bar')).toBeNull();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:status_bar' });
    });
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
  });

  it('the Mailbox menu switches folders', async () => {
    renderShell();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:mailbox:sent' });
    });
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();
  });
});
