// src/aprs/digipeatPath.ts
//
// Pure resolution of a heard frame's digipeat path into drawable segments.
// RF-honesty: a segment is SOLID only between two hops we have a real heard
// position for; a run of hops with unknown positions (WIDEn-N aliases, unheard
// digis) is bridged by a DASHED connector carrying their callsigns as pos?
// labels — never a fabricated intermediate pin. Only digis that actually
// relayed the frame (H-bit set) count as traversed hops.

import type { ViaHop } from './aprsTypes';

export interface LatLon {
  lat: number;
  lon: number;
}

export interface PathSegment {
  kind: 'solid' | 'dashed';
  from: LatLon;
  to: LatLon;
  /// Callsigns of the unlocatable hops this dashed segment bridges (pos? markers).
  unknownLabels?: string[];
}

export interface ResolveInput {
  src: LatLon & { call: string };
  via: ViaHop[];
  /// Callsign-SSID → latest heard fix, for geolocating intermediate hops.
  located: Map<string, LatLon>;
  /// Operator's own position (grid-square centre), or null when unknown.
  operator: LatLon | null;
}

interface Hop {
  call: string;
  pos: LatLon | null;
}

export function resolveDigipeatPath(input: ResolveInput): PathSegment[] {
  const { src, via, located, operator } = input;

  // Ordered anchor list: src (always located) → traversed digis → operator.
  const hops: Hop[] = [{ call: src.call, pos: { lat: src.lat, lon: src.lon } }];
  for (const h of via) {
    if (!h.repeated) continue; // only digis that actually relayed
    hops.push({ call: h.call, pos: located.get(h.call) ?? null });
  }
  if (operator) hops.push({ call: 'YOU', pos: operator });

  // Walk located anchors; bridge runs of unlocated hops with a dashed segment.
  const segments: PathSegment[] = [];
  let lastLocated = 0; // hops[0] (src) is always located
  for (let i = 1; i < hops.length; i++) {
    if (hops[i].pos == null) continue; // defer — bridged below
    const from = hops[lastLocated].pos as LatLon;
    const to = hops[i].pos as LatLon;
    const between = hops.slice(lastLocated + 1, i).map((h) => h.call);
    segments.push(
      between.length > 0
        ? { kind: 'dashed', from, to, unknownLabels: between }
        : { kind: 'solid', from, to },
    );
    lastLocated = i;
  }
  // Trailing unlocated hops after the last located anchor are dropped: there is
  // no honest endpoint to draw to.
  return segments;
}
