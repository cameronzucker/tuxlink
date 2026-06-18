/**
 * Tests for OfflineMapsSettings — the region-pack manager (tuxlink-ndi4, phase 4).
 * The basemap_* commands + useLocationConfig are mocked; this verifies the UI
 * wiring (proactive area offer anchored on grid, continent pick, list/delete,
 * change signalling), not the Rust backend.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';

type ProgressView = {
  bytes: number;
  total: number;
  percent: number;
  finishing: boolean;
  rateBps: number | null;
  etaSecs: number | null;
  status: string;
  error: string | null;
  trackedId: string | null;
};
const IDLE_PROGRESS: ProgressView = {
  bytes: 0,
  total: 0,
  percent: 0,
  finishing: false,
  rateBps: null,
  etaSecs: null,
  status: 'idle',
  error: null,
  trackedId: null,
};

const h = vi.hoisted(() => ({
  packsResp: { packs: [] as Array<Record<string, unknown>>, total_bytes: 0 },
  downloadPack: vi.fn().mockResolvedValue({}),
  deletePack: vi.fn().mockResolvedValue(true),
  cancelDownload: vi.fn().mockResolvedValue(undefined),
  refreshManifest: vi.fn().mockResolvedValue(undefined),
  emitPacksChanged: vi.fn(),
  // Mutable progress view the mocked hook returns (the UI tests drive states).
  progress: {
    bytes: 0,
    total: 0,
    percent: 0,
    finishing: false,
    rateBps: null as number | null,
    etaSecs: null as number | null,
    status: 'idle',
    error: null as string | null,
    trackedId: null as string | null,
  },
}));

vi.mock('./useDownloadProgress', () => ({
  useDownloadProgress: () => h.progress,
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
        { id: 'local', label: 'Local', half_deg: [1, 0.75], maxzoom: 8, typical_bytes: 17_000_000 },
        { id: 'wide', label: 'Wide', half_deg: [7.5, 6], maxzoom: 13, typical_bytes: 1_000_000_000, default: true },
      ],
      continents: [
        { id: 'na', label: 'North America', bbox: [-170, 5, -50, 84], typical_bytes: 30_000_000_000 },
      ],
    }),
  refreshManifest: h.refreshManifest,
  downloadPack: h.downloadPack,
  deletePack: h.deletePack,
  cancelDownload: h.cancelDownload,
  emitPacksChanged: h.emitPacksChanged,
  // Real derivation (mirrors the Rust backend) so Cancel-before-first-event
  // targets the correct id (C5).
  packIdForArgs: (args: Record<string, unknown>) => {
    const tok = (v: number, pos: string, neg: string) =>
      `${v < 0 ? neg : pos}${Math.round(Math.abs(v))}`;
    if (args.kind === 'tier') {
      return `tier-${args.tier_id}-${tok(args.lat0 as number, 'n', 's')}-${tok(
        args.lon0 as number,
        'e',
        'w',
      )}`;
    }
    return `continent-${args.continent_id}`;
  },
  // Real mirror of the Rust continent_estimate so the detail-picker option labels
  // render honest sizes in tests (tuxlink-8g28).
  continentEstimateBytes: (baselineZ14: number, maxzoom: number) =>
    Math.max(1, Math.ceil(baselineZ14 / 2 ** Math.max(0, 14 - maxzoom))),
}));

import { OfflineMapsSettings, formatBytes, formatRate, formatEta } from './OfflineMapsSettings';

beforeEach(() => {
  h.packsResp = { packs: [], total_bytes: 0 };
  h.downloadPack.mockClear();
  h.deletePack.mockClear();
  h.cancelDownload.mockClear();
  h.refreshManifest.mockClear();
  h.emitPacksChanged.mockClear();
  h.progress = { ...IDLE_PROGRESS };
});

describe('formatBytes', () => {
  it('renders GB/MB/KB', () => {
    expect(formatBytes(1_000_000_000)).toBe('1.0 GB');
    expect(formatBytes(203_000_000)).toBe('203 MB');
    expect(formatBytes(17_000_000)).toBe('17 MB');
  });

  it('rolls up at unit boundaries instead of "1000 KB"/"1000 MB"', () => {
    // 999_500 rounds to 1000 KB → should roll to 1.0 MB.
    expect(formatBytes(999_500)).toBe('1.0 MB');
    // 999_500_000 rounds to 1000 MB → should roll to 1.0 GB.
    expect(formatBytes(999_500_000)).toBe('1.0 GB');
    // Just below the boundary still reads in the lower unit.
    expect(formatBytes(999_000)).toBe('999 KB');
  });
});

describe('formatRate', () => {
  it('renders MB/s and KB/s, dash for unknown', () => {
    expect(formatRate(14_800_000)).toBe('14.8 MB/s');
    expect(formatRate(250_000)).toBe('250 KB/s');
    expect(formatRate(null)).toBe('—');
    expect(formatRate(0)).toBe('—');
  });
});

describe('formatEta', () => {
  it('renders minutes/seconds/hours, empty for unknown', () => {
    expect(formatEta(120)).toBe('~2 min left');
    expect(formatEta(45)).toBe('~45 sec left');
    expect(formatEta(3700)).toBe('~1 hr 2 min left');
    expect(formatEta(null)).toBe('');
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

  // Codex #5: when a download installs durably but live registration fails
  // (requiresRestart), the pack is on disk but not yet servable. The UI must NOT
  // signal the live map (which would add a source that 404s every tile) and must
  // surface an honest "restart to use" notice instead.
  it('shows a restart notice and does not signal the map when registration is deferred', async () => {
    h.downloadPack.mockResolvedValueOnce({ id: 'tier-wide-n34-w111', requiresRestart: true });
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/restart Tuxlink to use it offline/i)).toBeInTheDocument();
    expect(h.emitPacksChanged).not.toHaveBeenCalled();
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

  it('downloads a named continent at the default (smallest) detail tier', async () => {
    render(<OfflineMapsSettings />);
    const select = await screen.findByLabelText('Continent');
    fireEvent.change(select, { target: { value: 'na' } });
    fireEvent.click(screen.getByText('Download'));
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(1));
    // tuxlink-8g28: the continent download carries the chosen detail tier, and the
    // default is the smallest (first) tier — never the full-detail runaway.
    expect(h.downloadPack.mock.calls[0][0]).toMatchObject({
      kind: 'continent',
      continent_id: 'na',
      tier_id: 'local',
    });
  });

  it('passes the selected detail tier through to the continent download', async () => {
    render(<OfflineMapsSettings />);
    const continentSel = await screen.findByLabelText('Continent');
    fireEvent.change(continentSel, { target: { value: 'na' } });
    fireEvent.change(screen.getByLabelText('Detail level'), { target: { value: 'wide' } });
    fireEvent.click(screen.getByText('Download'));
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(1));
    expect(h.downloadPack.mock.calls[0][0]).toMatchObject({
      kind: 'continent',
      continent_id: 'na',
      tier_id: 'wide',
    });
  });

  it('shows an inline progress row with bar, percent, rate, eta, and Cancel while downloading', async () => {
    // Keep the download promise pending so the row stays mounted.
    let resolveDl: (v: unknown) => void = () => {};
    h.downloadPack.mockImplementationOnce(() => new Promise((r) => (resolveDl = r)));
    h.progress = {
      bytes: 1_400_000_000,
      total: 2_700_000_000,
      percent: 0.53,
      finishing: false,
      rateBps: 14_800_000,
      etaSecs: 120,
      status: 'downloading',
      error: null,
      trackedId: 'tier-wide-n34-w112',
    };
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);

    expect(await screen.findByLabelText('Download progress')).toBeInTheDocument();
    expect(screen.getByText('53%')).toBeInTheDocument();
    // Denominator is the estimate, labelled `~` to stay honest (C4).
    expect(screen.getByText(/1\.4 GB \/ ~2\.7 GB/)).toBeInTheDocument();
    expect(screen.getByText('14.8 MB/s')).toBeInTheDocument();
    expect(screen.getByText('~2 min left')).toBeInTheDocument();

    const cancel = screen.getByText('Cancel');
    fireEvent.click(cancel);
    // C5: Cancel targets the deterministic id derived from the args (DM43
    // centroid ≈ 33.5,-111 → n34/w111), not the hook's latched trackedId.
    expect(h.cancelDownload).toHaveBeenCalledWith('tier-wide-n34-w111');
    resolveDl({});
  });

  it('shows Download failed + Retry on a failed download', async () => {
    h.downloadPack.mockRejectedValueOnce('go-pmtiles exit 1: boom');
    h.progress = {
      ...IDLE_PROGRESS,
      status: 'error',
      error: 'go-pmtiles exit 1: boom',
      trackedId: 'tier-wide-n34-w112',
    };
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);

    expect(await screen.findByText(/Download failed: go-pmtiles exit 1: boom/)).toBeInTheDocument();
    const retry = screen.getByText('Retry');
    fireEvent.click(retry);
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(2));
  });

  it('returns to idle (no error/Retry) when a download is cancelled', async () => {
    h.downloadPack.mockRejectedValueOnce('download cancelled');
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalled());
    // No error row, no Retry — a cancel is an operator action, not a failure.
    await waitFor(() => expect(screen.queryByText(/Download failed/)).not.toBeInTheDocument());
    expect(screen.queryByText('Retry')).not.toBeInTheDocument();
  });

  it('clears the progress row when a download succeeds (returns to idle)', async () => {
    h.downloadPack.mockResolvedValueOnce({});
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalled());
    // After resolve, the row is gone (no progressbar, no Cancel).
    await waitFor(() => expect(screen.queryByLabelText('Download progress')).not.toBeInTheDocument());
    expect(screen.queryByText('Cancel')).not.toBeInTheDocument();
  });

  // B1: a deployed app must pick up the operator's weekly manifest bump without
  // an app release — the settings UI refreshes the remote manifest on mount.
  it('refreshes the remote manifest on mount (B1)', async () => {
    render(<OfflineMapsSettings />);
    await waitFor(() => expect(h.refreshManifest).toHaveBeenCalledTimes(1));
  });

  it('still renders when the mount-time manifest refresh fails (best-effort)', async () => {
    h.refreshManifest.mockRejectedValueOnce(new Error('offline'));
    render(<OfflineMapsSettings />);
    // The local manifest still loads → presets render despite the refresh error.
    expect(await screen.findByText(/Wide · ~1\.0 GB/)).toBeInTheDocument();
  });

  // C5: cancel must work immediately — before the first progress event latches a
  // trackedId in the hook. The UI derives the deterministic backend id from the
  // args it sent and cancels that directly.
  it('cancels with the derived pack id immediately, before any progress event', async () => {
    // Keep the download pending and the hook idle (trackedId null = no event yet).
    let resolveDl: (v: unknown) => void = () => {};
    h.downloadPack.mockImplementationOnce(() => new Promise((r) => (resolveDl = r)));
    h.progress = { ...IDLE_PROGRESS, status: 'downloading', trackedId: null };
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);

    const cancel = await screen.findByText('Cancel');
    fireEvent.click(cancel);
    // Even though trackedId is null, Cancel fires with the derived id.
    expect(h.cancelDownload).toHaveBeenCalledWith('tier-wide-n34-w111');
    resolveDl({});
  });

  // C6: retry must fully reset the progress hook. Re-dispatching the SAME hook
  // key would skip the hook's reset effect, leaking the prior error view into the
  // retry. The UI appends an attempt counter so the key changes per attempt and
  // the hook re-subscribes clean. (The hook-level reset itself is covered in
  // useDownloadProgress.test.ts; here we assert the UI clears the error row.)
  it('retry resets the view so the stale error/Retry row does not carry over', async () => {
    h.downloadPack.mockRejectedValueOnce('go-pmtiles exit 1: boom');
    h.progress = {
      ...IDLE_PROGRESS,
      status: 'error',
      error: 'go-pmtiles exit 1: boom',
      trackedId: 'tier-wide-n34-w111',
    };
    render(<OfflineMapsSettings />);
    const wide = await screen.findByText(/Wide · ~1\.0 GB/);
    fireEvent.click(wide);
    expect(await screen.findByText(/Download failed: go-pmtiles exit 1: boom/)).toBeInTheDocument();

    // The retry succeeds and the hook (which re-keys) reports idle again.
    h.downloadPack.mockResolvedValueOnce({});
    h.progress = { ...IDLE_PROGRESS };
    fireEvent.click(screen.getByText('Retry'));
    await waitFor(() => expect(h.downloadPack).toHaveBeenCalledTimes(2));
    // No stale error row after a successful retry.
    await waitFor(() => expect(screen.queryByText(/Download failed/)).not.toBeInTheDocument());
    expect(screen.queryByText('Retry')).not.toBeInTheDocument();
  });
});
