// src/radio/sections/SessionLogSection.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, within, fireEvent } from '@testing-library/react';
import {
  SESSION_LOG_VISIBLE_ENTRY_LIMIT,
  SessionLogSection,
  type SessionLogEntry,
} from './SessionLogSection';

const FIXTURE: SessionLogEntry[] = [
  { ts: '05:35:58', level: 'info', message: 'Connecting to cms.winlink.org:8773 (CMS-SSL)' },
  { ts: '05:35:59', level: 'ok',   message: 'TLS handshake complete · secure-login OK' },
  { ts: '05:36:00', level: 'info', message: 'Negotiating messages…' },
  { ts: '05:36:01', level: 'warn', message: 'Unknown client types are not allowed on production servers — use cms-z.winlink.org' },
  { ts: '05:36:01', level: 'alert', message: 'CMS connect failed: transport error',
    raw: 'RemoteError: "Unknown client types are not allowed on production servers — use cms-z.winlink.org — Disconnecting (68.2.111.142)"' },
];

describe('<SessionLogSection>', () => {
  it('renders the log entries with severity classes', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    const root = screen.getByTestId('session-log-section');
    expect(within(root).getByText(/Connecting to cms\.winlink\.org/)).toBeInTheDocument();
    expect(within(root).getByText(/TLS handshake complete/)).toBeInTheDocument();
    // Severity glyphs / classes:
    expect(within(root).getByText(/CMS connect failed/).closest('.log-entry'))
      .toHaveClass('log-entry-alert');
    // The warn message and the alert's raw block both contain
    // "Unknown client types"; anchor on the warn-only prefix to disambiguate.
    expect(within(root).getByText(/^Unknown client types/).closest('.log-entry'))
      .toHaveClass('log-entry-warn');
  });

  it('renders multi-paragraph errors (summary + raw)', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    expect(screen.getByText(/RemoteError:/)).toBeInTheDocument();
  });

  it('renders the Show raw + Auto-scroll controls', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    expect(screen.getByLabelText(/Show raw/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Auto-scroll/i)).toBeInTheDocument();
  });

  it('hides entries with kind=raw when Show raw is unchecked', () => {
    const withRaw: SessionLogEntry[] = [
      ...FIXTURE,
      { ts: '05:36:02', level: 'raw', message: '[B2F] FQ' },
    ];
    render(<SessionLogSection entries={withRaw} />);
    expect(screen.queryByText('[B2F] FQ')).not.toBeInTheDocument();
  });

  it('caps rendered rows to the latest visible entries and makes the display cap explicit', () => {
    const manyEntries: SessionLogEntry[] = Array.from(
      { length: SESSION_LOG_VISIBLE_ENTRY_LIMIT + 2 },
      (_, idx) => ({ ts: '05:36:02', level: 'info', message: `line ${idx + 1}` }),
    );

    render(<SessionLogSection entries={manyEntries} />);

    expect(screen.getByTestId('session-log-limit-note')).toHaveTextContent(
      `Showing latest ${SESSION_LOG_VISIBLE_ENTRY_LIMIT} of ${SESSION_LOG_VISIBLE_ENTRY_LIMIT + 2} lines`,
    );
    expect(screen.queryByText('line 1')).toBeNull();
    expect(screen.queryByText('line 2')).toBeNull();
    expect(screen.getByText('line 3')).toBeInTheDocument();
    expect(screen.getByText(`line ${SESSION_LOG_VISIBLE_ENTRY_LIMIT + 2}`)).toBeInTheDocument();
  });

  it('copies the full filtered history even when older rows are not rendered', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    const manyEntries: SessionLogEntry[] = Array.from(
      { length: SESSION_LOG_VISIBLE_ENTRY_LIMIT + 2 },
      (_, idx) => ({ ts: '05:36:02', level: 'info', message: `line ${idx + 1}` }),
    );

    render(<SessionLogSection entries={manyEntries} />);
    fireEvent.click(screen.getByTestId('log-copy-btn'));

    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = writeText.mock.calls[0][0] as string;
    expect(copied).toContain('line 1');
    expect(copied).toContain(`line ${SESSION_LOG_VISIBLE_ENTRY_LIMIT + 2}`);
  });

  // Operator smoke 2026-05-31: Clear button alongside Copy. The owning hook
  // decides whether clear is local-only or also drains backend history.
  describe('Clear control', () => {
    it('does NOT render a Clear button when onClear is omitted', () => {
      render(<SessionLogSection entries={FIXTURE} />);
      expect(screen.queryByTestId('log-clear-btn')).toBeNull();
    });

    it('renders a Clear button when onClear is provided', () => {
      render(<SessionLogSection entries={FIXTURE} onClear={() => {}} />);
      expect(screen.getByTestId('log-clear-btn')).toBeInTheDocument();
    });

    it('fires onClear when the Clear button is clicked', () => {
      const onClear = vi.fn();
      render(<SessionLogSection entries={FIXTURE} onClear={onClear} />);
      fireEvent.click(screen.getByTestId('log-clear-btn'));
      expect(onClear).toHaveBeenCalledTimes(1);
    });
  });
});
