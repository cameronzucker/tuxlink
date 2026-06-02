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
import './../forms'; // side-effect: register ICS-213

// Mock useMessage so MessageView integration tests don't need Tauri or
// QueryClientProvider.
vi.mock('./useMessage', () => ({
  useMessage: vi.fn(),
}));
import { useMessage } from './useMessage';

// Reply/forward open a compose window via openReplyWindow — mock that side
// effect so the action-bar tests assert wiring, not Tauri behavior. Keep
// the real hasReplyWithFormSupport export so MessageView's gate logic
// (Codex r2 P2 #1) is exercised end-to-end.
vi.mock('./replyActions', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./replyActions')>();
  return { ...actual, openReplyWindow: vi.fn().mockResolvedValue(undefined) };
});
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

  // Winlink form → placeholder for now; never render raw XML.
  it('shows form placeholder when isForm', () => {
    render(<MessageViewLoaded message={parsed({ isForm: true, body: '<?xml...' })} />);
    expect(screen.getByTestId('message-form-placeholder')).toBeInTheDocument();
    // The raw XML body must NOT appear in place of the placeholder.
    expect(screen.queryByTestId('message-body')).toBeNull();
  });

  it('renders the registered View component for a known form payload', () => {
    const msg = {
      id: 'TEST-FORM',
      subject: 'ICS-213 test',
      from: 'X@winlink.org',
      to: ['Y@winlink.org'],
      cc: [],
      date: '2026-05-30T14:30:00Z',
      body: 'plain rendered text',
      attachments: [],
      isForm: true,
      routing: null,
      formId: 'ICS213_Initial',
      formPayload: {
        formId: 'ICS213_Initial',
        formParameters: {
          xmlFileVersion: '1.0', rmsExpressVersion: 'Tuxlink/0.3.0',
          submissionDatetime: '20260530143000', sendersCallsign: 'N0CALL',
          gridSquare: 'FM18', displayForm: 'ICS213_Initial_Viewer.html',
          replyTemplate: 'ICS213_SendReply.0',
        },
        fields: [
          ['inc_name', 'WALDO'],
          ['to_name', 'JOHN'],
          ['subjectline', 'TEST'],
          ['mdate', '2026-05-30'],
          ['mtime', '14:30Z'],
          ['message', 'Need bandages.'],
        ],
      },
    };
    render(<MessageViewLoaded message={msg as any} />);
    expect(screen.getByTestId('message-form-rendered')).toBeInTheDocument();
    expect(screen.getByText('WALDO')).toBeInTheDocument();
  });

  it('renders KeyValueView fallback when form_id is unknown', () => {
    const msg = {
      id: 'TEST-UNKNOWN', subject: 'unknown', from: 'X@winlink.org',
      to: ['Y@winlink.org'], cc: [], date: '2026-05-30T14:30:00Z',
      body: 'plain rendered', attachments: [], isForm: true, routing: null,
      formId: 'Unknown_Form',
      formPayload: {
        formId: 'Unknown_Form',
        formParameters: {
          xmlFileVersion: '1.0', rmsExpressVersion: '',
          submissionDatetime: '', sendersCallsign: '', gridSquare: '',
          displayForm: '', replyTemplate: '',
        },
        fields: [['random_field', 'random_value']],
      },
    };
    render(<MessageViewLoaded message={msg as any} />);
    expect(screen.getByTestId('message-form-unknown')).toBeInTheDocument();
    expect(screen.getByText('random_field')).toBeInTheDocument();
  });

  // Attachment strip lists names + sizes; no download/preview yet.
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

  // tuxlink-ca5x: Archive button is rendered only when the parent supplies
  // onArchive. AppShell omits it when the open message is already in Archive,
  // so absence is the no-op signal.
  it('renders Archive button when onArchive is supplied', () => {
    const onArchive = vi.fn();
    render(<MessageViewLoaded message={parsed()} onArchive={onArchive} />);
    expect(screen.getByTestId('archive-btn')).toBeInTheDocument();
  });

  it('does NOT render Archive button when onArchive is omitted', () => {
    render(<MessageViewLoaded message={parsed()} />);
    expect(screen.queryByTestId('archive-btn')).toBeNull();
  });

  it('clicking Archive fires onArchive', () => {
    const onArchive = vi.fn();
    render(<MessageViewLoaded message={parsed()} onArchive={onArchive} />);
    fireEvent.click(screen.getByTestId('archive-btn'));
    expect(onArchive).toHaveBeenCalledOnce();
  });

  // Codex P2 #6 (T8.1 follow-up): "Reply with form…" is a fourth action that
  // appears ONLY for messages whose form_id resolves in the registry (so the
  // operator can author a same-form reply with sender↔recipient swap).
  it('does NOT show the Reply-with-form button for a plain (non-form) message', () => {
    render(<MessageViewLoaded message={parsed({ isForm: false })} />);
    expect(screen.queryByTestId('reply-with-form-btn')).toBeNull();
  });

  it('does NOT show the Reply-with-form button for a form with an unknown formId', () => {
    render(
      <MessageViewLoaded
        message={parsed({ isForm: true, formId: 'Unknown_Form_v1', formPayload: undefined })}
      />,
    );
    expect(screen.queryByTestId('reply-with-form-btn')).toBeNull();
  });

  it('shows the Reply-with-form button when formId is registered (ICS213_Initial)', () => {
    render(
      <MessageViewLoaded message={parsed({ isForm: true, formId: 'ICS213_Initial' })} />,
    );
    expect(screen.getByTestId('reply-with-form-btn')).toBeInTheDocument();
  });

  it('Reply-with-form opens a replyWithForm compose window', () => {
    const m = parsed({ isForm: true, formId: 'ICS213_Initial' });
    render(<MessageViewLoaded message={m} />);
    fireEvent.click(screen.getByTestId('reply-with-form-btn'));
    expect(openReplyWindow).toHaveBeenCalledWith(m, 'replyWithForm');
  });

  // Codex r2 P2 #1: forms registered via Phase 9 (Bulletin/Position/ICS-309/
  // Damage Assessment) lookup successfully but don't have per-form reply
  // mappings in buildReplyDraft — hiding the button avoids the half-populated
  // form draft trap. Add per-form mappings + remove the gate in a follow-up
  // to light it up for these forms.
  it('does NOT show the Reply-with-form button on Phase 9 forms without reply mappings', () => {
    render(
      <MessageViewLoaded message={parsed({ isForm: true, formId: 'Bulletin_Initial' })} />,
    );
    expect(screen.queryByTestId('reply-with-form-btn')).toBeNull();
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
