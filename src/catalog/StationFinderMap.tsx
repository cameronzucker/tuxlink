// Left-pane station map (design §7). One pin per station at its grid centroid,
// coloured/sized by its reachability tier on the selected band; an operator
// "you" pin; click-to-select.
//
// Leaflet re-expression (tuxlink-mncq; strangler-fig twin of the MapLibre
// edition): each station is an `L.circleMarker` rendered on an explicit `L.svg()`
// renderer — the SVG path is robust under the Pi's software-GL WebKitGTK (the
// map's preferCanvas vector path has no 2D context there) and is unit-inspectable
// in jsdom, mirroring AprsPositionsMap's uncertainty discs. Radius + colour are
// data-driven per tier; selection bumps the radius + adds a bright-white rim and
// a soft glow disc BENEATH the pin (the Leaflet analog of the MapLibre
// feature-state emphasis). Click-select is a per-marker `marker.on('click', …)`.
// Markers are created once per station and updated in place across re-renders
// (no churn). Render fidelity is grim-verified; the unit test proves the marker
// wiring (count, tier style, selection emphasis, click→onSelect, operator pin).
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import L from 'leaflet';
import { LeafletMap } from '../map/LeafletMap';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import { usePersistedViewport } from '../map/usePersistedViewport';
import { LeafletRecenterControl } from '../map/LeafletRecenterControl';
import { PeerLayer, type PeerVisual } from '../map/PeerLayer';
import { gridToLatLon } from '../forms/position/maidenhead';
import { reportFrontendError } from '../frontendErrorLog';
import { type ReachTier } from './reachability';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';
import { usePeers, useP2pCapabilities } from '../peers/usePeers';
import { aggregatePeers, type AggregatedPeer } from '../peers/peerModel';

export interface StationFinderMapProps {
  stations: Station[];
  operatorGrid: string;
  tiers: Map<string, ReachTier>;
  selectedKey: string | null;
  onSelect: (station: Station) => void;
}

// Recenter zoom on the operator, on the z0–14 scale (matches the MapLibre edition).
const OPERATOR_ZOOM = 6;

// Per-tier pin colour — mirrors the --reach-* CSS vars and the MapLibre
// TIER_COLOR_MATCH ramp exactly (Leaflet path paint can't read CSS custom
// properties either). Six-step green→red→grey ramp; see reachability.ts.
const TIER_COLOR: Record<string, string> = {
  good: '#41ba6c',
  fair: '#8cc23f',
  marginal: '#d9b13a',
  poor: '#e2862f',
  unlikely: '#d64a40', // red — almost certainly not
  skip: '#6c5a5a', // grey — not reachable, inside radius
};
const UNTIERED_COLOR = '#9fb6cc'; // no usable channel / no prediction

// Base + selected pin radii per tier — mirror the MapLibre circle-radius match
// expressions (selected gets a MODEST bump, not a balloon — operator 2026-06-16).
const BASE_RADIUS: Record<string, number> = {
  good: 10,
  fair: 8,
  marginal: 6.5,
  poor: 5.5,
  unlikely: 5,
  skip: 4.5,
};
const SELECTED_RADIUS: Record<string, number> = {
  good: 12,
  fair: 10,
  marginal: 8.5,
  poor: 7.5,
  unlikely: 7,
  skip: 6.5,
};
const UNTIERED_BASE_RADIUS = 7;
const UNTIERED_SELECTED_RADIUS = 9;

const colorFor = (tier: string): string => TIER_COLOR[tier] ?? UNTIERED_COLOR;
const baseRadiusFor = (tier: string): number => BASE_RADIUS[tier] ?? UNTIERED_BASE_RADIUS;
const selectedRadiusFor = (tier: string): number => SELECTED_RADIUS[tier] ?? UNTIERED_SELECTED_RADIUS;

/** The circleMarker style for a station at a given tier + selection state. The
 * grey "not reachable" bottom tier sits back (dimmer) so the live red/orange/green
 * stations read first; selected pins get a bright-white rim, others a thin white
 * rim for basemap contrast. */
function pinStyle(tier: string, selected: boolean): L.CircleMarkerOptions {
  return {
    radius: selected ? selectedRadiusFor(tier) : baseRadiusFor(tier),
    fillColor: colorFor(tier),
    fillOpacity: tier === 'skip' ? 0.7 : 1,
    color: '#ffffff',
    weight: selected ? 2 : 0.6,
  };
}

/** One station's owned circleMarker + the tier it was last styled for. */
interface Pin {
  marker: L.CircleMarker;
  tier: string;
}

