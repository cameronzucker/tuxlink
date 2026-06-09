/**
 * GridMapPicker shape test — SHAPE ONLY (C1).
 *
 * Proves wiring: boxZoom disabled on the substrate, pin-click reports a 4-char
 * grid, box-drag fires onBoxChange + toggles map dragging, a temp rectangle
 * appears during a drag, and modes are separated. The live rubber-band
 * preview, no-pan-during-drag, post-drag click suppression, and real
 * projection are verified via grim — NOT asserted here.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { fireMapEvent, getMockMap, resetMapMock } from './testMapMock';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('./testMapMock')).createLeafletMock());
vi.mock('./assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

import { GridMapPicker } from './GridMapPicker';

describe('<GridMapPicker> (shape only)', () => {
  beforeEach(() => {
    resetMapMock();
  });

  it('disables native box-zoom on the substrate', () => {
    render(<GridMapPicker mode="box" gridOverlay={false} />);
    expect(screen.getByTestId('leaflet-map').dataset.boxzoom).toBe('false');
  });

  it('pin mode: a click reports the 4-char broadcast grid', () => {
    const onGridChange = vi.fn();
    render(<GridMapPicker mode="pin" onGridChange={onGridChange} gridOverlay={false} />);
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    expect(onGridChange).toHaveBeenCalledOnce();
    expect(onGridChange.mock.calls[0][0]).toHaveLength(4);
  });

  it('box mode: a drag fires onBoxChange with the two corners and toggles map dragging', () => {
    const onBoxChange = vi.fn();
    render(<GridMapPicker mode="box" onBoxChange={onBoxChange} gridOverlay={false} />);
    act(() => {
      fireMapEvent('mousedown', { lat: 60.2, lng: -120.9 });
    });
    act(() => {
      fireMapEvent('mouseup', { lat: 40.8, lng: -140.1 });
    });
    expect(onBoxChange).toHaveBeenCalledWith({ lat: 60.2, lon: -120.9 }, { lat: 40.8, lon: -140.1 });
    const map = getMockMap();
    expect(map.dragging.disable).toHaveBeenCalledOnce();
    expect(map.dragging.enable).toHaveBeenCalledOnce();
  });

  it('box mode: dragging shows a temporary selection rectangle', () => {
    render(<GridMapPicker mode="box" onBoxChange={vi.fn()} gridOverlay={false} />);
    expect(screen.queryByTestId('leaflet-rectangle')).toBeNull();
    act(() => {
      fireMapEvent('mousedown', { lat: 10, lng: 10 });
    });
    act(() => {
      fireMapEvent('mousemove', { lat: 20, lng: 25 });
    });
    expect(screen.getByTestId('leaflet-rectangle')).toBeInTheDocument();
  });

  it('box mode: a plain click does not report a pin grid', () => {
    const onGridChange = vi.fn();
    render(<GridMapPicker mode="box" onGridChange={onGridChange} gridOverlay={false} />);
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    expect(onGridChange).not.toHaveBeenCalled();
  });

  it('pin mode: renders a marker + grid-square highlight for the held grid', () => {
    render(<GridMapPicker mode="pin" grid="CN87us" gridOverlay={false} />);
    expect(screen.getByTestId('leaflet-marker')).toBeInTheDocument();
    expect(screen.getByTestId('leaflet-rectangle')).toBeInTheDocument();
  });

  it('box mode: releasing off-map aborts the drag and re-enables dragging (codex C-impl)', () => {
    const onBoxChange = vi.fn();
    render(<GridMapPicker mode="box" onBoxChange={onBoxChange} gridOverlay={false} />);
    act(() => {
      fireMapEvent('mousedown', { lat: 10, lng: 10 });
    });
    act(() => {
      fireMapEvent('mousemove', { lat: 20, lng: 25 });
    });
    expect(screen.getByTestId('leaflet-rectangle')).toBeInTheDocument();
    // Pointer released OUTSIDE the map container → only the window mouseup fires.
    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup'));
    });
    expect(screen.queryByTestId('leaflet-rectangle')).toBeNull(); // temp cleared
    expect(getMockMap().dragging.enable).toHaveBeenCalled(); // panning restored
    expect(onBoxChange).not.toHaveBeenCalled(); // abort, not a completed box
  });
});
