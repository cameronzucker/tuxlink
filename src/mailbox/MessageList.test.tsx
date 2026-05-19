import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  MessageRow,
  MessageList,
  formatListDate,
  formatSize,
  toColumnLabel,
  EMPTY_FOLDER_COPY,
  NOT_CONNECTED_COPY,
} from './MessageList';
import type { MessageMeta } from './types';

function meta(over: Partial<MessageMeta> = {}): MessageMeta {
  return {
    id: 'MID1',
    subject: 'Hello',
    from: 'W4PHS@winlink.org',
    to: [],
    date: '2026-05-19T14:05:00Z',
    unread: false,
    bodySize: 2048,
    hasAttachments: false,
    ...over,
  };
}

describe('formatters', () => {
  it('formatListDate renders compact UTC label', () => {
    expect(formatListDate('2026-05-19T14:05:00Z')).toBe('2026-05-19 14:05Z');
  });

  it('formatListDate falls back to raw on unparseable input', () => {
    expect(formatListDate('not-a-date')).toBe('not-a-date');
  });

  it('formatSize is empty for zero, scales otherwise', () => {
    expect(formatSize(0)).toBe('');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(2048)).toBe('2.0 KB');
  });

  it('toColumnLabel: recipients win; Inbox falls back to sender; Sent blank', () => {
    expect(toColumnLabel(meta({ to: ['A', 'B'] }), 'sent')).toBe('A, B');
    expect(toColumnLabel(meta({ to: [], from: 'X' }), 'inbox')).toBe('X');
    expect(toColumnLabel(meta({ to: [] }), 'sent')).toBe('');
  });
});

describe('<MessageRow>', () => {
  // Task-12 test (3): renders subject/from/to/size for a row.
  it('renders subject, from, to, and size columns', () => {
    render(
      <MessageRow
        message={meta({ subject: 'ICS-213', from: 'KK4XYZ', to: ['W4PHS'], bodySize: 2048 })}
        folder="sent"
        selected={false}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('row-subject')).toHaveTextContent('ICS-213');
    expect(screen.getByTestId('row-from')).toHaveTextContent('KK4XYZ');
    expect(screen.getByTestId('row-to')).toHaveTextContent('W4PHS');
    expect(screen.getByTestId('row-size')).toHaveTextContent('2.0 KB');
  });

  // Task-12 test (5): unread row gets the `unread` class (+ bold weight).
  it('unread row carries the unread class and bold weight', () => {
    render(<MessageRow message={meta({ unread: true })} folder="inbox" selected={false} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('unread');
    expect(row).toHaveStyle({ fontWeight: '700' });
  });

  it('read row carries the read class and normal weight', () => {
    render(<MessageRow message={meta({ unread: false })} folder="inbox" selected={false} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('read');
    expect(row).toHaveStyle({ fontWeight: '400' });
  });

  it('shows the attachment marker only when hasAttachments', () => {
    const { rerender } = render(
      <MessageRow message={meta({ hasAttachments: true })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-attach')).toHaveTextContent('#');
    rerender(
      <MessageRow message={meta({ hasAttachments: false })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-attach')).toHaveTextContent('');
  });

  it('click and Enter both fire onSelect with the id', () => {
    const onSelect = vi.fn();
    render(<MessageRow message={meta()} folder="inbox" selected={false} onSelect={onSelect} />);
    const row = screen.getByTestId('message-row-MID1');
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledTimes(2);
    expect(onSelect).toHaveBeenCalledWith('MID1');
  });
});

describe('<MessageList>', () => {
  // Task-12 test (4): empty-folder → empty-state copy.
  it('shows the empty-folder copy when there are no messages', () => {
    render(<MessageList folder="inbox" messages={[]} selectedId={null} onSelect={() => {}} />);
    expect(screen.getByTestId('message-list-empty')).toHaveTextContent(EMPTY_FOLDER_COPY);
  });

  it('shows the not-connected copy when offline', () => {
    render(<MessageList folder="inbox" messages={[]} selectedId={null} onSelect={() => {}} notConnected />);
    expect(screen.getByTestId('message-list-empty')).toHaveTextContent(NOT_CONNECTED_COPY);
  });

  it('mounts the virtualized list container when messages exist', () => {
    // react-virtuoso renders into a zero-height scroller under jsdom, so we
    // assert the container mounts (row output is covered by <MessageRow>).
    render(
      <MessageList folder="inbox" messages={[meta()]} selectedId={null} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
  });
});
