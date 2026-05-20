import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';

// ---------------------------------------------------------------------------
// Tauri IPC mocks. The Mock D shell mounts the tab strip, the list, the reader
// (useMessage → message_read), and the status bar (useStatusData → config_read
// + backend_status). Stub the IPC so the shell mounts under jsdom. The `menu`
// listener is captured (via vi.hoisted) so tests can fire View-menu events.
// ---------------------------------------------------------------------------
const h = vi.hoisted(() => ({ menuHandler: null as null | ((e: { payload: string }) => void) }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null; // status bar shows just the state word
    if (cmd === 'backend_status') return null; // null → "Idle"
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

// Mock react-virtuoso so rows render in jsdom (the real Virtuoso emits no items
// under jsdom). Renders every item synchronously.
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

// Deterministic per-folder messages without a backend.
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

describe('<AppShell> — Mock D topology', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    h.menuHandler = null;
  });

  it('renders the Mock D regions: tab strip, list, reader, status bar', () => {
    renderShell();
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
    expect(screen.getByTestId('tab-strip')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toBeInTheDocument();
    expect(screen.getByTestId('rows-pane')).toBeInTheDocument();
    expect(screen.getByTestId('status-bar')).toBeInTheDocument();
    // Reader shows the empty state until a message is selected.
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
  });

  it('drops the synthesis chrome (no ribbon / sidebar / default session log / dock)', () => {
    renderShell();
    expect(screen.queryByTestId('dashboard-ribbon')).toBeNull();
    expect(screen.queryByTestId('folder-sidebar')).toBeNull();
    expect(screen.queryByTestId('region-sessionlog')).toBeNull();
    expect(screen.queryByTestId('session-log-root')).toBeNull();
    expect(screen.queryByTestId('region-dock-reserved')).toBeNull();
  });

  it('shows only Inbox + Sent tabs (mock literal), badge = unread count', () => {
    renderShell();
    expect(screen.getByTestId('tab-inbox')).toBeInTheDocument();
    expect(screen.getByTestId('tab-sent')).toBeInTheDocument();
    // Outbox/Drafts are NOT tabs in the literal Mock D (reached via Mailbox menu).
    expect(screen.queryByTestId('tab-outbox')).toBeNull();
    expect(screen.queryByTestId('tab-drafts')).toBeNull();
    // Inbox fixture msg is unread → badge "1"; Sent msg is read → 0 unread → no badge.
    expect(screen.getByTestId('tab-count-inbox')).toHaveTextContent('1');
    expect(screen.queryByTestId('tab-count-sent')).toBeNull();
  });

  it('the Mailbox menu switches folders (Outbox/Sent have no-or-one tab)', async () => {
    renderShell();
    await act(async () => {
      h.menuHandler?.({ payload: 'menu:mailbox:sent' });
    });
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();
  });

  it('starts on the Inbox tab (active) with the reader showing the empty state', () => {
    renderShell();
    expect(screen.getByTestId('tab-inbox').className).toContain('active');
    expect(screen.getByTestId('message-view-empty')).toHaveTextContent('Select a message to read.');
  });

  // Selecting a row updates selection state and does NOT remount the shell.
  it('selecting a row updates ONLY the reader and does not remount the shell', () => {
    renderShell();
    const shellBefore = screen.getByTestId('app-shell-root');
    const tabsBefore = screen.getByTestId('tab-strip');

    fireEvent.click(screen.getByTestId('message-row-INBOX1'));

    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();
    // Same DOM nodes — no remount/route.
    expect(screen.getByTestId('app-shell-root')).toBe(shellBefore);
    expect(screen.getByTestId('tab-strip')).toBe(tabsBefore);
    expect(screen.getByTestId('virtuoso-mock')).toBeInTheDocument();
  });

  it('switching tabs resets the message selection and swaps the list', () => {
    renderShell();
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('tab-sent'));
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    expect(screen.getByTestId('tab-sent').className).toContain('active');
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
  });

  // View → Session Log (menu:view:session_log) toggles the bottom log strip,
  // which is hidden by default in Mock D.
  it('View → Session Log toggles the (default-hidden) session log strip', async () => {
    renderShell();
    expect(screen.queryByTestId('region-sessionlog')).toBeNull();

    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:session_log' });
    });
    expect(screen.getByTestId('region-sessionlog')).toBeInTheDocument();
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument();

    await act(async () => {
      h.menuHandler?.({ payload: 'menu:view:session_log' });
    });
    expect(screen.queryByTestId('region-sessionlog')).toBeNull();
  });

  // View → Toggle Status Bar (menu:view:status_bar) hides/shows the status bar.
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
});
