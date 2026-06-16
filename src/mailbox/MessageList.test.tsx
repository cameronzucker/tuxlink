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

  it('click fires onRowClick and Enter also fires onRowClick (plain-click path)', () => {
    const onRowClick = vi.fn();
    const onSelect = vi.fn();
    render(<MessageRow message={meta()} folder="inbox" isOpen={false} inSelection={false} onRowClick={onRowClick} onSelect={onSelect} />);
    const row = screen.getByTestId('message-row-MID1');
    // Plain mouse click → onRowClick with ctrl:false/shift:false
    fireEvent.click(row);
    expect(onRowClick).toHaveBeenCalledTimes(1);
    expect(onRowClick).toHaveBeenCalledWith('MID1', { ctrl: false, shift: false });
    onRowClick.mockClear();
    // Enter key → same plain-click path via onRowClick (Fix 2: clears selection + opens)
    fireEvent.keyDown(row, { key: 'Enter' });
    expect(onRowClick).toHaveBeenCalledTimes(1);
    expect(onRowClick).toHaveBeenCalledWith('MID1', { ctrl: false, shift: false });
    // onSelect is called internally by onRowClick's plain-click branch — but at
    // the MessageRow level the call goes via onRowClick, not onSelect directly.
    expect(onSelect).not.toHaveBeenCalled();
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

  it('Ctrl+click on an already-selected row removes it from the selection', () => {
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
    fireEvent.click(screen.getByTestId('message-row-M2'), { ctrlKey: true });
    expect(onSelect).not.toHaveBeenCalled();
    expect(onSelectionChange).toHaveBeenLastCalledWith(new Set()); // M2 toggled off
  });

  it('keyboard contract: Enter opens, Space toggles selection (does not open)', () => {
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
    fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('M2');

    onSelect.mockClear();
    fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: ' ' });
    expect(onSelect).not.toHaveBeenCalled();                      // Space no longer opens
    expect(onSelectionChange).toHaveBeenLastCalledWith(new Set(['M2'])); // Space toggles into the set
  });

  // Fix 2 (Codex P2): Enter must clear the selection set, just like a plain
  // mouse click. Before the fix, Enter called onSelect directly, bypassing the
  // onRowClick plain-click path that clears the set — leaving stale selection
  // highlights after keyboard-opening a row.
  it('Enter clears the selection set and opens the message (Fix 2)', () => {
    const onSelect = vi.fn();
    const onSelectionChange = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={onSelect}
        selectedIds={new Set(['M1', 'M3'])}
        onSelectionChange={onSelectionChange}
      />,
    );
    // Enter on M2 while M1+M3 are in the selection set: must open M2 AND clear the set.
    fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('M2');
    expect(onSelectionChange).toHaveBeenCalledWith(new Set()); // selection cleared
  });
});

