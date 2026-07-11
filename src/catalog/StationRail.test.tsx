import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { GATEWAY_PREFILL_EVENT } from '../favorites/prefillEvent';
import { StationRail } from './StationRail';
import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { AggregatedPeer } from '../peers/peerModel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke);

const station: Station = {
  baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug Jarmuth', location: 'Wickenburg, AZ',
  modes: ['vara-hf', 'ardop-hf', 'packet'], fetchedAtMs: 1, gatewayAntenna: null,
  channels: [
    { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
    { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'ardop-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
  ],
};
const prediction: PathPrediction = {
  bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
  channels: [
    { frequencyKhz: 3590, voacapMhz: 4, relByHour: Array(24).fill(0.74), snrByHour: Array(24).fill(10), mufdayByHour: Array(24).fill(0.9) },
    { frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(15), mufdayByHour: Array(24).fill(1) },
  ],
};

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.restoreAllMocks());

describe('StationRail', () => {
  it('shows an empty state when no station is selected', () => {
    render(<StationRail station={null} prediction={null} predictionStatus="idle" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/select a station/i)).toBeTruthy();
  });

  it('renders the selected-station header', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText('N0DAJ')).toBeTruthy();
    expect(screen.getByText(/Doug Jarmuth/)).toBeTruthy();
    expect(screen.getByText(/Wickenburg, AZ/)).toBeTruthy();
  });

  it('shows bearing + distance from the prediction', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/318°/)).toBeTruthy();
    expect(screen.getByTestId('aim-distance').textContent).toMatch(/\d+ mi/);
  });

  it('renders the path forecast with best-band-now when prediction is ok', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByText(/best now: 40 m/i)).toBeTruthy();
  });

  it('colours each band bar by its reachability tier (not a static fill)', () => {
    // 80m rel 0.74 → fair, 40m rel 0.86 → good on the recalibrated ramp; both are
    // coloured by a per-tier --reach-* var. Unmodelled bands have no channel and
    // stay uncoloured. The point: the bar background is driven by relToTier, not a
    // fixed CSS colour.
    const { container } = render(
      <StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />,
    );
    const fills = Array.from(container.querySelectorAll('.station-finder__fill')) as HTMLElement[];
    const coloured = fills.filter((f) => /^var\(--reach-(good|fair|marginal|poor|skip)\)$/.test(f.style.background));
    expect(coloured.length).toBeGreaterThanOrEqual(2); // 80m + 40m bars
    // Tier-driven, not a single static hue: the two modelled bands differ in tier.
    expect(fills.some((f) => f.style.background === 'var(--reach-good)')).toBe(true);
    expect(fills.some((f) => f.style.background === 'var(--reach-fair)')).toBe(true);
  });

  it('hides the forecast and shows a degrade note when prediction is unavailable', () => {
    render(<StationRail station={station} prediction={null} predictionStatus="unavailable" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.queryByText(/best now/i)).toBeNull();
    expect(screen.getByText(/forecast unavailable/i)).toBeTruthy();
  });

  it('groups channels by mode and shows per-channel reliability', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    // "VARA HF"/"ARDOP HF" appear as both a mode badge and a channel-group header.
    expect(screen.getAllByText('VARA HF').length).toBeGreaterThan(0);
    expect(screen.getAllByText('ARDOP HF').length).toBeGreaterThan(0);
    expect(screen.getAllByText(/86%/).length).toBeGreaterThan(0);
  });

  it('Use → emits a prefill dial for a channel matching the active modem', () => {
    const handler = vi.fn();
    window.addEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    const vara40 = screen.getByTestId('use-vara-hf-7103');
    fireEvent.click(vara40);
    expect(handler).toHaveBeenCalled();
    const evt = handler.mock.calls[0][0] as CustomEvent;
    // tuxlink-8fkkk Task B: the event detail is now { dial, candidates }. The
    // primary dial is the clicked channel; candidates is the station's ranked
    // vara-hf list (reliability DESC: 40m 0.86 then 80m 0.74).
    expect(evt.detail.dial).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
    expect(evt.detail.candidates).toEqual([
      { mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' },
      { mode: 'vara-hf', gateway: 'N0DAJ', freq: '3.590', grid: 'DM34oa' },
    ]);
    window.removeEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
  });

  it('Use → keeps the CLICKED channel as candidates[0] even when it is not the highest-ranked', () => {
    // tuxlink-8fkkk Codex Fix 2: 40m ranks higher (rel 0.86) than 80m (0.74),
    // but the operator clicked 80m. The clicked dial MUST be the PRIMARY
    // candidate — the backend dials candidates[0] first and a non-empty list
    // overrides the form target/freq, so a misordered list would dial 40m
    // first. Assert the clicked 80m dial leads, with 40m following as QSY.
    const handler = vi.fn();
    window.addEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    fireEvent.click(screen.getByTestId('use-vara-hf-3590'));
    const evt = handler.mock.calls[0][0] as CustomEvent;
    expect(evt.detail.dial).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '3.590', grid: 'DM34oa' });
    expect(evt.detail.candidates).toEqual([
      { mode: 'vara-hf', gateway: 'N0DAJ', freq: '3.590', grid: 'DM34oa' },
      { mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' },
    ]);
    // No duplicate of the clicked dial appears in the QSY tail.
    expect(
      evt.detail.candidates.filter(
        (d: { freq?: string }) => d.freq === '3.590',
      ).length,
    ).toBe(1);
    window.removeEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
  });

  it('enables Use → for any dialable channel (arm-on-demand), not just the open modem', () => {
    // tuxlink-s0r1: Use → now opens the matching modem on demand, so a channel
    // whose mode is not the currently-open modem is still usable, not greyed.
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} activePrefillMode="vara-hf" />);
    expect(screen.getByTestId('use-ardop-hf-7103').hasAttribute('disabled')).toBe(false);
  });

  it('Use → calls onUse with the dial (arm-on-demand path) when provided', () => {
    const onUse = vi.fn();
    // No active modem at all — the old behavior would grey every button.
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} onUse={onUse} />);
    const ardop = screen.getByTestId('use-ardop-hf-7103');
    expect(ardop.hasAttribute('disabled')).toBe(false);
    fireEvent.click(ardop);
    // tuxlink-8fkkk Task B: onUse now receives (dial, candidates). The station
    // has one ardop-hf channel, so the ranked list is the single clicked dial.
    expect(onUse).toHaveBeenCalledWith(
      { mode: 'ardop-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' },
      [{ mode: 'ardop-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' }],
    );
  });

  // tuxlink-5016 — save-to-favorites affordance.
  it('renders NO save (★) button when onSaveFavorite is not provided', () => {
    render(<StationRail station={station} prediction={prediction} predictionStatus="ok" operatorGrid="DM43bp" utcHour={21} />);
    expect(screen.queryByTestId('save-vara-hf-7103')).toBeNull();
  });

  it('★ Save calls onSaveFavorite with the channel dial', () => {
    const onSaveFavorite = vi.fn();
    render(
      <StationRail
        station={station} prediction={prediction} predictionStatus="ok"
        operatorGrid="DM43bp" utcHour={21}
        onSaveFavorite={onSaveFavorite} isSaved={() => false}
      />,
    );
    const star = screen.getByTestId('save-vara-hf-7103');
    expect(star.textContent).toBe('☆'); // not yet saved
    expect(star.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(star);
    expect(onSaveFavorite).toHaveBeenCalledWith({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
  });

  it('★ shows the saved (filled) state when isSaved returns true for that dial', () => {
    // Saved only for the ARDOP 40m channel; the VARA channels stay unsaved.
    const isSaved = (d: { mode: string; gateway: string }) => d.mode === 'ardop-hf' && d.gateway === 'N0DAJ';
    render(
      <StationRail
        station={station} prediction={prediction} predictionStatus="ok"
        operatorGrid="DM43bp" utcHour={21}
        onSaveFavorite={vi.fn()} isSaved={isSaved}
      />,
    );
    const ardopStar = screen.getByTestId('save-ardop-hf-7103');
    expect(ardopStar.textContent).toBe('★');
    expect(ardopStar.getAttribute('aria-pressed')).toBe('true');
    expect(ardopStar.className).toMatch(/is-saved/);
    // A different channel is not saved.
    expect(screen.getByTestId('save-vara-hf-7103').textContent).toBe('☆');
  });

  it('uses the SSID as the gateway when saving a packet channel', () => {
    const onSaveFavorite = vi.fn();
    render(
      <StationRail
        station={station} prediction={prediction} predictionStatus="ok"
        operatorGrid="DM43bp" utcHour={21}
        onSaveFavorite={onSaveFavorite} isSaved={() => false}
      />,
    );
    fireEvent.click(screen.getByTestId('save-packet-145710'));
    expect(onSaveFavorite).toHaveBeenCalledWith(
      expect.objectContaining({ mode: 'packet', gateway: 'N0DAJ-10' }),
    );
  });
});

// tuxlink-c39af Task 23a — the peer-row Connect fires a REAL outbound P2P dial
// (Flow 2) through connectFor, reaching the same backend command the mode's
// panel uses with intent/sessionType=p2p and the channel's via/path/freq
// threaded. This is the click→backend seam Task 28 traces; a CMS-defaulting
// dial here would silence the peer recorder and leave the store empty.
describe('StationRail — peer-row Connect fires a P2P dial (Task 23a)', () => {
  // A peer with one RF channel per transport (VARA/ARDOP/packet) + a telnet
  // endpoint, so every protocol has a clickable peer-dial path.
  const peer: AggregatedPeer = {
    id: 'peer-1',
    callsign: 'W7XYZ-5',
    origin: 'incoming',
    tier: 'unconfirmed',
    grid: 'CN87',
    mapPlaceable: true,
    lastSeen: null,
    lastOk: null,
    channels: [
      {
        transport: 'vara-fm', target_callsign: 'W7XYZ-5', via: ['RELAY1'],
        freq_hz: 145_030_000, bandwidth: null, direction: 'outgoing',
        counts: { ok: 0, fail: 0 }, last_seen: '2026-07-10T00:00:00Z', last_ok: null,
      },
      {
        transport: 'ardop', target_callsign: 'W7XYZ', via: [],
        freq_hz: 7_105_000, bandwidth: null, direction: 'outgoing',
        counts: { ok: 0, fail: 0 }, last_seen: '2026-07-10T00:00:00Z', last_ok: null,
      },
      {
        transport: 'packet', target_callsign: 'W7XYZ-1', via: ['WIDE1-1'],
        freq_hz: 144_390_000, bandwidth: null, direction: 'outgoing',
        counts: { ok: 0, fail: 0 }, last_seen: '2026-07-10T00:00:00Z', last_ok: null,
      },
    ],
    endpoints: [
      {
        id: 'ep-1', host: '10.0.0.5', port: 8774,
        provenance: 'operator', last_seen: '2026-07-10T00:00:00Z', last_ok: null,
      },
    ],
  };

  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });
  afterEach(() => mockInvoke.mockReset());

  function renderPeer() {
    // station=null still renders the peer rows (they are independent of the
    // map-pin selection); operatorGrid feeds the telnet handshake locator.
    render(
      <StationRail
        station={null} prediction={null} predictionStatus="idle"
        operatorGrid="CN85nm" utcHour={0} peers={[peer]}
      />,
    );
  }

  it('VARA peer Connect → modem_vara_b2f_exchange with intent=p2p + the channel via/freq', async () => {
    renderPeer();
    fireEvent.click(screen.getByTestId('peer-use-peer-1-0'));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        'modem_vara_b2f_exchange',
        expect.objectContaining({
          target: 'W7XYZ-5', intent: 'p2p', transportKind: 'vara-fm',
          via: ['RELAY1'], freqHz: 145_030_000,
        }),
      ),
    );
    expect(mockInvoke).toHaveBeenCalledWith(
      'vara_open_session',
      expect.objectContaining({ intent: 'p2p', transportKind: 'vara-fm' }),
    );
  });

  it('ARDOP peer Connect → modem_ardop_b2f_exchange with intent=p2p', async () => {
    renderPeer();
    fireEvent.click(screen.getByTestId('peer-use-peer-1-1'));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        'modem_ardop_b2f_exchange',
        expect.objectContaining({ target: 'W7XYZ', intent: 'p2p', transportKind: 'ardop' }),
      ),
    );
  });

  it('packet peer Connect → packet_connect with intent=p2p + the channel path', async () => {
    renderPeer();
    fireEvent.click(screen.getByTestId('peer-use-peer-1-2'));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        'packet_connect',
        expect.objectContaining({ call: 'W7XYZ-1', path: ['WIDE1-1'], intent: 'p2p' }),
      ),
    );
  });

  it('telnet peer endpoint Connect → telnet_p2p_connect (never cms_connect)', async () => {
    renderPeer();
    fireEvent.click(screen.getByTestId('peer-endpoint-connect-ep-1'));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        'telnet_p2p_connect',
        expect.objectContaining({
          req: expect.objectContaining({
            host: '10.0.0.5', port: 8774, peer_callsign: 'W7XYZ-5', locator: 'CN85nm',
          }),
        }),
      ),
    );
    expect(mockInvoke).not.toHaveBeenCalledWith('cms_connect');
  });
});
