#!/usr/bin/env tsx
/**
 * build-weather-map.ts — v2 nationwide NWS-zone → Winlink-catalog weather mapping
 * (tuxlink-z1b7). Audit trail: scripts/build-weather-map.md.
 *
 * Replaces the 8-state per-zone mapping with all-state coverage by keying each
 * zone on its NWS forecast office (CWA) and mapping the office to its catalog
 * product. The self-adrev (dev/adversarial/2026-06-11-cwa-weather-resolver-selfadrev.md)
 * showed the catalog is office-organized but NOT 1:1 with CWAs, so:
 *
 *   zone -> product resolution order:
 *     1. EXACT zone-name match preserved from the committed map (keeps the 8 fine
 *        states' per-zone precision exact, e.g. Seattle WAZ -> WA_ZON_SEA).
 *     2. office (CWA) -> office-wide product, matched by the office CITY appearing
 *        in a product description (PSR "NWS Phoenix" -> AZ_TAB_PHOE).
 *     3. unmapped -> no primary card; the always-on "Browse all <ST>" carries it.
 *
 * Inputs (all already on disk; no network):
 *   dev/scratch/request-geo/raw/<ST>.json   bulk zones w/ geometry + cwa (4024)
 *   dev/scratch/cwa-offices.json            cwa -> office name (121)
 *   src-tauri/resources/catalog/winlink-queries.txt
 *   src/request/nws-zone-to-catalog.json    existing 8-state exact map (preserved)
 *
 * Outputs:
 *   src/request/nws-zones.geo.json          ALL zones, simplified (replaces 216)
 *   src/request/nws-zone-to-catalog.json    { map, unmapped, noLandForecast }
 *
 * Usage: pnpm tsx scripts/build-weather-map.ts [--write]
 *   (without --write: dry-run report only; with --write: emit the JSON files)
 */
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const RAW_DIR = resolve(ROOT, 'dev/scratch/request-geo/raw');
const OFFICES = resolve(ROOT, 'dev/scratch/cwa-offices.json');
const CATALOG = resolve(ROOT, 'src-tauri/resources/catalog/winlink-queries.txt');
const ZONE_MAP_PATH = resolve(ROOT, 'src/request/nws-zone-to-catalog.json');
const GEO_PATH = resolve(ROOT, 'src/request/nws-zones.geo.json');
const WRITE = process.argv.includes('--write');

// Territories whose catalog products are all marine/discussion (no land forecast).
const NO_LAND_FORECAST = ['PR', 'VI', 'GU', 'AS', 'MP'];

// ---- catalog products per state -------------------------------------------
interface Product { filename: string; desc: string; type: 'ZON' | 'TAB' | 'FOR' | 'OTHER'; }
function productType(fn: string): Product['type'] {
  if (/_ZON(_|$)/.test(fn)) return 'ZON';
  if (/_TAB(_|$)/.test(fn)) return 'TAB';
  if (/_FOR(_|$)/.test(fn)) return 'FOR';
  return 'OTHER';
}
function loadCatalog(): Map<string, Product[]> {
  const raw = readFileSync(CATALOG, 'utf8');
  const text = raw.startsWith('﻿') ? raw.slice(1) : raw;
  const byState = new Map<string, Product[]>();
  for (const line of text.split('\n')) {
    const p = line.trim().split('|');
    if (p.length < 3) continue;
    const m = /^WX_US_([A-Z]{2})$/.exec(p[0]);
    if (!m) continue;
    const st = m[1];
    if (!byState.has(st)) byState.set(st, []);
    byState.get(st)!.push({ filename: p[1], desc: p[2], type: productType(p[1]) });
  }
  return byState;
}

// ---- office city tokens ----------------------------------------------------
const STATE_NAMES = /\b(alabama|alaska|arizona|arkansas|california|colorado|connecticut|delaware|florida|georgia|hawaii|idaho|illinois|indiana|iowa|kansas|kentucky|louisiana|maine|maryland|massachusetts|michigan|minnesota|mississippi|missouri|montana|nebraska|nevada|new hampshire|new jersey|new mexico|new york|north carolina|north dakota|ohio|oklahoma|oregon|pennsylvania|rhode island|south carolina|south dakota|tennessee|texas|utah|vermont|virginia|washington|west virginia|wisconsin|wyoming)\b/g;
/** Extract lower-cased city keywords from an office name like
 *  "NWS Flagstaff, AZ" / "Fort Worth/Dallas, TX" / "Miami - South Florida". */
function cityTokens(officeName: string): string[] {
  let s = officeName.toLowerCase().replace(/\bnws\b/g, ' ');
  s = s.replace(/,\s*[a-z]{2}\b/g, ' '); // drop ", AZ"
  const parts = s.split(/[/\-]/).map((x) => x.trim()).filter(Boolean);
  const toks = new Set<string>();
  for (const part of parts) {
    const clean = part.replace(STATE_NAMES, ' ').replace(/[^a-z ]/g, ' ').replace(/\s+/g, ' ').trim();
    if (clean.length >= 3) toks.add(clean);
  }
  return [...toks];
}

