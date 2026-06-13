/**
 * PositionPickerOverlay (tuxlink-sdbd / design §6) — the expand-to-overlay
 * Position location picker + precision selector.
 *
 * SHAPE/WIRING ONLY: real MapLibre projection + pin render are grim-verified. The
 * map is the global maplibre test double; clicks/zoom are driven on the
 * constructed map. The precision gate (sixCharAllowed) is unit-tested in
 * sixCharAllowed.test.ts — here we assert the overlay HONORS it: zoomed out ⇒
 * 6-char disabled, 4-char the default, confirm returns the precision-applied grid.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, act, fireEvent } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { PositionPickerOverlay } from './PositionPickerOverlay';

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}
function setZoom(map: MapLibreMock, zoom: number) {
  act(() => {
    map.__setZoom(zoom);
    map.__emit('moveend');
  });
}

describe('PositionPickerOverlay (tuxlink-sdbd / §6)', () => {
  it('renders an in-app overlay (dimmed backdrop, not an OS window)', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-overlay')).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: /pick.*location/i })).toBeInTheDocument();
  });

  it('seeds the readout from initialGrid and updates it from a map pin', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/CN87/);
    const map = loadLast();
    act(() => map.__emit('click', { lngLat: { lng: -118.2, lat: 33.6 } }));
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/^[A-Z]{2}\d{2}/);
  });

  it('defaults to 4-char precision and confirms a 4-char locator', () => {
    const onConfirm = vi.fn();
    render(<PositionPickerOverlay initialGrid="" onConfirm={onConfirm} onCancel={vi.fn()} />);
    const map = loadLast();
    act(() => map.__emit('click', { lngLat: { lng: -118.2, lat: 33.6 } }));
    fireEvent.click(screen.getByTestId('position-picker-confirm'));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onConfirm.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}$/);
  });

  it('disables 6-char until the view is zoomed past the subsquare threshold', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const map = loadLast(); // initialZoom 6 (placed grid) — below the threshold
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    expect(screen.getByTestId('precision-hint').textContent).toMatch(/zoom in/i);
    setZoom(map, 4); // still zoomed out
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
  });

  it('6-char gate tracks live map zoom via onZoomChange', () => {
    render(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const map = loadLast();
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    setZoom(map, 8); // below SIX_CHAR_MIN_ZOOM (9)
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    setZoom(map, 12); // above the threshold → unlock
    expect(screen.getByTestId('precision-6char')).not.toBeDisabled();
    expect(screen.queryByTestId('precision-hint')).toBeNull();
  });

  it('Reset to GPS fix returns the pin to the arbiter grid', () => {
    const onConfirm = vi.fn();
    render(<PositionPickerOverlay initialGrid="CN87us" gpsGrid="EM26" onConfirm={onConfirm} onCancel={vi.fn()} />);
    const map = loadLast();
    act(() => map.__emit('click', { lngLat: { lng: 10, lat: 10 } }));
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