function StationLayers({ stations, tiers, selectedKey, onSelect }: Omit<StationFinderMapProps, 'operatorGrid'>) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);

  const byKey = useMemo(() => {
    const m = new Map<string, Station>();
    for (const s of stations) m.set(stationKey(s), s);
    return m;
  }, [stations]);
  const byKeyRef = useRef(byKey);
  byKeyRef.current = byKey;
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  // Hold selection in a ref so the marker-reconcile effect does NOT depend on it:
  // a selection click must not re-create every marker (mirrors AprsPositionsMap's
  // ref discipline). Reconcile runs ONLY on station/tier changes; a dedicated
  // effect re-styles on selection.
  const selectedKeyRef = useRef<string | null>(selectedKey);
  selectedKeyRef.current = selectedKey;

  const pinsRef = useRef<Map<string, Pin>>(new Map());
  // Soft selection GLOW — one white disc moved beneath the selected pin (the
  // Leaflet analog of the MapLibre feature-state glow layer). Radius 0 / hidden
  // when nothing is selected.
  const glowRef = useRef<L.CircleMarker | null>(null);
  // SVG renderer (NOT the map's jsdom-absent canvas) — few markers, DOM-rendered,
  // robust under software-GL WebKitGTK and inspectable in jsdom.
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  // Guard every Leaflet layer mutation: a transient throw (add/remove landing mid
  // zoom/pan animation) is logged + skipped, never crashed to the app
  // ErrorBoundary. Mirrors AprsPositionsMap's `safe`.
  const safe = (what: string, fn: () => void): void => {
    try {
      fn();
    } catch (e) {
      reportFrontendError(
        'station-finder-map',
        `${what}: ${e instanceof Error ? e.message : String(e)}`,
        e instanceof Error ? e.stack : undefined,
      );
    }
  };

  /** Reposition + show/hide the glow disc under the currently-selected pin. */
  const syncGlow = (): void => {
    const glow = glowRef.current;
    if (!glow) return;
    const cur = selectedKeyRef.current;
    const pin = cur != null ? pinsRef.current.get(cur) : undefined;
    if (pin) {
      glow.setLatLng(pin.marker.getLatLng());
      glow.setStyle({ radius: 12, fillOpacity: 0.25 });
    } else {
      glow.setStyle({ radius: 0, fillOpacity: 0 });
    }
  };

  // Diff-based reconciliation: create new stations, update existing markers IN
  // PLACE (stable identity — no churn), remove dropped ones. Runs on station/tier
  // change only (NOT selection — that is handled below via refs).
  useEffect(() => {
    if (!map || !group) return;
    const pins = pinsRef.current;

    // Lazily create the glow disc once (drawn first so pins sit above it).
    if (!glowRef.current) {
      glowRef.current = L.circleMarker([0, 0], {
        renderer: rendererRef.current ?? undefined,
        radius: 0,
        fillColor: '#ffffff',
        fillOpacity: 0,
        stroke: false,
        interactive: false,
      });
      safe('add glow', () => group.addLayer(glowRef.current!));
    }

    const live = new Set<string>();
    for (const s of stations) {
      const ll = gridToLatLon(s.grid);
      if (!ll) continue; // drop gridless stations
      const key = stationKey(s);
      live.add(key);
      const tier = tiers.get(key) ?? 'untiered';
      const selected = selectedKeyRef.current === key;
      safe(`reconcile ${key}`, () => {
        let pin = pins.get(key);
        if (!pin) {
          const marker = L.circleMarker([ll.lat, ll.lon], {
            renderer: rendererRef.current ?? undefined,
            ...pinStyle(tier, selected),
          });
          marker.on('click', () => {
            const station = byKeyRef.current.get(key);
            if (station) onSelectRef.current(station);
          });
          pin = { marker, tier };
          pins.set(key, pin);
          group.addLayer(marker);
        } else {
          // A re-fetch can move a station (grid edit) or change its tier.
          pin.marker.setLatLng([ll.lat, ll.lon]);
          pin.marker.setStyle(pinStyle(tier, selected));
          pin.tier = tier;
        }
      });
    }

    // Remove dropped stations entirely.
    for (const [key, pin] of pins) {
      if (!live.has(key)) {
        safe(`drop ${key}`, () => group.removeLayer(pin.marker));
        pins.delete(key);
      }
    }

    syncGlow();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- selection handled via refs + the dedicated effect below; depending on it would churn markers
  }, [map, group, stations, tiers]);

  // Selection re-style: clear the previously-selected pin's emphasis, apply it to
  // the new one, and move the glow — WITHOUT recreating any marker (the Leaflet
  // analog of setFeatureState: one click flips two markers' styles, not the set).
  const prevSelectedRef = useRef<string | null>(null);
  useEffect(() => {
    if (!map) return;
    const pins = pinsRef.current;
    const prev = prevSelectedRef.current;
    if (prev != null && prev !== selectedKey) {
      const p = pins.get(prev);
      if (p) safe(`deselect ${prev}`, () => p.marker.setStyle(pinStyle(p.tier, false)));
    }
    if (selectedKey != null) {
      const p = pins.get(selectedKey);
      if (p) safe(`select ${selectedKey}`, () => p.marker.setStyle(pinStyle(p.tier, true)));
    }
    prevSelectedRef.current = selectedKey;
    syncGlow();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- pins/safe/syncGlow are stable refs; re-style only when the selection changes
  }, [map, selectedKey]);

  return null;
}

