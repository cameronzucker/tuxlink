import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import type { WinlinkPin } from './winlinkPins';
import './WinlinkGatewayLayer.css';

export function WinlinkGatewayLayer({ pins, onSelect }: { pins: WinlinkPin[]; onSelect: (gateway: string) => void }): null {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    if (!group) return;
    group.clearLayers();
    for (const p of pins) {
      const icon = L.divIcon({
        className: 'winlink-pin-icon',
        html: `<div class="winlink-pin ${p.tierClass}"></div>`,
        iconSize: [18, 18], iconAnchor: [9, 9],
      });
      const m = L.marker([p.lat, p.lon], { icon, keyboard: false });
      m.on('click', () => onSelectRef.current(p.gateway));
      group.addLayer(m);
    }
    return () => { group.clearLayers(); };
  }, [group, pins]);

  return null;
}
