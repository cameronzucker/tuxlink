/**
 * Drag-select hazard tests for the MapLibre GridPicker (tuxlink-ndi4, phase 2,
 * finding 8 — the historically bug-prone interaction). Wiring only; the live
 * rubber-band render is grim-verified.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from './testMapLibreMock';
import { GridPicker } from './GridPicker';

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

describe('GridPicker — box mode drag-select', () => {
  it('disables pan on mousedown and re-enables + fires onBoxChange on mouseup', () => {
    const onBoxChange = vi.fn();
    render(<GridPicker mode="box" onBoxChange={onBoxChange} />);
    const map = loadLast();
    act(() => map.__emit('mousedown', { lngLat: { lng: -130, lat: 40 } }));
    expect(map.dragPan.disable).toHaveBeenCalled();
    act(() => map.__emit('mousemove', { lngLat: { lng: -120, lat: 50 } }));
    act(() => map.__emit('mouseup', { lngLat: { lng: -120, lat: 50 } }));
    expect(onBoxChange).toHaveBeenCalledWith({ lat: 40, lon: -130 }, { lat: 50, lon: -120 });
    expect(map.dragPan.enable).toHaveBeenCalled();
  });

  it('a window mouseup aborts the drag (re-enables pan, no onBoxChange)', () => {
    const onBoxChange = vi.fn();
    render(<GridPicker mode="box" onBoxChange={onBoxChange} />);
    const map = loadLast();
    act(() => map.__emit('mousedown', { lngLat: { lng: -130, lat: 40 } }));
    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup'));
    });
    expect(onBoxChange).not.toHaveBeenCalled();
    expect(map.dragPan.enable).toHaveBeenCalled();
  });

  it('suppresses the click that fires right after a drag', () => {
    const onGridChange = vi.fn();
    // a box-mode picker still wires click; the post-drag click must be eaten
    render(<GridPicker mode="box" onGridChange={onGridChange} onBoxChange={vi.fn()} />);
    const map = loadLast();
    act(() => map.__emit('mousedown', { lngLat: { lng: -130, lat: 40 } }));
    act(() => map.__emit('mouseup', { lngLat: { lng: -120, lat: 50 } }));
    act(() => map.__emit('click', { lngLat: { lng: -120, lat: 50 } }));
    expect(onGridChange).not.toHaveBeenCalled();
  });
});

describe('GridPicker — pin mode', () => {
  it('reports the 4-char grid for a clicked point', () => {
    const onGridChange = vi.fn();
    render(<GridPicker mode="pin" onGridChange={onGridChange} />);
    const map = loadLast();
    act(() => map.__emit('click', { lngLat: { lng: 0, lat: 0 } }));
    expect(onGridChange).toHaveBeenCalledTimes(1);
    expect(onGridChange.mock.calls[0][0]).toHaveLength(4);
  });

  it('does not start a box drag in pin mode', () => {
    const onBoxChange = vi.fn();
    render(<GridPicker mode="pin" onBoxChange={onBoxChange} />);
    const map = loadLast();
    act(() => map.__emit('mousedown', { lngLat: { lng: -130, lat: 40 } }));
    expect(map.dragPan.disable).not.toHaveBeenCalled();
    act(() => map.__emit('mouseup', { lngLat: { lng: -120, lat: 50 } }));
    expect(onBoxChange).not.toHaveBeenCalled();
  });
});

describe('GridPicker — composition', () => {
  it('builds a maplibre map with the grid lattice + selection sources', () => {
    render(<GridPicker mode="box" />);
    const map = loadLast();
    expect(map.getSource('maidenhead-grid')).toBeTruthy();
    expect(map.getSource('grid-selection')).toBeTruthy();
  });
});
