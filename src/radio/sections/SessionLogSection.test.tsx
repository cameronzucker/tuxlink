// src/radio/sections/SessionLogSection.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, within, fireEvent } from '@testing-library/react';
import { SessionLogSection, type SessionLogEntry } from './SessionLogSection';

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

  // Operator smoke 2026-05-31: Clear button alongside Copy. Local-only reset;
  // does NOT touch the backend snapshot buffer.
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
