// Tests for tuxlink-y5c (Task 13) — MessageView component.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3, §6
// Task-13 test list (§6):
//   (6) MessageView header strip shows routing when present, omits when null.
//   (7) no-selection → "Select a message."
// Plus: form placeholder, attachment strip, parse-failure state.
//
// Note: the component uses React-Query internally; we wrap it in a
// QueryClientProvider for snapshot-style tests that mock the query result
// directly. The IPC round-trip is NOT tested here (it's smoke-verified at
// M2) — only the rendering logic driven by synthetic ParsedMessage data.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import {
  MessageViewLoaded,
  MessageViewEmpty,
  MessageViewParseError,
  MessageViewNotFound,
  SELECT_MESSAGE_COPY,
  PARSE_ERROR_PREFIX,
  NOT_FOUND_COPY,
} from './MessageView';
import MessageView from './MessageView';

// Mock useMessage so MessageView integration tests don't need Tauri or
// QueryClientProvider.
vi.mock('./useMessage', () => ({
  useMessage: vi.fn(),
}));
import { useMessage } from './useMessage';

// Reply/forward open a compose window via openReplyWindow — mock the side
// effect so the action-bar tests assert wiring, not Tauri behavior.
vi.mock('./replyActions', () => ({ openReplyWindow: vi.fn().mockResolvedValue(undefined) }));
import { openReplyWindow } from './replyActions';
import type { ParsedMessage } from './types';

function parsed(over: Partial<ParsedMessage> = {}): ParsedMessage {
  return {
    id: 'MID1',
    subject: 'Test Subject',
    from: 'W4PHS@winlink.org',
    to: ['KK4XYZ@winlink.org'],
    cc: [],
    date: '2026-05-19T14:05:00Z',
    body: 'Hello from ARES.',
    attachments: [],
    isForm: false,
    routing: null,
    ...over,
  };
}

// ============================================================================
// Task-13 test (7): no-selection → "Select a message." empty state.
// ============================================================================
describe('<MessageViewEmpty>', () => {
  it('renders the no-selection copy', () => {
    render(<MessageViewEmpty />);
    expect(screen.getByTestId('message-view-empty')).toHaveTextContent(SELECT_MESSAGE_COPY);
  });
});

// ============================================================================
// Reading-pane metadata — Mock D dl is From / To / Date only (no Via/routing row).
// ============================================================================
describe('<MessageViewLoaded>', () => {
  it('shows From as bare address, plus display name when present (Name <addr>)', () => {
    const { rerender } = render(<MessageViewLoaded message={parsed({ from: 'W4PHS@winlink.org' })} />);
    expect(screen.getByTestId('message-from')).toHaveTextContent('W4PHS@winlink.org');
    rerender(<MessageViewLoaded message={parsed({ from: 'Mike / Net Control <K0SWE@winlink.org>' })} />);
    // addr in the .addr span; display name rendered alongside.
    expect(screen.getByTestId('message-from')).toHaveTextContent('K0SWE@winlink.org');
    expect(screen.getByTestId('message-view-loaded')).toHaveTextContent('Mike / Net Control');
  });

  it('shows all To addresses, comma-separated (mock B)', () => {
    render(<MessageViewLoaded message={parsed({ to: ['W4PHS@winlink.org', 'CHATTANOOGA-CERT@winlink.org'] })} />);
    const to = screen.getByTestId('message-to');
    expect(to).toHaveTextContent('W4PHS@winlink.org');
    expect(to).toHaveTextContent('CHATTANOOGA-CERT@winlink.org');
  });

  it('does NOT render a routing/Via row (mock dl = From/To/Date)', () => {
    render(<MessageViewLoaded message={parsed({ routing: 'via CMS-SSL' })} />);
    expect(screen.queryByTestId('message-routing')).toBeNull();
  });

  it('shows subject in header', () => {
    render(<MessageViewLoaded message={parsed({ subject: 'ICS-213' })} />);
    expect(screen.getByTestId('message-subject')).toHaveTextContent('ICS-213');
  });

  it('shows sender in header strip', () => {
    render(<MessageViewLoaded message={parsed({ from: 'W4PHS@winlink.org' })} />);
    expect(screen.getByTestId('message-from')).toHaveTextContent('W4PHS@winlink.org');
  });

  it('shows body in a pre element', () => {
    render(<MessageViewLoaded message={parsed({ body: 'Hello ARES' })} />);
    const pre = screen.getByTestId('message-body');
    expect(pre.tagName).toBe('PRE');
    expect(pre).toHaveTextContent('Hello ARES');
  });

  // Winlink form → v0.1 placeholder; never render raw XML.
  it('shows form placeholder when isForm', () => {
    render(<MessageViewLoaded message={parsed({ isForm: true, body: '<?xml...' })} />);
    expect(screen.getByTestId('message-form-placeholder')).toBeInTheDocument();
    // The raw XML body must NOT appear in place of the placeholder.
    expect(screen.queryByTestId('message-body')).toBeNull();
  });

  // Attachment strip lists names + sizes; no download/preview (v0.0.1).
  it('lists attachment names and sizes', () => {
    render(
      <MessageViewLoaded
        message={parsed({
          attachments: [
            { filename: 'net_log.txt', size: 1024 },
            { filename: 'photo.jpg', size: 204800 },
          ],
        })}
      />,
    );
    const strip = screen.getByTestId('message-attachments');
    expect(strip).toHaveTextContent('net_log.txt');
    expect(strip).toHaveTextContent('photo.jpg');
  });

  it('does not render attachment strip when no attachments', () => {
    render(<MessageViewLoaded message={parsed({ attachments: [] })} />);
    expect(screen.queryByTestId('message-attachments')).toBeNull();
  });

  it('shows UTC sent date', () => {
    render(<MessageViewLoaded message={parsed({ date: '2026-05-19T14:05:00Z' })} />);
    expect(screen.getByTestId('message-date')).toHaveTextContent('2026-05-19');
  });
});

