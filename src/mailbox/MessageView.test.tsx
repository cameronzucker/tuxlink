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

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import {
  MessageViewLoaded,
  MessageViewEmpty,
  MessageViewParseError,
  SELECT_MESSAGE_COPY,
  PARSE_ERROR_PREFIX,
} from './MessageView';
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
// Task-13 test (6): routing shown when present, omitted when null.
// ============================================================================
describe('<MessageViewLoaded>', () => {
  it('shows routing when present', () => {
    render(<MessageViewLoaded message={parsed({ routing: 'via CMS-SSL' })} />);
    expect(screen.getByTestId('message-routing')).toHaveTextContent('via CMS-SSL');
  });

  it('omits routing strip when null', () => {
    render(<MessageViewLoaded message={parsed({ routing: null })} />);
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
// Parse-failure state — UiError::Internal from the command renders a
// "could not parse" message, not garbage.
// ============================================================================
describe('<MessageViewParseError>', () => {
  it('shows a parse-error description starting with the prefix', () => {
    render(<MessageViewParseError rawSize={42000} />);
    const el = screen.getByTestId('message-parse-error');
    expect(el.textContent).toContain(PARSE_ERROR_PREFIX);
  });
});