// ---- main ------------------------------------------------------------------
const offices = JSON.parse(readFileSync(OFFICES, 'utf8')) as Record<string, { name: string }>;
const catalog = loadCatalog();
const existing = JSON.parse(readFileSync(ZONE_MAP_PATH, 'utf8')) as { map: Record<string, string> };
const existingMap = existing.map ?? {};

interface RawZone { id: string; name: string; state: string; cwa: string; geometry: unknown; }
const GEOM_DIR = resolve(ROOT, 'dev/scratch/request-geo/geom');
function loadGeom(id: string): unknown {
  // The bulk /zones list returns geometry:null; per-zone /zones/forecast/<id>
  // (cached here by fetch-geom.py) carries the real Polygon/MultiPolygon.
  const p = resolve(GEOM_DIR, `${id}.json`);
  try { return (JSON.parse(readFileSync(p, 'utf8')) as { geometry: unknown }).geometry; }
  catch { return null; }
}
function loadZones(): RawZone[] {
  const zones: RawZone[] = [];
  for (const f of readdirSync(RAW_DIR).filter((x) => x.endsWith('.json'))) {
    const st = f.slice(0, -5);
    const fc = JSON.parse(readFileSync(resolve(RAW_DIR, f), 'utf8')) as {
      features: Array<{ properties: { id: string; name: string; cwa?: string | string[] } }>;
    };
    for (const ft of fc.features) {
      const c = ft.properties.cwa;
      const cwa = Array.isArray(c) ? c[0] ?? '' : c ?? '';
      zones.push({ id: ft.properties.id, name: ft.properties.name, state: st, cwa, geometry: loadGeom(ft.properties.id) });
    }
  }
  return zones;
}

const allZones = loadZones();

// ---- geometry: representative point per zone, centroid per cwa --------------
function firstPoint(geom: unknown): [number, number] | null {
  // Walk into the coordinate nesting until a [lon,lat] pair is found.
  let c: unknown = geom && typeof geom === 'object' ? (geom as { coordinates?: unknown }).coordinates : null;
  while (Array.isArray(c) && Array.isArray(c[0])) c = c[0];
  if (Array.isArray(c) && typeof c[0] === 'number' && typeof c[1] === 'number') return [c[0], c[1]];
  return null;
}
// state bbox + per-cwa centroid (mean of zone representative points)
const stateBox = new Map<string, { minLon: number; maxLon: number; minLat: number; maxLat: number }>();
const cwaPts = new Map<string, [number, number][]>();
for (const z of allZones) {
  const pt = firstPoint(z.geometry);
  if (!pt) continue;
  const b = stateBox.get(z.state) ?? { minLon: 1e9, maxLon: -1e9, minLat: 1e9, maxLat: -1e9 };
  b.minLon = Math.min(b.minLon, pt[0]); b.maxLon = Math.max(b.maxLon, pt[0]);
  b.minLat = Math.min(b.minLat, pt[1]); b.maxLat = Math.max(b.maxLat, pt[1]);
  stateBox.set(z.state, b);
  const k = `${z.state}:${z.cwa}`;
  if (!cwaPts.has(k)) cwaPts.set(k, []);
  cwaPts.get(k)!.push(pt);
}
/** Office position within its state as fractional (fx,fy) in [0,1], or null. */
function cwaFrac(state: string, cwa: string): [number, number] | null {
  const pts = cwaPts.get(`${state}:${cwa}`);
  const b = stateBox.get(state);
  if (!pts || !pts.length || !b) return null;
  const lon = pts.reduce((s, p) => s + p[0], 0) / pts.length;
  const lat = pts.reduce((s, p) => s + p[1], 0) / pts.length;
  const fx = (b.maxLon - b.minLon) > 0 ? (lon - b.minLon) / (b.maxLon - b.minLon) : 0.5;
  const fy = (b.maxLat - b.minLat) > 0 ? (lat - b.minLat) / (b.maxLat - b.minLat) : 0.5;
  return [fx, fy];
}
/** Target position a product's direction words denote, or null if non-directional.
 *  fx: 0=west 1=east; fy: 0=south 1=north. Compound words (southeast) combine. */
function productTarget(desc: string): [number, number] | null {
  const d = ` ${desc.toLowerCase()} `;
  let tx: number | null = null;
  let ty: number | null = null;
  if (/\bsoutheast|\bsouth east/.test(d)) { tx = 1; ty = 0; }
  else if (/\bsouthwest|\bsouth west/.test(d)) { tx = 0; ty = 0; }
  else if (/\bnortheast|\bnorth east/.test(d)) { tx = 1; ty = 1; }
  else if (/\bnorthwest|\bnorth west/.test(d)) { tx = 0; ty = 1; }
  else {
    if (/\beastern\b|\beast\b/.test(d)) tx = 1;
    else if (/\bwestern\b|\bwest\b/.test(d)) tx = 0;
    if (/\bnorthern\b|\bnorth\b/.test(d)) ty = 1;
    else if (/\bsouthern\b|\bsouth\b/.test(d)) ty = 0;
    if (/\bcentral\b/.test(d)) { if (tx === null) tx = 0.5; if (ty === null) ty = 0.5; }
  }
  if (tx === null && ty === null) return null;
  return [tx ?? 0.5, ty ?? 0.5];
}

