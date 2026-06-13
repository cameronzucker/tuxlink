import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
import { AprsChatPanel, formatTime } from './AprsChatPanel';
import type { Thread } from './aprsTypes';

const noThreads: Record<string, Thread> = {};
const send = vi.fn().mockResolvedValue('A1');
function renderPanel(over: Partial<Parameters<typeof AprsChatPanel>[0]> = {}) {
  return render(
    <AprsChatPanel threads={noThreads} listening={false} send={send} {...over} />,
  );
}

describe('AprsChatPanel', () => {
  it('renders the composer and a listening indicator', () => {
    renderPanel();
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('renders an injected controlStrip slot when provided', () => {
    renderPanel({ controlStrip: <div data-testid="probe-strip">strip</div> });
    expect(screen.getByTestId('probe-strip')).toBeInTheDocument();
  });

  it('renders a thread with its messages from props', () => {
    const threads: Record<string, Thread> = {
      'W7RPT-9': {
        callsign: 'W7RPT-9',
        messages: [
          { id: 'm1', direction: 'in', text: 'ping', msgid: '04', at: Date.now() },
        ],
      },
    };
    renderPanel({ threads });
    expect(screen.getByText('ping')).toBeInTheDocument();
  });

  it('shows the empty-state guidance when no threads', () => {
    renderPanel();
    expect(screen.getByText(/no conversations yet/i)).toBeInTheDocument();
  });

  it('renders a Start/Stop listening toggle', () => {
    renderPanel();
    expect(
      screen.getByRole('button', { name: /start|stop|listen/i }),
    ).toBeInTheDocument();
  });

  it('formats an epoch-ms timestamp as a short HH:MM time', () => {
    expect(formatTime(new Date(2026, 5, 12, 14, 8).getTime())).toMatch(/\b\d{1,2}:\d{2}\b/);
  });

  it('renders a timestamp on each message bubble', () => {
    const fixedAt = new Date(2026, 5, 12, 14, 8).getTime();
    const threads: Record<string, Thread> = {
      'KK6XYZ': {
        callsign: 'KK6XYZ',
        messages: [
          { id: 'm1', direction: 'in', text: 'ping', msgid: null, at: fixedAt },
        ],
      },
    };
    renderPanel({ threads });
    // Click the thread item to select it, making the bubble visible
    const threadItem = screen.getByTestId('aprs-thread-item');
    fireEvent.click(threadItem);
    expect(screen.getByTestId('aprs-bubble-time')).toBeInTheDocument();
  });

  it('shows the ACK time on an acked outbound bubble', () => {
    const ackedAt = new Date(2026, 5, 12, 14, 8).getTime();
    const threads: Record<string, Thread> = {
      'KK6XYZ': {
        callsign: 'KK6XYZ',
        messages: [
          {
            id: 'm1',
            direction: 'out',
            text: 'hello',
            msgid: 'A1',
            state: 'acked',
            at: ackedAt - 3000,
            ackedAt,
          },
        ],
      },
    };
    renderPanel({ threads });
    // Click the thread to make the bubble visible
    const threadItem = screen.getByTestId('aprs-thread-item');
    fireEvent.click(threadItem);
    expect(screen.getByText(/^Acked \d{1,2}:\d{2}$/)).toBeInTheDocument();
  });

  it('shows a live n/67 character counter for the message field', () => {
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hello' } });
    expect(screen.getByTestId('aprs-char-count')).toHaveTextContent('5 / 67');
  });

  it('shows a quiet open-channel honesty cue', () => {
    renderPanel();
    const cue = screen.getByTestId('aprs-open-channel');
    expect(cue).toBeInTheDocument();
    expect(cue).toHaveTextContent(/heard by all stations in range/i);
  });
});
