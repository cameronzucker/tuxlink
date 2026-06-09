/**
 * MaidenheadOverlay shape test — SHAPE ONLY (C1).
 *
 * Asserts the overlay maps grid geometry to the right COUNT of react-leaflet
 * elements and honours the visible toggle. The line/label coordinates are
 * already proven in gridGeometry.test.ts — do NOT re-assert them through the
 * mock. Real rendering is grim-verified.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { resetMapMock } from './testMapMock';
import { gridLines, GridLevel } from './gridGeometry';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('./testMapMock')).createLeafletMock());

import { MaidenheadOverlay } from './MaidenheadOverlay';

const WORLD = { south: -90, west: -180, north: 90, east: 180 };

describe('<MaidenheadOverlay> (shape only)', () => {
  beforeEach(() => {
    resetMapMock();
  });

  it('renders one polyline per grid line for the given bounds+level', () => {
    const expected = gridLines(WORLD, GridLevel.Field);
    render(<MaidenheadOverlay bounds={WORLD} level={GridLevel.Field} />);
    expect(screen.getAllByTestId('leaflet-polyline')).toHaveLength(
      expected.lonLines.length + expected.latLines.length,
    );
  });

  it('renders a label marker per cell', () => {
    const expected = gridLines(WORLD, GridLevel.Field);
    render(<MaidenheadOverlay bounds={WORLD} level={GridLevel.Field} />);
    expect(screen.getAllByTestId('leaflet-marker')).toHaveLength(expected.labels.length);
  });

  it('renders nothing when visible={false}', () => {
    render(<MaidenheadOverlay bounds={WORLD} level={GridLevel.Field} visible={false} />);
    expect(screen.queryByTestId('leaflet-polyline')).toBeNull();
    expect(screen.queryByTestId('leaflet-marker')).toBeNull();
  });

  it('self-drives from the map bounds/zoom when no overrides are given', () => {
    // canonical mock map → world bounds at zoom 1 → Field level
    const expected = gridLines(WORLD, GridLevel.Field);
    render(<MaidenheadOverlay />);
    expect(screen.getAllByTestId('leaflet-polyline')).toHaveLength(
      expected.lonLines.length + expected.latLines.length,
    );
  });
});
