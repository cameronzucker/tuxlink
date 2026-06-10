/**
 * GridPickerOverlay + GridEdit "Pick on map" wiring (triage #18).
 *
 * SHAPE/WIRING ONLY: the real Leaflet projection + pin render are grim-verified.
 * Here we mock react-leaflet/leaflet (the shared testMapMock) and prove that a
 * map pin produces a locator that confirm commits through the field's existing
 * onCommit path.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act, fireEvent } from '@testing-library/react';
import { fireMapEvent, resetMapMock } from '../map/testMapMock';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

import { GridPickerOverlay } from './GridPickerOverlay';
import { GridEdit } from './GridEdit';

describe('GridPickerOverlay (triage #18)', () => {
  beforeEach(() => resetMapMock());

  it('confirm is disabled until a pin is dropped', () => {
    render(<GridPickerOverlay onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('grid-picker-confirm')).toBeDisabled();
    expect(screen.getByTestId('grid-picker-readout').textContent).toMatch(/click the map/i);
  });

  it('a map pin yields a locator; confirm commits the normalized grid', () => {
    const onConfirm = vi.fn();
    render(<GridPickerOverlay onConfirm={onConfirm} onCancel={vi.fn()} />);
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    const readout = screen.getByTestId('grid-picker-readout').textContent ?? '';
    expect(readout).toMatch(/Locator: [A-Z]{2}\d{2}/);
    const confirm = screen.getByTestId('grid-picker-confirm');
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
    // normalized: upper AA00 field/square form
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
  beforeEach(() => resetMapMock());

  it('opens the picker from edit mode and commits the pinned grid via onCommit', async () => {
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
    // Enter edit mode by clicking the grid value.
    fireEvent.click(screen.getByTestId('grid-value-display'));
    // The Pick-on-map affordance is present in edit mode.
    const pick = screen.getByTestId('grid-pick-on-map');
    fireEvent.mouseDown(pick);
    // Overlay opens.
    expect(screen.getByTestId('grid-picker-overlay')).toBeInTheDocument();
    // Drop a pin and confirm.
    act(() => {
      fireMapEvent('click', { lat: 33.6, lng: -118.2 });
    });
    fireEvent.click(screen.getByTestId('grid-picker-confirm'));
    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}/);
  });
});
