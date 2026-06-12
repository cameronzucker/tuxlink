import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { GATEWAY_PREFILL_EVENT } from '../favorites/prefillEvent';
import { StationRail } from './StationRail';
import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';

const station: Station = {
  baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug Jarmuth', location: 'Wickenburg, AZ',
  modes: ['vara-hf', 'ardop-hf', 'packet'], fetchedAtMs: 1,
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
    expect(evt.detail).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
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
    expect(onUse).toHaveBeenCalledWith({ mode: 'ardop-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
  });
});
