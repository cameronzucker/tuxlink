import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  MessageRow,
  MessageList,
  formatListDate,
  formatSize,
  correspondentLabel,
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

  // The compact (mock-d) row shows ONE correspondent, not From+To columns:
  // Inbox/Drafts/Deleted -> the sender is salient; Sent/Outbox -> the
  // recipient(s). Unlike the old toColumnLabel, Inbox ALWAYS shows the sender
  // even when `to` is populated.
  it('correspondentLabel: inbox shows sender, sent shows recipients', () => {
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: [] }), 'inbox')).toBe('KK4OBN');
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: ['W6ABC'] }), 'inbox')).toBe('KK4OBN');
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: ['W6ABC', 'W7DEF'] }), 'sent')).toBe(
      'W6ABC, W7DEF',
    );
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: [] }), 'sent')).toBe('KK4OBN');
  });
});

describe('<MessageRow> (compact 2-line, mock-d)', () => {
  it('renders correspondent, subject, and date', () => {
    render(
      <MessageRow
        message={meta({ subject: 'ICS-213', from: 'KK4XYZ', to: [], date: '2026-05-19T14:05:00Z' })}
        folder="inbox"
        selected={false}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('KK4XYZ');
    expect(screen.getByTestId('row-subject')).toHaveTextContent('ICS-213');
    expect(screen.getByTestId('row-date')).toHaveTextContent('2026-05-19 14:05Z');
  });

  it('inbox row shows the sender; sent row shows the recipient', () => {
    const m = meta({ from: 'KK4OBN', to: ['W6ABC'] });
    const { rerender } = render(
      <MessageRow message={m} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('KK4OBN');
    rerender(<MessageRow message={m} folder="sent" selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('W6ABC');
  });

  it('unread row carries the unread class and shows the unread dot', () => {
    render(<MessageRow message={meta({ unread: true })} folder="inbox" selected={false} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('unread');
    expect(screen.getByTestId('row-unread-dot')).toBeInTheDocument();
  });

  it('read row carries the read class and shows no unread dot', () => {
    render(<MessageRow message={meta({ unread: false })} folder="inbox" selected={false} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('read');
    expect(screen.queryByTestId('row-unread-dot')).toBeNull();
  });

  it('selected row carries the selected class', () => {
    render(<MessageRow message={meta()} folder="inbox" selected={true} onSelect={() => {}} />);
    expect(screen.getByTestId('message-row-MID1').className).toContain('selected');
  });

  it('shows the attachment marker only when hasAttachments', () => {
    const { rerender } = render(
      <MessageRow message={meta({ hasAttachments: true })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-attach')).toBeInTheDocument();
    rerender(
      <MessageRow message={meta({ hasAttachments: false })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('row-attach')).toBeNull();
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
  it('shows the empty-folder copy when there are no messages', () => {
    render(<MessageList folder="inbox" messages={[]} selectedId={null} onSelect={() => {}} />);
    expect(screen.getByTestId('message-list-empty')).toHaveTextContent(EMPTY_FOLDER_COPY);
  });

  it('shows the not-connected copy when offline', () => {
    render(<MessageList folder="inbox" messages={[]} selectedId={null} onSelect={() => {}} notConnected />);
    expect(screen.getByTestId('message-list-empty')).toHaveTextContent(NOT_CONNECTED_COPY);
  });

  it('mounts the virtualized list container when messages exist', () => {
    render(
      <MessageList folder="inbox" messages={[meta()]} selectedId={null} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
  });
});
