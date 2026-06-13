/**
 * MapTileSourceSettings — inline Settings section for configuring a LAN tile
 * source (tuxlink-dyop plan, Phase 8.2; design §8.7). Renders inline as a
 * panel section, NOT a pop-up window (operator pet-peeve: no window clutter).
 *
 * Fields (§8.7): tile URL template, XYZ/TMS scheme (default XYZ), min/max
 * zoom, cache budget (MB), optional attribution, source label. The source
 * serves standard Web Mercator (EPSG:3857) XYZ tiles; no CRS negotiation is
 * needed on the wire (tuxlink-7h2m, Task 3).
 *
 * Actions consume the Phase-7 wrappers in ../map/tileSource:
 *   - Test source     → testTileSource(source)      (probe, no persist)
 *   - Use this source → configureTileSource(source)  (persist + activate)
 *   - Clear tile cache → clearTileCache()
 *
 * Host policy (design §8.3): the config UX stays TRUSTING. A host that looks
 * public earns a non-blocking warning, never a hard block — the operator's
 * chosen LAN host is authoritative, and the TCP-layer access boundary, not
 * this form, is the security control.
 */
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  configureTileSource,
  testTileSource,
  clearTileCache,
  type TileScheme,
  type TileSource,
  type TileSourceStatus,
} from '../map/tileSource';
import { emitTileSourceChanged } from '../map/tileSourceEvent';

/** Render a probe (dry-run "Test") status as plain, present-indicative copy. */
function statusMessage(status: TileSourceStatus): string {
  switch (status.kind) {
    case 'lan-live':
    case 'lan-cached':
    case 'partial':
      return 'source validated';
    case 'incompatible':
      return 'incompatible tile source — the server responded but did not return standard image tiles';
    case 'unreachable':
      return 'tiles unreachable';
    case 'bundled':
      return 'no source configured';
    default:
      return 'source status unknown';
  }
}

/**
 * Render a bind ("Use this source") status. The success copy is DISTINCT from
 * `statusMessage`'s "source validated": "Test" only probes, but "Use this
 * source" activates + persists the source, so the operator must be able to tell
 * a dry-run probe from an actual bind — they returned identical text before, so
 * a source could be silently bound yet read as if nothing happened. On success
 * it confirms the source is active and states the new zoom ceiling so the bind's
 * effect on the map is legible. Failures reuse the shared explanation.
 */
function bindMessage(status: TileSourceStatus): string {
  switch (status.kind) {
    case 'lan-live':
    case 'lan-cached':
    case 'partial':
      return `source active — map zoom reaches level ${status.zoom}`;
    default:
      return statusMessage(status);
  }
}

/**
 * True when the host portion of the URL is NOT a private/LAN address. A
 * public-looking host earns a non-blocking warning (design §8.3). Conservative
 * by design: an unparseable URL is treated as not-public (no false warning on
 * a half-typed template). RFC1918 / loopback / link-local / `.local` mDNS /
 * bare hostnames are all treated as LAN.
 */
function looksPublic(url: string): boolean {
  let host: string;
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return false;
  }
  if (host === 'localhost' || host.endsWith('.local')) return false;
  if (host === '127.0.0.1' || host.startsWith('127.')) return false;
  // RFC1918 + link-local (IPv4).
  if (host.startsWith('10.')) return false;
  if (host.startsWith('192.168.')) return false;
  if (/^172\.(1[6-9]|2\d|3[01])\./.test(host)) return false;
  if (host.startsWith('169.254.')) return false;
  // IPv6 loopback / unique-local / link-local.
  if (host === '::1' || host.startsWith('fc') || host.startsWith('fd') || host.startsWith('fe80')) {
    return false;
  }
  // A bare single-label hostname (no dots) is a LAN name, not a public FQDN.
  if (!host.includes('.')) return false;
  return true;
}

