// src/aprs/DigipeatFadeLayer.tsx
//
// Connection trace-line rework (tuxlink-k0zz, supersedes the reverted cn84 rAF
// draw-in). Renders the honest multi-hop digipeat path on the APRS map and
// FADES it in/out via maplibre's built-in paint-property transitions — NO
// requestAnimationFrame loop, NO per-frame setData, NO re-tessellation, NO
// preserveDrawingBuffer. Each path's geometry is uploaded ONCE; the fade is a
// bounded `line-opacity` transition the renderer drives and that STOPS when
// done — the per-frame CPU cost that broke the map on llvmpipe is gone.
//
// Independent concurrent dwell: a small POOL of path slots (source + solid +
// dashed layer each), one per active trace, with its own opacity transition +
// dwell timers — so a station heard in quick succession gets its own fade and
// never clobbers another's timeline.
//
// Triggers: hover a pin (fade in, hold, fade out on mouse-out) + a newly-heard
// frame (fade in, dwell, fade out). RF-honesty: solid through located hops,
// dashed `pos?` across hops we can't locate (see resolveDigipeatPath).

import { useEffect, useMemo, useRef } from 'react';
import { useMapContext } from '../map/MapContext';
import { useMapOverlay } from '../map/mapHooks';
import { resolveDigipeatPath, type LatLon, type PathSegment } from './digipeatPath';
import type { HeardPosition } from './aprsTypes';

const POOL_SIZE = 6;
const FADE_IN_MS = 400;
const FADE_OUT_MS = 700;
const DWELL_MS = 2500; // live trace holds this long before fading out
const SOLID_COLOR = '#7fe6a3';
const DASHED_COLOR = '#f0c987';
// The heard-station pin layer hover is bound to (defined in AprsPositionsMap).
const PIN_LAYER = 'aprs-position-pins-color';

const EMPTY_FC = { type: 'FeatureCollection', features: [] as unknown[] };

function slotSource(i: number) {
  return `aprs-trace-slot-${i}`;
}
function slotSolid(i: number) {
  return `aprs-trace-slot-${i}-solid`;
}
function slotDashed(i: number) {
  return `aprs-trace-slot-${i}-dashed`;
}

/// One LineString feature per resolved segment, tagged by kind so the slot's two
/// line layers (solid / dashed) each draw their own. Geometry only — uploaded
/// once per trace, never per frame.
function pathSegmentsFC(segments: PathSegment[]) {
  return {
    type: 'FeatureCollection',
    features: segments.map((s) => ({
      type: 'Feature',
      properties: { kind: s.kind },
      geometry: {
        type: 'LineString',
        coordinates: [
          [s.from.lon, s.from.lat],
          [s.to.lon, s.to.lat],
        ],
      },
    })),
  };
}

/// The two line layers for one pool slot. `line-opacity` starts at 0 and is
/// transitioned to 1 / back to 0 via `line-opacity-transition` — the bounded
/// fade. No other animation.
function slotLayers(i: number): Array<Record<string, unknown> & { id: string }> {
  const common = {
    layout: { 'line-cap': 'round', 'line-join': 'round' },
  };
  return [
    {
      id: slotSolid(i),
      type: 'line',
      source: slotSource(i),
      filter: ['==', ['get', 'kind'], 'solid'],
      ...common,
      paint: {
        'line-color': SOLID_COLOR,
        'line-width': 2.5,
        'line-opacity': 0,
        'line-opacity-transition': { duration: FADE_IN_MS },
      },
    },
    {
      id: slotDashed(i),
      type: 'line',
      source: slotSource(i),
      filter: ['==', ['get', 'kind'], 'dashed'],
      ...common,
      paint: {
        'line-color': DASHED_COLOR,
        'line-width': 2,
        'line-dasharray': [1.5, 1.5],
        'line-opacity': 0,
        'line-opacity-transition': { duration: FADE_IN_MS },
      },
    },
  ] as unknown as Array<Record<string, unknown> & { id: string }>;
}

type Mode = 'hover' | 'live';
interface ActiveTrace {
  slot: number;
  mode: Mode;
  key: string;
  startedAt: number;
  timers: ReturnType<typeof setTimeout>[];
}

/// The structural subset of the maplibre map this layer drives imperatively.
interface MapFace {
  getSource(id: string): { setData?: (d: unknown) => void } | undefined;
  // Optional + guarded: the maplibre test double omits it, and a fade timer must
  // never throw there. The real map always provides it.
  setPaintProperty?(layer: string, prop: string, value: unknown): void;
  on(type: string, layer: string, h: (...a: unknown[]) => void): unknown;
  off(type: string, layer: string, h: (...a: unknown[]) => void): unknown;
}

/// One pool slot's source + layers, registered via the owned overlay hook (which
/// handles styledata re-add). One component per slot keeps the hooks rules clean.
function TraceSlot({ index }: { index: number }) {
  const map = useMapContext();
  useMapOverlay(map, slotSource(index), { type: 'geojson', data: EMPTY_FC }, slotLayers(index));
  return null;
}

export interface DigipeatFadeLayerProps {
  positions: HeardPosition[];
  operator: LatLon | null;
}

