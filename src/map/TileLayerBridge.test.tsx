/**
 * TileLayerBridge shape test — SHAPE ONLY (C1).
 *
 * Asserts the bridge renders a stock react-leaflet TileLayer pointed at the
 * Tauri `tile://` URI scheme with the right wiring: the Linux template, an
 * empty subdomains array (no `{s}`), tms derived from the source scheme, and
 * maxNativeZoom capped to the app max. Real tile fetch/render is grim-verified.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { resetMapMock } from './testMapMock';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());

import { TileLayerBridge, TILE_URL_TEMPLATE } from './TileLayerBridge';
import type { TileSource } from './tileSource';

const XYZ_SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  crs: 'Geodetic',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: null,
  label: 'XYZ source',
};

const TMS_SOURCE: TileSource = { ...XYZ_SOURCE, scheme: 'Tms', label: 'TMS source' };

describe('<TileLayerBridge> (shape only)', () => {
  beforeEach(() => {
    resetMapMock();
  });

  it('renders a TileLayer pointed at the tile:// URI scheme template', () => {
    render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
    const tl = screen.getByTestId('leaflet-tilelayer');
    expect(tl.dataset.url).toBe(TILE_URL_TEMPLATE);
    expect(TILE_URL_TEMPLATE).toBe('tile://localhost/{z}/{x}/{y}');
  });

  it('passes an empty subdomains array (no {s} placeholder)', () => {
    render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
    const tl = screen.getByTestId('leaflet-tilelayer');
    expect(tl.dataset.subdomains).toBe(JSON.stringify([]));
  });

  it('sets tms=false for an XYZ source', () => {
    render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.tms).toBe('false');
  });

  it('sets tms=true for a TMS source', () => {
    render(<TileLayerBridge source={TMS_SOURCE} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.tms).toBe('true');
  });

  it('sets maxNativeZoom to the source max when within the app max', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, maxZoom: 14 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.maxnativezoom).toBe('14');
  });

  it('caps maxNativeZoom to the app max when the source exceeds it', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, maxZoom: 20 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.maxnativezoom).toBe('16');
  });
});