export function MapTileSourceSettings() {
  const [url, setUrl] = useState('');
  const [scheme, setScheme] = useState<TileScheme>('Xyz');
  const [minZoom, setMinZoom] = useState('0');
  const [maxZoom, setMaxZoom] = useState('16');
  const [cacheBudgetMb, setCacheBudgetMb] = useState('256');
  const [attribution, setAttribution] = useState('');
  const [label, setLabel] = useState('');
  const [feedback, setFeedback] = useState<string | null>(null);

  // tuxlink-9rek: hydrate the form from the persisted source on mount so
  // reopening Settings shows the saved source instead of blank defaults. Reads
  // the same `config_read` → `map_tile_source` the map's useTileSource reads.
  useEffect(() => {
    let mounted = true;
    // `await` (not `.then`) so a mock/non-promise invoke return is harmless:
    // `await undefined` resolves to undefined and we keep defaults, rather than
    // calling `.then` on undefined (which crashes when a parent test mounts this
    // panel without stubbing the Tauri bridge — the full-suite mount path).
    void (async () => {
      try {
        const config = await invoke<{ map_tile_source?: TileSource | null }>('config_read');
        const saved = config?.map_tile_source ?? null;
        if (!mounted || !saved) return;
        setUrl(saved.url);
        setScheme(saved.scheme);
        setMinZoom(String(saved.minZoom));
        setMaxZoom(String(saved.maxZoom));
        setCacheBudgetMb(String(saved.cacheBudgetMb));
        setAttribution(saved.attribution ?? '');
        setLabel(saved.label);
      } catch {
        /* no persisted source / IPC error → keep blank defaults */
      }
    })();
    return () => {
      mounted = false;
    };
  }, []);

  const publicWarning = looksPublic(url);

  function buildSource(): TileSource {
    return {
      url,
      scheme,
      minZoom: parseInt(minZoom, 10) || 0,
      maxZoom: parseInt(maxZoom, 10) || 0,
      cacheBudgetMb: parseInt(cacheBudgetMb, 10) || 0,
      attribution: attribution.trim() === '' ? null : attribution.trim(),
      label,
    };
  }

  async function handleTest() {
    setFeedback(null);
    try {
      const status = await testTileSource(buildSource());
      setFeedback(statusMessage(status));
    } catch (e) {
      setFeedback(`Test failed: ${e}`);
    }
  }

  async function handleUse() {
    setFeedback(null);
    try {
      const status = await configureTileSource(buildSource());
      setFeedback(bindMessage(status));
      // tuxlink-9rek: tell any mounted map to re-read the (possibly newly
      // activated) source so it applies without an app restart.
      emitTileSourceChanged();
    } catch (e) {
      setFeedback(`Save failed: ${e}`);
    }
  }

  async function handleClearCache() {
    setFeedback(null);
    try {
      await clearTileCache();
      setFeedback('tile cache cleared');
    } catch (e) {
      setFeedback(`Clear failed: ${e}`);
    }
  }

  return (
    <section className="tux-map-tile-source" data-testid="map-tile-source-settings">
      <h2>LAN map tiles</h2>
      <p className="tux-mts-intro">
        Configure a tile source on the local network to raise the map zoom
        ceiling beyond the bundled raster. The map functions fully offline
        without a source; a source is a pure enhancement.
      </p>

      <label className="tux-mts-field">
        <span>Tile URL template</span>
        <input
          type="text"
          value={url}
          placeholder="http://192.168.1.10:8080/{z}/{x}/{y}.png"
          onChange={(e) => setUrl(e.target.value)}
        />
        <span className="tux-mts-help">
          The source serves standard Web Mercator (EPSG:3857) XYZ tiles — e.g.
          a self-hosted TileServer GL, OpenStreetMap-format LAN server, or any
          standard {'{z}/{x}/{y}'} source. Only private/LAN hosts are accepted.
        </span>
      </label>

      {publicWarning && (
        <p className="tux-mts-warning" role="alert">
          The host looks like a public Internet address rather than a local
          network host. LAN tile sources stay on the local network; verify the
          host before activating the source.
        </p>
      )}

      <fieldset className="tux-mts-scheme">
        <legend>Tile scheme</legend>
        <label>
          <input
            type="radio"
            name="tile-scheme"
            value="Xyz"
            checked={scheme === 'Xyz'}
            onChange={() => setScheme('Xyz')}
          />
          XYZ (slippy)
        </label>
        <label>
          <input
            type="radio"
            name="tile-scheme"
            value="Tms"
            checked={scheme === 'Tms'}
            onChange={() => setScheme('Tms')}
          />
          TMS (y flipped)
        </label>
        <span className="tux-mts-help">
          XYZ is the default. An <code>.mbtiles</code>-backed source is usually
          TMS; the scheme cannot be auto-detected.
        </span>
      </fieldset>

      <label className="tux-mts-field">
        <span>Minimum zoom</span>
        <input
          type="number"
          value={minZoom}
          onChange={(e) => setMinZoom(e.target.value)}
        />
      </label>

      <label className="tux-mts-field">
        <span>Maximum zoom</span>
        <input
          type="number"
          value={maxZoom}
          onChange={(e) => setMaxZoom(e.target.value)}
        />
      </label>

      <label className="tux-mts-field">
        <span>Cache budget (MB)</span>
        <input
          type="number"
          value={cacheBudgetMb}
          onChange={(e) => setCacheBudgetMb(e.target.value)}
        />
      </label>

      <label className="tux-mts-field">
        <span>Attribution (optional)</span>
        <input
          type="text"
          value={attribution}
          onChange={(e) => setAttribution(e.target.value)}
        />
        <span className="tux-mts-help">
          LAN tiles may be OSM-derived and attribution-bound even when
          self-hosted.
        </span>
      </label>

      <label className="tux-mts-field">
        <span>Source label</span>
        <input
          type="text"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>

      <div className="tux-mts-actions">
        <button type="button" onClick={handleTest}>
          Test source
        </button>
        <button type="button" onClick={handleUse}>
          Use this source
        </button>
        <button type="button" onClick={handleClearCache}>
          Clear tile cache
        </button>
      </div>

      {feedback && (
        <p className="tux-mts-feedback" role="status">
          {feedback}
        </p>
      )}
    </section>
  );
}
