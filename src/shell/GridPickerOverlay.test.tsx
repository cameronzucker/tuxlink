/**
 * GridPickerOverlay + GridEdit "Pick on map" wiring (triage #18).
 *
 * SHAPE/WIRING ONLY: the real MapLibre projection + pin render are grim-verified.
 * The map is the global maplibre test double; we drive a pin-mode click on the
 * constructed map and prove it yields a locator that confirm commits through the
 * field's existing onCommit path.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, act, fireEvent } from '@testing-library/react';
import { getLastMap } from '../map/testMapLibreMock';

import { GridPickerOverlay } from './GridPickerOverlay';
import { GridEdit } from './GridEdit';

/** Load the most-recently constructed map and fire a pin-mode click on it. */
function dropPin(lng: number, lat: number) {
  const map = getLastMap()!;
  act(() => map.__emit('load')); // interactions subscribe once the map loads
  act(() => map.__emit('click', { lngLat: { lng, lat } }));
}

describe('GridPickerOverlay (triage #18)', () => {
  it('confirm is disabled until a pin is dropped', () => {
    render(<GridPickerOverlay onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('grid-picker-confirm')).toBeDisabled();
    expect(screen.getByTestId('grid-picker-readout').textContent).toMatch(/click the map/i);
  });

  it('a map pin yields a locator; confirm commits the normalized grid', () => {
    const onConfirm = vi.fn();
    render(<GridPickerOverlay onConfirm={onConfirm} onCancel={vi.fn()} />);
    dropPin(-118.2, 33.6);
    const readout = screen.getByTestId('grid-picker-readout').textContent ?? '';
    expect(readout).toMatch(/Locator: [A-Z]{2}\d{2}/);
    const confirm = screen.getByTestId('grid-picker-confirm');
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onConfirm.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}/);
  });

  it('backdrop click and Cancel both cancel', () => {
    const onCancel = vi.fn();
    render(<GridPickerOverlay onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByTestId('grid-picker-overlay'));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });
});

describe('GridEdit "Pick on map" wiring (triage #18)', () => {
  it('opens the picker from edit mode and commits the pinned grid via onCommit', () => {
    const onCommit = vi.fn();
    render(
      <GridEdit
        grid="DM33"
        source="Manual"
        gpsReady={false}
        onCommit={onCommit}
        onUseGps={vi.fn()}
        onUseManual={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('grid-value-display'));
    const pick = screen.getByTestId('grid-pick-on-map');
    fireEvent.mouseDown(pick);
    expect(screen.getByTestId('grid-picker-overlay')).toBeInTheDocument();
    dropPin(-118.2, 33.6);
    fireEvent.click(screen.getByTestId('grid-picker-confirm'));
    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}/);
  });
});
