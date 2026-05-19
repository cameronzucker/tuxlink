import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ReactNode } from 'react';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';

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
// a backend / QueryClient.
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

describe('<AppShell> selection (no-full-view-swap invariant)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
  });

  it('renders the shell with all grid regions present', () => {
    render(<AppShell />);
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('region-ribbon-placeholder')).toBeInTheDocument();
    expect(screen.getByTestId('region-reader-placeholder')).toBeInTheDocument();
    expect(screen.getByTestId('region-sessionlog-placeholder')).toBeInTheDocument();
    expect(screen.getByTestId('region-statusbar-placeholder')).toBeInTheDocument();
    expect(screen.getByTestId('region-dock-reserved')).toBeInTheDocument();
  });

  it('starts on Inbox with no message selected', () => {
    render(<AppShell />);
    expect(screen.getByTestId('reader-empty')).toHaveTextContent('Select a message to read.');
  });

  // Task-12 test (6): selecting a row updates selection state and does NOT
  // remount the shell (selection-state change, not a route/full-view-swap).
  it('selecting a row updates ONLY the reader and does not remount the shell', () => {
    render(<AppShell />);
    const shellBefore = screen.getByTestId('app-shell-root');
    const sidebarBefore = screen.getByTestId('folder-sidebar');

    fireEvent.click(screen.getByTestId('message-row-INBOX1'));

    // Reader now reflects the selection (carrying the folder, Codex F1).
    expect(screen.getByTestId('reader-selection-debug')).toHaveTextContent('inbox / INBOX1');

    // The shell root + sidebar are the SAME DOM nodes — no remount/route.
    expect(screen.getByTestId('app-shell-root')).toBe(shellBefore);
    expect(screen.getByTestId('folder-sidebar')).toBe(sidebarBefore);
    // The list container also persists (not swapped out for a detail view).
    expect(screen.getByTestId('virtuoso-mock')).toBeInTheDocument();
  });

  it('selecting a different folder resets the message selection and swaps the list', () => {
    render(<AppShell />);
    // Select an inbox message first.
    fireEvent.click(screen.getByTestId('message-row-INBOX1'));
    expect(screen.getByTestId('reader-selection-debug')).toBeInTheDocument();

    // Switch to Sent — selection resets to null (reader shows empty again).
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(screen.getByTestId('reader-empty')).toBeInTheDocument();
    // The list now shows the Sent message, not the Inbox one.
    expect(screen.getByTestId('message-row-SENT1')).toBeInTheDocument();
    expect(screen.queryByTestId('message-row-INBOX1')).not.toBeInTheDocument();

    // And the shell itself never remounted across the folder switch.
    expect(screen.getByTestId('app-shell-root')).toBeInTheDocument();
  });

  it('carries the new folder when selecting a message after switching folders', () => {
    render(<AppShell />);
    fireEvent.click(screen.getByTestId('folder-sent'));
    fireEvent.click(screen.getByTestId('message-row-SENT1'));
    // The carried folder is `sent`, not the default `inbox` (Codex F1: a bare
    // id would recreate the Inbox-only bug).
    expect(screen.getByTestId('reader-selection-debug')).toHaveTextContent('sent / SENT1');
  });
});