describe('<MessageList> — selection-aware context menu (tuxlink-l80q)', () => {
  function ctxProps() {
    return {
      onMoveMessage: vi.fn(),
      onArchiveMessage: vi.fn(),
      onSetReadState: vi.fn(),
      onBulkMove: vi.fn(),
      onBulkArchive: vi.fn(),
      onBulkSetReadState: vi.fn(),
      onSelectionChange: vi.fn(),
    };
  }

  it('right-click on a SELECTED row → selection-mode menu acting on the whole set', () => {
    const p = ctxProps();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M1', 'M2'])}
        {...p}
      />,
    );
    fireEvent.contextMenu(screen.getByTestId('message-row-M1'));
    // Selection-mode header reflects the whole set, not the one clicked row.
    expect(screen.getByTestId('ctx-selection-header')).toHaveTextContent('2 messages');

    fireEvent.click(screen.getByTestId('ctx-move-sent'));
    expect(p.onBulkMove).toHaveBeenCalledWith(new Set(['M1', 'M2']), 'sent');
    expect(p.onMoveMessage).not.toHaveBeenCalled();
  });

  it('selection-mode Archive + read items drive the bulk handlers', () => {
    const p = ctxProps();
    const { rerender } = render(
      <MessageList folder="inbox" messages={THREE_MSGS} selectedId={null} onSelect={() => {}} selectedIds={new Set(['M1', 'M2'])} {...p} />,
    );
    fireEvent.contextMenu(screen.getByTestId('message-row-M2'));
    fireEvent.click(screen.getByTestId('ctx-archive'));
    expect(p.onBulkArchive).toHaveBeenCalledWith(new Set(['M1', 'M2']));

    rerender(<MessageList folder="inbox" messages={THREE_MSGS} selectedId={null} onSelect={() => {}} selectedIds={new Set(['M1', 'M2'])} {...p} />);
    fireEvent.contextMenu(screen.getByTestId('message-row-M2'));
    fireEvent.click(screen.getByTestId('ctx-set-read'));
    expect(p.onBulkSetReadState).toHaveBeenCalledWith(new Set(['M1', 'M2']), true);
  });

  it('right-click on an UNSELECTED row → resets selection and acts single-target', () => {
    const p = ctxProps();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M1', 'M2'])}
        {...p}
      />,
    );
    // M3 is NOT in the selection.
    fireEvent.contextMenu(screen.getByTestId('message-row-M3'));
    // Selection resets to the clicked row (OS convention) and the menu is
    // single-target — the prior M1/M2 selection is abandoned.
    expect(p.onSelectionChange).toHaveBeenCalledWith(new Set(['M3']));
    expect(screen.queryByTestId('ctx-selection-header')).toBeNull();

    fireEvent.click(screen.getByTestId('ctx-move-sent'));
    expect(p.onMoveMessage).toHaveBeenCalledWith('M3', 'inbox', 'sent');
    expect(p.onBulkMove).not.toHaveBeenCalled();
  });

  it('right-click with no selection → single-target, no reset call', () => {
    const p = ctxProps();
    render(
      <MessageList folder="inbox" messages={THREE_MSGS} selectedId={null} onSelect={() => {}} selectedIds={new Set()} {...p} />,
    );
    fireEvent.contextMenu(screen.getByTestId('message-row-M2'));
    expect(screen.queryByTestId('ctx-selection-header')).toBeNull();
    fireEvent.click(screen.getByTestId('ctx-archive'));
    expect(p.onArchiveMessage).toHaveBeenCalledWith('M2', 'inbox');
    expect(p.onBulkArchive).not.toHaveBeenCalled();
  });
});

describe('<MessageList> — selection-aware drag (tuxlink-hh1j)', () => {
  const MSG_MIME = 'application/x-tuxlink-message';
  // DataTransfer stub that captures setData so we can assert the drag payload.
  function captureDt() {
    const store: Record<string, string> = {};
    return {
      store,
      dt: {
        setData: (mime: string, val: string) => {
          store[mime] = val;
        },
        getData: (mime: string) => store[mime] ?? '',
        types: [] as string[],
        dropEffect: '',
        effectAllowed: '',
      },
    };
  }

  it('dragging a row inside a multi-selection carries the WHOLE selection', () => {
    const { store, dt } = captureDt();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M1', 'M3'])}
        onSelectionChange={() => {}}
      />,
    );
    fireEvent.dragStart(screen.getByTestId('message-row-M1'), { dataTransfer: dt });
    const payload = JSON.parse(store[MSG_MIME]);
    expect(new Set(payload.ids)).toEqual(new Set(['M1', 'M3']));
  });

  it('dragging an UNSELECTED row carries only that row (single move)', () => {
    const { store, dt } = captureDt();
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M1', 'M3'])}
        onSelectionChange={() => {}}
      />,
    );
    // M2 is not part of the highlighted set — OS convention: drag just it.
    fireEvent.dragStart(screen.getByTestId('message-row-M2'), { dataTransfer: dt });
    const payload = JSON.parse(store[MSG_MIME]);
    expect(payload.ids).toEqual(['M2']);
  });

  it('a single-row selection drags just that row (no spurious bulk)', () => {
    const { store, dt } = captureDt();
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
    fireEvent.dragStart(screen.getByTestId('message-row-M2'), { dataTransfer: dt });
    const payload = JSON.parse(store[MSG_MIME]);
    expect(payload.ids).toEqual(['M2']);
  });
});

