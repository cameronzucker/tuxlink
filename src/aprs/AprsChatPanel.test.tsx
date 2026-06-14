import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
import { AprsChatPanel, formatTime, parseCompose } from './AprsChatPanel';
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

describe('parseCompose (inline addressing)', () => {
  it('treats a leading callsign + colon as a directed addressee', () => {
    expect(parseCompose('W1AW: hello there')).toEqual({ recipient: 'W1AW', body: 'hello there' });
  });

  it('treats a leading callsign + whitespace as a directed addressee', () => {
    expect(parseCompose('W1AW hello there')).toEqual({ recipient: 'W1AW', body: 'hello there' });
  });

  it('parses a callsign with an SSID suffix', () => {
    expect(parseCompose('W1AW-9: roger')).toEqual({ recipient: 'W1AW-9', body: 'roger' });
  });

  it('normalizes the recipient to uppercase', () => {
    expect(parseCompose('w1aw-9: roger')).toEqual({ recipient: 'W1AW-9', body: 'roger' });
  });

  it('tolerates leading whitespace before the token', () => {
    expect(parseCompose('   W1AW: hi')).toEqual({ recipient: 'W1AW', body: 'hi' });
  });

  it('treats a token-only input (empty body) as directed with empty body', () => {
    expect(parseCompose('W1AW: ')).toEqual({ recipient: 'W1AW', body: '' });
  });

  it('treats a normal sentence with no callsign token as a broadcast', () => {
    expect(parseCompose('hello everyone on frequency')).toEqual({
      recipient: null,
      body: 'hello everyone on frequency',
    });
  });

  it('treats a leading word that is not a valid callsign as broadcast', () => {
    // "Hello" has no digit, so it is not a callsign; the whole thing is the body.
    expect(parseCompose('Hello W1AW are you there')).toEqual({
      recipient: null,
      body: 'Hello W1AW are you there',
    });
  });

  it('treats empty input as broadcast with empty body', () => {
    expect(parseCompose('')).toEqual({ recipient: null, body: '' });
  });
});

describe('AprsChatPanel (open channel)', () => {
  it('renders a single compose field + target indicator and NO separate To field', () => {
    renderPanel();
    expect(screen.getByTestId('aprs-composer-text')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-compose-target')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    // The separate recipient input + mode indicator are GONE (settled: no To field).
    expect(screen.queryByTestId('aprs-composer-recipient')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-recipient-mode')).not.toBeInTheDocument();
  });

  it('does NOT render a connect control inside the panel (moved to the dock header)', () => {
    renderPanel();
    expect(screen.queryByTestId('aprs-listen-toggle')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-listening-indicator')).not.toBeInTheDocument();
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
    expect(screen.queryByTestId('aprs-delivery-chip')).not.toBeInTheDocument();
  });

  it('shows a live → target indicator derived from the compose field', () => {
    renderPanel();
    const target = screen.getByTestId('aprs-compose-target');
    expect(target).toHaveTextContent(/all/i);
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'W7RPT-9: hi' } });
    expect(target).toHaveTextContent(/W7RPT-9/);
  });

  it('sends a directed message parsed from the inline leading callsign token', async () => {
    send.mockClear();
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'w7rpt-9: hi' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(send).toHaveBeenCalledWith('W7RPT-9', 'hi'));
  });

  it('sends a broadcast (null recipient) when there is no leading callsign token', async () => {
    send.mockClear();
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'CQ everyone' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(send).toHaveBeenCalledWith(null, 'CQ everyone'));
  });

  it('clears the compose field after a successful send', async () => {
    send.mockClear();
    renderPanel();
    const input = screen.getByTestId('aprs-composer-text') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'CQ' } });
    fireEvent.click(screen.getByRole('button', { name: /send/i }));
    await waitFor(() => expect(input.value).toBe(''));
  });

  it('shows a live n/67 character counter for the message field', () => {
    renderPanel();
    fireEvent.change(screen.getByTestId('aprs-composer-text'), { target: { value: 'hello' } });
    expect(screen.getByTestId('aprs-char-count')).toHaveTextContent('5 / 67');
  });

  it('seeds the compose field with a callsign token when an inbound feed row is tapped', () => {
    const messages: ChannelMessage[] = [
      { id: 'm1', direction: 'in', from: 'KK6XYZ', to: null, text: 'hi all', msgid: null, at: Date.now() },
    ];
    renderPanel({ messages });
    const rows = screen.getAllByTestId('aprs-feed-row');
    fireEvent.click(rows[0]);
    const input = screen.getByTestId('aprs-composer-text') as HTMLInputElement;
    expect(input.value).toBe('KK6XYZ: ');
  });

  it('does NOT seed from tapping an outbound row', () => {
    const messages: ChannelMessage[] = [
      { id: 'o1', direction: 'out', from: 'me', to: 'KK6XYZ', text: 'hi', msgid: 'o1', state: 'sent', at: Date.now() },
    ];
    renderPanel({ messages });
    const rows = screen.getAllByTestId('aprs-feed-row');
    fireEvent.click(rows[0]);
    const input = screen.getByTestId('aprs-composer-text') as HTMLInputElement;
    expect(input.value).toBe('');
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

  it('formats an epoch-ms timestamp as a short HH:MM time', () => {
    expect(formatTime(new Date(2026, 5, 12, 14, 8).getTime())).toMatch(/\b\d{1,2}:\d{2}\b/);
  });
});
