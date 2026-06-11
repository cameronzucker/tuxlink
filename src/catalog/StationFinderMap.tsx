// Left-pane station map (design §7). One pin per station at its grid centroid,
// coloured/sized by its reachability tier on the selected band; an operator
// "you" pin; click-to-select. Pins are BaseMap children (its props are frozen,
// C11). When a station has no known tier (engine unavailable / off-band) it
// renders 'untiered' and stays clickable — distance-only still selects.

import { Marker } from 'react-leaflet';
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

export function StationFinderMap(props: StationFinderMapProps) {
  const tileSource = useTileSource();
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  return (
    <div className="station-finder__map" data-testid="station-map">
      <BaseMap initialCenter={me ?? undefined} initialZoom={2} tileSource={tileSource ?? undefined}>
        {me && (
          <Marker position={[me.lat, me.lon]}>
            <span data-testid="me-pin" className="station-finder__me" />
          </Marker>
        )}
        {props.stations.map((s) => {
          const ll = gridToLatLon(s.grid);
          if (!ll) return null;
          const key = stationKey(s);
          const tier = props.tiers.get(key);
          const cls = tier
            ? `station-finder__pin station-finder__pin--${tier}`
            : 'station-finder__pin station-finder__pin--untiered';
          return (
            <Marker key={key} position={[ll.lat, ll.lon]}>
              <button
                type="button"
                data-testid="station-pin"
                className={`${cls}${props.selectedKey === key ? ' is-selected' : ''}`}
                onClick={() => props.onSelect(s)}
                title={`${s.baseCallsign} · ${s.grid}`}
              >
                <span className="station-finder__pin-dot" />
                <span className="station-finder__pin-tag">{s.baseCallsign}</span>
              </button>
            </Marker>
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
