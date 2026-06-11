#!/usr/bin/env tsx
// Build the geo data pipeline for the location-aware Request Center hero.
//
// Task 1 (this script's --fetch-only path): fetch + cache NWS public forecast
// zone GeoJSON per US state derived from the winlink-queries catalog.
//
// Later tasks add geometry simplification (--simplify) and the zone→catalog
// map (default full-pipeline mode).

import { readFileSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const RAW_CACHE_DIR = resolve(REPO_ROOT, 'dev/scratch/request-geo/raw');
const CATALOG_PATH = resolve(
  REPO_ROOT,
  'src-tauri/resources/catalog/winlink-queries.txt',
);

const UA = 'tuxlink-dev (cameronzucker@gmail.com)';

// ---------------------------------------------------------------------------
// CLI flags
// ---------------------------------------------------------------------------
const FETCH_ONLY = process.argv.includes('--fetch-only');
const FORCE = process.argv.includes('--force');

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
const INTER_REQUEST_DELAY_MS = 200;

async function sleep(ms: number): Promise<void> {
  await new Promise((r) => setTimeout(r, ms));
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const states = catalogStates();
  console.log(
    `Catalog state set: ${states.length} states with zone-forecast entries`,
  );
  console.log(`States: ${states.join(', ')}`);

  if (!FETCH_ONLY) {
    // Future tasks will add simplify + map steps here; for now, require the flag.
    console.error(
      'Full pipeline not yet implemented. Run with --fetch-only for Task 1.',
    );
    process.exit(1);
  }

  // Ensure cache dir exists
  mkdirSync(RAW_CACHE_DIR, { recursive: true });

  let fetched = 0;
  let skipped = 0;
  let maxEffectiveDate = '';

  for (const st of states) {
    const outPath = resolve(RAW_CACHE_DIR, `${st}.json`);

    if (!FORCE && existsSync(outPath)) {
      console.log(`  [${st}] cached — skip (use --force to re-fetch)`);
      // Still track effectiveDate from cached file for accurate logging
      try {
        const cached = JSON.parse(readFileSync(outPath, 'utf8')) as {
          features?: Array<{ properties?: { effectiveDate?: string } }>;
        };
        for (const feat of cached.features ?? []) {
          const ed = feat.properties?.effectiveDate ?? '';
          if (ed > maxEffectiveDate) maxEffectiveDate = ed;
        }
      } catch {
        // Non-fatal — cached file may be malformed; leave maxEffectiveDate
      }
      skipped++;
      continue;
    }

    console.log(`  [${st}] fetching…`);
    const data = (await fetchState(st)) as {
      features?: Array<{ properties?: { effectiveDate?: string } }>;
    };

    // Track max effectiveDate
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

main().catch((err: unknown) => {
  console.error('build-request-geo failed:', err);
  process.exit(1);
});
