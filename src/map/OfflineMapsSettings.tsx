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
  cancelDownload,
  emitPacksChanged,
  type Continent,
  type InstalledPack,
  type RegionManifest,
  type Tier,
} from './offlineMaps';
import { useDownloadProgress } from './useDownloadProgress';
import './OfflineMapsSettings.css';

/** Human-readable byte size (MB/GB), for the pack list + preset estimates. */
export function formatBytes(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)} GB`;
  if (n >= 1_000_000) return `${Math.round(n / 1_000_000)} MB`;
  if (n >= 1000) return `${Math.round(n / 1000)} KB`;
  return `${n} B`;
}

/** Transfer rate (e.g. `14.8 MB/s`) for the download progress row. */
export function formatRate(bytesPerSec: number | null): string {
  if (bytesPerSec == null || !Number.isFinite(bytesPerSec) || bytesPerSec <= 0) return '—';
  if (bytesPerSec >= 1_000_000) return `${(bytesPerSec / 1_000_000).toFixed(1)} MB/s`;
  if (bytesPerSec >= 1000) return `${Math.round(bytesPerSec / 1000)} KB/s`;
  return `${Math.round(bytesPerSec)} B/s`;
}

/** Rough time-remaining (e.g. `~2 min left`, `~45 sec left`) for the progress row. */
export function formatEta(secs: number | null): string {
  if (secs == null || !Number.isFinite(secs) || secs < 0) return '';
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600);
    const m = Math.round((secs % 3600) / 60);
    return `~${h} hr${m ? ` ${m} min` : ''} left`;
  }
  if (secs >= 60) return `~${Math.round(secs / 60)} min left`;
  return `~${Math.max(1, Math.round(secs))} sec left`;
}

export function OfflineMapsSettings() {
  const { grid, fixLat, fixLon } = useLocationConfig();
  const [manifest, setManifest] = useState<RegionManifest | null>(null);
  const [packs, setPacks] = useState<InstalledPack[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [continentId, setContinentId] = useState('');
  // The active *download* busy key (tier-*/continent-*, not delete-*) drives the
  // inline progress row. A failed download stays here so the row shows the error
  // + Retry until the operator retries or starts something else.
  const [downloadKey, setDownloadKey] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [retry, setRetry] = useState<(() => void) | null>(null);

  const progress = useDownloadProgress(downloadKey);

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

  // A download: drives the inline progress row (download-done event clears it).
  // On failure the row shows the error + Retry; the failure also lands in
  // `downloadError` so it persists after `busy` clears.
  async function runDownloadOp(label: string, fn: () => Promise<unknown>, key: string) {
    setBusy(key);
    setError(null);
    setDownloadError(null);
    setDownloadKey(key);
    setRetry(() => () => void runDownloadOp(label, fn, key));
    try {
      await fn();
      emitPacksChanged();
      await refresh();
      setDownloadKey(null);
    } catch (e) {
      // A cancel surfaces as the backend's "download cancelled" error — that is
      // an operator action, not a failure: return the row to idle (no error +
      // Retry). Any other rejection keeps the message for the inline error row.
      if (String(e).includes('download cancelled')) {
        setDownloadKey(null);
      } else {
        setDownloadError(`${label} failed: ${e}`);
      }
    } finally {
      setBusy(null);
    }
  }

  // A delete: no progress bar, just the inline "Deleting…" affordance.
  async function runDeleteOp(label: string, fn: () => Promise<unknown>, key: string) {
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
    void runDownloadOp(
      `Download ${t.label}`,
      () => downloadPack({ kind: 'tier', tier_id: t.id, lon0: centroid.lon, lat0: centroid.lat }),
      `tier-${t.id}`,
    );
  }

  function onDownloadContinent(c: Continent) {
    void runDownloadOp(
      `Download ${c.label}`,
      () => downloadPack({ kind: 'continent', continent_id: c.id }),
      `continent-${c.id}`,
    );
  }

  function onDelete(p: InstalledPack) {
    void runDeleteOp(`Delete ${p.label}`, () => deletePack(p.id), `delete-${p.id}`);
  }

  function onCancel() {
    // The progress hook tracks the backend pack id; the command takes it. Since
    // only one download runs at a time, cancelling the tracked pack is correct.
    if (progress.trackedId) void cancelDownload(progress.trackedId);
  }

  const downloading = busy != null;
  const continent = manifest?.continents.find((c) => c.id === continentId);
  // The inline progress row renders for an active download (or a failed one
  // awaiting Retry) — never for a delete.
  const showProgressRow =
    downloadKey != null && (busy === downloadKey || downloadError != null);

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

      {/* Inline download progress row (no popup) — a determinate bar plus the
          live byte/rate/eta readout and a Cancel, or an error + Retry on failure.
          Renders only while a download is active or awaiting Retry. */}
      {showProgressRow && (
        <div className="tux-offlinemaps-progress" role="status" aria-live="polite">
          {downloadError ? (
            <div className="tux-offlinemaps-progress-failed">
              <span className="tux-offlinemaps-progress-error">
                Download failed: {progress.error ?? downloadError}
              </span>
              <button
                type="button"
                className="tux-offlinemaps-progress-retry"
                onClick={() => retry?.()}
              >
                Retry
              </button>
            </div>
          ) : (
            <>
              <div className="tux-offlinemaps-progress-bar-row">
                <progress
                  className="tux-offlinemaps-progress-bar"
                  max={1}
                  value={progress.percent}
                  aria-label="Download progress"
                />
                <span className="tux-offlinemaps-progress-pct">
                  {Math.round(progress.percent * 100)}%
                </span>
                <button
                  type="button"
                  className="tux-offlinemaps-progress-cancel"
                  onClick={onCancel}
                >
                  Cancel
                </button>
              </div>
              <div className="tux-offlinemaps-progress-meta">
                <span>
                  {formatBytes(progress.bytes)} / {formatBytes(progress.total)}
                </span>
                <span>{formatRate(progress.rateBps)}</span>
                {formatEta(progress.etaSecs) && <span>{formatEta(progress.etaSecs)}</span>}
              </div>
            </>
          )}
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
