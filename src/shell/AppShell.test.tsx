import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';

// ---------------------------------------------------------------------------
// Tauri IPC mocks. AppShell now mounts the REAL ribbon (config_read /
// backend_status), session log (session_log_snapshot + listen), reader
// (useMessage → message_read), and status bar. Stub the IPC so the shell
// mounts under jsdom without a Tauri runtime. These return benign empties so
// the wired components render their "no data yet" states.
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null; // ribbon shows empty until config
    if (cmd === 'backend_status') return null; // null → "Idle · …" fallback
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'message_read') {
      // A benign ParsedMessage so the reader leaves the empty state without
      // a TanStack "query returned undefined" warning.
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
  // listen returns an unlisten fn; never fires in this suite.
  listen: vi.fn(async () => () => {}),
}));

// Mock react-virtuoso so rows actually render in jsdom (the real Virtuoso
// renders into a zero-height scroller and emits no items under jsdom). The
// mock renders every item synchronously — fine for asserting selection
// behavior, which is what this suite covers.
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

// Mock useMailbox so the shell has deterministic messages per folder without
// a backend. The hook is called for the selected folder AND for the three
// backend-folder counts (inbox/outbox/sent).
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
  // MessageView's useMessage uses useQuery, which needs a QueryClient.
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

describe('<AppShell> selection (no-full-view-swap invariant)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
  });

  // Integration commit: the shell now mounts the REAL components in each
  // region (no placeholders). Assert the real components are present.
  it('renders the shell with all real region components present', () => {
    renderShell();
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('dashboard-ribbon')).toBeInTheDocument(); // Task 16
    expect(screen.getByTestId('session-log-root')).toBeInTheDocument(); // Task 15
    expect(screen.getByTestId('status-bar')).toBeInTheDocument(); // Task 16
    expect(screen.getByTestId('region-dock-reserved')).toBeInTheDocument();
    // Reader shows MessageView's empty state until a message is selected.
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
  });

  it('starts on Inbox with the reader showing the empty state', () => {
    renderShell();
    expect(screen.getByTestId('message-view-empty')).toHaveTextContent(
      'Select a message to read.',
    );
  });

  // Task-12 test (6): selecting a row updates selection state and does NOT
  // remount the shell (selection-state change, not a route/full-view-swap).
  it('selecting a row updates ONLY the reader and does not remount the shell', () => {
    renderShell();
    const shellBefore = screen.getByTestId('app-shell-root');
    const sidebarBefore = screen.getByTestId('folder-sidebar');

    fireEvent.click(screen.getByTestId('message-row-INBOX1'));

    // Reader is no longer the empty state — a message is selected. (The loaded
    // view depends on an async message_read; under the mocked IPC it stays in
    // the loading state, which is enough to prove the selection took effect.)
    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();

    // The shell root + sidebar are the SAME DOM nodes — no remount/route.
    expect(screen.getByTestId('app-shell-root')).toBe(shellBefore);
    expect(screen.getByTestId('folder-sidebar')).toBe(sidebarBefore);
    // The list container also persists (not swapped out for a detail view).
    expect(screen.getByTestId('virtuoso-mock')).toBeInTheDocument();
  });

  it('selecting a different folder resets the message selection and swaps the list', () => {
    renderShell();
    // Select an inbox message first.
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    expect(screen.queryByTestId('message-view-empty')).not.toBeInTheDocument();

    // Switch to Sent — selection resets to null (reader shows empty again).
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(screen.getByTestId('message-view-empty')).toBeInTheDocument();
    // The list now shows the Sent message, not the Inbox one.
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();

    // And the shell itself never remounted across the folder switch.
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
  });

  // Codex Task-12 finding 2: the sidebar receives counts for the backend
  // folders. Inbox has 1 message, Sent has 1 — both render a count badge.
  it('passes per-folder counts to the sidebar', () => {
    renderShell();
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('1');
    expect(screen.getByTestId('folder-count-sent')).toHaveTextContent('1');
    // Outbox has 0 messages → no badge (FolderSidebar suppresses zero counts).
    expect(screen.queryByTestId('folder-count-outbox')).not.toBeInTheDocument();
  });
});
