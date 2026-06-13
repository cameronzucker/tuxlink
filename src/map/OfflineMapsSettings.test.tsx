/**
 * Tests for OfflineMapsSettings — the region-pack manager (tuxlink-ndi4, phase 4).
 * The basemap_* commands + useLocationConfig are mocked; this verifies the UI
 * wiring (proactive area offer anchored on grid, continent pick, list/delete,
 * change signalling), not the Rust backend.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';

const h = vi.hoisted(() => ({
  packsResp: { packs: [] as Array<Record<string, unknown>>, total_bytes: 0 },
  downloadPack: vi.fn().mockResolvedValue({}),
  deletePack: vi.fn().mockResolvedValue(true),
  emitPacksChanged: vi.fn(),
}));

vi.mock('../location/useLocationConfig', () => ({
  useLocationConfig: () => ({ grid: 'DM43', fixLat: null, fixLon: null }),
}));

vi.mock('./offlineMaps', () => ({
  listPacks: () => Promise.resolve(h.packsResp),
  getManifest: () =>
    Promise.resolve({
      schema: 'tuxlink-basemap-manifest/1',
      planet_build: '20260608',
      planet_url: 'https://build.protomaps.com/20260608.pmtiles',
      pmtiles_schema: { planetiler_version: 4, vector_layers: [] },
      tiers: [
        { id: 'local', label: 'Local', half_deg: [1, 0.75], typical_bytes: 17_000_000 },
        { id: 'wide', label: 'Wide', half_deg: [7.5, 6], typical_bytes: 1_000_000_000, default: true },
      ],
      continents: [
        { id: 'na', label: 'North America', bbox: [-170, 5, -50, 84], typical_bytes: 30_000_000_000 },
      ],
    }),
  downloadPack: h.downloadPack,
  deletePack: h.deletePack,
  emitPacksChanged: h.emitPacksChanged,
}));

import { OfflineMapsSettings, formatBytes } from './OfflineMapsSettings';

beforeEach(() => {
  h.packsResp = { packs: [], total_bytes: 0 };
  h.downloadPack.mockClear();
  h.deletePack.mockClear();
  h.emitPacksChanged.mockClear();
});

describe('formatBytes', () => {
  it('renders GB/MB/KB', () => {
    expect(formatBytes(1_000_000_000)).toBe('1.0 GB');
    expect(formatBytes(203_000_000)).toBe('203 MB');
    expect(formatBytes(17_000_000)).toBe('17 MB');
  });
});

describe('OfflineMapsSettings', () => {
  it('offers area presets anchored on the operator grid (F-2)', async () => {
    render(<OfflineMapsSettings />);
    expect(await screen.findByText(/Detail for your area \(DM43\)/)).toBeInTheDocument();
    // The Wide default preset shows its size estimate.
    expect(screen.getByText(/Wide · ~1\.0 GB/)).toBeInTheDocument();
    expect(screen.getByText(/Local · ~17 MB/)).toBeInTheDocument();
  });

  it('downloads a tier anchored on the grid centroid, then signals the map', async () => {
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(1));
    const arg = h.downloadPack.mock.calls[0][0];
    expect(arg.kind).toBe('tier');
    expect(arg.tier_id).toBe('wide');
    expect(typeof arg.lon0).toBe('number');
    expect(typeof arg.lat0).toBe('number');
    expect(h.emitPacksChanged).toHaveBeenCalled();
  });

  it('lists installed packs with size + total disk, and deletes them', async () => {
    h.packsResp = {
      packs: [
        {
          id: 'tier-wide-n34-w112',
          label: 'Wide — 33.5,-112.0',
          bbox: [-119.5, 27.5, -104.5, 39.5],
          minzoom: 0,
          maxzoom: 14,
          schema_version: '3.7.1',
          bytes: 1_000_000_000,
          source_build: '20260608',
          installed_at: '2026-06-13T00:00:00Z',
        },
      ],
      total_bytes: 1_000_000_000,
    };
    render(<OfflineMapsSettings />);
    expect(await screen.findByText('Wide — 33.5,-112.0')).toBeInTheDocument();
    expect(screen.getByText(/1\.0 GB on disk/)).toBeInTheDocument();
    fireEvent.click(screen.getByText('Delete'));
    await waitFor(() => expect(h.deletePack).toHaveBeenCalledWith('tier-wide-n34-w112'));
    expect(h.emitPacksChanged).toHaveBeenCalled();
  });

  it('downloads a named continent', async () => {
    render(<OfflineMapsSettings />);
    const select = await screen.findByLabelText('Continent');
    fireEvent.change(select, { target: { value: 'na' } });
    fireEvent.click(screen.getByText('Download'));
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(1));
    expect(h.downloadPack.mock.calls[0][0]).toMatchObject({ kind: 'continent', continent_id: 'na' });
  });
});
