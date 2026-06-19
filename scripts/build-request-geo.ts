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
// Task 3 (default pipeline, after Task 2): auto-match NWS zones to Winlink catalog.
//   Emits:
//     src/request/nws-zone-to-catalog.json  — map of NWS zone ID → catalog filename
//     src/request/nws-zone-unmapped.json    — multi-zone regional entries
//     dev/scratch/request-geo/unresolved.txt — abbreviated entries without exact match
//
// Task 4 (--prune-geometry): filter nws-zones.geo.json to mapped zones only.
//   Reads the COMMITTED nws-zones.geo.json + nws-zone-to-catalog.json (no network).
//   Rewrites nws-zones.geo.json with only features whose properties.id is a key in
//   the map. Safe to run after Tasks 2+3 without risk of clobbering mapping JSONs.
//   Idempotent: re-running on an already-pruned file is a no-op (same IDs filtered).
//
// Task 5 (--radar): emit src/request/radar-regions.json — bbox table for all 161 WX_US_RAD
//   catalog entries. Bboxes are derived from us-states.geo.json state extents, applying
//   direction-qualifier parsing (W/E/N/S/NE/NW/SE/SW/CENTRAL splits) plus a hand-curated
//   OVERRIDE map for metro/feature names that don't parse to a state+direction.
//   Merge-preserves any existing manual override entries on re-run.
//   Critical nesting verified: PSND area < NWWA area < PNW area, Seattle inside all three.
//   Output: src/request/radar-regions.json
//
// Usage:
//   pnpm tsx scripts/build-request-geo.ts --fetch-only      # Task 1 zone-list fetch
//   pnpm tsx scripts/build-request-geo.ts                   # Tasks 2+3 full pipeline
//   pnpm tsx scripts/build-request-geo.ts --match-only      # Task 3 match only (skip geometry)
//   pnpm tsx scripts/build-request-geo.ts --force           # re-fetch everything
//   pnpm tsx scripts/build-request-geo.ts --prune-geometry  # Task 4 prune to mapped zones
//   pnpm tsx scripts/build-request-geo.ts --radar           # Task 5 radar-region bbox table

import { readFileSync, mkdirSync, writeFileSync, existsSync, appendFileSync } from 'node:fs';
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
const ZONE_MAP_PATH = resolve(REPO_ROOT, 'src/request/nws-zone-to-catalog.json');
const ZONE_UNMAPPED_PATH = resolve(REPO_ROOT, 'src/request/nws-zone-unmapped.json');
const UNRESOLVED_PATH = resolve(REPO_ROOT, 'dev/scratch/request-geo/unresolved.txt');
const RADAR_REGIONS_PATH = resolve(REPO_ROOT, 'src/request/radar-regions.json');
const US_STATES_GEO_PATH = resolve(REPO_ROOT, 'src/request/us-states.geo.json');

const UA = 'tuxlink-dev (https://tuxlink.org)';

// ---------------------------------------------------------------------------
// CLI flags
// ---------------------------------------------------------------------------
const FETCH_ONLY = process.argv.includes('--fetch-only');
const SIMPLIFY_ONLY = process.argv.includes('--simplify-only');
const MATCH_ONLY = process.argv.includes('--match-only');
const FORCE = process.argv.includes('--force');
const PRUNE_GEOMETRY = process.argv.includes('--prune-geometry');
const RADAR = process.argv.includes('--radar');

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
    // v2 (tuxlink-z1b7): include ALL catalog states, not only those carrying a
    // per-NWS-zone product. Coarse-region states (AZ, TX, CA, …) must be fetched
    // too so the per-zone resolver can cover the whole country, not just the 8
    // states whose products happen to be per-zone. `description` is unused now.
    void description;
    states.add(match[1]);
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
// Task 3 — Auto-match NWS zones ↔ Winlink catalog zone-forecast entries
// ---------------------------------------------------------------------------

/**
 * USPS two-letter code → lower-cased full state name.
 * Used to generate the state-stripped normalisation variant.
 * Source: src/request/usStateName.ts (kept in sync manually).
 */
const USPS_TO_LOWER_NAME: Record<string, string> = {
  AL: 'alabama', AK: 'alaska', AZ: 'arizona', AR: 'arkansas',
  CA: 'california', CO: 'colorado', CT: 'connecticut', DE: 'delaware',
  FL: 'florida', GA: 'georgia', HI: 'hawaii', ID: 'idaho',
  IL: 'illinois', IN: 'indiana', IA: 'iowa', KS: 'kansas',
  KY: 'kentucky', LA: 'louisiana', ME: 'maine', MD: 'maryland',
  MA: 'massachusetts', MI: 'michigan', MN: 'minnesota', MS: 'mississippi',
  MO: 'missouri', MT: 'montana', NE: 'nebraska', NV: 'nevada',
  NH: 'new hampshire', NJ: 'new jersey', NM: 'new mexico', NY: 'new york',
  NC: 'north carolina', ND: 'north dakota', OH: 'ohio', OK: 'oklahoma',
  OR: 'oregon', PA: 'pennsylvania', RI: 'rhode island', SC: 'south carolina',
  SD: 'south dakota', TN: 'tennessee', TX: 'texas', UT: 'utah',
  VT: 'vermont', VA: 'virginia', WA: 'washington', WV: 'west virginia',
  WI: 'wisconsin', WY: 'wyoming', DC: 'district of columbia',
  PR: 'puerto rico', VI: 'us virgin islands', GU: 'guam',
  AS: 'american samoa', MP: 'northern mariana islands',
};

/**
 * Normalise a zone name or catalog description for comparison.
 * Strips " Zone Forecast" and any trailing junk, removes punctuation,
 * collapses whitespace, lower-cases.
 */
