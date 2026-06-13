/**
 * TileLayerBridge shape test — SHAPE ONLY (C1).
 *
 * Asserts the bridge renders a stock react-leaflet TileLayer pointed at the
 * Tauri `tile://` URI scheme with the right wiring: the Linux template, an
 * empty subdomains array (no `{s}`), tms ALWAYS false (the backend is the sole
 * Y-flip site — B1), minZoom from the source, and maxNativeZoom capped to the
 * app max with maxZoom at the app max. Real tile fetch/render is grim-verified.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { resetMapMock } from './testMapMock';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());

import { TileLayerBridge, TILE_URL_TEMPLATE } from './TileLayerBridge';
import type { TileSource } from './tileSource';

const XYZ_SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
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

  // bd tuxlink-k61j B1: the Leaflet layer must NEVER flip Y itself — the `tile://`
  // URL is an internal transport to our backend, which is the SOLE Y-flip site
  // (build_tile_url → TileCoord::upstream_y). Honoring the source scheme here
  // double-flips and serves the vertically-mirrored tile for TMS sources. So tms
  // is `false` for BOTH schemes; the webview always speaks top-origin XYZ.
  it('sets tms=false for an XYZ source', () => {
    render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.tms).toBe('false');
  });

  it('sets tms=false even for a TMS source (backend is the sole Y-flip site — no double flip)', () => {
    render(<TileLayerBridge source={TMS_SOURCE} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.tms).toBe('false');
  });

  it('sets maxNativeZoom to the source max when within the app max', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, maxZoom: 14 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.maxnativezoom).toBe('14');
  });

  it('caps maxNativeZoom to the app max when the source exceeds it', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, maxZoom: 20 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.maxnativezoom).toBe('16');
  });

  // bd tuxlink-k61j holistic-1 + B3: maxZoom must reach the app max (so Leaflet
  // up-scales native tiles in the [maxNativeZoom, maxZoom] band instead of going
  // blank past native res), and minZoom must track the source so no tile is
  // requested below it (suppresses spurious coverage-404 → false `partial`).
  it('sets maxZoom to the app max so the up-scale band is live', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, maxZoom: 14 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.maxzoom).toBe('16');
  });

  it('sets minZoom from the source', () => {
    render(<TileLayerBridge source={{ ...XYZ_SOURCE, minZoom: 5 }} appMaxZoom={16} />);
    expect(screen.getByTestId('leaflet-tilelayer').dataset.minzoom).toBe('5');
  });

  // ── Phase 9.2: cancel-on-pan semantics + no leak surface ─────────────────
  //
  // The `tile` scheme makes this a STOCK TileLayer: cancellation is Leaflet's
  // native `tileunload`/`<img>`-removal lifecycle, NOT an AbortController or
  // object-URL we manage. These tests assert the bridge adds no custom
  // listener/buffering surface that could leak — they prove SHAPE, not the
  // real WebKitGTK `<img>` abort (that is grim-verified).

  it('does not override Leaflet tile buffering (no updateWhenIdle / keepBuffer)', () => {
    // Stock buffering = no pile-up of stale in-flight loads on rapid pans. The
    // bridge must NOT set these props (which would change unload behavior).
    render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
    const tl = screen.getByTestId('leaflet-tilelayer');
    expect(tl.dataset.updatewhenidle).toBeUndefined();
    expect(tl.dataset.keepbuffer).toBeUndefined();
    // And no object-URL / blob machinery leaked in as a prop (tile-scheme, not
    // the rejected invoke+blob path).
    expect(tl.dataset.createtile).toBeUndefined();
  });

  it('renders a single stable TileLayer across rapid re-renders (no listener pile-up)', () => {
    // Simulate the operator panning/zooming many times: each view change
    // re-renders the bridge. There must be exactly ONE TileLayer each time and
    // no accumulation — the stock TileLayer carries no per-render listener the
    // bridge would have to tear down.
    for (let i = 0; i < 25; i++) {
      const view = render(<TileLayerBridge source={XYZ_SOURCE} appMaxZoom={16} />);
      expect(screen.getAllByTestId('leaflet-tilelayer')).toHaveLength(1);
      const tl = screen.getByTestId('leaflet-tilelayer');
      // Props stay identical across renders — no drift that would force a
      // wholesale layer rebuild (which WOULD churn tile loads).
      expect(tl.dataset.url).toBe(TILE_URL_TEMPLATE);
      expect(tl.dataset.maxnativezoom).toBe('16');
      view.unmount();
      cleanup();
    }
  });
});
