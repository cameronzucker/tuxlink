import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

// Mock the leaflet substrate so this is a wiring/shape test (per the map
// subsystem's C1 convention — real drag/render is grim-verified). The Marker
// stub exposes its position + draggable flag for assertions.
vi.mock('../map/BaseMap', () => ({
  BaseMap: ({ children }: { children?: React.ReactNode }) => <div data-testid="basemap">{children}</div>,
}));
vi.mock('../map/useTileSource', () => ({ useTileSource: () => null }));
vi.mock('react-leaflet', () => ({
  Marker: (p: { position: [number, number]; draggable?: boolean }) => (
    <div
      data-testid="marker"
      data-draggable={p.draggable ? 'true' : 'false'}
      data-pos={`${p.position[0]},${p.position[1]}`}
    />
  ),
  Rectangle: () => <div data-testid="grid-square" />,
}));

import { LocationMap } from './LocationMap';

describe('LocationMap', () => {
  it('places a draggable marker at the live fix when a GPS source is selected', () => {
    render(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
    );
    const m = screen.getByTestId('marker');
    expect(m).toHaveAttribute('data-draggable', 'true'); // flow 3: pin is draggable even with a fix
    expect(m).toHaveAttribute('data-pos', '36.1,-86.8'); // at the precise fix
  });

  it('follows the manual grid (not the live fix) when source is manual', () => {
    // A fix is present but the operator picked manual — the marker must not jump
    // to the fix (flow 3: a hand-set pin is not overridden by an arriving fix).
    render(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="manual" onGridChange={vi.fn()} />,
    );
    const m = screen.getByTestId('marker');
    expect(m).toHaveAttribute('data-draggable', 'true');
    expect(m.getAttribute('data-pos')).not.toBe('36.1,-86.8'); // grid center, not the fix
  });

  it('renders a draggable marker + grid square when there is no fix', () => {
    render(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('marker')).toHaveAttribute('data-draggable', 'true');
    expect(screen.getByTestId('grid-square')).toBeInTheDocument();
  });

  it('renders the map with no marker when grid is empty and there is no fix', () => {
    render(<LocationMap grid="" fixLatLon={null} selectedSource="manual" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('location-map')).toBeInTheDocument();
    expect(screen.queryByTestId('marker')).not.toBeInTheDocument();
  });
});
