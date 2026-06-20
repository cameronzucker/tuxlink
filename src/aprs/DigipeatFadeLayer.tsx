// src/aprs/DigipeatFadeLayer.tsx
//
// Connection trace-line rework (tuxlink-k0zz, supersedes the reverted cn84 rAF
// draw-in). Renders the honest multi-hop digipeat path on the APRS map and
// FADES it in/out via maplibre's built-in paint-property transitions — NO
// requestAnimationFrame loop, NO per-frame setData, NO re-tessellation, NO
// preserveDrawingBuffer. Each path's geometry is uploaded ONCE; the fade is a
// bounded `line-opacity` / `text-opacity` transition the renderer drives and
// that STOPS when done — the per-frame CPU cost that broke the map on llvmpipe
// is gone.
//
// Independent concurrent dwell: a small POOL of path slots (line source + solid
// + dashed + pos?-label layer each), one per active trace, with its own
// transition + dwell timers — so a station heard in quick succession gets its
// own fade and never clobbers another's timeline.
//
// Triggers: hover a pin (fade in, hold, fade out on mouse-out) + a newly-heard
// frame (fade in, dwell, fade out). RF-honesty: solid through located hops,
// dashed `pos?` (with the unknown hop's callsign) across hops we can't locate.

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
const PIN_LAYER = 'aprs-position-pins-color';

const EMPTY_FC = { type: 'FeatureCollection', features: [] as unknown[] };

function lineSource(i: number) {
  return `aprs-trace-slot-${i}`;
}
function labelSource(i: number) {
  return `aprs-trace-slot-${i}-labels`;
}
function solidLayer(i: number) {
  return `aprs-trace-slot-${i}-solid`;
}
function dashedLayer(i: number) {
  return `aprs-trace-slot-${i}-dashed`;
}
function labelLayer(i: number) {
  return `aprs-trace-slot-${i}-label`;
}

