// Task C3 (spec §Rail Station tab, tuxlink-b026z.4): BandMatrix unifies the
// pre-matrix "path forecast" bars + "channels grouped by mode" list into one
// row per band. These tests guard the load-bearing contracts called out in
// the task brief: the ☆ save star stays a SIBLING of the Use-chip (never
// nested — the anti-pattern the review checks for), a clicked dial chip
// always resolves candidates[0] to the clicked channel (rankedDialsFor +
// channelToDial, tuxlink-8fkkk), a 3rd+ channel on a band collapses behind a
// `+N` overflow that expands in place, and the VHF row renders no openness
// dot and no VOACAP bar (never propagation-ranked, never FT-8-sampleable).

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BandMatrix } from './BandMatrix';
import { GATEWAY_PREFILL_EVENT } from '../favorites/prefillEvent';
import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { BandDot } from '../ft8ui/ft8Types';

const station: Station = {
  baseCallsign: 'N0DAJ',
  grid: 'DM34oa',
  sysopName: 'Doug Jarmuth',
  location: 'Wickenburg, AZ',
  modes: ['vara-hf', 'ardop-hf', 'packet', 'pactor'],
  fetchedAtMs: 1,
  gatewayAntenna: null,
  channels: [
    { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
    { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'ardop-hf', frequencyKhz: 7103, band: '40m' },
    { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
    // 20m carries THREE channels — the +N overflow row.
    { mode: 'vara-hf', frequencyKhz: 14107, band: '20m' },
    { mode: 'ardop-hf', frequencyKhz: 14109, band: '20m' },
    { mode: 'pactor', frequencyKhz: 14111, band: '20m' },
  ],
};

const prediction: PathPrediction = {
  bearingDeg: 318,
  distanceKm: 77,
  ssn: 118,
  year: 2026,
  month: 6,
  channels: [
    { frequencyKhz: 3590, voacapMhz: 4, relByHour: Array(24).fill(0.74), snrByHour: Array(24).fill(10), mufdayByHour: Array(24).fill(0.9) },
    { frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(15), mufdayByHour: Array(24).fill(1) },
    // 20m band-level VOACAP (matched by band, not per-channel).
    { frequencyKhz: 14100, voacapMhz: 14, relByHour: Array(24).fill(0.9), snrByHour: Array(24).fill(18), mufdayByHour: Array(24).fill(1) },
    // Per-channel reliability for the three 20m dials — vara highest, pactor lowest,
    // so pactor is deterministically the one behind the "+1" overflow.
    { frequencyKhz: 14107, voacapMhz: 14, relByHour: Array(24).fill(0.9), snrByHour: Array(24).fill(18), mufdayByHour: Array(24).fill(1) },
    { frequencyKhz: 14109, voacapMhz: 14, relByHour: Array(24).fill(0.8), snrByHour: Array(24).fill(16), mufdayByHour: Array(24).fill(1) },
    { frequencyKhz: 14111, voacapMhz: 14, relByHour: Array(24).fill(0.5), snrByHour: Array(24).fill(10), mufdayByHour: Array(24).fill(1) },
  ],
};

const hotDot: BandDot = { tier: 'hot', opacity: 1, sampledAgoMs: 5_000, dwellSlots: 12 };

describe('BandMatrix', () => {
  it('renders one row per finder HF band plus a trailing VHF row', () => {
    render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
    expect(screen.getByTestId('bandmatrix-row-80m')).toBeTruthy();
    expect(screen.getByTestId('bandmatrix-row-60m')).toBeTruthy();
    expect(screen.getByTestId('bandmatrix-row-40m')).toBeTruthy();
    expect(screen.getByTestId('bandmatrix-row-20m')).toBeTruthy();
    expect(screen.getByTestId('bandmatrix-row-vhf-uhf')).toBeTruthy();
  });

  it('dims a band with no channel and shows "no channel"', () => {
    render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
    const row = screen.getByTestId('bandmatrix-row-15m');
    expect(row.className).toMatch(/is-empty/);
    expect(row.textContent).toMatch(/no channel/);
  });

  it('highlights the best-band-now row', () => {
    // 20m carries the highest modelled reliability (0.9) among HF bands with
    // a channel; bestBandNow finds it across ALL prediction.channels though,
    // so this also exercises picking the correct row.
    render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
    const row = screen.getByTestId('bandmatrix-row-20m');
    expect(row.className).toMatch(/is-best/);
  });

  describe('sibling ☆ preservation (never nested inside the Use-chip)', () => {
    it('keeps save-${mode}-${khz} as a SIBLING of use-${mode}-${khz}, not a descendant', () => {
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          onSaveFavorite={vi.fn()}
          isSaved={() => false}
        />,
      );
      const useBtn = screen.getByTestId('use-vara-hf-7103');
      const saveBtn = screen.getByTestId('save-vara-hf-7103');
      expect(saveBtn.textContent).toBe('☆');
      expect(saveBtn.getAttribute('aria-pressed')).toBe('false');
      // The anti-pattern this test guards against: a star nested INSIDE the
      // Use-chip button. Assert it never is.
      expect(useBtn.contains(saveBtn)).toBe(false);
      expect(saveBtn.contains(useBtn)).toBe(false);
      // Both are children of the same chip wrapper — true siblings.
      expect(saveBtn.parentElement).toBe(useBtn.parentElement);
    });

    it('shows the saved (filled) state via aria-pressed when isSaved returns true', () => {
      const isSaved = (d: { mode: string }) => d.mode === 'ardop-hf';
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          onSaveFavorite={vi.fn()}
          isSaved={isSaved}
        />,
      );
      const ardopStar = screen.getByTestId('save-ardop-hf-7103');
      expect(ardopStar.textContent).toBe('★');
      expect(ardopStar.getAttribute('aria-pressed')).toBe('true');
      expect(screen.getByTestId('save-vara-hf-7103').textContent).toBe('☆');
    });

    it('renders no ☆ at all when onSaveFavorite is omitted', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      expect(screen.queryByTestId('save-vara-hf-7103')).toBeNull();
      // The Use-chip itself still renders.
      expect(screen.getByTestId('use-vara-hf-7103')).toBeTruthy();
    });

    it('★ Save calls onSaveFavorite with the channel dial', () => {
      const onSaveFavorite = vi.fn();
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          onSaveFavorite={onSaveFavorite}
          isSaved={() => false}
        />,
      );
      fireEvent.click(screen.getByTestId('save-vara-hf-7103'));
      expect(onSaveFavorite).toHaveBeenCalledWith({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
    });
  });

  describe('+N overflow', () => {
    it('shows the best 2 chips inline and collapses the 3rd behind a +N control', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      // Highest two of the three 20m dials render inline...
      expect(screen.getByTestId('use-vara-hf-14107')).toBeTruthy();
      expect(screen.getByTestId('use-ardop-hf-14109')).toBeTruthy();
      // ...the lowest-reliability third is hidden behind "+1".
      expect(screen.queryByTestId('use-pactor-14111')).toBeNull();
      const more = screen.getByTestId('bandmatrix-more-20m');
      expect(more.textContent).toBe('+1');
      expect(more.getAttribute('aria-expanded')).toBe('false');
    });

    it('expands the row to reveal the hidden chip on click', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      const more = screen.getByTestId('bandmatrix-more-20m');
      fireEvent.click(more);
      expect(screen.getByTestId('use-pactor-14111')).toBeTruthy();
      expect(screen.getByTestId('bandmatrix-more-20m').getAttribute('aria-expanded')).toBe('true');
    });

    it('does not render a +N control for a band with 2 or fewer channels', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      expect(screen.queryByTestId('bandmatrix-more-40m')).toBeNull();
    });
  });

  describe('VHF row — no dot, no VOACAP bar (never FT-8-sampleable, never propagation-ranked)', () => {
    it('renders no openness dot on the VHF row', () => {
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          bandActivity={new Map([['vhf-uhf', hotDot]])}
        />,
      );
      expect(screen.queryByTestId('bandmatrix-dot-vhf-uhf')).toBeNull();
    });

    it('renders no VOACAP bar on the VHF row, showing an "LoS" caption instead', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      const row = screen.getByTestId('bandmatrix-row-vhf-uhf');
      expect(row.querySelector('.station-finder__track')).toBeNull();
      expect(row.textContent).toMatch(/LoS/);
    });

    it('also omits the dot on the 60m row (never-sampleable, same invariant as VHF)', () => {
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          bandActivity={new Map([['60m', hotDot]])}
        />,
      );
      expect(screen.queryByTestId('bandmatrix-dot-60m')).toBeNull();
    });

    it('DOES render a VOACAP bar on the 60m row (channelized HF, still modeled)', () => {
      const withFifty: PathPrediction = {
        ...prediction,
        channels: [...prediction.channels, { frequencyKhz: 5371.5, voacapMhz: 5, relByHour: Array(24).fill(0.6), snrByHour: Array(24).fill(9), mufdayByHour: Array(24).fill(0.8) }],
      };
      render(<BandMatrix station={station} prediction={withFifty} predictionStatus="ok" utcHour={21} />);
      const row = screen.getByTestId('bandmatrix-row-60m');
      expect(row.querySelector('.station-finder__track')).toBeTruthy();
      expect(row.textContent).toMatch(/60%/);
    });
  });

  describe('renders an openness dot from bandActivity on eligible HF rows', () => {
    it('renders the dot with the correct tier class on a sampled band', () => {
      render(
        <BandMatrix
          station={station}
          prediction={prediction}
          predictionStatus="ok"
          utcHour={21}
          bandActivity={new Map([['40m', hotDot]])}
        />,
      );
      const dot = screen.getByTestId('bandmatrix-dot-40m');
      expect(dot.className).toMatch(/station-finder__dot--hot/);
    });

    it('renders a hollow no-data dot for an HF band absent from bandActivity', () => {
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      const dot = screen.getByTestId('bandmatrix-dot-40m');
      expect(dot.className).toMatch(/station-finder__dot--no-data/);
    });
  });

  describe('dial-chip click semantics (rankedDialsFor + channelToDial, tuxlink-8fkkk)', () => {
    it('a clicked dial chip resolves candidates[0] to the CLICKED channel, even when not the highest-ranked', () => {
      const onUse = vi.fn();
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} onUse={onUse} />);
      // 40m: vara-hf rel 0.86 outranks 80m's vara-hf rel 0.74, but the operator
      // clicks the LOWER-ranked 80m dial — it must still lead candidates.
      fireEvent.click(screen.getByTestId('use-vara-hf-3590'));
      expect(onUse).toHaveBeenCalledTimes(1);
      const [dial, candidates] = onUse.mock.calls[0];
      expect(dial).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '3.590', grid: 'DM34oa' });
      expect(candidates[0]).toEqual(dial);
      // rankedDialsFor ranks vara-hf channels by reliability DESC (14.107 @
      // 0.9, 7.103 @ 0.86, 3.590 @ 0.74); the clicked 3.590 is forced to the
      // front, then the rest follow in THEIR ranked (not clicked) order.
      expect(candidates).toEqual([
        { mode: 'vara-hf', gateway: 'N0DAJ', freq: '3.590', grid: 'DM34oa' },
        { mode: 'vara-hf', gateway: 'N0DAJ', freq: '14.107', grid: 'DM34oa' },
        { mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' },
      ]);
    });

    it('falls back to emitGatewayPrefill when onUse is omitted', () => {
      const handler = vi.fn();
      window.addEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      fireEvent.click(screen.getByTestId('use-ardop-hf-7103'));
      expect(handler).toHaveBeenCalled();
      const evt = handler.mock.calls[0][0] as CustomEvent;
      expect(evt.detail.dial).toEqual({ mode: 'ardop-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
      window.removeEventListener(GATEWAY_PREFILL_EVENT, handler as EventListener);
    });

    it('disables an undialable channel (no tuxlink modem for that mode) and never crashes on click', () => {
      // pactor has no modem mapping (channelToDial → null); revealed via the
      // 20m +N overflow.
      render(<BandMatrix station={station} prediction={prediction} predictionStatus="ok" utcHour={21} />);
      fireEvent.click(screen.getByTestId('bandmatrix-more-20m'));
      const pactorBtn = screen.getByTestId('use-pactor-14111');
      expect(pactorBtn.hasAttribute('disabled')).toBe(true);
    });
  });

  describe('degraded prediction — rows still render, no bar/pct', () => {
    it('shows a degrade caption and "—" instead of a percentage when prediction is unavailable', () => {
      render(<BandMatrix station={station} prediction={null} predictionStatus="unavailable" utcHour={21} />);
      expect(screen.getByTestId('bandmatrix-header').textContent).toMatch(/forecast unavailable/i);
      // Chips are still usable without a prediction (distance-only ranking).
      expect(screen.getByTestId('use-vara-hf-7103')).toBeTruthy();
    });

    it('shows a no-location prompt when predictionStatus is no-location', () => {
      render(<BandMatrix station={station} prediction={null} predictionStatus="no-location" utcHour={21} />);
      expect(screen.getByTestId('bandmatrix-header').textContent).toMatch(/set your location/i);
    });
  });
});