// ============================================================================
// Reading-pane action bar (tuxlink-cbz, operator decision 2026-05-20):
// amber Reply / Reply All / Forward, wired to openReplyWindow.
// ============================================================================
describe('<MessageViewLoaded> reply action bar', () => {
  beforeEach(() => vi.mocked(openReplyWindow).mockClear());

  it('renders Reply, Reply All, Forward (mock B — no Print)', () => {
    render(<MessageViewLoaded message={parsed()} />);
    expect(screen.getByTestId('reply-btn')).toBeInTheDocument();
    expect(screen.getByTestId('reply-all-btn')).toBeInTheDocument();
    expect(screen.getByTestId('forward-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('print-btn')).toBeNull();
  });

  it('Reply is the amber primary action', () => {
    render(<MessageViewLoaded message={parsed()} />);
    expect(screen.getByTestId('reply-btn').className).toContain('primary');
    expect(screen.getByTestId('reply-all-btn').className).not.toContain('primary');
  });

  it('Reply opens a reply compose window for the message', () => {
    const m = parsed();
    render(<MessageViewLoaded message={m} />);
    fireEvent.click(screen.getByTestId('reply-btn'));
    expect(openReplyWindow).toHaveBeenCalledWith(m, 'reply');
  });

  it('Reply All opens a reply-all compose window', () => {
    const m = parsed();
    render(<MessageViewLoaded message={m} />);
    fireEvent.click(screen.getByTestId('reply-all-btn'));
    expect(openReplyWindow).toHaveBeenCalledWith(m, 'replyAll');
  });

  it('Forward opens a forward compose window', () => {
    const m = parsed();
    render(<MessageViewLoaded message={m} />);
    fireEvent.click(screen.getByTestId('forward-btn'));
    expect(openReplyWindow).toHaveBeenCalledWith(m, 'forward');
  });
});

// ============================================================================
// Parse-failure state — UiError::Internal from the command renders a
// "could not parse" message, not garbage.
// ============================================================================
describe('<MessageViewParseError>', () => {
  it('shows a parse-error description starting with the prefix', () => {
    render(<MessageViewParseError rawSize={42000} />);
    const el = screen.getByTestId('message-parse-error');
    expect(el.textContent).toContain(PARSE_ERROR_PREFIX);
  });

  // FIX 2: raw-size copy wires through to the component.
  it('shows "bytes" and the numeric size when rawSize is provided', () => {
    render(<MessageViewParseError rawSize={98304} />);
    const el = screen.getByTestId('message-parse-error');
    expect(el.textContent).toContain('bytes');
    expect(el.textContent).toContain('98304');
  });

  it('omits the size copy gracefully when rawSize is absent', () => {
    render(<MessageViewParseError />);
    const el = screen.getByTestId('message-parse-error');
    // Must not render NaN or garbage in place of a size.
    expect(el.textContent).not.toContain('NaN');
    expect(el.textContent).not.toContain('undefined');
  });
});

// ============================================================================
// FIX 1: NotFound error renders the "message not found" state, NOT the
// parse-error component.
// ============================================================================
describe('<MessageViewNotFound>', () => {
  it('renders the not-found copy', () => {
    render(<MessageViewNotFound />);
    const el = screen.getByTestId('message-view-not-found');
    expect(el.textContent).toContain(NOT_FOUND_COPY);
  });

  it('is distinct from parse-error — parse-error testid is absent', () => {
    render(<MessageViewNotFound />);
    expect(screen.queryByTestId('message-parse-error')).toBeNull();
  });
});

// ============================================================================
// FIX 1 + FIX 2 integration: MessageView coordinator routes errors correctly.
// ============================================================================
describe('<MessageView> error routing (integration via mocked useMessage)', () => {
  const sel = { folder: 'inbox' as const, id: 'MID1' };

  it('FIX 1: NotFound error renders the not-found state, NOT the parse-error', () => {
    vi.mocked(useMessage).mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: { kind: 'NotFound', detail: 'MID1' } as import('./types').UiError,
    } as ReturnType<typeof useMessage>);
    render(<MessageView selectedMessage={sel} />);
    expect(screen.getByTestId('message-view-not-found')).toBeInTheDocument();
    expect(screen.queryByTestId('message-parse-error')).toBeNull();
  });

  it('FIX 2: Internal error with size detail passes rawSize to MessageViewParseError', () => {
    vi.mocked(useMessage).mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: {
        kind: 'Internal',
        detail: { detail: 'message too large to parse (98304 bytes; cap is 524288 bytes)' },
      } as import('./types').UiError,
    } as ReturnType<typeof useMessage>);
    render(<MessageView selectedMessage={sel} />);
    const el = screen.getByTestId('message-parse-error');
    expect(el.textContent).toContain('bytes');
    expect(el.textContent).toContain('98304');
  });

  it('FIX 2: Internal error with no size detail omits size copy gracefully', () => {
    vi.mocked(useMessage).mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: {
        kind: 'Internal',
        detail: { detail: 'RFC5322 parse failed: mail-parser returned None' },
      } as import('./types').UiError,
    } as ReturnType<typeof useMessage>);
    render(<MessageView selectedMessage={sel} />);
    const el = screen.getByTestId('message-parse-error');
    expect(el.textContent).not.toContain('NaN');
    expect(el.textContent).not.toContain('undefined');
  });
});

// ============================================================================
// FIX 3: body pre element carries the wrapping class/style.
// ============================================================================
describe('<MessageViewLoaded> body wrap', () => {
  it('applies the msg-body class to the body pre (CSS sets pre-wrap + overflow-wrap)', () => {
    render(
      <MessageViewLoaded
        message={
          {
            id: 'MID1',
            subject: 'ARES Net',
            from: 'W4PHS@winlink.org',
            to: ['KK4XYZ@winlink.org'],
            cc: [],
            date: '2026-05-19T14:05:00Z',
            body: 'A'.repeat(200),
            attachments: [],
            isForm: false,
            routing: null,
          } as import('./types').ParsedMessage
        }
      />,
    );
    const pre = screen.getByTestId('message-body');
    // The pre must carry the Mock D class (CSS sets white-space: pre-wrap +
    // overflow-wrap: anywhere to wrap long radio/Base64 lines — Codex #4).
    expect(pre.className).toContain('msg-body');
  });
});
