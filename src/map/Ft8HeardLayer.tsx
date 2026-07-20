// FT-8 heard-station layer on the finder map (Task 4, spec L3 traffic map): one
// small SNR-coloured circle marker per station heard over FT-8 in the live
// aggregation window, at that station's grid centroid. Mirrors `StationLayers`'
// (`StationFinderMap.tsx`) structural template but reconciles the way
// `PeerLayer.tsx` does: a full clear + rebuild each time `rows` changes, which
// is cheap at the ring's <= 240-entry cap (already aggregated per call by the
// caller via `aggregateLiveDecodes`, so this component never touches the raw
// ring itself).
//
// Colour is an "openness" ramp keyed on best-heard SNR, not a reachability
// tier: this is EVIDENCE (a station was actually heard), a different axis
// from the propagation-prediction tiers `StationLayers` draws. The three
// ramp colours reuse the CSS custom-property VALUES already defined for the
// band-openness dots in `StationFinderPanel.css` (`--open-hot` / `--open-warm`
// / `--open-quiet`); Leaflet path paint needs literal strings (same
// constraint `TIER_COLOR` in `StationFinderMap.tsx` documents), so the
// literals are duplicated here and `rampFor` is the single place that maps
// an SNR value to one.
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from './LeafletMapContext';
import { useLeafletLayerGroup } from './leafletHooks';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { LiveDecodeRow } from '../catalog/LiveDecodesTab';

/** SNR (dB) at or above which a heard station gets the HOT ramp colour. */
export const SNR_HOT_DB = -10;
/** SNR (dB) at or above which a heard station gets the WARM ramp colour;
 *  below this it gets QUIET. Between `SNR_WARM_DB` and `SNR_HOT_DB` is WARM. */
export const SNR_WARM_DB = -17;

const RAMP_HOT = '#ff5470';
const RAMP_WARM = '#ffcf5c';
const RAMP_QUIET = '#5c92b3';

/** Map a station's best-heard SNR to its marker fill colour on the openness
 *  ramp (`SNR_HOT_DB`/`SNR_WARM_DB` thresholds). Exported so the value is
 *  independently testable without inspecting a live Leaflet marker. */
export function rampFor(snrDb: number): string {
  if (snrDb >= SNR_HOT_DB) return RAMP_HOT;
  if (snrDb >= SNR_WARM_DB) return RAMP_WARM;
  return RAMP_QUIET;
}

export interface Ft8HeardLayerProps {
  /** Pre-aggregated by the caller (`aggregateLiveDecodes`); this component
   *  never reads the raw decode ring. A row with no grid yet plots nothing
   *  (mirrors the Live-decodes tab's own gridless-row handling). */
  rows: LiveDecodeRow[];
  /** Layer-control visibility (mirrors `StationLayers`' `visible`/Gateways
   *  toggle): false renders no markers at all, not a dimmed layer. */
  enabled: boolean;
  /** Optional shared SVG renderer. Falls back to an internally-owned one
   *  (mirrors `StationLayers`'/`OperatorPin`'s own `L.svg()` instance; the
   *  panel does not yet expose a single renderer shared across its layers). */
  renderer?: L.Renderer;
}

export function Ft8HeardLayer({ rows, enabled, renderer }: Ft8HeardLayerProps): null {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const fallbackRendererRef = useRef<L.Renderer | null>(null);
  if (!fallbackRendererRef.current) fallbackRendererRef.current = L.svg({ padding: 2 });

  useEffect(() => {
    if (!group) return;
    group.clearLayers();
    if (enabled) {
      for (const row of rows) {
        if (!row.grid) continue; // no grid heard yet, nothing to plot
        const ll = gridToLatLon(row.grid);
        if (!ll) continue; // malformed/garbage grid, never throws, never plots
        const marker = L.circleMarker([ll.lat, ll.lon], {
          renderer: renderer ?? fallbackRendererRef.current ?? undefined,
          radius: 4,
          stroke: false,
          fillOpacity: 0.9,
          fillColor: rampFor(row.bestSnrDb),
        });
        marker.bindTooltip(`${row.call} · ${row.bestSnrDb} dB`);
        group.addLayer(marker);
      }
    }
    return () => {
      group.clearLayers();
    };
  }, [group, rows, enabled, renderer]);

  return null;
}
