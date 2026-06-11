#!/usr/bin/env tsx
// Build the geo data pipeline for the location-aware Request Center hero.
//
// Task 1 (--fetch-only): fetch + cache NWS public forecast zone list per US
//   state derived from the winlink-queries catalog.
//   Output: dev/scratch/request-geo/raw/<ST>.json (one FeatureCollection per state)
//
// Task 2 (default or --simplify-only): fetch per-zone geometry, simplify, emit.
//   Step 0: GET api.weather.gov/zones/forecast/<ZONEID> for each zone; cache to
//           dev/scratch/request-geo/geom/<ZONEID>.json (idempotent).
//   Step 1: Douglas–Peucker simplification at tolerance 0.005° → 4-decimal coords.
//   Step 2: Emit src/request/nws-zones.geo.json as a pruned FeatureCollection.
//
// Usage:
//   pnpm tsx scripts/build-request-geo.ts --fetch-only   # Task 1 zone-list fetch
//   pnpm tsx scripts/build-request-geo.ts                # Task 2 full pipeline
//   pnpm tsx scripts/build-request-geo.ts --force        # re-fetch everything

import { readFileSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const RAW_CACHE_DIR = resolve(REPO_ROOT, 'dev/scratch/request-geo/raw');
const GEOM_CACHE_DIR = resolve(REPO_ROOT, 'dev/scratch/request-geo/geom');
const CATALOG_PATH = resolve(
  REPO_ROOT,
  'src-tauri/resources/catalog/winlink-queries.txt',
);
const OUTPUT_PATH = resolve(REPO_ROOT, 'src/request/nws-zones.geo.json');

const UA = 'tuxlink-dev (cameronzucker@gmail.com)';

// ---------------------------------------------------------------------------
// CLI flags
// ---------------------------------------------------------------------------
const FETCH_ONLY = process.argv.includes('--fetch-only');
const SIMPLIFY_ONLY = process.argv.includes('--simplify-only');
const FORCE = process.argv.includes('--force');

// Simplification tolerance in degrees (Douglas–Peucker).
// 0.005 → ~4.7 MB, 0.01 → ~2.9 MB, 0.02 → ~1.8 MB (< 2 MB target; chosen).
// Raise further only if the output grows beyond 2 MB on a future NWS dataset refresh.
const DP_TOLERANCE = 0.02;

// ---------------------------------------------------------------------------
// Step 1 — Derive the catalog state set
// ---------------------------------------------------------------------------

/**
 * Read the pipe-delimited winlink-queries catalog and return the distinct
 * two-letter state codes (XX from WX_US_XX) that have at least one entry
 * whose DESCRIPTION matches /zone forecast/i.
 *
 * Categories with more than two uppercase letters after "WX_US_" (e.g.
 * WX_US_GUAM, WX_US_SAMOA, WX_US_SELCTY, WX_US_OUTDR, WX_US_COAST,
 * WX_US_RAD) are excluded by the strict two-letter regex.
 */
function catalogStates(): string[] {
  const raw = readFileSync(CATALOG_PATH, 'utf8');
  // Strip UTF-8 BOM if present
  const text = raw.startsWith('﻿') ? raw.slice(1) : raw;

  const states = new Set<string>();
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split('|');
    if (parts.length < 3) continue;
    const [category, , description] = parts;
    // Only WX_US_ categories with exactly two uppercase letters
    const match = /^WX_US_([A-Z]{2})$/.exec(category);
    if (!match) continue;
    if (/zone forecast/i.test(description)) {
      states.add(match[1]);
    }
  }

  const sorted = [...states].sort();
  return sorted;
}

// ---------------------------------------------------------------------------
// Step 2 — Fetch zones per state
// ---------------------------------------------------------------------------