// Per state: office(cwa) -> office-wide product. City match first, then direction.
const cwaProduct = new Map<string, string>(); // `${state}:${cwa}` -> filename
const typeRank: Record<Product['type'], number> = { ZON: 3, TAB: 2, FOR: 1, OTHER: 0 };
for (const [st, prods] of catalog) {
  if (NO_LAND_FORECAST.includes(st)) continue;
  const cwasInState = new Set(allZones.filter((z) => z.state === st).map((z) => z.cwa).filter(Boolean));
  for (const cwa of cwasInState) {
    const toks = cityTokens(offices[cwa]?.name ?? '');
    let best: Product | null = null;
    // Pass 1 — office city named in the description.
    for (const p of prods) {
      if (!toks.some((t) => p.desc.toLowerCase().includes(t))) continue;
      if (!best || typeRank[p.type] > typeRank[best.type]) best = p;
    }
    // Pass 2 — pick the directional product whose denoted position best aligns
    // with the office's actual position within the state (continuous, no cliffs).
    if (!best) {
      const frac = cwaFrac(st, cwa);
      if (frac) {
        let bestDist = Infinity;
        for (const p of prods) {
          const tgt = productTarget(p.desc);
          if (!tgt) continue;
          const dist = (frac[0] - tgt[0]) ** 2 + (frac[1] - tgt[1]) ** 2;
          // tie-break toward the more local product type (ZON>TAB>FOR)
          const score = dist - typeRank[p.type] * 1e-3;
          if (score < bestDist) { bestDist = score; best = p; }
        }
      }
    }
    if (best) cwaProduct.set(`${st}:${cwa}`, best.filename);
  }
}

// Per zone: exact-name(existing) -> cwa product -> unmapped.
const map: Record<string, string> = {};
const unmapped: Record<string, string> = {};
for (const z of allZones) {
  if (existingMap[z.id]) { map[z.id] = existingMap[z.id]; continue; }     // preserve 8-state exact
  if (NO_LAND_FORECAST.includes(z.state)) { unmapped[z.id] = 'no land forecast'; continue; }
  const viaCwa = cwaProduct.get(`${z.state}:${z.cwa}`);
  if (viaCwa) { map[z.id] = viaCwa; continue; }
  unmapped[z.id] = `no office product (cwa=${z.cwa || 'none'})`;
}

// ---- report ----------------------------------------------------------------
const mappedCount = Object.keys(map).length;
const statesCovered = new Set(allZones.filter((z) => map[z.id]).map((z) => z.state));
console.log(`zones=${allZones.length} mapped=${mappedCount} unmapped=${Object.keys(unmapped).length}`);
console.log(`states with >=1 mapped zone: ${statesCovered.size}`);
// Spot-checks
function check(zid: string, want: string) {
  const got = map[zid] ?? `UNMAPPED(${unmapped[zid] ?? '?'})`;
  console.log(`  ${zid} -> ${got}  ${got === want ? 'OK' : `EXPECTED ${want}`}`);
}
console.log('spot-checks:');
check('AZZ543', 'AZ_TAB_PHOE'); // Central Phoenix (PSR)
check('AZZ004', 'AZ_ZON_NOFLA'); // Kaibab Plateau (FGZ)
check('AZZ504', 'AZ_ZON_SE'); // Tucson metro (TWC)
check('WAZ315', 'WA_ZON_SEA'); // City of Seattle (exact-name preserved)

if (!WRITE) { console.log('\n(dry-run; pass --write to emit JSON)'); process.exit(0); }

// ---- emit (geometry simplified by 4-decimal rounding) ----------------------
function round(coords: unknown): unknown {
  if (typeof coords === 'number') return Math.round(coords * 1e4) / 1e4;
  if (Array.isArray(coords)) return coords.map(round);
  return coords;
}
const features = allZones.map((z) => ({
  type: 'Feature',
  properties: { id: z.id, name: z.name, state: z.state, cwa: z.cwa },
  geometry: round(z.geometry),
}));
writeFileSync(GEO_PATH, JSON.stringify({ type: 'FeatureCollection', features }));
const sortKeys = (o: Record<string, string>) =>
  Object.fromEntries(Object.keys(o).sort().map((k) => [k, o[k]]));
writeFileSync(
  ZONE_MAP_PATH,
  JSON.stringify({ map: sortKeys(map), unmapped: sortKeys(unmapped), noLandForecast: NO_LAND_FORECAST }, null, 2),
);
console.log(`\nWROTE ${GEO_PATH} (${features.length} features) + ${ZONE_MAP_PATH}`);
