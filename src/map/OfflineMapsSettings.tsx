/**
 * OfflineMapsSettings — the region-pack manager (tuxlink-ndi4, phase 4 F-1/F-2).
 *
 * Inline Settings section (Tools→Settings→Offline maps; no pop-up window, per the
 * inline-UI rule). Lets the operator download offline map detail for their area
 * (tier presets anchored on the operator grid — the proactive offer, F-2), pick a
 * named continent, and list / delete installed packs with disk usage. Downloads
 * run through the go-pmtiles sidecar via the basemap_* commands; on change it
 * signals the live map (emitPacksChanged) so the map recomposites immediately.
 */
import { useCallback, useEffect, useState } from 'react';
import { useLocationConfig } from '../location/useLocationConfig';
import { gridToLatLon } from '../forms/position/maidenhead';
import {
  listPacks,
  getManifest,
  downloadPack,
  deletePack,
  emitPacksChanged,
  type Continent,
  type InstalledPack,
  type RegionManifest,
  type Tier,
} from './offlineMaps';
import './OfflineMapsSettings.css';

/** Human-readable byte size (MB/GB), for the pack list + preset estimates. */
export function formatBytes(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)} GB`;
  if (n >= 1_000_000) return `${Math.round(n / 1_000_000)} MB`;
  if (n >= 1000) return `${Math.round(n / 1000)} KB`;
  return `${n} B`;
}

export function OfflineMapsSettings() {
  const { grid, fixLat, fixLon } = useLocationConfig();
  const [manifest, setManifest] = useState<RegionManifest | null>(null);
  const [packs, setPacks] = useState<InstalledPack[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [continentId, setContinentId] = useState('');

  // Operator centroid: prefer the precise live fix, else the configured grid.
  const centroid =
    fixLat != null && fixLon != null ? { lat: fixLat, lon: fixLon } : gridToLatLon(grid || '');

  const refresh = useCallback(async () => {
    try {
      const list = await listPacks();
      setPacks(list.packs);
      setTotalBytes(list.total_bytes);
    } catch (e) {
      setError(`Could not list installed maps: ${e}`);
    }
  }, []);

  useEffect(() => {
    getManifest()
      .then(setManifest)
      .catch(() => setManifest(null));
    void refresh();
  }, [refresh]);

  async function runDownload(label: string, fn: () => Promise<unknown>, key: string) {
    setBusy(key);
    setError(null);
    try {
      await fn();
      emitPacksChanged();
      await refresh();
    } catch (e) {
      setError(`${label} failed: ${e}`);
    } finally {
      setBusy(null);
    }
  }

  function onDownloadTier(t: Tier) {
    if (!centroid) {
      setError('Set your location first to download detail for your area.');
      return;
    }
    void runDownload(
      `Download ${t.label}`,
      () => downloadPack({ kind: 'tier', tier_id: t.id, lon0: centroid.lon, lat0: centroid.lat }),
      `tier-${t.id}`,
    );
  }

  function onDownloadContinent(c: Continent) {
    void runDownload(
      `Download ${c.label}`,
      () => downloadPack({ kind: 'continent', continent_id: c.id }),
      `continent-${c.id}`,
    );
  }

  function onDelete(p: InstalledPack) {
    void runDownload(`Delete ${p.label}`, () => deletePack(p.id), `delete-${p.id}`);
  }

  const downloading = busy != null;
  const continent = manifest?.continents.find((c) => c.id === continentId);

  return (
    <section className="tux-offlinemaps" aria-label="Offline maps">
      <h3 className="tux-offlinemaps-title">Offline maps</h3>
      <p className="tux-offlinemaps-help">
        The world map works offline at low detail. Download detailed map packs for areas you
        operate in — one online download, then permanent and fully offline.
      </p>

      {/* F-2: the proactive offer, anchored on the operator grid. */}
      <div className="tux-offlinemaps-group">
        <div className="tux-offlinemaps-group-head">
          Detail for your area{grid ? ` (${grid})` : ''}
        </div>
        {centroid ? (
          <div className="tux-offlinemaps-presets">
            {(manifest?.tiers ?? []).map((t) => (
              <button
                key={t.id}
                type="button"
                className={`tux-offlinemaps-preset${t.default ? ' is-default' : ''}`}
                disabled={downloading}
                onClick={() => onDownloadTier(t)}
              >
                {busy === `tier-${t.id}` ? 'Downloading…' : `${t.label} · ~${formatBytes(t.typical_bytes)}`}
              </button>
            ))}
          </div>
        ) : (
          <p className="tux-offlinemaps-hint">Set your location to download detail for your area.</p>
        )}
      </div>

      {/* Named continents. */}
      {manifest && manifest.continents.length > 0 && (
        <div className="tux-offlinemaps-group">
          <div className="tux-offlinemaps-group-head">A whole continent</div>
          <div className="tux-offlinemaps-continent">
            <select
              aria-label="Continent"
              value={continentId}
              disabled={downloading}
              onChange={(e) => setContinentId(e.target.value)}
            >
              <option value="">Choose a continent…</option>
              {manifest.continents.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label} (~{formatBytes(c.typical_bytes)})
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={downloading || !continent}
              onClick={() => continent && onDownloadContinent(continent)}
            >
              {busy === `continent-${continentId}` ? 'Downloading…' : 'Download'}
            </button>
          </div>
        </div>
      )}

      {/* Installed packs. */}
      <div className="tux-offlinemaps-group">
        <div className="tux-offlinemaps-group-head">
          Installed map packs{packs.length > 0 ? ` · ${formatBytes(totalBytes)} on disk` : ''}
        </div>
        {packs.length === 0 ? (
          <p className="tux-offlinemaps-hint">No map packs installed yet.</p>
        ) : (
          <ul className="tux-offlinemaps-list">
            {packs.map((p) => (
              <li key={p.id} className="tux-offlinemaps-item">
                <span className="tux-offlinemaps-item-label">{p.label}</span>
                <span className="tux-offlinemaps-item-size">{formatBytes(p.bytes)}</span>
                <button
                  type="button"
                  className="tux-offlinemaps-delete"
                  disabled={downloading}
                  onClick={() => onDelete(p)}
                >
                  {busy === `delete-${p.id}` ? 'Deleting…' : 'Delete'}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {error && (
        <p className="tux-offlinemaps-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
