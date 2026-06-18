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
  refreshManifest,
  downloadPack,
  deletePack,
  cancelDownload,
  emitPacksChanged,
  packIdForArgs,
  continentEstimateBytes,
  type Continent,
  type DownloadArgs,
  type InstalledPack,
  type RegionManifest,
  type Tier,
} from './offlineMaps';
import { useDownloadProgress } from './useDownloadProgress';
import './OfflineMapsSettings.css';

/** Human-readable byte size (MB/GB), for the pack list + preset estimates.
 * Picks the unit AFTER rounding so a boundary value rolls up cleanly — e.g.
 * 999_500 B becomes "1.0 MB", not "1000 KB". */
export function formatBytes(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)} GB`;
  // KB/MB use integer rounding; a value that rounds to 1000 of its unit rolls
  // up to the next unit (1000 KB → 1.0 MB, 1000 MB → 1.0 GB).
  if (n >= 1_000_000) {
    const mb = Math.round(n / 1_000_000);
    return mb >= 1000 ? `${(n / 1_000_000_000).toFixed(1)} GB` : `${mb} MB`;
  }
  if (n >= 1000) {
    const kb = Math.round(n / 1000);
    return kb >= 1000 ? `${(n / 1_000_000).toFixed(1)} MB` : `${kb} KB`;
  }
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
  // A non-error advisory (e.g. a pack installed but needs a restart to serve —
  // Codex #5). Distinct from `error` so it renders in a neutral, not alarming, row.
  const [notice, setNotice] = useState<string | null>(null);
  const [continentId, setContinentId] = useState('');
  // The detail tier applied to a continent download (tuxlink-8g28). Defaults to the
  // first (smallest-detail) tier so a continent download never silently defaults to
  // the full-detail, multi-GB extract that was the bug. Set once the manifest loads.
  const [continentTierId, setContinentTierId] = useState('');
  // The active *download* busy key (tier-*/continent-*, not delete-*) drives the
  // inline progress row. A failed download stays here so the row shows the error
  // + Retry until the operator retries or starts something else. The hook key
  // appends an attempt counter (`#N`) so a retry forces a fresh subscription /
  // full reset — re-dispatching the same key would not re-run the hook's reset
  // effect, leaking stale error/rate/sample state into the retry (C6).
  const [downloadKey, setDownloadKey] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [retry, setRetry] = useState<(() => void) | null>(null);
  // The deterministic backend pack id of the in-flight download, derived from the
  // args we sent. Lets Cancel target the id immediately, before the first progress
  // event latches it in the hook (C5).
  const [activePackId, setActivePackId] = useState<string | null>(null);

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
    // B1: refresh the remote manifest best-effort on mount so the DISPLAYED
    // presets/estimates reflect the operator's weekly planet_url bump. The refresh
    // writes the backend's cached manifest; we then read it back via getManifest()
    // for display regardless of whether the refresh succeeded (offline → the
    // cached/bundled manifest still renders). Download-time freshness no longer
    // depends on this: the backend `basemap_download_pack` refreshes the manifest
    // itself before building the request (Codex #1), so a quick click can't outrun
    // a stale planet_url.
    void refreshManifest()
      .catch(() => {})
      .finally(() => {
        getManifest()
          .then((m) => {
            setManifest(m);
            // Default the continent detail to the smallest tier (first listed) so a
            // continent download starts at a manageable size, not the full-detail
            // extract (tuxlink-8g28). Only seed if unset, so it survives re-mounts.
            setContinentTierId((prev) => prev || m?.tiers[0]?.id || '');
          })
          .catch(() => setManifest(null));
      });
    void refresh();
  }, [refresh]);

  // A download: drives the inline progress row (download-done event clears it).
  // On failure the row shows the error + Retry; the failure also lands in
  // `downloadError` so it persists after `busy` clears.
  //
  // `busyKey` is the stable per-pack key (`tier-*`/`continent-*`) the buttons
  // compare against for their "Downloading…" label. `attempt` increments on each
  // retry so the *hook* key (`<busyKey>#<attempt>`) changes — re-dispatching the
  // same hook key would skip useDownloadProgress's reset effect, leaking stale
  // error/rate state into the retry (C6).
  async function runDownloadOp(label: string, args: DownloadArgs, busyKey: string, attempt = 0) {
    const hookKey = `${busyKey}#${attempt}`;
    setBusy(busyKey);
    setError(null);
    setNotice(null);
    setDownloadError(null);
    setActivePackId(packIdForArgs(args)); // C5: cancel can target this immediately
    setDownloadKey(hookKey);
    setRetry(() => () => void runDownloadOp(label, args, busyKey, attempt + 1));
    try {
      const result = await downloadPack(args);
      // Codex #5: only tell the live map to composite the new pack when it is
      // actually servable. If registration failed (requiresRestart), the pack is
      // on disk but `tile://pmtiles/<id>` can't serve it until the next restart —
      // adding the source now would 404 every tile. Show an honest notice instead;
      // `refresh()` still lists it as installed either way.
      if (result.requiresRestart) {
        setNotice(`${label.replace(/^Download /, '')} installed — restart Tuxlink to use it offline.`);
      } else {
        emitPacksChanged();
      }
      await refresh();
      setDownloadKey(null);
      setActivePackId(null);
    } catch (e) {
      // A cancel surfaces as the backend's "download cancelled" error — that is
      // an operator action, not a failure: return the row to idle (no error +
      // Retry). Any other rejection keeps the message for the inline error row.
      if (String(e).includes('download cancelled')) {
        setDownloadKey(null);
        setActivePackId(null);
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
      { kind: 'tier', tier_id: t.id, lon0: centroid.lon, lat0: centroid.lat },
      `tier-${t.id}`,
    );
  }

  function onDownloadContinent(c: Continent, t: Tier) {
    void runDownloadOp(
      `Download ${c.label} (${t.label})`,
      { kind: 'continent', continent_id: c.id, tier_id: t.id },
      `continent-${c.id}`,
    );
  }

  function onDelete(p: InstalledPack) {
    void runDeleteOp(`Delete ${p.label}`, () => deletePack(p.id), `delete-${p.id}`);
  }

  function onCancel() {
    // C5: cancel the deterministic pack id we derived from the download args at
    // dispatch — available immediately, before any progress event. The hook's
    // latched `trackedId` only appears ~500ms+ in (first event), so guarding on
    // it made early Cancel a no-op. Fall back to it only if the derived id is
    // somehow absent. Since only one download runs at a time, this is correct.
    const id = activePackId ?? progress.trackedId;
    if (id) void cancelDownload(id);
  }

  const downloading = busy != null;
  const continent = manifest?.continents.find((c) => c.id === continentId);
  // The detail tier applied to a continent download (tuxlink-8g28). Falls back to
  // the first tier if the saved id no longer resolves (e.g. manifest changed).
  const continentTier =
    manifest?.tiers.find((t) => t.id === continentTierId) ?? manifest?.tiers[0];
  // The inline progress row renders for an active download (or a failed one
  // awaiting Retry) — never for a delete. An active download is in flight when
  // `busy` is a download key (the hook key drops a `#N` attempt suffix, so
  // compare the prefix). `downloadKey` being set with no busy = the failed/Retry
  // state, gated on `downloadError`.
  const busyIsActiveDownload =
    busy != null && downloadKey != null && downloadKey.startsWith(`${busy}#`);
  const showProgressRow =
    downloadKey != null && (busyIsActiveDownload || downloadError != null);

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

      {/* Named continents, at a chosen detail level (tuxlink-8g28). Detail is the
          size lever at continent scale: Local pulls a small low-zoom extract, Wide a
          large high-zoom one. The estimate updates with both the continent and the
          detail tier so the operator sees the real download size before committing. */}
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
                  {c.label}
                </option>
              ))}
            </select>
            <select
              aria-label="Detail level"
              value={continentTier?.id ?? ''}
              disabled={downloading || !continent}
              onChange={(e) => setContinentTierId(e.target.value)}
            >
              {manifest.tiers.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                  {continent ? ` · ~${formatBytes(continentEstimateBytes(continent.typical_bytes, t.maxzoom))}` : ''}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={downloading || !continent || !continentTier}
              onClick={() => continent && continentTier && onDownloadContinent(continent, continentTier)}
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
                {/* C4: once bytes meet/exceed the manifest estimate the true
                    size is unknown, so render an indeterminate bar (no `value`)
                    rather than a stuck 99%. */}
                <progress
                  className="tux-offlinemaps-progress-bar"
                  max={1}
                  value={progress.finishing ? undefined : progress.percent}
                  aria-label="Download progress"
                />
                <span className="tux-offlinemaps-progress-pct">
                  {progress.finishing ? 'Finishing…' : `${Math.round(progress.percent * 100)}%`}
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
                {/* The denominator is the manifest estimate (clamped up to bytes
                    in `finishing`); label it ~ so the readout stays honest. */}
                <span>
                  {formatBytes(progress.bytes)} / ~{formatBytes(progress.total)}
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

      {notice && (
        <p className="tux-offlinemaps-notice" role="status">
          {notice}
        </p>
      )}

      {error && (
        <p className="tux-offlinemaps-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