/// One LineString feature per resolved segment, tagged by kind so the slot's two
/// line layers (solid / dashed) each draw their own. Geometry only — uploaded
/// once per trace, never per frame.
function lineFC(segments: PathSegment[]) {
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

/// `pos?` markers: one amber label at the midpoint of each dashed connector,
/// naming the unlocatable hop(s) — the honesty cue (Codex k0zz review).
function labelFC(segments: PathSegment[]) {
  const features: unknown[] = [];
  for (const s of segments) {
    if (s.kind !== 'dashed' || !s.unknownLabels?.length) continue;
    features.push({
      type: 'Feature',
      properties: { label: `${s.unknownLabels.join('/')} ?` },
      geometry: {
        type: 'Point',
        coordinates: [(s.from.lon + s.to.lon) / 2, (s.from.lat + s.to.lat) / 2],
      },
    });
  }
  return { type: 'FeatureCollection', features };
}

function slotLineLayers(i: number): Array<Record<string, unknown> & { id: string }> {
  const common = { layout: { 'line-cap': 'round', 'line-join': 'round' } };
  return [
    {
      id: solidLayer(i),
      type: 'line',
      source: lineSource(i),
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
      id: dashedLayer(i),
      type: 'line',
      source: lineSource(i),
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

function slotLabelLayers(i: number): Array<Record<string, unknown> & { id: string }> {
  return [
    {
      id: labelLayer(i),
      type: 'symbol',
      source: labelSource(i),
      layout: {
        'text-field': ['get', 'label'],
        'text-size': 10,
        'text-offset': [0, -0.8],
        'text-anchor': 'bottom',
        'text-allow-overlap': true,
      },
      paint: {
        'text-color': DASHED_COLOR,
        'text-halo-color': '#0c1620',
        'text-halo-width': 1.2,
        'text-opacity': 0,
        'text-opacity-transition': { duration: FADE_IN_MS },
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
  fadingOut: boolean;
  timers: ReturnType<typeof setTimeout>[];
}

interface MapFace {
  getSource(id: string): { setData?: (d: unknown) => void } | undefined;
  // Optional + guarded: the maplibre test double omits these, and a fade timer
  // must never throw there or when the style was rebuilt out from under a layer.
  getLayer?(id: string): unknown;
  setPaintProperty?(layer: string, prop: string, value: unknown): void;
  on(type: string, layer: string, h: (...a: unknown[]) => void): unknown;
  off(type: string, layer: string, h: (...a: unknown[]) => void): unknown;
}

/// One pool slot's sources + layers, via the owned overlay hook (handles styledata
/// re-add). One component per slot keeps the hooks rules clean.
function TraceSlot({ index }: { index: number }) {
  const map = useMapContext();
  useMapOverlay(map, lineSource(index), { type: 'geojson', data: EMPTY_FC }, slotLineLayers(index));
  useMapOverlay(map, labelSource(index), { type: 'geojson', data: EMPTY_FC }, slotLabelLayers(index));
  return null;
}

export interface DigipeatFadeLayerProps {
  positions: HeardPosition[];
  operator: LatLon | null;
}

export function DigipeatFadeLayer({ positions, operator }: DigipeatFadeLayerProps) {
  const map = useMapContext();

  const lookup = useMemo(() => {
    const m = new Map<string, HeardPosition>();
    const located = new Map<string, LatLon>();
    for (const p of positions) {
      m.set(p.call, p);
      located.set(p.call, { lat: p.lat, lon: p.lon });
    }
    return { m, located };
  }, [positions]);
  const dataRef = useRef(lookup);
  dataRef.current = lookup;
  const operatorRef = useRef(operator);
  operatorRef.current = operator;

  const activeRef = useRef<Map<string, ActiveTrace>>(new Map());
  const freeSlotsRef = useRef<number[]>(Array.from({ length: POOL_SIZE }, (_, i) => i));

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

  // Set the slot's three layers' opacity (line + label) with the given transition
  // duration. Guards each layer in case a style rebuild removed it (Codex review).
  const setOpacity = (m: MapFace, slot: number, value: number, durationMs: number) => {
    if (!m.setPaintProperty) return;
    const apply = (layer: string, prop: string) => {
      if (m.getLayer && !m.getLayer(layer)) return; // layer gone (style swap)
      m.setPaintProperty!(layer, `${prop}-transition`, { duration: durationMs });
      m.setPaintProperty!(layer, prop, value);
    };
    apply(solidLayer(slot), 'line-opacity');
    apply(dashedLayer(slot), 'line-opacity');
    apply(labelLayer(slot), 'text-opacity');
  };

  const clearSlotData = (m: MapFace, slot: number) => {
    m.getSource(lineSource(slot))?.setData?.(EMPTY_FC);
    m.getSource(labelSource(slot))?.setData?.(EMPTY_FC);
  };

  const releaseSlot = (m: MapFace, t: ActiveTrace) => {
    for (const timer of t.timers) clearTimeout(timer);
    clearSlotData(m, t.slot);
    setOpacity(m, t.slot, 0, 0); // INSTANT reset so the next reuse fades from 0 (Codex)
    activeRef.current.delete(t.key);
    if (!freeSlotsRef.current.includes(t.slot)) freeSlotsRef.current.push(t.slot);
  };

  // Allocate a slot: a free one, else evict the oldest LIVE trace (never a hover —
  // the operator is looking at it). releaseSlot resets the evicted slot to opacity 0.
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

  const fadeIn = (m: MapFace, t: ActiveTrace) => {
    // Fade in on the next tick so maplibre sees the 0 → 1 transition.
    t.timers.push(setTimeout(() => setOpacity(m, t.slot, 1, FADE_IN_MS), 16));
  };

  const scheduleFadeOut = (m: MapFace, t: ActiveTrace, afterMs: number) => {
    t.timers.push(
      setTimeout(() => {
        t.fadingOut = true;
        setOpacity(m, t.slot, 0, FADE_OUT_MS);
        t.timers.push(setTimeout(() => releaseSlot(m, t), FADE_OUT_MS + 50));
      }, afterMs),
    );
  };

  const startTrace = (m: MapFace, key: string, call: string, mode: Mode) => {
    const existing = activeRef.current.get(key);
    if (existing) {
      // Re-trigger (e.g. hover re-enters during fade-out): cancel the pending
      // fade-out and fade back in; do not start a second slot (Codex review).
      if (existing.fadingOut) {
        for (const timer of existing.timers) clearTimeout(timer);
        existing.timers = [];
        existing.fadingOut = false;
        fadeIn(m, existing);
        if (mode === 'live') scheduleFadeOut(m, existing, FADE_IN_MS + DWELL_MS);
      }
      return;
    }
    const segments = segmentsFor(call);
    if (!segments) return;
    const slot = allocate(m);
    if (slot == null) return;
    const src = m.getSource(lineSource(slot));
    if (!src?.setData) {
      // Sources not added yet (pre-load / style-rebuild window): do NOT record an
      // active trace or it would occupy a slot showing nothing (Codex review).
      freeSlotsRef.current.push(slot);
      return;
    }
    src.setData(lineFC(segments));
    m.getSource(labelSource(slot))?.setData?.(labelFC(segments));
    const t: ActiveTrace = { slot, mode, key, startedAt: Date.now(), fadingOut: false, timers: [] };
    activeRef.current.set(key, t);
    fadeIn(m, t);
    if (mode === 'live') scheduleFadeOut(m, t, FADE_IN_MS + DWELL_MS);
  };

  const endHover = (m: MapFace, key: string) => {
    const t = activeRef.current.get(key);
    if (!t || t.fadingOut) return;
    for (const timer of t.timers) clearTimeout(timer);
    t.timers = [];
    t.fadingOut = true;
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
