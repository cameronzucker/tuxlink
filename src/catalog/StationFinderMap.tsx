// Left-pane station map (design §7). One pin per station at its grid centroid,
// coloured/sized by its reachability tier on the selected band; an operator
// "you" pin; click-to-select.
//
// Pins are real Leaflet markers built with L.divIcon (the MaidenheadOverlay.tsx
// pattern) + click wired via Marker `eventHandlers`. The earlier implementation
// rendered custom <button> elements as <Marker> CHILDREN — which real
// react-leaflet ignores (children are only for Popup/Tooltip), so the live map
// showed default blue markers with no colour and no click (tuxlink-ku2b). This
// layer is validated by browser smoke, not unit tests: the test map mock renders
// Marker children as a div and cannot represent divIcon HTML or eventHandlers.

import { useEffect } from 'react';
import { Marker, useMap } from 'react-leaflet';
import L from 'leaflet';
import { BaseMap } from '../map/BaseMap';
import { useTileSource } from '../map/useTileSource';
import { gridToLatLon } from '../forms/position/maidenhead';
import { type ReachTier } from './reachability';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';

export interface StationFinderMapProps {
  stations: Station[];
  operatorGrid: string;
  tiers: Map<string, ReachTier>;
  selectedKey: string | null;
  onSelect: (station: Station) => void;
}

// Pin diameter (px) per reachability tier — good biggest, skip smallest,
// untiered (no prediction / off-band) a neutral medium dot.
const PIN_SIZE: Record<string, number> = {
  good: 20,
  fair: 16,
  marginal: 13,
  skip: 10,
  untiered: 14,
};

export function stationPinIcon(tier: ReachTier | undefined, selected: boolean, label: string): L.DivIcon {
  const t = tier ?? 'untiered';
  const sz = PIN_SIZE[t];
  // The visible dot is a <span> sized to fill this divIcon's wrapper, whose
  // width/height Leaflet sets from `iconSize` via the CSSOM. The dot's pixel
  // size therefore comes from `iconSize` (below) + the `width:100%/height:100%`
  // CSS rule — NOT an inline style="" attribute, which Tauri's packaged CSP
  // strips in WebKitGTK and which produced the oblong "black blob" pins
  // (tuxlink-s0r1). Classes are global CSS (StationFinderPanel.css) since
  // Leaflet injects this outside the React tree.
  const sel = selected ? ' is-selected' : '';
  const safeLabel = label.replace(/"/g, '');
  return L.divIcon({
    className: 'station-finder__divpin',
    html: `<span class="station-finder__pindot station-finder__pindot--${t}${sel}" title="${safeLabel}"></span>`,
    iconSize: [sz, sz],
    iconAnchor: [sz / 2, sz / 2],
  });
}

const ME_SIZE = 16;
function operatorPinIcon(): L.DivIcon {
  return L.divIcon({
    className: 'station-finder__divpin',
    html: `<span class="station-finder__me" title="Your location"></span>`,
    iconSize: [ME_SIZE, ME_SIZE],
    iconAnchor: [ME_SIZE / 2, ME_SIZE / 2],
  });
}

// Zoom applied when recentering on the operator (clamped by BaseMap's
// raster-native maxZoom). Mirrors the map's `initialZoom` for a placed operator.
const OPERATOR_ZOOM = 3;

/**
 * Imperatively recenter the map on the operator's location.
 *
 * `<MapContainer>`'s `center`/`zoom` are read ONCE at mount and are NOT
 * reactive (react-leaflet contract). The operator grid arrives asynchronously
 * (StationFinderPanel's `config_read`) AFTER the map has mounted, so a static
 * `initialCenter` leaves the view parked at [0,0] (mid-Atlantic) forever. This
 * child lives inside the MapContainer, gets the live map via `useMap()`, and
 * `setView`s whenever the operator latlon changes (null→value on first load, or
 * a later grid edit). It does not fight panning: the effect only fires when the
 * lat/lon/zoom deps change, not on every render.
 */
function RecenterOnOperator({ lat, lon, zoom }: { lat: number; lon: number; zoom: number }) {
  const map = useMap();
  useEffect(() => {
    map.setView([lat, lon], zoom);
  }, [map, lat, lon, zoom]);
  return null;
}

export function StationFinderMap(props: StationFinderMapProps) {
  const tileSource = useTileSource();
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  return (
    <div className="station-finder__map" data-testid="station-map">
      <BaseMap initialCenter={me ?? undefined} initialZoom={me ? OPERATOR_ZOOM : 1} tileSource={tileSource ?? undefined}>
        {me && <RecenterOnOperator lat={me.lat} lon={me.lon} zoom={OPERATOR_ZOOM} />}
        {me && (
          <Marker
            position={[me.lat, me.lon]}
            icon={operatorPinIcon()}
            interactive={false}
            zIndexOffset={1000}
          />
        )}
        {props.stations.map((s) => {
          const ll = gridToLatLon(s.grid);
          if (!ll) return null;
          const key = stationKey(s);
          const tier = props.tiers.get(key);
          return (
            <Marker
              key={key}
              position={[ll.lat, ll.lon]}
              icon={stationPinIcon(tier, props.selectedKey === key, `${s.baseCallsign} · ${s.grid}`)}
              eventHandlers={{ click: () => props.onSelect(s) }}
            />
          );
        })}
      </BaseMap>
      <div className="station-finder__reachkey" aria-hidden>
        <span className="k good" /> good
        <span className="k fair" /> fair
        <span className="k marginal" /> marginal
        <span className="k skip" /> unlikely
      </div>
    </div>
  );
}
