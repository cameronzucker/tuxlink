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

import { Marker } from 'react-leaflet';
import L from 'leaflet';
import { BaseMap } from '../map/BaseMap';
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
  // The visible dot is a styled <span> in the icon HTML; classes are global CSS
  // (StationFinderPanel.css) since Leaflet injects this outside the React tree.
  const sel = selected ? ' is-selected' : '';
  const safeLabel = label.replace(/"/g, '');
  return L.divIcon({
    className: 'station-finder__divpin',
    html:
      `<span class="station-finder__pindot station-finder__pindot--${t}${sel}" ` +
      `style="width:${sz}px;height:${sz}px" title="${safeLabel}"></span>`,
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

export function StationFinderMap(props: StationFinderMapProps) {
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  return (
    <div className="station-finder__map" data-testid="station-map">
      <BaseMap initialCenter={me ?? undefined} initialZoom={me ? 3 : 1}>
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
