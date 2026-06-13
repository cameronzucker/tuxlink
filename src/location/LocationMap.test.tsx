import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

// Mock the leaflet substrate so this is a wiring/shape test (per the map
// subsystem's C1 convention — real drag/render is grim-verified).
vi.mock('../map/BaseMap', () => ({
  BaseMap: ({ children }: { children?: React.ReactNode }) => <div data-testid="basemap">{children}</div>,
}));
vi.mock('../map/useTileSource', () => ({ useTileSource: () => null }));
vi.mock('react-leaflet', () => ({
  Marker: (p: { draggable?: boolean }) => (
    <div data-testid={p.draggable ? 'manual-marker' : 'gps-marker'} />
  ),
  Rectangle: () => <div data-testid="grid-square" />,
}));

import { LocationMap } from './LocationMap';

describe('LocationMap', () => {
  it('renders a precise (non-draggable) GPS marker when a fix is present', () => {
    render(<LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} onGridChange={vi.fn()} />);
    expect(screen.getByTestId('gps-marker')).toBeInTheDocument();
    expect(screen.queryByTestId('manual-marker')).not.toBeInTheDocument();
  });

  it('renders a draggable manual marker + grid square when there is no fix', () => {
    render(<LocationMap grid="EM75km" fixLatLon={null} onGridChange={vi.fn()} />);
    expect(screen.getByTestId('manual-marker')).toBeInTheDocument();
    expect(screen.getByTestId('grid-square')).toBeInTheDocument();
  });

  it('renders the map even with an empty grid (no markers, no crash)', () => {
    render(<LocationMap grid="" fixLatLon={null} onGridChange={vi.fn()} />);
    expect(screen.getByTestId('location-map')).toBeInTheDocument();
    expect(screen.queryByTestId('manual-marker')).not.toBeInTheDocument();
  });
});
