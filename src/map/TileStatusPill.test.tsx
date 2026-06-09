import { describe, test, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TileStatusPill } from './TileStatusPill';

describe('TileStatusPill', () => {
  test.each([
    [{ kind: 'bundled', zoom: 2 }, /z2 · bundled/],
    [{ kind: 'lan-live', zoom: 13 }, /LAN live/],
    [{ kind: 'lan-cached', zoom: 13, cachedAt: '2026-06-09T10:00:00Z' }, /LAN cached as of/],
    [{ kind: 'partial', zoom: 13 }, /LAN live \(partial\)/],
    [{ kind: 'unreachable', zoom: 2 }, /tiles unreachable — bundled/],
    [{ kind: 'incompatible', zoom: 2 }, /incompatible tile source/],
  ])('renders %o', (status, re) => {
    render(<TileStatusPill status={status as any} zoomCapReason="bundled raster max" />);
    expect(screen.getByTestId('tile-status-pill').textContent).toMatch(re);
  });
});
