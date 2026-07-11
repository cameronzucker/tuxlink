// Peer layer (Task 24, spec §6) — a circle-shaped pin per map-placeable P2P
// peer (Task 22's `aggregatePeers`), mounted on BOTH maps: the finder
// (`StationFinderMap.tsx`) and the tac-chat map (`AprsPositionsMap.tsx`).
// Gated end-to-end on `useP2pCapabilities().map_peers` [R5-8] via the
// `enabled` prop — false renders nothing (the whole layer, not a dimmed one).
//
// Shape encodes entity (spec §6): circle = peer, distinct from the Winlink
// diamond (`WinlinkGatewayLayer.tsx`) and the APRS sprite. Mirrors
// `WinlinkGatewayLayer.tsx`'s idiom exactly — raw `L.divIcon` +
// `marker.on('click', …)` reading a ref, NOT a react-leaflet `<Marker>`
// (children on that component silently no-op here — see project memory
// `feedback_react_leaflet_marker_children_false_green`). A full
// clear+rebuild each reconcile (not diff-based) mirrors WinlinkGatewayLayer's
// simplicity — peer counts are small, so churn is cheap.
//
// COLOR IS PER-MAP, NOT INVENTED HERE (spec §6): color keeps its established
// meaning on each map, so PeerLayer is scheme-agnostic — the parent supplies a
// `visualFor(peer)` resolver. The finder passes the six-step propagation
// reachability ramp (the same `ReachTier` stations use); the tac-chat map
// passes the WinlinkGatewayLayer outcome-tier vocabulary (reached / failed /
// stale / live). PeerLayer only owns the SHAPE (circle) + the escape boundary.
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from './LeafletMapContext';
import { useLeafletLayerGroup } from './leafletHooks';
import { gridToLatLon } from '../forms/position/maidenhead';
import { baseCallsign } from '../catalog/stationModel';
import type { AggregatedPeer } from '../peers/peerModel';
import './PeerLayer.css';

/// Escapes any peer-supplied string before it reaches the divIcon HTML. The
/// callsign is wire-influenced (observed via traffic, aggregated, or
/// operator-entered) — the backend curation floor is a separate defense; the
/// divIcon HTML is its own XSS surface and gets escaped here too, mirroring
/// `AprsPositionsMap.tsx`'s `esc()` at the same render boundary.
const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/// The per-map visual for one peer pin: the color-tier CSS modifier class
/// (appended to `.peer-pin`) and whether the pin renders dashed. The parent
/// map computes this from its own established color axis (spec §6) — PeerLayer
/// never derives color itself.
export interface PeerVisual {
  /// A `.peer-pin--*` modifier class. Finder: a reachability-ramp class
  /// (`peer-pin--good`…`--skip`/`--untiered`). Tac-chat: an outcome class
  /// (`peer-pin--reached`/`--failed`/`--stale`/`--live`/`--unknown`).
  tierClass: string;
  /// Dashed outline. Finder: a peer without a prediction. Tac-chat: a
  /// never-connected MANUAL peer (spec §6).
  dashed: boolean;
}

/// Build the peer pin `divIcon`: a circle, styled by the parent-supplied
/// `visual`, with an escaped callsign label beneath it (mirrors the APRS pin's
/// label idiom).
function peerIcon(p: AggregatedPeer, selected: boolean, visual: PeerVisual): L.DivIcon {
  const call = p.callsign;
  const cls = [
    'peer-pin',
    visual.tierClass,
    visual.dashed ? 'peer-pin--dashed' : '',
    selected ? 'peer-pin--selected' : '',
  ]
    .filter(Boolean)
    .join(' ');
  const html =
    `<div class="${cls}" data-call="${esc(call)}"></div>` +
    `<span class="peer-pin-label">${esc(call)}</span>`;
  return L.divIcon({ className: 'peer-pin-icon', html, iconSize: [14, 14], iconAnchor: [7, 7] });
}

export interface PeerLayerProps {
  /// The aggregated peer roster (Task 22's `aggregatePeers`). Only peers with a
  /// resolvable grid get a pin — rail-only (gridless/telnet-only) peers are
  /// surfaced elsewhere, never here (`mapPlaceable` is `Boolean(grid)`, so the
  /// grid check alone suffices).
  peers: AggregatedPeer[];
  /// Gated on `useP2pCapabilities().map_peers` [R5-8] — false renders NO peer
  /// markers at all (capability-hide, not a dimmed/disabled state).
  enabled: boolean;
  /// Per-map color/dashed resolver (spec §6) — see `PeerVisual`.
  visualFor: (peer: AggregatedPeer) => PeerVisual;
  onSelect: (peer: AggregatedPeer) => void;
  /// The currently-selected peer id, if any — re-styles that pin in place
  /// (`peer-pin--selected`) without changing selection semantics elsewhere.
  selectedId?: string | null;
  /// Callsigns (base, uppercased) currently live on APRS. A peer matching one
  /// is skipped here — the APRS sprite already represents that station on the
  /// map, and live RF truth wins over the stored peer record (spec §6).
  /// Tac-chat only; the finder map has no live APRS feed.
  liveAprsCallsigns?: Set<string>;
}

export function PeerLayer({
  peers,
  enabled,
  visualFor,
  onSelect,
  selectedId,
  liveAprsCallsigns,
}: PeerLayerProps): null {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    if (!group) return;
    group.clearLayers();
    if (enabled) {
      for (const p of peers) {
        if (!p.grid) continue; // rail-only (gridless/telnet-only) — no pin
        if (liveAprsCallsigns?.has(baseCallsign(p.callsign))) continue; // APRS sprite wins
        const ll = gridToLatLon(p.grid);
        if (!ll) continue;
        const m = L.marker([ll.lat, ll.lon], { icon: peerIcon(p, p.id === selectedId, visualFor(p)) });
        m.on('click', () => onSelectRef.current(p));
        group.addLayer(m);
      }
    }
    return () => {
      group.clearLayers();
    };
  }, [group, enabled, peers, selectedId, liveAprsCallsigns, visualFor]);

  return null;
}