describe('<MessageList> — sort wiring (tuxlink-2x0l)', () => {
  it('omits the sort header when onSortStateChange is absent and selection is empty', () => {
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

// bd-tuxlink-y8tf: the "Filter by identity" select was removed from the
// message-list toolbar (it was the gray, unstyled dropdown). The toolbar now
// carries only the sort control / bulk bar; no identity filtering is applied.
describe('<MessageList> — no identity filter (bd-tuxlink-y8tf)', () => {
  const MIXED = [
    meta({ id: 'IA', date: '2026-06-03T10:00:00Z', identity: 'W1ABC', subject: 'from-w1abc' }),
    meta({ id: 'IB', date: '2026-06-02T10:00:00Z', identity: 'W7XYZ', subject: 'from-w7xyz' }),
    meta({ id: 'IC', date: '2026-06-01T10:00:00Z', subject: 'untagged' }),
  ];

  it('never renders the identity select', () => {
    render(
      <MessageList
        folder="inbox"
        messages={MIXED}
        selectedId={null}
        onSelect={() => {}}
        onSortStateChange={() => {}}
      />,
    );
    expect(screen.queryByTestId('mailbox-identity-filter')).toBeNull();
  });

  it('shows every message regardless of its identity tag (no filtering)', () => {
    render(<MessageList folder="inbox" messages={MIXED} selectedId={null} onSelect={() => {}} />);
    expect(screen.getByTestId('message-row-IA')).toBeInTheDocument();
    expect(screen.getByTestId('message-row-IB')).toBeInTheDocument();
    expect(screen.getByTestId('message-row-IC')).toBeInTheDocument();
  });
});

describe('<MessageList> — bulk bar (tuxlink-etxt Task 10)', () => {
  it('shows the bulk bar when a selection exists, even with no sort handler', () => {
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set(['M1', 'M2'])}
        onSelectionChange={() => {}}
      />,
    );
    expect(screen.getByRole('toolbar', { name: /selection actions/i })).toBeInTheDocument();
  });

  it('hides the bulk bar when the selection is empty', () => {
    render(
      <MessageList
        folder="inbox"
        messages={THREE_MSGS}
        selectedId={null}
        onSelect={() => {}}
        selectedIds={new Set()}
        onSelectionChange={() => {}}
        onSortStateChange={() => {}}
      />,
    );
    expect(screen.queryByRole('toolbar', { name: /selection actions/i })).toBeNull();
    // Sort control must return when selection clears (toggle-back contract).
    expect(screen.getByTestId('message-list-sort-trigger')).toBeInTheDocument();
  });
});

// M1 is unread in this fixture so pressing U should flip to read=true.
const M1_UNREAD = meta({ id: 'M1', date: '2026-06-03T10:00:00Z', unread: true });
const M2_READ = meta({ id: 'M2', date: '2026-06-02T10:00:00Z', unread: false });
const TWO_MSGS = [M1_UNREAD, M2_READ];

describe('<MessageList> — U keyboard shortcut (tuxlink-etxt Task 13)', () => {
  it('U toggles the focused message read-state (unread → read=true)', () => {
    const onSetReadState = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={TWO_MSGS}
        selectedId={null}
        onSelect={() => {}}
        onSetReadState={onSetReadState}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('message-row-M1'), { key: 'u' });
    expect(onSetReadState).toHaveBeenCalledWith('M1', expect.any(String), true);
  });

  it('U on a read message marks it unread (read → read=false)', () => {
    const onSetReadState = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={TWO_MSGS}
        selectedId={null}
        onSelect={() => {}}
        onSetReadState={onSetReadState}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('message-row-M2'), { key: 'u' });
    expect(onSetReadState).toHaveBeenCalledWith('M2', expect.any(String), false);
  });

  it('uppercase U also works', () => {
    const onSetReadState = vi.fn();
    render(
      <MessageList
        folder="inbox"
        messages={TWO_MSGS}
        selectedId={null}
        onSelect={() => {}}
        onSetReadState={onSetReadState}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('message-row-M1'), { key: 'U' });
    expect(onSetReadState).toHaveBeenCalledWith('M1', expect.any(String), true);
  });
});