/// The operator's own position pin ("you") — a blue-ringed dot drawn distinctly so
/// it never reads as a heard/reachable station.
function OperatorPin({ location }: { location: { lat: number; lon: number } | null }) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });
  useEffect(() => {
    if (!map || !group || !location) return;
    const marker = L.circleMarker([location.lat, location.lon], {
      renderer: rendererRef.current ?? undefined,
      radius: 7,
      fillColor: '#eaf3fb',
      fillOpacity: 1,
      color: '#2f86f0',
      weight: 3,
      interactive: false,
    });
    group.addLayer(marker);
    return () => {
      if (group.hasLayer(marker)) group.removeLayer(marker);
    };
  }, [map, group, location?.lat, location?.lon]);
  return null;
}

export function StationFinderMap(props: StationFinderMapProps) {
  const me = props.operatorGrid ? gridToLatLon(props.operatorGrid) : null;
  // tuxlink-dwzu: restore the operator's last viewport. A saved view wins over the
  // operator-centred default AND suppresses the async operator flyTo — a stable
  // saved center passed at mount makes LeafletMap skip its arrival recenter
  // (skipConstructCenter), so the map opens exactly where it was left. First run
  // (no saved view) keeps the operator-centred behaviour.
  const { saved, onViewportChange } = usePersistedViewport('tuxlink:map-viewport:station-finder');
  const initialCenter = saved ? saved.center : (me ?? undefined);
  const initialZoom = saved ? saved.zoom : me ? OPERATOR_ZOOM : 2;

  // Task 24 (spec §6): the peer circle layer, gated end-to-end on
  // `map_peers` [R5-8] — false (or still loading) HIDES every peer pin, not
  // merely dims it. Reads its own peers/capabilities (like AprsPositionsMap
  // reads its own Winlink layer state) rather than threading them through
  // StationFinderPanel's props.
  const p2pCapabilities = useP2pCapabilities();
  const mapPeersEnabled = p2pCapabilities.capabilities?.map_peers === true;
  const peersData = usePeers();
  const aggregatedPeers = useMemo(() => aggregatePeers(peersData.peers), [peersData.peers]);
  const [selectedPeer, setSelectedPeer] = useState<AggregatedPeer | null>(null);

  // Finder color axis is PROPAGATION REACHABILITY (spec §6), never a peer's
  // attempt history — peers take the SAME six-step ramp stations use. A peer
  // shares a predicted tier only when its base+grid coincides with a predicted
  // station key (the finder runs voacap over `stations`, not peers); otherwise
  // the peer has no prediction and renders dashed-outline untiered grey, exactly
  // as the spec prescribes for a peer without prediction. This is the reuse the
  // reviewer required — no invented peer-color scheme.
  const peerVisualFor = useCallback(
    (peer: AggregatedPeer): PeerVisual => {
      const tier = peer.grid
        ? props.tiers.get(`${peer.canonicalBase.toUpperCase()}|${peer.grid}`)
        : undefined;
      if (!tier) return { tierClass: 'peer-pin--untiered', dashed: true };
      return { tierClass: `peer-pin--${tier}`, dashed: false };
    },
    [props.tiers],
  );

  return (
    <div className="station-finder__map" data-testid="station-map">
      <LeafletMap initialCenter={initialCenter} initialZoom={initialZoom} onViewportChange={onViewportChange}>
        <StationLayers
          stations={props.stations}
          tiers={props.tiers}
          selectedKey={props.selectedKey}
          onSelect={props.onSelect}
        />
        <OperatorPin location={me} />
        <PeerLayer
          peers={aggregatedPeers}
          enabled={mapPeersEnabled}
          visualFor={peerVisualFor}
          onSelect={setSelectedPeer}
          selectedId={selectedPeer?.id ?? null}
        />
        <LeafletRecenterControl target={me} zoom={OPERATOR_ZOOM} />
      </LeafletMap>
      <div className="station-finder__reachkey" aria-hidden>
        <span className="k good" /> good
        <span className="k fair" /> fair
        <span className="k marginal" /> marginal
        <span className="k poor" /> maybe not
        <span className="k unlikely" /> unlikely
        <span className="k skip" /> not reachable
      </div>
    </div>
  );
}
