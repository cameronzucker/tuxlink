import { render, screen, act, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => { delete handlers[name]; });
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));
import { AprsChatPanel, formatTime } from './AprsChatPanel';

describe('AprsChatPanel', () => {
  it('renders the composer and a listening indicator', () => {
    render(<AprsChatPanel />);
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('shows the empty-state guidance when no threads', () => {
    render(<AprsChatPanel />);
    expect(screen.getByText(/no conversations yet/i)).toBeInTheDocument();
  });

  it('renders a Start/Stop listening toggle (Task 14)', () => {
    render(<AprsChatPanel />);
    expect(
      screen.getByRole('button', { name: /start|stop|listen/i }),
    ).toBeInTheDocument();
  });

  it('formats an epoch-ms timestamp as a short HH:MM time', () => {
    expect(formatTime(new Date(2026, 5, 12, 14, 8).getTime())).toMatch(/\b\d{1,2}:\d{2}\b/);
  });

  it('shows the ACK time on an acked outbound bubble', async () => {
    render(<AprsChatPanel />);
    await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
    // Fill callsign + message fields and submit to produce an outbound bubble (msgid A1)
    await act(async () => {
      fireEvent.change(screen.getByTestId('aprs-composer-callsign'), { target: { value: 'KK6XYZ' } });
      fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hello' } });
    });
    await act(async () => {
      fireEvent.submit(screen.getByTestId('aprs-composer-callsign').closest('form')!);
    });
    // Inject the acked state transition via the captured handler
    await act(async () => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    expect(await screen.findByText(/^Acked \d{1,2}:\d{2}$/)).toBeInTheDocument();
  });

  it('renders a timestamp on each message bubble', async () => {
    render(<AprsChatPanel />);
    // Drain microtasks so all listen() promises resolve and handlers register
    await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
    // Inject an inbound message and select the thread so the bubble pane renders
    await act(async () => { handlers['aprs-message:new']?.({ payload: { sender: 'KK6XYZ', text: 'ping', msgid: '04' } }); });
    // Click the thread to make it active (activeThread is null until selected or callsign typed)
    const threadItem = await screen.findByTestId('aprs-thread-item');
    await act(async () => { threadItem.click(); });
    expect(await screen.findByTestId('aprs-bubble-time')).toBeInTheDocument();
  });

  it('shows a live n/67 character counter for the message field', async () => {
    render(<AprsChatPanel />);
    await act(async () => {
      fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hello' } });
    });
    expect(screen.getByTestId('aprs-char-count')).toHaveTextContent('5 / 67');
  });

  it('shows a quiet open-channel honesty cue', () => {
    render(<AprsChatPanel />);
    const cue = screen.getByTestId('aprs-open-channel');
    expect(cue).toBeInTheDocument();
    expect(cue).toHaveTextContent(/heard by all stations in range/i);
  });
});