async function fetchState(st: string): Promise<unknown> {
  const url = `https://api.weather.gov/zones?type=public&area=${st}&include_geometry=true`;
  for (let attempt = 0; attempt < 2; attempt++) {
    const res = await fetch(url, {
      headers: { 'User-Agent': UA, Accept: 'application/geo+json' },
    });
    if (res.ok) return res.json();
    if (attempt === 0) {
      console.warn(`  [${st}] HTTP ${res.status} — retrying in 1 s…`);
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
  throw new Error(`NWS fetch failed for ${st}`);
}

/** Polite delay between requests (ms) */
const INTER_REQUEST_DELAY_MS = 175;

async function sleep(ms: number): Promise<void> {
  await new Promise((r) => setTimeout(r, ms));
}

// ---------------------------------------------------------------------------
// Step 0 (Task 2) — Fetch per-zone geometry
// ---------------------------------------------------------------------------

interface ZoneGeomResponse {
  type: string;
  properties?: Record<string, unknown>;
  geometry: AnyGeometry | null;
}

interface GeoGeometry {
  type: 'Polygon' | 'MultiPolygon';
  coordinates: number[][][] | number[][][][];
}

interface GeoGeometryCollection {
  type: 'GeometryCollection';
  geometries: Array<GeoGeometry | GeoGeometryCollection>;
}

type AnyGeometry = GeoGeometry | GeoGeometryCollection;

/**
 * Flatten a GeoJSON geometry (including GeometryCollections) to a list of
 * Polygon coordinate arrays (each a number[][][]).  Used to normalise NWS
 * GeometryCollection responses — NWS sometimes returns a GeometryCollection
 * with a MultiPolygon (land) + Polygon (water boundary) pair for coastal zones.
 */
function flattenToPolygons(geom: AnyGeometry): number[][][][] {
  if (geom.type === 'Polygon') {
    return [geom.coordinates as number[][][]];
  } else if (geom.type === 'MultiPolygon') {
    return geom.coordinates as number[][][][];
  } else if (geom.type === 'GeometryCollection') {
    const polys: number[][][][] = [];
    for (const sub of geom.geometries) {
      polys.push(...flattenToPolygons(sub as AnyGeometry));
    }
    return polys;
  }
  return [];
}

async function fetchZoneGeometry(zoneId: string): Promise<ZoneGeomResponse> {
  const url = `https://api.weather.gov/zones/forecast/${zoneId}`;
  for (let attempt = 0; attempt < 2; attempt++) {
    const res = await fetch(url, {
      headers: { 'User-Agent': UA, Accept: 'application/geo+json' },
    });
    if (res.ok) return res.json() as Promise<ZoneGeomResponse>;
    if (attempt === 0) {
      console.warn(`  [${zoneId}] HTTP ${res.status} — retrying in 1 s…`);
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
  throw new Error(`NWS geometry fetch failed for ${zoneId}`);
}

// ---------------------------------------------------------------------------
// Step 1 (Task 2) — Douglas–Peucker geometry simplification
// ---------------------------------------------------------------------------

/** Perpendicular distance from point p to the line segment ab (in "coordinate units"). */
function dpPerpendicularDistance(
  p: [number, number],
  a: [number, number],
  b: [number, number],
): number {
  const [px, py] = p;
  const [ax, ay] = a;
  const [bx, by] = b;
  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy;
  if (lenSq === 0) {
    // a === b; distance to the point itself
    const ex = px - ax;
    const ey = py - ay;
    return Math.sqrt(ex * ex + ey * ey);
  }
  // Projection parameter t (clamped is fine for polygon rings)
  const t = ((px - ax) * dx + (py - ay) * dy) / lenSq;
  const projX = ax + t * dx;
  const projY = ay + t * dy;
  const fx = px - projX;
  const fy = py - projY;
  return Math.sqrt(fx * fx + fy * fy);
}

/** Douglas–Peucker recursive simplification of a ring (array of [lon, lat] pairs).
 *  Preserves first and last point so the ring can remain closed. */
function douglasPeucker(
  points: [number, number][],
  tolerance: number,
): [number, number][] {
  if (points.length < 3) return points;

  let maxDist = 0;
  let maxIdx = 0;
  const first = points[0];
  const last = points[points.length - 1];

  for (let i = 1; i < points.length - 1; i++) {
    const d = dpPerpendicularDistance(points[i], first, last);
    if (d > maxDist) {
      maxDist = d;
      maxIdx = i;
    }
  }

  if (maxDist > tolerance) {
    const left = douglasPeucker(points.slice(0, maxIdx + 1), tolerance);
    const right = douglasPeucker(points.slice(maxIdx), tolerance);
    // left ends at maxIdx, right starts at maxIdx → concat dropping duplicate
    return [...left.slice(0, -1), ...right];
  }

  return [first, last];
}

/** Round a coordinate to 4 decimal places (matches us-states.geo.json precision). */
function round4(x: number): number {
  return Math.round(x * 10000) / 10000;
}

/**
 * Fallback tolerance ladder: try progressively less aggressive simplification
 * when DP at the primary tolerance degenerates a ring.
 * Tolerance 0 means "keep original ring vertices" — always a valid polygon.
 */
const FALLBACK_TOLERANCES = [0.01, 0.005, 0] as const;

/**
 * Attempt to simplify a single ring at the given tolerance.
 * Returns the simplified (and closed) ring if valid (>= 4 points), or null.
 */
function trySimplifyRing(
  pts: [number, number][],
  tol: number,
): [number, number][] | null {
  let simple = tol === 0 ? [...pts] : douglasPeucker(pts, tol);
  // Ensure ring is closed
  if (
    simple.length > 0 &&
    (simple[0][0] !== simple[simple.length - 1][0] ||
      simple[0][1] !== simple[simple.length - 1][1])
  ) {
    simple = [...simple, simple[0]];
  }
  // A valid polygon ring needs at least 4 points (3 unique + closing repeat)
  if (simple.length < 4) return null;
  return simple;
}

/**
 * Simplify a single ring at primaryTol, falling back through FALLBACK_TOLERANCES
 * (including tolerance 0 = original) if the ring degenerates.
 * Returns the simplified ring (never null — tolerance 0 always succeeds if the
 * source ring was valid) and whether a fallback was needed.
 */
function simplifyRingWithFallback(
  pts: [number, number][],
  primaryTol: number,
): { ring: [number, number][]; usedFallback: boolean } {
  const result = trySimplifyRing(pts, primaryTol);
  if (result) return { ring: result, usedFallback: false };

  for (const tol of FALLBACK_TOLERANCES) {
    if (tol >= primaryTol) continue; // only fall BACK (less aggressive)
    const fb = trySimplifyRing(pts, tol);
    if (fb) return { ring: fb, usedFallback: true };
  }

  // Tolerance 0 always produces the original ring as long as it has >= 4 points.
  // If the source ring has < 4 points it was already degenerate before simplification.
  const original = trySimplifyRing(pts, 0);
  if (original) return { ring: original, usedFallback: true };

  // Degenerate source ring — cannot save.
  return { ring: pts as [number, number][], usedFallback: true };
}

/** Simplify a Polygon ring array using per-ring fallback on degeneration.
 *  Returns null only if a source ring was already degenerate before simplification. */
function simplifyPolygon(
  coords: number[][][],
  tol: number,
): { result: number[][][]; hadFallback: boolean } | null {
  const simplified: number[][][] = [];
  let hadFallback = false;
  for (const ring of coords) {
    const pts = ring as [number, number][];
    // Check source ring validity first
    if (pts.length < 4) return null; // degenerate source
    const { ring: simplifiedRing, usedFallback } = simplifyRingWithFallback(pts, tol);
    if (usedFallback) hadFallback = true;
    simplified.push(simplifiedRing.map(([x, y]) => [round4(x), round4(y)]));
  }
  return { result: simplified, hadFallback };
}

/** Simplify a GeoJSON geometry with per-ring fallback. Returns null only if source is degenerate.
 *  GeometryCollection is flattened to MultiPolygon before simplification. */
function simplifyGeometry(
  geom: AnyGeometry,
  tol: number,
): { geom: GeoGeometry; hadFallback: boolean } | null {
  // Flatten GeometryCollection to MultiPolygon
  if (geom.type === 'GeometryCollection') {
    const allPolys = flattenToPolygons(geom);
    if (allPolys.length === 0) return null;
    const simplifiedPolys: number[][][][] = [];
    let hadFallback = false;
    for (const poly of allPolys) {
      const r = simplifyPolygon(poly, tol);
      if (r) {
        simplifiedPolys.push(r.result);
        if (r.hadFallback) hadFallback = true;
      }
    }
    if (simplifiedPolys.length === 0) return null;
    return {
      geom: { type: 'MultiPolygon', coordinates: simplifiedPolys },
      hadFallback,
    };
  }

  if (geom.type === 'Polygon') {
    const coords = geom.coordinates as number[][][];
    const r = simplifyPolygon(coords, tol);
    if (!r) return null;
    return {
      geom: { type: 'Polygon', coordinates: r.result },
      hadFallback: r.hadFallback,
    };
  } else if (geom.type === 'MultiPolygon') {
    const coords = geom.coordinates as number[][][][];
    const simplifiedPolys: number[][][][] = [];
    let hadFallback = false;
    for (const poly of coords) {
      const r = simplifyPolygon(poly, tol);
      if (r) {
        simplifiedPolys.push(r.result);
        if (r.hadFallback) hadFallback = true;
      }
      // If a sub-polygon's source is degenerate, skip only that sub-polygon
    }
    if (simplifiedPolys.length === 0) return null;
    return {
      geom: { type: 'MultiPolygon', coordinates: simplifiedPolys },
      hadFallback,
    };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Derive state code from zone id (e.g. "WAZ315" → "WA")
// ---------------------------------------------------------------------------

function stateFromZoneId(zoneId: string): string {
  // NWS forecast zone ids follow the pattern: <2-letter-state-code><1-letter><3-digits>
  // e.g. WAZ315 → WA, FLZ050 → FL, PRZ001 → PR
  return zoneId.slice(0, 2).toUpperCase();
}

// ---------------------------------------------------------------------------
// Output feature type
// ---------------------------------------------------------------------------

interface OutputFeature {
  type: 'Feature';
  properties: { id: string; name: string; state: string };
  geometry: GeoGeometry;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function fetchZoneList(): Promise<void> {
  const states = catalogStates();
  console.log(
    `Catalog state set: ${states.length} states with zone-forecast entries`,
  );
  console.log(`States: ${states.join(', ')}`);

  mkdirSync(RAW_CACHE_DIR, { recursive: true });

  let fetched = 0;
  let skipped = 0;
  let maxEffectiveDate = '';

  for (const st of states) {
    const outPath = resolve(RAW_CACHE_DIR, `${st}.json`);

    if (!FORCE && existsSync(outPath)) {
      console.log(`  [${st}] cached — skip (use --force to re-fetch)`);
      try {
        const cached = JSON.parse(readFileSync(outPath, 'utf8')) as {
          features?: Array<{ properties?: { effectiveDate?: string } }>;
        };
        for (const feat of cached.features ?? []) {
          const ed = feat.properties?.effectiveDate ?? '';
          if (ed > maxEffectiveDate) maxEffectiveDate = ed;
        }
      } catch {
        // Non-fatal — cached file may be malformed
      }
      skipped++;
      continue;
    }

    console.log(`  [${st}] fetching…`);
    const data = (await fetchState(st)) as {
      features?: Array<{ properties?: { effectiveDate?: string } }>;
    };

    for (const feat of data.features ?? []) {
      const ed = feat.properties?.effectiveDate ?? '';
      if (ed > maxEffectiveDate) maxEffectiveDate = ed;
    }

    writeFileSync(outPath, JSON.stringify(data));
    fetched++;

    if (fetched < states.length - skipped) {
      await sleep(INTER_REQUEST_DELAY_MS);
    }
  }

  console.log(
    `\nFetch complete: ${fetched} fetched, ${skipped} skipped (cached).`,
  );
  if (maxEffectiveDate) {
    console.log(`Dataset version (max effectiveDate): ${maxEffectiveDate}`);
  } else {
    console.log('Dataset version: no effectiveDate found in responses.');
  }
}

async function fetchGeometryAndEmit(): Promise<void> {
  const states = catalogStates();

  // Collect all zone ids from the raw cache
  interface RawFeature {
    properties: {
      id: string;
      name: string;
      state?: string;
      effectiveDate?: string;
    };
    geometry: GeoGeometry | null;
  }

  const allZones: Array<{ id: string; name: string; state: string }> = [];
  for (const st of states) {
    const cachePath = resolve(RAW_CACHE_DIR, `${st}.json`);
    if (!existsSync(cachePath)) {
      console.error(
        `  Missing raw cache for ${st} — run with --fetch-only first.`,
      );
      process.exit(1);
    }
    const data = JSON.parse(readFileSync(cachePath, 'utf8')) as {
      features: RawFeature[];
    };
    for (const feat of data.features ?? []) {
      const zoneId = feat.properties.id;
      const name = feat.properties.name;
      // Derive state from zone id prefix (authoritative) or properties.state
      const state = stateFromZoneId(zoneId) || feat.properties.state || st;
      allZones.push({ id: zoneId, name, state });
    }
  }

  console.log(`Total zones to process: ${allZones.length}`);

  if (!SIMPLIFY_ONLY) {
    // Step 0 — Fetch per-zone geometry
    mkdirSync(GEOM_CACHE_DIR, { recursive: true });

    let geomFetched = 0;
    let geomSkipped = 0;
    let geomFailed = 0;
    let nullGeomCount = 0;

    for (let i = 0; i < allZones.length; i++) {
      const { id: zoneId } = allZones[i];
      const cachePath = resolve(GEOM_CACHE_DIR, `${zoneId}.json`);

      if (!FORCE && existsSync(cachePath)) {
        geomSkipped++;
      } else {
        // Space out requests politely
        if (geomFetched > 0) {
          await sleep(INTER_REQUEST_DELAY_MS);
        }

        try {
          const data = await fetchZoneGeometry(zoneId);
          writeFileSync(cachePath, JSON.stringify(data));
          geomFetched++;
        } catch (err) {
          console.warn(`  [${zoneId}] fetch error: ${String(err)}`);
          geomFailed++;
        }
      }

      // Log progress every 100 zones
      if ((i + 1) % 100 === 0 || i + 1 === allZones.length) {
        const geomWithNull = nullGeomCount;
        void geomWithNull; // reference to suppress lint
        console.log(
          `  Progress: ${i + 1}/${allZones.length} (fetched=${geomFetched} skipped=${geomSkipped} failed=${geomFailed})`,
        );
      }
    }

    console.log(
      `\nGeometry fetch complete: ${geomFetched} fetched, ${geomSkipped} skipped, ${geomFailed} failed.`,
    );
  }

  // Step 1 + 2 — Simplify and emit
  console.log('\nSimplifying geometry and emitting nws-zones.geo.json…');

  const features: OutputFeature[] = [];
  let nullGeom = 0;
  let degenSourceGeom = 0;
  let fallbackUsed = 0;
  const nullGeomIds: string[] = [];
  const degenSourceIds: string[] = [];

  for (const { id: zoneId, name, state } of allZones) {
    const cachePath = resolve(GEOM_CACHE_DIR, `${zoneId}.json`);
    if (!existsSync(cachePath)) {
      console.warn(`  [${zoneId}] no geometry cache — skipping`);
      nullGeom++;
      nullGeomIds.push(`${zoneId} (no cache)`);
      continue;
    }

    let data: ZoneGeomResponse;
    try {
      data = JSON.parse(readFileSync(cachePath, 'utf8')) as ZoneGeomResponse;
    } catch {
      console.warn(`  [${zoneId}] malformed geometry cache — skipping`);
      nullGeom++;
      nullGeomIds.push(`${zoneId} (malformed)`);
      continue;
    }

    if (!data.geometry) {
      nullGeom++;
      nullGeomIds.push(zoneId);
      continue;
    }

    const simplified = simplifyGeometry(data.geometry, DP_TOLERANCE);
    if (!simplified) {
      // Only reaches here if source geometry itself was degenerate (< 4 points)
      console.warn(`  [${zoneId}] source geometry is degenerate — skipping`);
      degenSourceGeom++;
      degenSourceIds.push(zoneId);
      continue;
    }

    if (simplified.hadFallback) {
      fallbackUsed++;
    }

    features.push({
      type: 'Feature',
      properties: { id: zoneId, name, state },
      geometry: simplified.geom,
    });
  }

  if (nullGeomIds.length > 0) {
    console.log(
      `\nSkipped ${nullGeomIds.length} zones with null/missing geometry: ${nullGeomIds.slice(0, 10).join(', ')}${nullGeomIds.length > 10 ? ` … (+${nullGeomIds.length - 10} more)` : ''}`,
    );
  }
  if (degenSourceGeom > 0) {
    console.log(`Skipped ${degenSourceGeom} zones with degenerate SOURCE geometry (< 4 pts): ${degenSourceIds.join(', ')}`);
  }
  if (fallbackUsed > 0) {
    console.log(`Used min-vertex fallback tolerance for ${fallbackUsed} tiny zone(s) — preserved in output.`);
  }

  const geojson = {
    type: 'FeatureCollection',
    features,
  };

  writeFileSync(OUTPUT_PATH, JSON.stringify(geojson));

  const sizeKb = Math.round(
    Buffer.byteLength(JSON.stringify(geojson)) / 1024,
  );

  console.log(`\nEmitted: ${OUTPUT_PATH}`);
  console.log(`Features: ${features.length}`);
  console.log(`Size: ~${sizeKb} KB`);

  if (sizeKb > 2048) {
    console.warn(
      `\nWARNING: output is ${sizeKb} KB (> 2 MB). Re-run with higher tolerance:`,
    );
    console.warn(
      `  Increase DP_TOLERANCE (currently ${DP_TOLERANCE}) and re-run with --simplify-only`,
    );
  }

  // Step 3 — Spot-check WAZ315 (Seattle, per grounding corrections)
  const waz315 = features.find((f) => f.properties.id === 'WAZ315');
  if (waz315) {
    console.log(
      `\nSpot-check WAZ315: name="${waz315.properties.name}" state="${waz315.properties.state}" geomType="${waz315.geometry.type}"`,
    );
  } else {
    console.warn('\nWARNING: WAZ315 not found in output — check WA raw cache.');
  }
}

async function main(): Promise<void> {
  if (FETCH_ONLY) {
    await fetchZoneList();
    return;
  }

  // Ensure raw cache is available
  const states = catalogStates();
  const missingRaw = states.filter(
    (st) => !existsSync(resolve(RAW_CACHE_DIR, `${st}.json`)),
  );
  if (missingRaw.length > 0) {
    console.log(`Raw cache missing for: ${missingRaw.join(', ')}`);
    console.log('Fetching zone lists first…\n');
    await fetchZoneList();
  }

  await fetchGeometryAndEmit();
}

main().catch((err: unknown) => {
  console.error('build-request-geo failed:', err);
  process.exit(1);
});