export function DigipeatFadeLayer({ positions, operator }: DigipeatFadeLayerProps) {
  const map = useMapContext();

  // Current data, read by the long-lived hover/live handlers through refs.
  const byCall = useMemo(() => {
    const m = new Map<string, HeardPosition>();
    const located = new Map<string, LatLon>();
    for (const p of positions) {
      m.set(p.call, p);
      located.set(p.call, { lat: p.lat, lon: p.lon });
    }
    return { m, located };
  }, [positions]);
  const dataRef = useRef(byCall);
  dataRef.current = byCall;
  const operatorRef = useRef(operator);
  operatorRef.current = operator;

  const activeRef = useRef<Map<string, ActiveTrace>>(new Map());
  const freeSlotsRef = useRef<number[]>(Array.from({ length: POOL_SIZE }, (_, i) => i));

  // Resolve a station's honest path segments, or null if nothing to draw.
  const segmentsFor = (call: string): PathSegment[] | null => {
    const p = dataRef.current.m.get(call);
    if (!p || p.isObject) return null; // object pins aren't the transmitter
    const segs = resolveDigipeatPath({
      src: { call: p.call, lat: p.lat, lon: p.lon },
      via: p.via ?? [],
      located: dataRef.current.located,
      operator: operatorRef.current,
    });
    return segs.length ? segs : null;
  };

  const setOpacity = (m: MapFace, slot: number, value: number, durationMs: number) => {
    if (!m.setPaintProperty) return;
    for (const layer of [slotSolid(slot), slotDashed(slot)]) {
      m.setPaintProperty(layer, 'line-opacity-transition', { duration: durationMs });
      m.setPaintProperty(layer, 'line-opacity', value);
    }
  };

  const releaseSlot = (m: MapFace, t: ActiveTrace) => {
    for (const timer of t.timers) clearTimeout(timer);
    m.getSource(slotSource(t.slot))?.setData?.(EMPTY_FC);
    activeRef.current.delete(t.key);
    if (!freeSlotsRef.current.includes(t.slot)) freeSlotsRef.current.push(t.slot);
  };

  // Allocate a slot: a free one, else evict the oldest LIVE trace (never evict a
  // hover — the operator is looking at it).
  const allocate = (m: MapFace): number | null => {
    if (freeSlotsRef.current.length) return freeSlotsRef.current.shift()!;
    let oldest: ActiveTrace | null = null;
    for (const t of activeRef.current.values()) {
      if (t.mode === 'live' && (!oldest || t.startedAt < oldest.startedAt)) oldest = t;
    }
    if (!oldest) return null; // pool full of hovers — drop this request
    releaseSlot(m, oldest);
    return freeSlotsRef.current.shift() ?? null;
  };

  const startTrace = (m: MapFace, key: string, call: string, mode: Mode) => {
    if (activeRef.current.has(key)) return; // already showing this trace
    const segments = segmentsFor(call);
    if (!segments) return;
    const slot = allocate(m);
    if (slot == null) return;
    m.getSource(slotSource(slot))?.setData?.(pathSegmentsFC(segments));
    const t: ActiveTrace = { slot, mode, key, startedAt: Date.now(), timers: [] };
    activeRef.current.set(key, t);
    // Fade in on the next tick so maplibre sees the 0 → 1 change (transition).
    t.timers.push(
      setTimeout(() => setOpacity(m, slot, 1, FADE_IN_MS), 16),
    );
    if (mode === 'live') {
      // Hold for the dwell, then fade out, then release the slot.
      t.timers.push(
        setTimeout(() => {
          setOpacity(m, slot, 0, FADE_OUT_MS);
          t.timers.push(setTimeout(() => releaseSlot(m, t), FADE_OUT_MS + 50));
        }, FADE_IN_MS + DWELL_MS),
      );
    }
  };

  const endHover = (m: MapFace, key: string) => {
    const t = activeRef.current.get(key);
    if (!t) return;
    for (const timer of t.timers) clearTimeout(timer);
    t.timers = [];
    setOpacity(m, t.slot, 0, FADE_OUT_MS);
    t.timers.push(setTimeout(() => releaseSlot(m, t), FADE_OUT_MS + 50));
  };

  // Hover trigger.
  useEffect(() => {
    if (!map) return;
    const m = map as unknown as MapFace;
    const enter = (e: { features?: Array<{ properties?: { call?: unknown } }> }) => {
      const call = e.features?.[0]?.properties?.call;
      if (call != null) startTrace(m, `hover:${String(call)}`, String(call), 'hover');
    };
    const leave = () => {
      // Fade out whichever hover is active (only one at a time in practice).
      for (const key of activeRef.current.keys()) {
        if (key.startsWith('hover:')) endHover(m, key);
      }
    };
    m.on('mouseenter', PIN_LAYER, enter as (...a: unknown[]) => void);
    m.on('mouseleave', PIN_LAYER, leave as (...a: unknown[]) => void);
    return () => {
      m.off('mouseenter', PIN_LAYER, enter as (...a: unknown[]) => void);
      m.off('mouseleave', PIN_LAYER, leave as (...a: unknown[]) => void);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map]);

  // Live trigger: the newest fix gets a one-shot fade-in → dwell → fade-out.
  const newest = positions.length ? positions.reduce((a, b) => (b.at > a.at ? b : a)) : null;
  const newestKey = newest ? `${newest.call}:${newest.at}` : '';
  useEffect(() => {
    if (!map || !newest) return;
    startTrace(map as unknown as MapFace, `live:${newestKey}`, newest.call, 'live');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map, newestKey]);

  // Clean up all timers on unmount.
  useEffect(() => {
    const active = activeRef.current;
    return () => {
      for (const t of active.values()) {
        for (const timer of t.timers) clearTimeout(timer);
      }
      active.clear();
    };
  }, []);

  return (
    <>
      {Array.from({ length: POOL_SIZE }, (_, i) => (
        <TraceSlot key={i} index={i} />
      ))}
    </>
  );
}
