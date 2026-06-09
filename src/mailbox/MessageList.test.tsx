import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ReactNode } from 'react';
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

// react-virtuoso renders into a zero-height scroller under jsdom and does not
// call itemContent, so individual rows are invisible to queries. Mock it to
// render items directly so multi-select tests can fire events on row elements.
// This mock is scoped to this file; existing tests that only probe the list
// container (`message-list`, `message-list-empty`) are unaffected.
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
    it('today → time-of-day HH:MM (UTC clock, no Z — matches the mock)', () => {
      expect(formatRowDate('2026-05-20T12:18:00Z', now)).toBe('12:18');
      // a clock-skew future timestamp still reads as time-of-day, not negative
      expect(formatRowDate('2026-05-20T23:59:00Z', now)).toBe('23:59');
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

// Convenience: a no-op onRowClick for MessageRow tests that don't exercise
// click modifier semantics (the handler is required on the interface).
const noopRowClick = () => {};

describe('<MessageRow> (3-line, Mock D)', () => {
  it('renders correspondent, subject, date, and size', () => {
    render(
      <MessageRow
        // an old date → stable absolute formatRowDate output
        message={meta({ subject: 'Net check-in', from: 'KK4XYZ', date: '2024-03-09T14:05:00Z', bodySize: 2458 })}
        folder="inbox"
        isOpen={false}
        inSelection={false}
        onRowClick={noopRowClick}
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
        isOpen={false}
        inSelection={false}
        onRowClick={noopRowClick}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('row-preview')).toHaveTextContent('NWS Memphis');
    rerender(
      <MessageRow message={meta({ preview: undefined })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('row-preview')).toBeNull();
  });

  it('renders the form-tag badge only when formTag is set', () => {
    const { rerender } = render(
      <MessageRow message={meta({ formTag: 'ICS-213' })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-form-tag')).toHaveTextContent('ICS-213');
    rerender(
      <MessageRow message={meta({ formTag: undefined })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('row-form-tag')).toBeNull();
  });

  it('omits the size when bodySize is zero', () => {
    render(<MessageRow message={meta({ bodySize: 0 })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    expect(screen.queryByTestId('row-size')).toBeNull();
  });

  it('inbox row shows the sender; sent row shows the recipient', () => {
    const m = meta({ from: 'KK4OBN', to: ['W6ABC'] });
    const { rerender } = render(
      <MessageRow message={m} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />,
    );
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('KK4OBN');
    rerender(<MessageRow message={m} folder="sent" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    expect(screen.getByTestId('row-correspondent')).toHaveTextContent('W6ABC');
  });

  it('unread row carries the unread class and shows the unread dot', () => {
    render(<MessageRow message={meta({ unread: true })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('row');
    expect(row.className).toContain('unread');
    expect(screen.getByTestId('row-unread-dot')).toBeInTheDocument();
  });

  it('read row has no unread class and no unread dot', () => {
    render(<MessageRow message={meta({ unread: false })} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    const row = screen.getByTestId('message-row-MID1');
    expect(row.className).toContain('row');
    expect(row.className).not.toContain('unread');
    expect(screen.queryByTestId('row-unread-dot')).toBeNull();
  });

  it('isOpen row carries the selected class', () => {
    render(<MessageRow message={meta()} folder="inbox" isOpen={true} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    expect(screen.getByTestId('message-row-MID1').className).toContain('selected');
  });

  it('click fires onRowClick and Enter fires onSelect with the id', () => {
    const onRowClick = vi.fn();
    const onSelect = vi.fn();
    render(<MessageRow message={meta()} folder="inbox" isOpen={false} inSelection={false} onRowClick={onRowClick} onSelect={onSelect} />);
    const row = screen.getByTestId('message-row-MID1');
    fireEvent.click(row);
    expect(onRowClick).toHaveBeenCalledTimes(1);
    expect(onRowClick).toHaveBeenCalledWith('MID1', { ctrl: false, shift: false });
    fireEvent.keyDown(row, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith('MID1');
  });
});

describe('<MessageRow> — highlight + folder-tag (find-messages Task 17)', () => {
  it('renders <mark> around matched ranges when matchHighlight is provided', () => {
    const m: MessageMeta = {
      id: 'm1', subject: 'DAMAGE report', from: 'KX5DD', to: ['N7CPZ'],
      date: '2024-05-20T10:13:00Z', unread: true, bodySize: 100, hasAttachments: false,
    };
    render(<MessageRow message={m} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}}
                       matchHighlight={[{ field: 'subject', start: 0, end: 6 }]} />);
    const mark = screen.getByTestId('row-subject').querySelector('mark');
    expect(mark).not.toBeNull();
    expect(mark).toHaveTextContent('DAMAGE');
  });

  it('renders folder badge when showFolderTag and message.folder set', () => {
    const m: MessageMeta = {
      id: 'm1', subject: 'x', from: 'y', to: ['z'],
      date: '2024-05-20T10:13:00Z', unread: false, bodySize: 0, hasAttachments: false,
      folder: 'sent',
    };
    render(<MessageRow message={m} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} showFolderTag />);
    expect(screen.getByTestId('row-folder-tag')).toHaveTextContent(/sent/i);
  });

  it('does not render folder badge when showFolderTag is absent', () => {
    const m: MessageMeta = {
      id: 'm1', subject: 'x', from: 'y', to: ['z'],
      date: '2024-05-20T10:13:00Z', unread: false, bodySize: 0, hasAttachments: false,
      folder: 'sent',
    };
    render(<MessageRow message={m} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    expect(screen.queryByTestId('row-folder-tag')).toBeNull();
  });

  it('renders subject without mark when matchHighlight is absent', () => {
    const m: MessageMeta = {
      id: 'm1', subject: 'Hello world', from: 'y', to: ['z'],
      date: '2024-05-20T10:13:00Z', unread: false, bodySize: 0, hasAttachments: false,
    };
    render(<MessageRow message={m} folder="inbox" isOpen={false} inSelection={false} onRowClick={noopRowClick} onSelect={() => {}} />);
    const subject = screen.getByTestId('row-subject');
    expect(subject.querySelector('mark')).toBeNull();
    expect(subject).toHaveTextContent('Hello world');
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

// Three-message fixture sorted date-desc (M1 newest → M3 oldest) so the
// rendered order under the default sort is M1, M2, M3 — matching the range
// test assertions in the multi-select suite below.
const M1 = meta({ id: 'M1', date: '2026-06-03T10:00:00Z' });
const M2 = meta({ id: 'M2', date: '2026-06-02T10:00:00Z' });
const M3 = meta({ id: 'M3', date: '2026-06-01T10:00:00Z' });
const THREE_MSGS = [M1, M2, M3];

describe('<MessageList> — multi-select / selection set (tuxlink-etxt Task 8)', () => {
  it('Ctrl+click adds a row to the selection without opening it', () => {
    const onSelect = vi.fn();
    const onSelectionChange = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={onSelect}
        selectedIds={new Set()}
        onSelectionChange={onSelectionChange}
      />,
    );
    fireEvent.click(screen.getByTestId('message-row-M2'), { ctrlKey: true });
    expect(onSelect).not.toHaveBeenCalled();
    expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M2']));
  });

  it('plain click opens and clears the selection set', () => {
    const onSelect = vi.fn();
    const onSelectionChange = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={onSelect}
        selectedIds={new Set(['M2'])}
        onSelectionChange={onSelectionChange}
      />,
    );
    fireEvent.click(screen.getByTestId('message-row-M1'));
    expect(onSelect).toHaveBeenCalledWith('M1');
    expect(onSelectionChange).toHaveBeenLastCalledWith(new Set());
  });

  it('Shift+click selects the contiguous range from the anchor', () => {
    const onSelect = vi.fn();
    const onSelectionChange = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={onSelect}
        selectedIds={new Set()}
        onSelectionChange={onSelectionChange}
      />,
    );
    // Anchor M1 via Ctrl+click, then extend to M3 via Shift+click.
    fireEvent.click(screen.getByTestId('message-row-M1'), { ctrlKey: true });
    fireEvent.click(screen.getByTestId('message-row-M3'), { shiftKey: true });
    expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M1', 'M2', 'M3']));
  });

  it('rows in the selection set carry the in-selection class', () => {
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M2'])}
        onSelectionChange={() => {}}
      />,
    );
    expect(screen.getByTestId('message-row-M2').className).toContain('in-selection');
    expect(screen.getByTestId('message-row-M1').className).not.toContain('in-selection');
  });
});

describe('<MessageList> — sort wiring (tuxlink-2x0l)', () => {
  it('omits the sort header when onSortStateChange is absent', () => {
    render(<MessageList folder="inbox" messages={[meta()]} selectedId={null} onSelect={() => {}} />);
    expect(screen.queryByTestId('rows-pane-header')).toBeNull();
    expect(screen.queryByTestId('message-list-sort-trigger')).toBeNull();
  });

  it('renders the sort header (icon trigger only — popup is lazily portaled)', () => {
    render(
      <MessageList
        folder="inbox"
        messages={[meta()]}
        selectedId={null}
        onSelect={() => {}}
        sortState={{ key: 'date', direction: 'desc' }}
        onSortStateChange={() => {}}
      />,
    );
    expect(screen.getByTestId('rows-pane-header')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-trigger')).toBeInTheDocument();
    // Popup not open yet.
    expect(screen.queryByTestId('message-list-sort-menu')).toBeNull();
  });

  it('opening the popup and picking a key fires onSortStateChange with key+direction', () => {
    const onSortStateChange = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={[meta()]}
        selectedId={null}
        onSelect={() => {}}
        sortState={{ key: 'date', direction: 'desc' }}
        onSortStateChange={onSortStateChange}
      />,
    );
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    fireEvent.click(screen.getByTestId('message-list-sort-key-size'));
    expect(onSortStateChange).toHaveBeenCalledWith({ key: 'size', direction: 'desc' });
  });

  it('reflects sortState through to the popup (controlled-input contract)', () => {
    const { rerender } = render(
      <MessageList
        folder="inbox"
        messages={[meta()]}
        selectedId={null}
        onSelect={() => {}}
        sortState={{ key: 'subject', direction: 'asc' }}
        onSortStateChange={() => {}}
      />,
    );
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-key-subject')).toHaveAttribute('aria-checked', 'true');
    // Rerender with a new sortState while the popup is still open. Radix's
    // RadioGroup updates the checked indicator in-place from the controlled
    // `value` prop without remounting the items.
    rerender(
      <MessageList
        folder="inbox"
        messages={[meta()]}
        selectedId={null}
        onSelect={() => {}}
        sortState={{ key: 'date', direction: 'asc' }}
        onSortStateChange={() => {}}
      />,
    );
    expect(screen.getByTestId('message-list-sort-key-date')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('message-list-sort-key-subject')).toHaveAttribute('aria-checked', 'false');
  });
});
