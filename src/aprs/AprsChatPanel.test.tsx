import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
import { AprsChatPanel, formatTime } from './AprsChatPanel';
import type { ChannelMessage, HeardStation } from './aprsTypes';

const noMessages: ChannelMessage[] = [];
const noStations: HeardStation[] = [];
const send = vi.fn().mockResolvedValue('A1');
const getConfig = vi.fn().mockResolvedValue({ sourceSsid: 9, tocall: 'APZTUX', path: 'WIDE1-1,WIDE2-1' });
const setConfig = vi.fn().mockResolvedValue(undefined);

function renderPanel(over: Partial<Parameters<typeof AprsChatPanel>[0]> = {}) {
  return render(
    <AprsChatPanel
      messages={noMessages}
      heardStations={noStations}
      listening={false}
      send={send}
      getConfig={getConfig}
      setConfig={setConfig}
      {...over}
    />,
  );
}

describe('AprsChatPanel (open channel)', () => {
  it('renders the composer, recipient field, and a listening indicator', () => {
    renderPanel();
    expect(screen.getByTestId('aprs-composer-recipient')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-composer-text')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('renders an injected controlStrip slot when provided', () => {
    renderPanel({ controlStrip: <div data-testid="probe-strip">strip</div> });
    expect(screen.getByTestId('probe-strip')).toBeInTheDocument();
  });

  it('shows the open-channel honesty cue', () => {
    renderPanel();
    const cue = screen.getByTestId('aprs-open-channel');
    expect(cue).toBeInTheDocument();
    expect(cue).toHaveTextContent(/every station in range hears this/i);
  });

  it('shows the empty-state guidance when no messages', () => {
    renderPanel();
    expect(screen.getByTestId('aprs-empty-state')).toBeInTheDocument();
  });

  it('renders every heard message in one flat feed with from → to', () => {
    const messages: ChannelMessage[] = [
      { id: 'm1', direction: 'in', from: 'KK6XYZ', to: 'NN7LE-9', text: 'directed ping', msgid: '04', at: Date.now() },
      { id: 'm2', direction: 'in', from: 'W7RPT-9', to: null, text: 'broadcast hi', msgid: null, at: Date.now() },
    ];
    renderPanel({ messages });
    expect(screen.getByText('directed ping')).toBeInTheDocument();
    expect(screen.getByText('broadcast hi')).toBeInTheDocument();
    // Broadcast renders → all; directed renders the addressee.
    const addrs = screen.getAllByTestId('aprs-msg-addr').map((n) => n.textContent);
    expect(addrs.some((t) => /KK6XYZ.*→.*NN7LE-9/.test(t ?? ''))).toBe(true);
    expect(addrs.some((t) => /→.*all/.test(t ?? ''))).toBe(true);
  });

  it('shows a delivery chip with ACK time on an acked directed outbound message', () => {
    const ackedAt = new Date(2026, 5, 12, 14, 8).getTime();
    const messages: ChannelMessage[] = [
      { id: 'm1', direction: 'out', from: 'me', to: 'KK6XYZ', text: 'hello', msgid: 'A1', state: 'acked', at: ackedAt - 3000, ackedAt },
    ];
    renderPanel({ messages });
    expect(screen.getByText(/^Acked \d{1,2}:\d{2}$/)).toBeInTheDocument();
  });

  it('shows "broadcast · sent" and NO delivery chip for a broadcast outbound', () => {
    const messages: ChannelMessage[] = [
      { id: 'b1', direction: 'out', from: 'me', to: null, text: 'CQ', msgid: 'b1', state: 'sent', at: Date.now() },
    ];
    renderPanel({ messages });
    expect(screen.getByTestId('aprs-broadcast-chip')).toHaveTextContent(/broadcast/i);
    // No directed delivery chip on a broadcast.
    expect(screen.queryByTestId('aprs-delivery-chip')).not.toBeInTheDocument();
  });

  it('reflects broadcast mode when the recipient is empty and directed when filled', () => {
    renderPanel();
    const mode = screen.getByTestId('aprs-recipient-mode');
    expect(mode).toHaveTextContent(/all/i);
    fireEvent.change(screen.getByTestId('aprs-composer-recipient'), { target: { value: 'W7RPT-9' } });
    expect(mode).toHaveTextContent(/directed/i);
  });

  it('offers heard stations in the recipient datalist', () => {
    const heardStations: HeardStation[] = [
      { call: 'AAA', lastHeard: 2 },
      { call: 'BBB', lastHeard: 1 },
    ];
    renderPanel({ heardStations });
    const datalist = screen.getByTestId('aprs-heard-stations');
    const opts = datalist.querySelectorAll('option');
    expect(Array.from(opts).map((o) => o.getAttribute('value'))).toEqual(['AAA', 'BBB']);
  });

  it('sends a directed message with the typed recipient', async () => {
    send.mockClear();
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-recipient'), { target: { value: 'w7rpt-9' } });
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hi' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(send).toHaveBeenCalledWith('W7RPT-9', 'hi'));
  });

  it('sends a broadcast (null recipient) when the recipient field is empty', async () => {
    send.mockClear();
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'CQ' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(send).toHaveBeenCalledWith(null, 'CQ'));
  });

  it('shows a live n/67 character counter for the message field', () => {
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hello' } });
    expect(screen.getByTestId('aprs-char-count')).toHaveTextContent('5 / 67');
  });

  it('seeds the Path field from aprs_config_get and persists an edit via setConfig', async () => {
    getConfig.mockClear();
    setConfig.mockClear();
    renderPanel();
    const pathInput = await screen.findByTestId('aprs-composer-path');
    await waitFor(() => expect(pathInput).toHaveValue('WIDE1-1,WIDE2-1'));
    fireEvent.change(pathInput, { target: { value: 'WIDE1-1' } });
    fireEvent.blur(pathInput);
    await waitFor(() =>
      expect(setConfig).toHaveBeenCalledWith({ sourceSsid: 9, tocall: 'APZTUX', path: 'WIDE1-1' }),
    );
  });

  it('renders a Start/Stop listening toggle', () => {
    renderPanel();
    expect(screen.getByTestId('aprs-listen-toggle')).toBeInTheDocument();
  });

  it('formats an epoch-ms timestamp as a short HH:MM time', () => {
    expect(formatTime(new Date(2026, 5, 12, 14, 8).getTime())).toMatch(/\b\d{1,2}:\d{2}\b/);
  });
});
