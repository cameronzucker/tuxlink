/**
 * PositionPickerOverlay (tuxlink-sdbd / design §6) — the expand-to-overlay
 * Position location picker + precision selector.
 *
 * SHAPE/WIRING ONLY: real Leaflet projection + pin render are grim-verified.
 * react-leaflet/leaflet are mocked via the shared testMapMock; a map click is
 * simulated with fireMapEvent. The precision gate (sixCharAllowed) is tested in
 * tileSource.test.ts — here we assert the overlay HONORS it: raster-only ⇒
 * 6-char disabled, 4-char the default, confirm returns the precision-applied grid.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act, fireEvent, waitFor } from '@testing-library/react';
import { fireMapEvent, resetMapMock } from '../map/testMapMock';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { PositionPickerOverlay } from './PositionPickerOverlay';

// Default: no LAN tile source — the bundled-raster status. This is the common
// state, and the one that keeps 6-char gated off.
beforeEach(() => {
  resetMapMock();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'tile_source_status') {
      return { kind: 'bundled', zoom: 2, label: null, cachedAt: null };
    }
    return undefined;
  });
});

describe('PositionPickerOverlay (tuxlink-sdbd / §6)', () => {
  it('renders an in-app overlay (dimmed backdrop, not an OS window)', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-overlay')).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: /pick.*location/i })).toBeInTheDocument();
  });

  it('seeds the readout from initialGrid and updates it from a map pin', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    // Seeded at 4-char default precision.
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/CN87/);
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/^[A-Z]{2}\d{2}/);
  });

  it('defaults to 4-char precision and confirms a 4-char locator', () => {
    const onConfirm = vi.fn();
    render(<PositionPickerOverlay initialGrid="" onConfirm={onConfirm} onCancel={vi.fn()} />);
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    fireEvent.click(screen.getByTestId('position-picker-confirm'));
    expect(onConfirm).toHaveBeenCalledOnce();
    // 4-char default → exactly four characters returned.
    expect(onConfirm.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}$/);
  });

  it('disables the 6-char option on a raster-only (bundled) substrate, with an explanatory hint', async () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    // Wait for the tile-status fetch to settle.
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('tile_source_status'));
    const sixChar = screen.getByTestId('precision-6char');
    expect(sixChar).toBeDisabled();
    // The hint tells the operator HOW to unlock 6-char (LAN tiles + closer zoom).
    expect(screen.getByTestId('precision-hint').textContent).toMatch(/LAN tile|closer zoom|map tiles/i);
  });

  it('Reset to GPS fix returns the pin to the arbiter grid', () => {
    const onConfirm = vi.fn();
    render(
      <PositionPickerOverlay
        initialGrid="CN87us"
        gpsGrid="EM26"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    // Pan away by pinning elsewhere.
    act(() => {
      fireMapEvent('click', { lat: 10, lng: 10 });
    });
    fireEvent.click(screen.getByTestId('position-picker-reset-gps'));
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/EM26/);
    fireEvent.click(screen.getByTestId('position-picker-confirm'));
    expect(onConfirm).toHaveBeenCalledWith('EM26');
  });

  it('hides Reset to GPS fix when no GPS grid is available', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" gpsGrid={null} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.queryByTestId('position-picker-reset-gps')).toBeNull();
  });

  it('cancels on the × button, the Cancel button, and a backdrop click', () => {
    const onCancel = vi.fn();
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.click(screen.getByLabelText('Close'));
    fireEvent.mouseDown(screen.getByTestId('position-picker-overlay'));
    expect(onCancel).toHaveBeenCalledTimes(3);
  });

  it('confirm is disabled until a locator is set (no initial grid, no pin)', () => {
    render(<PositionPickerOverlay initialGrid="" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-confirm')).toBeDisabled();
  });
});
