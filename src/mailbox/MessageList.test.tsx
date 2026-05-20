import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  MessageRow,
  MessageList,
  formatRowDate,
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

  // formatRowDate: compact Mail.app-style smart date in UTC (now is injectable).
  describe('formatRowDate', () => {
    const now = new Date('2026-05-20T15:00:00Z');
    it('today → UTC time-of-day with a Z marker', () => {
      expect(formatRowDate('2026-05-20T12:18:00Z', now)).toBe('12:18Z');
      // a clock-skew future timestamp still reads as time-of-day, not negative
      expect(formatRowDate('2026-05-20T23:59:00Z', now)).toBe('23:59Z');
    });
    it('yesterday → "Yesterday"', () => {
      expect(formatRowDate('2026-05-19T14:05:00Z', now)).toBe('Yesterday');
    });
    it('within a week → "N days ago"', () => {
      expect(formatRowDate('2026-05-18T10:00:00Z', now)).toBe('2 days ago');
      expect(formatRowDate('2026-05-14T10:00:00Z', now)).toBe('6 days ago');
    });
    it('a week or more → calendar date (UTC)', () => {
      expect(formatRowDate('2026-05-13T10:00:00Z', now)).toBe('2026-05-13');
      expect(formatRowDate('2020-01-15T09:00:00Z', now)).toBe('2020-01-15');
    });
    it('falls back to raw on unparseable input', () => {
      expect(formatRowDate('not-a-date', now)).toBe('not-a-date');
    });
  });

  it('formatSize is empty for zero, scales otherwise', () => {
    expect(formatSize(0)).toBe('');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(2048)).toBe('2.0 KB');
  });

  // Mock D rows show ONE correspondent: Inbox/Drafts/Deleted → the sender;
  // Sent/Outbox → the recipient(s).
  it('correspondentLabel: inbox shows sender, sent shows recipients', () => {
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: [] }), 'inbox')).toBe('KK4OBN');
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: ['W6ABC'] }), 'inbox')).toBe('KK4OBN');
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: ['W6ABC', 'W7DEF'] }), 'sent')).toBe(
      'W6ABC, W7DEF',
    );
    expect(correspondentLabel(meta({ from: 'KK4OBN', to: [] }), 'sent')).toBe('KK4OBN');
  });
});

describe('<MessageRow> (3-line, Mock D)', () => {
  it('renders correspondent, subject, date, and size', () => {
    render(
      <MessageRow
        // an old date → stable absolute formatRowDate output
        message={meta({ subject: 'Net check-in', from: 'KK4XYZ', date: '2024-03-09T14:05:00Z', bodySize: 2458 })}
        folder="inbox"
        selected={false}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('KK4XYZ');
    expect(screen.getByTestId('row-subject')).toHaveTextContent('Net check-in');
    expect(screen.getByTestId('row-date')).toHaveTextContent('2024-03-09');
    expect(screen.getByTestId('row-size')).toHaveTextContent('2.4 KB');
  });

  it('renders the preview line when present, omits it when absent', () => {
    const { rerender } = render(
      <MessageRow
        message={meta({ preview: 'NWS Memphis has issued a severe thunderstorm watch…' })}
        folder="inbox"
        selected={false}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('row-preview')).toHaveTextContent('NWS Memphis');
    rerender(
      <MessageRow message={meta({ preview: undefined })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('row-preview')).toBeNull();
  });

  it('renders the form-tag badge only when formTag is set', () => {
    const { rerender } = render(
      <MessageRow message={meta({ formTag: 'ICS-213' })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-form-tag')).toHaveTextContent('ICS-213');
    rerender(
      <MessageRow message={meta({ formTag: undefined })} folder="inbox" selected={false} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('row-form-tag')).toBeNull();
  });

  it('omits the size when bodySize is zero', () => {
    render(<MessageRow message={meta({ bodySize: 0 })} folder="inbox" selected={false} onSelect={() => {}} />);
    expect(screen.queryByTestId('row-size')).toBeNull();
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
    expect(row.className).toContain('row');
    expect(row.className).toContain('unread');
    expect(screen.getByTestId('row-unread-dot')).toBeInTheDocument();
  });

  it('read row has no unread class and no unread dot', () => {
    render(<MessageRow message={meta({ unread: false })} folder="inbox" selected={false} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('row');
    expect(row.className).not.toContain('unread');
    expect(screen.queryByTestId('row-unread-dot')).toBeNull();
  });

  it('selected row carries the selected class', () => {
    render(<MessageRow message={meta()} folder="inbox" selected={true} onSelect={() => {}} />);
    expect(screen.getByTestId('message-row-MID1').className).toContain('selected');
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
  it('renders the rows-pane root', () => {
    render(<MessageList folder="inbox" messages={[]} selectedId={null} onSelect={() => {}} />);
    expect(screen.getByTestId('rows-pane')).toBeInTheDocument();
  });

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