function normalise(s: string): string {
  return s
    .toLowerCase()
    .replace(/&/g, 'and')
    .replace(/\s+zone forecast\b.*$/, '')
    .replace(/[^a-z0-9 ]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Produce the state-stripped variant of an already-normalised key: strip a
 * trailing " <full state name>" (e.g. " washington") if present.
 * Returns the stripped string if the state name was found at the end,
 * or the original string if not.
 */
function stripStateSuffix(norm: string, stateCode: string): string {
  const stateLower = USPS_TO_LOWER_NAME[stateCode.toUpperCase()];
  if (!stateLower) return norm;
  const suffix = ' ' + stateLower;
  if (norm.endsWith(suffix)) {
    return norm.slice(0, norm.length - suffix.length).trim();
  }
  return norm;
}

/**
 * Jaccard token-overlap score between two normalised strings.
 * Used to rank top-3 candidate NWS zones for unresolved catalog entries.
 */
function tokenOverlap(a: string, b: string): number {
  const sa = new Set(a.split(' ').filter(Boolean));
  const sb = new Set(b.split(' ').filter(Boolean));
  if (sa.size === 0 || sb.size === 0) return 0;
  let intersection = 0;
  for (const t of sa) {
    if (sb.has(t)) intersection++;
  }
  return intersection / (sa.size + sb.size - intersection);
}

/**
 * Build and emit:
 *   src/request/nws-zone-to-catalog.json
 *   src/request/nws-zone-unmapped.json
 *   dev/scratch/request-geo/unresolved.txt  (appended, not replaced)
 */
async function buildZoneCatalogMap(): Promise<void> {
  console.log('\n=== Task 3: auto-match NWS zones ↔ catalog ===');

  // ------------------------------------------------------------------
  // Step 1 — Build catalog zone-forecast index per state
  // ------------------------------------------------------------------
  interface CatalogEntry {
    filename: string;
    rawDescription: string;
    normPlain: string;
    normStateStripped: string;
  }

  const rawCatalog = readFileSync(CATALOG_PATH, 'utf8');
  const catalogText = rawCatalog.startsWith('﻿') ? rawCatalog.slice(1) : rawCatalog;

  // Map: stateCode → list of catalog entries for that state
  const catalogByState = new Map<string, CatalogEntry[]>();

  for (const line of catalogText.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split('|');
    if (parts.length < 3) continue;
    const [category, filename, description] = parts;
    const match = /^WX_US_([A-Z]{2})$/.exec(category);
    if (!match) continue;
    if (!/zone forecast/i.test(description)) continue;

    const st = match[1];
    const normPlain = normalise(description);
    const normStateStripped = stripStateSuffix(normPlain, st);

    const entry: CatalogEntry = { filename, rawDescription: description, normPlain, normStateStripped };
    if (!catalogByState.has(st)) catalogByState.set(st, []);
    catalogByState.get(st)!.push(entry);
  }

  // ------------------------------------------------------------------
  // Step 2 — Build NWS zone index per state from raw cache
  // ------------------------------------------------------------------
  interface NwsZone {
    id: string;
    name: string;
    normName: string;
  }

  const nwsByState = new Map<string, NwsZone[]>();
  const maxEffectiveDates: string[] = [];

  for (const st of catalogByState.keys()) {
    const cachePath = resolve(RAW_CACHE_DIR, `${st}.json`);
    if (!existsSync(cachePath)) {
      console.warn(`  [${st}] raw cache missing — skipping state in match pass`);
      continue;
    }
    const raw = JSON.parse(readFileSync(cachePath, 'utf8')) as {
      features: Array<{ properties: { id: string; name: string; effectiveDate?: string } }>;
    };
    const zones: NwsZone[] = raw.features.map((f) => ({
      id: f.properties.id,
      name: f.properties.name,
      normName: normalise(f.properties.name),
    }));
    nwsByState.set(st, zones);

    for (const f of raw.features) {
      const ed = f.properties.effectiveDate ?? '';
      if (ed) maxEffectiveDates.push(ed);
    }
  }

  // ------------------------------------------------------------------
  // Step 3 — Match per state
  // ------------------------------------------------------------------
  const mapEntries = new Map<string, string>(); // NWS ID → catalog filename
  const unmappedEntries = new Map<string, string>(); // catalog filename → 'multi-zone regional'
  const unresolvedLines: string[] = [];
  let collisionCount = 0;
  const collisionLog: string[] = [];

  for (const [st, catalogEntries] of catalogByState) {
    const nwsZones = nwsByState.get(st) ?? [];

    // Build NWS lookup by normalised name for this state
    const nwsByNorm = new Map<string, NwsZone[]>();
    for (const zone of nwsZones) {
      const n = zone.normName;
      if (!nwsByNorm.has(n)) nwsByNorm.set(n, []);
      nwsByNorm.get(n)!.push(zone);
    }

    // Also build NWS lookup by state-stripped norm (some NWS names include state)
    const nwsByNormStateStripped = new Map<string, NwsZone[]>();
    for (const zone of nwsZones) {
      const stripped = stripStateSuffix(zone.normName, st);
      if (!nwsByNormStateStripped.has(stripped)) nwsByNormStateStripped.set(stripped, []);
      nwsByNormStateStripped.get(stripped)!.push(zone);
    }

    for (const entry of catalogEntries) {
      const { filename, rawDescription, normPlain, normStateStripped } = entry;

      // Regional: starts with "zone forecast for" (case-insensitive, already lowercased)
      if (normPlain.startsWith('zone forecast for')) {
        unmappedEntries.set(filename, 'multi-zone regional');
        continue;
      }

      // Try exact match on plain variant
      let matchedZones = nwsByNorm.get(normPlain);

      // Try exact match on state-stripped catalog key vs plain NWS names
      if (!matchedZones && normStateStripped !== normPlain) {
        matchedZones = nwsByNorm.get(normStateStripped);
      }

      // Try exact match on plain catalog key vs state-stripped NWS names
      if (!matchedZones) {
        matchedZones = nwsByNormStateStripped.get(normPlain);
      }

      // Try exact match on state-stripped catalog key vs state-stripped NWS names
      if (!matchedZones && normStateStripped !== normPlain) {
        matchedZones = nwsByNormStateStripped.get(normStateStripped);
      }

      if (matchedZones && matchedZones.length > 0) {
        // Exact match found (one or more zones with this norm)
        if (matchedZones.length > 1) {
          // Multiple NWS zones with the same normalised name — pick the one
          // with shortest zone id (deterministic: lexicographic first)
          matchedZones.sort((a, b) => a.id.localeCompare(b.id));
          const collision = `COLLISION (NWS multi-zone): ${filename} matches zones ${matchedZones.map((z) => z.id).join(', ')} → using ${matchedZones[0].id}`;
          collisionLog.push(`[${st}] ${collision}`);
          collisionCount++;
          console.log(`  ${collision}`);
        }

        const zoneId = matchedZones[0].id;

        // Check if this NWS zone is already claimed by another catalog entry
        if (mapEntries.has(zoneId)) {
          // Multiple catalog entries map to the same NWS zone — collision on catalog side
          const existing = mapEntries.get(zoneId)!;
          // Pick deterministically: catalog entry whose RAW description is closest length to NWS zone name
          const existingEntry = catalogEntries.find((e) => e.filename === existing);
          const nwsName = matchedZones[0].name;
          const existingDelta = existingEntry
            ? Math.abs(existingEntry.rawDescription.length - nwsName.length)
            : Infinity;
          const newDelta = Math.abs(rawDescription.length - nwsName.length);

          const collision = `COLLISION (catalog multi-match): NWS ${zoneId} claimed by ${existing} (delta=${existingDelta}) and ${filename} (delta=${newDelta}) → using ${newDelta < existingDelta ? filename : existing}`;
          collisionLog.push(`[${st}] ${collision}`);
          collisionCount++;
          console.log(`  ${collision}`);

          if (newDelta < existingDelta) {
            mapEntries.set(zoneId, filename);
          }
          // If equal or existing wins, keep existing (first-encountered wins ties)
        } else {
          mapEntries.set(zoneId, filename);
        }
      } else {
        // No exact match — compute top-3 candidates by token overlap
        const scored = nwsZones
          .map((z) => ({ zone: z, score: tokenOverlap(normPlain, z.normName) }))
          .sort((a, b) => b.score - a.score || a.zone.id.localeCompare(b.zone.id))
          .slice(0, 3);

        const candidateStr = scored
          .map((s) => `${s.zone.name}(${s.zone.id},${s.score.toFixed(2)})`)
          .join('; ');

        unresolvedLines.push(
          `${st} ${filename} | ${rawDescription} | candidates: ${candidateStr}`,
        );
      }
    }
  }

  // ------------------------------------------------------------------
  // Step 4 — Emit outputs
  // ------------------------------------------------------------------

  // Determine dataset metadata
  const maxEffDate = maxEffectiveDates.length > 0
    ? maxEffectiveDates.reduce((a, b) => (a > b ? a : b))
    : '';

  // Count total NWS zones from raw cache
  let totalNwsZones = 0;
  for (const zones of nwsByState.values()) {
    totalNwsZones += zones.length;
  }

  // Build sorted map
  const sortedMapKeys = [...mapEntries.keys()].sort();
  const sortedMap: Record<string, string> = {};
  for (const k of sortedMapKeys) {
    sortedMap[k] = mapEntries.get(k)!;
  }

  // Build sorted unmapped
  const sortedUnmappedKeys = [...unmappedEntries.keys()].sort();
  const sortedUnmapped: Record<string, string> = {};
  for (const k of sortedUnmappedKeys) {
    sortedUnmapped[k] = unmappedEntries.get(k)!;
  }

  // Emit nws-zone-to-catalog.json
  const zoneMapOutput = {
    _source: {
      dataset: 'api.weather.gov/zones?type=public',
      fetched: '2026-06-10',
      zoneCount: totalNwsZones,
      datasetEffectiveDate: maxEffDate,
    },
    map: sortedMap,
  };
  writeFileSync(ZONE_MAP_PATH, JSON.stringify(zoneMapOutput, null, 2));
  console.log(`\nEmitted: ${ZONE_MAP_PATH} (${sortedMapKeys.length} entries)`);

  // Emit nws-zone-unmapped.json
  const unmappedOutput = { unmapped: sortedUnmapped };
  writeFileSync(ZONE_UNMAPPED_PATH, JSON.stringify(unmappedOutput, null, 2));
  console.log(`Emitted: ${ZONE_UNMAPPED_PATH} (${sortedUnmappedKeys.length} entries)`);

  // Append to unresolved.txt (create if not exists; overwrite to avoid stale data)
  mkdirSync(resolve(REPO_ROOT, 'dev/scratch/request-geo'), { recursive: true });
  writeFileSync(UNRESOLVED_PATH, unresolvedLines.join('\n') + (unresolvedLines.length > 0 ? '\n' : ''));
  console.log(`Emitted: ${UNRESOLVED_PATH} (${unresolvedLines.length} lines)`);

  // Summary
  console.log(
    `\nmapped=${sortedMapKeys.length} unmapped=${sortedUnmappedKeys.length} unresolved=${unresolvedLines.length} collisions=${collisionCount}`,
  );

  if (collisionLog.length > 0) {
    console.log('\nCollisions:');
    for (const c of collisionLog) console.log(`  ${c}`);
  }

  // ------------------------------------------------------------------
  // Spot-checks (WA)
  // ------------------------------------------------------------------
  console.log('\n=== WA spot-checks ===');
  const bluf = [...mapEntries.entries()].find(([, v]) => v === 'WA_ZON_BLUF');
  console.log(
    bluf
      ? `  WA_ZON_BLUF → mapped from NWS ${bluf[0]} ✓`
      : `  WA_ZON_BLUF → NOT in map (check unresolved)`,
  );

  const seaEntry = [...mapEntries.entries()].find(([, v]) => v === 'WA_ZON_SEA');
  console.log(
    seaEntry
      ? `  WA_ZON_SEA → mapped from NWS ${seaEntry[0]} (expected WAZ315)`
      : `  WA_ZON_SEA → NOT in map (in unresolved — expected if city-of-seattle-washington normalisation missed)`,
  );

  const forEast = unmappedEntries.get('WA_FOR_EAST');
  console.log(
    forEast
      ? `  WA_FOR_EAST → unmapped (${forEast}) ✓`
      : `  WA_FOR_EAST → NOT in unmapped (bug: should be multi-zone regional)`,
  );

  const cakcfInMap = [...mapEntries.values()].includes('WA_ZON_CAKCF');
  const cakcfInUnresolved = unresolvedLines.some((l) => l.includes('WA_ZON_CAKCF'));
  if (cakcfInMap) {
    console.log(`  WA_ZON_CAKCF → in map (unexpected — abbreviated name should not auto-match)`);
  } else if (cakcfInUnresolved) {
    console.log(`  WA_ZON_CAKCF → in unresolved ✓`);
  } else {
    console.log(`  WA_ZON_CAKCF → NOT found in map or unresolved (unexpected)`);
  }
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

// ---------------------------------------------------------------------------
// Task 4 — Prune nws-zones.geo.json to mapped zones only
//
// Reads the COMMITTED nws-zones.geo.json and nws-zone-to-catalog.json, filters
// the FeatureCollection to only features whose properties.id is a KEY in the
// map, then rewrites nws-zones.geo.json in-place. The two mapping JSONs
// (nws-zone-to-catalog.json, nws-zone-unmapped.json) are NEVER touched here.
//
// Idempotent: the same set of mapped IDs is filtered on every run, so
// re-running on an already-pruned file produces the identical output.
// ---------------------------------------------------------------------------

function pruneGeometry(): void {
  console.log('\n=== Task 4: prune nws-zones.geo.json to mapped zones ===');

  if (!existsSync(OUTPUT_PATH)) {
    console.error(`ERROR: ${OUTPUT_PATH} not found — run the full pipeline first.`);
    process.exit(1);
  }
  if (!existsSync(ZONE_MAP_PATH)) {
    console.error(`ERROR: ${ZONE_MAP_PATH} not found — run Tasks 2+3 first.`);
    process.exit(1);
  }

  // Load the zone map (keys are the mapped NWS zone IDs)
  const mapJson = JSON.parse(readFileSync(ZONE_MAP_PATH, 'utf8')) as {
    _source: unknown;
    map: Record<string, string>;
  };
  const mappedIds = new Set(Object.keys(mapJson.map));
  console.log(`Mapped zone IDs: ${mappedIds.size}`);

  // Load the current geometry bundle
  const geoRaw = readFileSync(OUTPUT_PATH, 'utf8');
  const beforeBytes = Buffer.byteLength(geoRaw);
  const geoJson = JSON.parse(geoRaw) as {
    type: string;
    features: Array<{ properties: { id: string } }>;
  };
  const before = geoJson.features.length;
  console.log(`Features before prune: ${before}  (~${Math.round(beforeBytes / 1024)} KB)`);

  // Filter to only mapped zones
  const pruned = geoJson.features.filter((f) => mappedIds.has(f.properties.id));
  console.log(`Features after prune:  ${pruned.length}`);

  // Verify WAZ315 survives (it is mapped → should always be in the output)
  const waz315 = pruned.find((f) => f.properties.id === 'WAZ315');
  if (waz315) {
    console.log(`WAZ315 (City of Seattle): present in pruned output ✓`);
  } else {
    console.warn(`WARNING: WAZ315 absent from pruned output — it may not be mapped`);
  }

  // Verify all mapped IDs are satisfied (referential-integrity gate)
  const prunedIds = new Set(pruned.map((f) => f.properties.id));
  const orphans = [...mappedIds].filter((id) => !prunedIds.has(id));
  if (orphans.length > 0) {
    console.warn(`WARNING: ${orphans.length} mapped zone(s) have no geometry in the bundle:`);
    for (const id of orphans) console.warn(`  ${id}`);
  } else {
    console.log(`Referential integrity: all ${mappedIds.size} mapped ids present in pruned output ✓`);
  }

  // Rewrite nws-zones.geo.json
  const output = { type: 'FeatureCollection', features: pruned };
  const outputStr = JSON.stringify(output);
  writeFileSync(OUTPUT_PATH, outputStr);

  const afterKb = Math.round(Buffer.byteLength(outputStr) / 1024);
  console.log(`\nEmitted: ${OUTPUT_PATH}`);
  console.log(`Size: ~${afterKb} KB`);

  if (afterKb > 1024) {
    console.warn(`WARNING: pruned output is ${afterKb} KB (> 1 MB) — unexpected for ${pruned.length} zones`);
  }
}

// ---------------------------------------------------------------------------
// Task 5 — Radar-region bbox table
//
// Derives bboxes for all 161 WX_US_RAD catalog entries from us-states.geo.json
// state extents. Direction qualifiers (W/E/N/S/NE/NW/SE/SW/CENTRAL) are applied
// as fractional bbox splits with a small overlap buffer. Metro/feature names that
// don't parse to a state+direction are handled via the RADAR_OVERRIDES map below.
//
// Merge-preservation: if radar-regions.json already exists and contains entries
// not in the derived map (e.g., manual corrections), they are preserved on re-run.
// The derived entries take precedence over preserved ones for any filename conflict.
//
// Critical nesting invariant (verified at runtime):
//   area(PSND) < area(NWWA) < area(PNW)  AND  Seattle (47.6042, -122.2917) ∈ all three.
// ---------------------------------------------------------------------------

interface StateBbox {
  usps: string;
  bbox: [number, number, number, number]; // [west, south, east, north]
}

/** Read all state bboxes from us-states.geo.json by computing min/max of coordinates. */
function loadStateBboxes(): Map<string, [number, number, number, number]> {
  const raw = JSON.parse(readFileSync(US_STATES_GEO_PATH, 'utf8')) as {
    features: Array<{
      properties: { usps: string };
      geometry: {
        type: string;
        coordinates: number[][][] | number[][][][];
      };
    }>;
  };

  const map = new Map<string, [number, number, number, number]>();

  for (const feat of raw.features) {
    const usps = feat.properties.usps;
    const coords: number[][] = [];

    const geom = feat.geometry;
    if (geom.type === 'Polygon') {
      for (const ring of geom.coordinates as number[][][]) {
        coords.push(...ring);
      }
    } else if (geom.type === 'MultiPolygon') {
      for (const poly of geom.coordinates as number[][][][]) {
        for (const ring of poly) {
          coords.push(...ring);
        }
      }
    }

    if (coords.length === 0) continue;
    const lons = coords.map((c) => c[0]);
    const lats = coords.map((c) => c[1]);
    map.set(usps, [
      Math.round(Math.min(...lons) * 10000) / 10000,
      Math.round(Math.min(...lats) * 10000) / 10000,
      Math.round(Math.max(...lons) * 10000) / 10000,
      Math.round(Math.max(...lats) * 10000) / 10000,
    ]);
  }

  return map;
}

type Bbox = [number, number, number, number];

function r4(b: number[]): Bbox {
  return b.map((x) => Math.round(x * 10000) / 10000) as Bbox;
}

function unionBbox(...bbs: Bbox[]): Bbox {
  return r4([
    Math.min(...bbs.map((b) => b[0])),
    Math.min(...bbs.map((b) => b[1])),
    Math.max(...bbs.map((b) => b[2])),
    Math.max(...bbs.map((b) => b[3])),
  ]);
}

function splitBbox(
  b: Bbox,
  dir: 'W' | 'E' | 'N' | 'S' | 'NW' | 'NE' | 'SW' | 'SE' | 'CENT',
  frac = 0.5,
  ov = 0.5,
): Bbox {
  const [w, s, e, n] = b;
  const lm = w + (e - w) * frac;
  const lam = s + (n - s) * frac;
  switch (dir) {
    case 'W': return r4([w, s, lm + ov, n]);
    case 'E': return r4([lm - ov, s, e, n]);
    case 'N': return r4([w, lam - ov, e, n]);
    case 'S': return r4([w, s, e, lam + ov]);
    case 'NW': return r4([w, lam - ov, lm + ov, n]);
    case 'NE': return r4([lm - ov, lam - ov, e, n]);
    case 'SW': return r4([w, s, lm + ov, lam + ov]);
    case 'SE': return r4([lm - ov, s, e, lam + ov]);
    case 'CENT': return r4([w + (e - w) * 0.25, s + (n - s) * 0.25, e - (e - w) * 0.25, n - (n - s) * 0.25]);
  }
}

/**
 * Build the complete radar-region bbox table.
 *
 * Every WX_US_RAD catalog entry must have an entry with a 4-element bbox (no nulls).
 * Uses state bboxes + directional splits for the majority; RADAR_OVERRIDES covers
 * the metro/feature/territory/Alaska sub-regions that don't parse to a state+direction.
 *
 * Override map rationale comments explain each hand-curated entry.
 */
function buildRadarRegions(): void {
  console.log('\n=== Task 5: radar-region bbox table ===');

  const stateBboxes = loadStateBboxes();
  console.log(`Loaded ${stateBboxes.size} state bboxes from us-states.geo.json`);

  function st(code: string): Bbox {
    const b = stateBboxes.get(code);
    if (!b) throw new Error(`State bbox not found: ${code}`);
    return b;
  }

  function W(c: string, frac = 0.5, ov = 0.5): Bbox { return splitBbox(st(c), 'W', frac, ov); }
  function E(c: string, frac = 0.5, ov = 0.5): Bbox { return splitBbox(st(c), 'E', frac, ov); }
  function N(c: string, frac = 0.5, ov = 0.5): Bbox { return splitBbox(st(c), 'N', frac, ov); }
  function S(c: string, frac = 0.5, ov = 0.5): Bbox { return splitBbox(st(c), 'S', frac, ov); }
  function NW(c: string, lf = 0.5, af = 0.5, ov = 0.5): Bbox {
    const b = st(c);
    const lm = b[0] + (b[2] - b[0]) * lf;
    const lam = b[1] + (b[3] - b[1]) * af;
    return r4([b[0], lam - ov, lm + ov, b[3]]);
  }
  function NE(c: string, lf = 0.5, af = 0.5, ov = 0.5): Bbox {
    const b = st(c);
    const lm = b[0] + (b[2] - b[0]) * lf;
    const lam = b[1] + (b[3] - b[1]) * af;
    return r4([lm - ov, lam - ov, b[2], b[3]]);
  }
  function SW(c: string, lf = 0.5, af = 0.5, ov = 0.5): Bbox {
    const b = st(c);
    const lm = b[0] + (b[2] - b[0]) * lf;
    const lam = b[1] + (b[3] - b[1]) * af;
    return r4([b[0], b[1], lm + ov, lam + ov]);
  }
  function SE(c: string, lf = 0.5, af = 0.5, ov = 0.5): Bbox {
    const b = st(c);
    const lm = b[0] + (b[2] - b[0]) * lf;
    const lam = b[1] + (b[3] - b[1]) * af;
    return r4([lm - ov, b[1], b[2], lam + ov]);
  }
  function CENT(c: string, lf1 = 0.25, lf2 = 0.75, af1 = 0.25, af2 = 0.75): Bbox {
    const b = st(c);
    return r4([
      b[0] + (b[2] - b[0]) * lf1,
      b[1] + (b[3] - b[1]) * af1,
      b[2] - (b[2] - b[0]) * (1 - lf2),
      b[3] - (b[3] - b[1]) * (1 - af2),
    ]);
  }

  // -----------------------------------------------------------------------
  // Override map for hand-curated metro/feature/territory regions.
  // Entries here take precedence over derived bboxes for the same filename.
  // Each entry has a comment explaining the basis for the bbox choice.
  // -----------------------------------------------------------------------
  const RADAR_OVERRIDES: Record<string, { name: string; bbox: Bbox }> = {
    // Alaska sub-regions — AK state bbox covers the whole chain; sub-regions hand-curated
    // from geographic knowledge of the Aleutian chain, Kenai Peninsula, SE panhandle.
    // West longitudes clamped to -180: far-western Aleutians past the antimeridian are out of
    // scope for alpha (proper antimeridian split would require two bbox tiles).
    'US.RAD.EALAK': { name: 'E Aleutian Isl to Palmer Ak', bbox: r4([Math.max(-180, -188.9), 51.6, -145.0, 62.0]) },
    'US.RAD.NSEAK': { name: 'N SE Alaska to South Central Ak', bbox: r4([-155.0, 55.0, -129.9, 66.0]) },
    'US.RAD.SCAK': { name: 'South Central Alaska', bbox: r4([-156.0, 58.0, -142.0, 63.0]) },
    'US.RAD.SEAK': { name: 'Southeast Alaska', bbox: r4([-138.0, 54.0, -129.9, 60.5]) },
    'US.RAD.SWAK': { name: 'Southwest Alaska', bbox: r4([-170.0, 54.0, -155.0, 62.5]) },
    'US.RAD.WFNAK': { name: 'Point Hope to Hooper Bay Alaska', bbox: r4([-170.0, 58.0, -155.0, 68.5]) },
    'US.RAD.SINAK': { name: 'S Cent to Int Palmer 2 Fort Yukon', bbox: r4([-155.0, 60.0, -142.0, 67.0]) },
    // Guam — Pacific island territory; coordinates from standard geographic references
    'US.RAD.GUAM': { name: 'Guam', bbox: r4([144.5, 13.2, 145.0, 13.7]) },
    // Hawaii sub-island groups — HI state bbox covers all; sub-groups hand-curated
    'US.RAD.HIKO': { name: 'Hawaii Kauai & Oahu', bbox: r4([-160.3, 21.2, -157.6, 22.3]) },
    'US.RAD.HIMH': { name: 'Hawaii Maui & Hawaii Isl', bbox: r4([-156.7, 18.9, -154.8, 21.0]) },
    'US.RAD.HIMMH': { name: 'Hawaii Maui Molokai & Hawaii Isl', bbox: r4([-157.4, 18.9, -154.8, 21.3]) },
    'US.RAD.HOMMH': { name: 'Hi Oah Maui Molokai & Hawaii Isl', bbox: r4([-158.3, 18.9, -154.8, 21.5]) },
    // CA coastal strips — coast-strip geometry doesn't match state directional splits well
    'US.RAD.COCAC': { name: 'Ca Coast Monterey - Santa Monica', bbox: r4([-122.5, 33.8, -117.5, 37.0]) },
    'US.RAD.COCAS': { name: 'Ca Cst San Luis Obispo - San Die', bbox: r4([-121.5, 32.5, -116.5, 35.5]) },
    // Cape Girardeau MO area — named metro region, not a state subdivision
    'US.RAD.CGI': { name: 'Cape Giardeau Mo Area', bbox: r4([-90.5, 36.5, -88.5, 38.0]) },
    // Texas Panhandle — irregular geometry; southern portion of the narrow panhandle strip
    'US.RAD.TXPH': { name: 'Texas Panhandle (Southern Part)', bbox: (() => { const b = st('TX'); return r4([b[0], b[3] - 3.0, b[0] + 5.5, b[3]]); })() },
    // TX & Padre Island — southernmost TX coast + barrier island strip
    'US.RAD.TXPI': { name: 'Tx & Padre Isl', bbox: (() => { const b = st('TX'); return r4([b[2] - 3.5, b[1], b[2], b[1] + 3.0]); })() },
    // NY cross-border regions including Canadian cities — added ~0.5° buffer north for Montreal/Toronto
    'US.RAD.CNNY': { name: 'Central N New York & Montreal Ca', bbox: unionBbox(CENT('NY'), r4([-73.8, 45.0, -73.2, 45.6])) },
    'US.RAD.NNY': { name: 'NNY & Montreal Ca', bbox: unionBbox(N('NY'), r4([-73.8, 45.0, -73.2, 45.6])) },
    'US.RAD.NWNY': { name: 'NNY & Toronto Ca', bbox: unionBbox(N('NY'), r4([-79.5, 43.5, -79.0, 43.9])) },
    // Pacific Northwest nested set — hand-curated to enforce area(PSND) < area(NWWA) < area(PNW)
    // with Seattle (47.6042, -122.2917) contained in all three. These are the critical
    // Task 8 resolver anchors; do NOT change without re-running the nesting verification test.
    'US.RAD.PSND': { name: 'Puget Sound & SJDF', bbox: r4([-124.9, 46.9, -121.4, 49.0]) },
    'US.RAD.NWWA': { name: 'W Washington & NW Oregon', bbox: r4([-124.9, 45.2, -120.8, 49.0]) },
    'US.RAD.PNW': { name: 'Pacific Northwest', bbox: r4([-125.0, 41.9, -116.5, 49.1]) },
    // CONUS full extent
    'US.RAD.CONUS': { name: 'Conus', bbox: r4([-130.0, 23.0, -65.0, 50.0]) },
    // Puerto Rico — PR state bbox from us-states.geo.json is accurate; no override needed
    // (included here as a comment to document that it's derived, not overridden)
  };

  // -----------------------------------------------------------------------
  // Derived entries — state bboxes + direction splits
  // -----------------------------------------------------------------------
  const derived: Record<string, { name: string; bbox: Bbox }> = {
    // Alaska (full state) — west clamped to -180: far-western Aleutians past the antimeridian
    // are out of scope for alpha (proper antimeridian split requires two bbox tiles).
    'US.RAD.ALASK': { name: 'Alaska', bbox: (() => { const b = st('AK'); return r4([Math.max(-180, b[0]), b[1], b[2], b[3]]); })() },
    // Arizona
    'US.RAD.AZ': { name: 'Arizona', bbox: st('AZ') },
    'US.RAD.SAZ': { name: 'S Arizona', bbox: S('AZ') },
    'US.RAD.SWAZ': { name: 'SW Arizona & SE California', bbox: unionBbox(SW('AZ'), SE('CA')) },
    // Arkansas
    'US.RAD.AR': { name: 'Arkansas', bbox: st('AR') },
    'US.RAD.NWAR': { name: 'NW Arkansas & NE Texas', bbox: unionBbox(NW('AR'), NE('TX', 0.5, 0.7)) },
    'US.RAD.WMO': { name: 'W Missouri & NE Arkansas', bbox: unionBbox(W('MO'), NE('AR')) },
    // California
    'US.RAD.NCA': { name: 'N Coastal Ca', bbox: (() => { const b = st('CA'); return r4([b[0], b[1] + (b[3] - b[1]) * 0.6, b[0] + 1.5, b[3]]); })() },
    'US.RAD.NCCA': { name: 'N Central Ca', bbox: (() => { const b = st('CA'); return r4([b[0] + 1.5, b[1] + (b[3] - b[1]) * 0.55, b[2] - 2.0, b[3]]); })() },
    'US.RAD.CCA': { name: 'S Central Ca', bbox: (() => { const b = st('CA'); return r4([b[0] + 1.0, b[1] + (b[3] - b[1]) * 0.3, b[2] - 1.5, b[1] + (b[3] - b[1]) * 0.7]); })() },
    'US.RAD.SCCA': { name: 'S Central California', bbox: (() => { const b = st('CA'); return r4([b[0] + 1.0, b[1] + (b[3] - b[1]) * 0.2, b[2] - 1.5, b[1] + (b[3] - b[1]) * 0.55]); })() },
    'US.RAD.SCA': { name: 'S Calf to Los Angeles', bbox: SW('CA', 0.5, 0.45) },
    'US.RAD.SWCA': { name: 'SW California', bbox: SW('CA', 0.5, 0.4) },
    'US.RAD.SNV': { name: 'S Nv / NW Az / E Ca', bbox: unionBbox(S('NV'), NW('AZ'), E('CA', 0.6)) },
    // Colorado
    'US.RAD.CCO': { name: 'Central Colorado', bbox: CENT('CO') },
    'US.RAD.SECO': { name: 'SE Colorado', bbox: SE('CO') },
    'US.RAD.WCO': { name: 'W Colorado / E Utah', bbox: unionBbox(W('CO'), E('UT')) },
    'US.RAD.WYCO': { name: 'Wyoming & Colorado', bbox: unionBbox(st('WY'), st('CO')) },
    // Florida
    'US.RAD.CEFL': { name: 'Central Florida', bbox: CENT('FL') },
    'US.RAD.EFLPH': { name: 'East Florida Ph & S Georgia', bbox: unionBbox(E('FL'), S('GA')) },
    'US.RAD.FLKW': { name: 'S Fl & Key West', bbox: S('FL', 0.3) },
    'US.RAD.NEFL': { name: 'N Florida / SE Georgia', bbox: unionBbox(N('FL'), SE('GA')) },
    'US.RAD.NFL': { name: 'N Florida / S Georgia', bbox: unionBbox(N('FL'), S('GA')) },
    'US.RAD.NWFL': { name: 'NW Florida/SE Al/SW Ga', bbox: unionBbox(NW('FL'), SE('AL'), SW('GA')) },
    'US.RAD.SFL': { name: 'Southern Florida', bbox: S('FL', 0.4) },
    // Georgia
    'US.RAD.GA': { name: 'Georgia', bbox: st('GA') },
    'US.RAD.NGA': { name: 'N Georgia / NE Alabama', bbox: unionBbox(N('GA'), NE('AL')) },
    // Hawaii (full state)
    'US.RAD.HAWAI': { name: 'Hawaii', bbox: st('HI') },
    // Idaho-related
    'US.RAD.EWA': { name: 'E Washington & Idaho Ph', bbox: unionBbox(E('WA'), st('ID')) },
    'US.RAD.NEOR': { name: 'NE Oregon & SE Washington', bbox: unionBbox(NE('OR'), SE('WA')) },
    'US.RAD.SEID': { name: 'SE Idaho & N Utah', bbox: unionBbox(SE('ID'), N('UT')) },
    'US.RAD.SWID': { name: 'SW Id & SE Or', bbox: unionBbox(SW('ID'), SE('OR')) },
    'US.RAD.NUT': { name: 'N Utah / SE Idaho & E Nevada', bbox: unionBbox(N('UT'), SE('ID'), E('NV')) },
    // Illinois
    'US.RAD.IL': { name: 'Illinois', bbox: st('IL') },
    'US.RAD.CWI': { name: 'Central Wisconsin / N Illinois', bbox: unionBbox(CENT('WI'), N('IL')) },
    'US.RAD.EIA': { name: 'E Iowa / W Il', bbox: unionBbox(E('IA'), W('IL')) },
    'US.RAD.ECMS': { name: 'E Central Missouri & S Illinois', bbox: unionBbox(E('MO', 0.5), S('IL')) },
    // Indiana
    'US.RAD.IN': { name: 'Indiana', bbox: st('IN') },
    'US.RAD.NIN': { name: 'N Indiana & S Michigan', bbox: unionBbox(N('IN'), S('MI')) },
    'US.RAD.SIN': { name: 'S Indianna / N Kentucky & W Ohio', bbox: unionBbox(S('IN'), N('KY'), W('OH')) },
    // Iowa
    'US.RAD.IA': { name: 'Iowa', bbox: st('IA') },
    'US.RAD.NEIA': { name: 'NE Iowa & S Wisconsin', bbox: unionBbox(NE('IA'), S('WI')) },
    'US.RAD.ENE': { name: 'E Ne / E Ks / NW Ms / SW Ia', bbox: unionBbox(E('NE'), E('KS'), NW('MO'), SW('IA')) },
    // Kansas
    'US.RAD.EKS': { name: 'E Kansas', bbox: E('KS') },
    'US.RAD.EKSWM': { name: 'E Kansas / W Missouri', bbox: unionBbox(E('KS'), W('MO')) },
    'US.RAD.NKS': { name: 'N Kansas / SW Neb & E Co', bbox: unionBbox(N('KS'), SW('NE'), E('CO')) },
    'US.RAD.SCKS': { name: 'S Central Ks', bbox: (() => { const b = st('KS'); return r4([b[0] + (b[2] - b[0]) * 0.2, b[1], b[2] - (b[2] - b[0]) * 0.2, b[1] + (b[3] - b[1]) * 0.6]); })() },
    'US.RAD.SEKS': { name: 'SE Kansas / NE Oklahoma', bbox: unionBbox(SE('KS'), NE('OK')) },
    // Kentucky
    'US.RAD.KY': { name: 'Kentucky', bbox: st('KY') },
    'US.RAD.CTN': { name: 'Cent Tennessee & Cent Kentucky', bbox: unionBbox(CENT('TN'), CENT('KY')) },
    'US.RAD.ETN': { name: 'E Tenn / E Kent & W N. Carolina', bbox: unionBbox(E('TN'), E('KY'), W('NC')) },
    'US.RAD.TN': { name: 'Tennessee / W Kentucky', bbox: unionBbox(st('TN'), W('KY')) },
    // Louisiana
    'US.RAD.NWLA': { name: 'NW Louisana & E Tx & SW Arkansas', bbox: unionBbox(NW('LA'), E('TX', 0.7), SW('AR')) },
    'US.RAD.SELA': { name: 'SE Louisiana / S Missippi', bbox: unionBbox(SE('LA'), S('MS')) },
    'US.RAD.WLA': { name: 'W Louisiana & NE Texas', bbox: unionBbox(W('LA'), NE('TX', 0.5, 0.6)) },
    // Maine
    'US.RAD.CME': { name: 'Coastal Maine', bbox: E('ME', 0.4) },
    'US.RAD.SWME': { name: 'SW Maine / New Hampshire & NE Vt', bbox: unionBbox(SW('ME'), st('NH'), NE('VT')) },
    // Michigan
    'US.RAD.EMI': { name: 'E Mich / N Indiana', bbox: unionBbox(E('MI'), N('IN')) },
    'US.RAD.MI': { name: 'E Mich / N Indiana', bbox: unionBbox(E('MI'), N('IN')) },
    'US.RAD.MIUP': { name: 'Michigan Up / N Wisconsin', bbox: unionBbox(N('MI', 0.7), N('WI')) },
    'US.RAD.NWMI': { name: 'NW Michigan & E Up', bbox: NW('MI', 0.5, 0.6) },
    'US.RAD.WMI': { name: 'W Mich / N Indiana', bbox: unionBbox(W('MI'), N('IN')) },
    'US.RAD.EWI': { name: 'E Wisconsin / L Mich', bbox: unionBbox(E('WI'), W('MI')) },
    // Minnesota
    'US.RAD.NCMI': { name: 'North Central Minnesota', bbox: N('MN', 0.5) },
    'US.RAD.NWI': { name: 'N Wi & E Mn', bbox: unionBbox(N('WI'), E('MN')) },
    'US.RAD.END': { name: 'E N. Dakota & W Minnesota', bbox: unionBbox(E('ND'), W('MN')) },
    // Mississippi
    'US.RAD.SMS': { name: 'S Mississippi', bbox: S('MS') },
    'US.RAD.NWAL': { name: 'NW Alabama / NE Miss / SW Tenn', bbox: unionBbox(NW('AL'), NE('MS'), SW('TN')) },
    'US.RAD.SAL': { name: 'S Alabama / S Miss & W Fl', bbox: unionBbox(S('AL'), S('MS'), W('FL')) },
    // Missouri
    'US.RAD.WTN': { name: 'W Tenn / E Ark / N Miss', bbox: unionBbox(W('TN'), E('AR'), N('MS')) },
    'US.RAD.SMVAL': { name: 'South Miss Valley', bbox: unionBbox(S('MO'), N('AR'), N('MS'), W('TN'), E('OK', 0.8)) },
    'US.RAD.SMVLR': { name: 'South Miss Valley (Low Res)', bbox: unionBbox(S('MO'), N('AR'), N('MS'), W('TN'), E('OK', 0.8)) },
    // Montana
    'US.RAD.CMT': { name: 'Central Mt', bbox: CENT('MT') },
    'US.RAD.NCMT': { name: 'North Central Mt', bbox: N('MT', 0.5) },
    'US.RAD.NEMT': { name: 'Northeast Montana', bbox: NE('MT') },
    'US.RAD.SCMT': { name: 'South Central Mt', bbox: (() => { const b = st('MT'); return r4([b[0] + (b[2] - b[0]) * 0.25, b[1], b[2] - (b[2] - b[0]) * 0.25, b[1] + (b[3] - b[1]) * 0.6]); })() },
    'US.RAD.WMT': { name: 'Western Montana', bbox: W('MT') },
    // Nebraska / Dakotas
    'US.RAD.CNE': { name: 'Central Nebraska', bbox: CENT('NE') },
    'US.RAD.ECNE': { name: 'E Central Nebraska & S S Dakota', bbox: unionBbox(E('NE', 0.45), S('SD')) },
    'US.RAD.NESD': { name: 'NE S Dakota & SE N Dakota', bbox: unionBbox(NE('SD'), SE('ND')) },
    'US.RAD.SESD': { name: 'SE South Dakota / W Minn', bbox: unionBbox(SE('SD'), W('MN')) },
    'US.RAD.NCAZ': { name: 'SE South Dakota / W Minn', bbox: unionBbox(SE('SD'), W('MN')) },
    // Nevada
    'US.RAD.NCNV': { name: 'N Central Nevada', bbox: N('NV', 0.5) },
    'US.RAD.WCNV': { name: 'W Central Nevada & E Ca', bbox: unionBbox(W('NV', 0.4), E('CA', 0.8)) },
    // New Mexico
    'US.RAD.CNM': { name: 'Central New Mexico', bbox: CENT('NM') },
    'US.RAD.ECNM': { name: 'E Central New Mexico', bbox: E('NM', 0.45) },
    'US.RAD.SENM': { name: 'SE New Mexico', bbox: SE('NM') },
    'US.RAD.SWNM': { name: 'SW Nw to S Central Nm', bbox: unionBbox(SW('NM'), CENT('NM', 0.1, 0.6, 0.0, 0.55)) },
    // New York
    'US.RAD.ENY': { name: 'E New York', bbox: E('NY') },
    'US.RAD.LINY': { name: 'Long Isl Ny / Conn / Ri / N Nj', bbox: unionBbox(SE('NY'), st('CT'), st('RI'), N('NJ')) },
    // North Carolina
    'US.RAD.ENC': { name: 'Eastern North Carolina', bbox: E('NC') },
    'US.RAD.NNC': { name: 'N Carolina / SE Virginia', bbox: unionBbox(N('NC'), SE('VA')) },
    'US.RAD.CNESC': { name: 'Cst NE S Carol & Cst SE N Carol', bbox: unionBbox(NE('SC'), SE('NC')) },
    'US.RAD.NWSC': { name: 'NW S Carolina / NE Ga & SW N Car', bbox: unionBbox(NW('SC'), NE('GA'), SW('NC')) },
    'US.RAD.CSC': { name: 'Coastal Sc', bbox: E('SC', 0.4) },
    // North Dakota
    'US.RAD.CND': { name: 'Central North Dakota', bbox: CENT('ND') },
    // Ohio
    'US.RAD.NEOH': { name: 'NE Ohio & NW Pennsylvania', bbox: unionBbox(NE('OH'), NW('PA')) },
    'US.RAD.NOH': { name: 'N Ohio & E Mi', bbox: unionBbox(N('OH'), E('MI')) },
    'US.RAD.WOH': { name: 'W Ohio / SE Indiana & N Kentucky', bbox: unionBbox(W('OH'), SE('IN'), N('KY')) },
    // Oklahoma
    'US.RAD.CEOK': { name: 'Central Oklahoma', bbox: CENT('OK') },
    'US.RAD.NEOK': { name: 'NE Oklahoma & NW Ark & SE Kansas', bbox: unionBbox(NE('OK'), NW('AR'), SE('KS')) },
    'US.RAD.NTX': { name: 'N Texas / SW Oklahoma', bbox: unionBbox(N('TX', 0.35), SW('OK')) },
    'US.RAD.OKPH': { name: 'Oklahoma Ph & Texas Ph', bbox: unionBbox(N('OK'), N('TX', 0.3)) },
    // Oregon
    'US.RAD.NWOR': { name: 'NW Oregon & SW Washington', bbox: unionBbox(NW('OR'), SW('WA')) },
    'US.RAD.SWOR': { name: 'SW Oregon / NW California', bbox: unionBbox(SW('OR'), NW('CA')) },
    // Pennsylvania
    'US.RAD.NWPA': { name: 'NW Pa', bbox: NW('PA') },
    'US.RAD.NJ': { name: 'Nj Swct De Epa', bbox: unionBbox(st('NJ'), SW('CT'), st('DE'), E('PA', 0.55)) },
    'US.RAD.MD': { name: 'Md / Ri Pa Va', bbox: unionBbox(st('MD'), st('RI'), st('PA'), N('VA')) },
    // Puerto Rico (full territory)
    'US.RAD.PR': { name: 'Puerto Rico', bbox: st('PR') },
    // South Carolina
    'US.RAD.SEVA': { name: 'SE Virginia & E S Carolina', bbox: unionBbox(SE('VA'), E('SC')) },
    'US.RAD.EVA': { name: 'E Virginia & E N Carolina', bbox: unionBbox(E('VA'), E('NC')) },
    // South Dakota
    'US.RAD.WSD': { name: 'Western South Dakota', bbox: W('SD') },
    // Tennessee
    'US.RAD.NALTN': { name: 'N Ala & S Tenn & NW Geo', bbox: unionBbox(N('AL'), S('TN'), NW('GA')) },
    'US.RAD.NAL': { name: 'N Alabama', bbox: N('AL') },
    'US.RAD.ECA': { name: 'E Cent Alabama & W Georgia', bbox: unionBbox(E('AL', 0.45), W('GA')) },
    // Texas
    'US.RAD.CTX': { name: 'Central Texas', bbox: CENT('TX') },
    'US.RAD.NCTX': { name: 'Central Texas', bbox: CENT('TX') },
    'US.RAD.SCTX': { name: 'S Central Texas', bbox: (() => { const b = st('TX'); return r4([b[0] + (b[2] - b[0]) * 0.2, b[1], b[2] - (b[2] - b[0]) * 0.2, b[1] + (b[3] - b[1]) * 0.5]); })() },
    'US.RAD.SECTX': { name: 'NE Coast Texas & S Cst Louisiana', bbox: unionBbox(NE('TX', 0.7, 0.25), S('LA', 0.4)) },
    'US.RAD.SPLA': { name: 'South Plains', bbox: unionBbox(N('TX', 0.45), SE('NM'), SW('OK')) },
    'US.RAD.SWTX': { name: 'SW Tx', bbox: SW('TX') },
    'US.RAD.TXCO': { name: 'NE Coast Texas & S Cst Louisiana', bbox: unionBbox(NE('TX', 0.65, 0.3), S('LA', 0.35)) },
    'US.RAD.WCTX': { name: 'W Central Tx', bbox: (() => { const b = st('TX'); return r4([b[0] + 1.0, b[1] + 5.0, b[0] + 7.5, b[1] + 9.5]); })() },
    // Utah
    'US.RAD.SWUT': { name: 'SW Utah / NW Az & SE Nv', bbox: unionBbox(SW('UT'), NW('AZ'), SE('NV')) },
    // Virginia / West Virginia
    'US.RAD.NVA': { name: 'N Virginia / Maryland SE Penn', bbox: unionBbox(N('VA'), st('MD'), SE('PA')) },
    'US.RAD.WWV': { name: 'W W. Va & SE Ohio & W Va', bbox: unionBbox(W('WV'), SE('OH'), st('WV')) },
    // Wide regional
    'US.RAD.GRLAK': { name: 'Cent Great Lakes', bbox: unionBbox(st('MI'), N('IN'), N('OH'), N('IL'), N('WI'), r4([-79.0, 43.0, -76.0, 45.5])) },
    'US.RAD.NEAST': { name: 'Northeast', bbox: unionBbox(st('ME'), st('NH'), st('VT'), st('MA'), st('CT'), st('RI'), st('NY'), st('NJ'), st('PA')) },
    'US.RAD.NROC': { name: 'North Rockies', bbox: unionBbox(st('MT'), st('ID'), N('WY'), N('CO')) },
    'US.RAD.SROC': { name: 'South Rockies', bbox: unionBbox(S('CO'), st('NM'), N('AZ'), N('TX', 0.2)) },
    'US.RAD.SEAST': { name: 'Southeast', bbox: unionBbox(st('GA'), st('AL'), st('MS'), st('FL'), st('SC'), st('NC'), st('TN')) },
    'US.RAD.UMVAL': { name: 'Up Miss Valley', bbox: unionBbox(st('MN'), N('WI'), N('IA'), N('IL'), N('MO')) },
    'US.RAD.UMVLR': { name: 'Up Miss Valley (Low Res)', bbox: unionBbox(st('MN'), N('WI'), N('IA'), N('IL'), N('MO')) },
    'US.RAD.PACSW': { name: 'Pacific Southwest', bbox: unionBbox(st('AZ'), SW('CA'), SW('NV')) },
    // Misc multi-state
    'US.RAD.MARI': { name: 'Ri & Ma', bbox: unionBbox(st('RI'), st('MA')) },
    'US.RAD.NEIL': { name: 'NE Illinois & SE Wisconsin', bbox: unionBbox(NE('IL'), SE('WI')) },
    // Wyoming
    'US.RAD.WY': { name: 'Wyoming', bbox: st('WY') },
  };

  // -----------------------------------------------------------------------
  // Merge: overrides take precedence over derived for the same filename
  // -----------------------------------------------------------------------
  const merged: Record<string, { name: string; bbox: Bbox }> = {
    ...derived,
    ...RADAR_OVERRIDES,
  };

  // -----------------------------------------------------------------------
  // Load catalog WX_US_RAD filenames for completeness check
  // -----------------------------------------------------------------------
  const rawCatalog = readFileSync(CATALOG_PATH, 'utf8');
  const catalogText = rawCatalog.startsWith('﻿') ? rawCatalog.slice(1) : rawCatalog;

  const catalogRadarFilenames = new Set<string>();
  for (const line of catalogText.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split('|');
    if (parts.length < 2) continue;
    if (parts[0] === 'WX_US_RAD') {
      catalogRadarFilenames.add(parts[1]);
    }
  }

  // -----------------------------------------------------------------------
  // Merge-preserve existing overrides from the on-disk file (if any)
  // -----------------------------------------------------------------------
  if (existsSync(RADAR_REGIONS_PATH)) {
    const existing = JSON.parse(readFileSync(RADAR_REGIONS_PATH, 'utf8')) as {
      regions: Array<{ filename: string; name: string; bbox: number[] }>;
    };
    // Preserve any entry that is NOT in the derived map — those are manual corrections
    for (const r of existing.regions) {
      if (!(r.filename in merged) && catalogRadarFilenames.has(r.filename)) {
        merged[r.filename] = { name: r.name, bbox: r.bbox as Bbox };
      }
    }
  }

  // -----------------------------------------------------------------------
  // Completeness check
  // -----------------------------------------------------------------------
  const missing = [...catalogRadarFilenames].filter((fn) => !(fn in merged));
  if (missing.length > 0) {
    console.error(`ERROR: ${missing.length} WX_US_RAD filename(s) have no bbox entry:`);
    for (const fn of missing) console.error(`  ${fn}`);
    process.exit(1);
  }

  // -----------------------------------------------------------------------
  // Nesting verification: PSND < NWWA < PNW, Seattle inside all three
  // -----------------------------------------------------------------------
  const bboxArea = (b: Bbox): number => (b[2] - b[0]) * (b[3] - b[1]);
  const bboxContains = (b: Bbox, lon: number, lat: number): boolean =>
    lon >= b[0] && lon <= b[2] && lat >= b[1] && lat <= b[3];
  const seattleLon = -122.2917;
  const seattleLat = 47.6042;

  const psndEntry = merged['US.RAD.PSND'];
  const nwwaEntry = merged['US.RAD.NWWA'];
  const pnwEntry = merged['US.RAD.PNW'];

  if (!psndEntry || !nwwaEntry || !pnwEntry) {
    console.error('ERROR: PSND / NWWA / PNW entries missing — cannot verify nesting');
    process.exit(1);
  }

  const psndArea = bboxArea(psndEntry.bbox);
  const nwwaArea = bboxArea(nwwaEntry.bbox);
  const pnwArea = bboxArea(pnwEntry.bbox);

  if (!(psndArea < nwwaArea && nwwaArea < pnwArea)) {
    console.error(`ERROR: nesting area ordering violated: PSND=${psndArea.toFixed(2)} NWWA=${nwwaArea.toFixed(2)} PNW=${pnwArea.toFixed(2)}`);
    process.exit(1);
  }

  const seattleInAll =
    bboxContains(psndEntry.bbox, seattleLon, seattleLat) &&
    bboxContains(nwwaEntry.bbox, seattleLon, seattleLat) &&
    bboxContains(pnwEntry.bbox, seattleLon, seattleLat);

  if (!seattleInAll) {
    console.error('ERROR: Seattle (47.6042, -122.2917) is not contained in all three of PSND/NWWA/PNW');
    process.exit(1);
  }

  console.log(`Nesting OK: PSND(${psndArea.toFixed(2)}) < NWWA(${nwwaArea.toFixed(2)}) < PNW(${pnwArea.toFixed(2)}); Seattle in all three ✓`);

  // -----------------------------------------------------------------------
  // Emit
  // -----------------------------------------------------------------------
  const regions = [...catalogRadarFilenames]
    .sort()
    .map((fn) => ({ filename: fn, name: merged[fn].name, bbox: merged[fn].bbox }));

  const derivedCount = regions.filter((r) => !(r.filename in RADAR_OVERRIDES)).length;
  const overrideCount = regions.filter((r) => r.filename in RADAR_OVERRIDES).length;

  const output = {
    _source:
      'Derived from us-states.geo.json state extents + region-name direction parsing; metro/feature regions hand-curated (see scripts/build-request-geo.ts radar section). bbox = [west, south, east, north] decimal degrees.',
    regions,
  };

  writeFileSync(RADAR_REGIONS_PATH, JSON.stringify(output, null, 2));

  console.log(`\nEmitted: ${RADAR_REGIONS_PATH}`);
  console.log(`Total regions: ${regions.length} (derived: ${derivedCount}, overrides: ${overrideCount})`);

  // Spot-check sample
  const samples = ['US.RAD.PSND', 'US.RAD.NWWA', 'US.RAD.PNW', 'US.RAD.AZ', 'US.RAD.IL', 'US.RAD.CONUS', 'US.RAD.GUAM', 'US.RAD.PR'];
  console.log('\nSpot-checks:');
  const byFn = new Map(regions.map((r) => [r.filename, r]));
  for (const fn of samples) {
    const r = byFn.get(fn);
    if (r) {
      console.log(`  ${fn} → "${r.name}" → [${r.bbox.join(', ')}]`);
    } else {
      console.warn(`  ${fn} → NOT FOUND`);
    }
  }
}

async function main(): Promise<void> {
  if (RADAR) {
    buildRadarRegions();
    return;
  }

  if (PRUNE_GEOMETRY) {
    pruneGeometry();
    return;
  }

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

  if (!MATCH_ONLY) {
    await fetchGeometryAndEmit();
  }

  await buildZoneCatalogMap();
}

main().catch((err: unknown) => {
  console.error('build-request-geo failed:', err);
  process.exit(1);
});
