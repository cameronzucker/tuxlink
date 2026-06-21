// src/aprs/stationBuckets.ts
//
// Curated APRS symbol → station-category bucket classifier (tuxlink-8fjx). The
// symbol space is finite and named in aprsSymbols.ts, so the buckets are authored
// by hand as an explicit data table rather than fuzzy heuristics. Precedence
// between buckets is resolved at AUTHORING time — each symbol (and each known
// overlay combo) is assigned its single correct bucket directly — so there is no
// runtime priority ladder. The only runtime rule is the weather-readings
// override. RF-honesty: an unmatched symbol returns `other` (visible), never
// dropped.
//
// Resolution order mirrors lookupAprsSymbol(): overlay combo → primary ('/') →
// alternate ('\\') → overlay-base (alternate) → other.

export type BucketKey =
  | 'weather'
  | 'igate'
  | 'digipeater'
  | 'emergency'
  | 'vehicles'
  | 'people'
  | 'fixed'
  | 'other';

export interface BucketMeta {
  key: BucketKey;
  label: string;
  glyph: string;
}

/** Display order for the layers panel. */
export const BUCKETS: BucketMeta[] = [
  { key: 'weather', label: 'Weather', glyph: '🌡️' },
  { key: 'igate', label: 'iGates & Gateways', glyph: '🌐' },
  { key: 'digipeater', label: 'Digipeaters & Nodes', glyph: '📡' },
  { key: 'emergency', label: 'Emergency / EmComm', glyph: '🚑' },
  { key: 'vehicles', label: 'Vehicles & Craft', glyph: '🚗' },
  { key: 'people', label: 'People', glyph: '🧍' },
  { key: 'fixed', label: 'Fixed & Places', glyph: '🏠' },
  { key: 'other', label: 'Other', glyph: '▫️' },
];

export const ALL_BUCKET_KEYS: BucketKey[] = BUCKETS.map((m) => m.key);

export function emptyCounts(): Record<BucketKey, number> {
  return {
    weather: 0, igate: 0, digipeater: 0, emergency: 0,
    vehicles: 0, people: 0, fixed: 0, other: 0,
  };
}

export interface StationBucketCtx {
  symbolTable: string;
  symbolCode: string;
  isWeather: boolean;
}

// Primary table ('/') code → bucket. Absent codes → other.
const PRIMARY_BUCKET: Record<string, BucketKey> = {
  '!': 'emergency', // police/sheriff
  '#': 'digipeater',
  '&': 'igate',     // HF gateway
  "'": 'vehicles',  // small aircraft
  ')': 'people',    // wheelchair
  '*': 'vehicles',  // snowmobile
  '+': 'emergency', // red cross
  '-': 'fixed',     // house
  ':': 'emergency', // fire
  ';': 'fixed',     // campground
  '<': 'vehicles',  // motorcycle
  '=': 'vehicles',  // railroad engine
  '>': 'vehicles',  // car
  '@': 'weather',   // hurricane forecast
  'A': 'emergency', // aid station
  'C': 'vehicles',  // canoe
  'D': 'fixed',     // depot
  'F': 'vehicles',  // farm vehicle
  'H': 'fixed',     // hotel
  'I': 'igate',     // TCP/IP station
  'K': 'fixed',     // school
  'O': 'vehicles',  // balloon
  'P': 'emergency', // police
  'R': 'vehicles',  // RV
  'U': 'vehicles',  // bus
  'V': 'vehicles',  // ATV
  'W': 'weather',   // weather service site
  'X': 'vehicles',  // helicopter
  'Y': 'vehicles',  // yacht
  '[': 'people',    // person
  '\\': 'emergency',// DF triangle
  ']': 'fixed',     // post office
  '^': 'vehicles',  // large aircraft
  '_': 'weather',   // weather station
  '`': 'fixed',     // dish antenna (QTH infrastructure)
  'a': 'emergency', // ambulance
  'b': 'people',    // bicycle
  'c': 'emergency', // incident command post
  'd': 'emergency', // fire department
  'e': 'people',    // horse/rider
  'f': 'emergency', // fire truck
  'g': 'vehicles',  // glider
  'h': 'fixed',     // hospital
  'j': 'vehicles',  // jeep
  'k': 'vehicles',  // truck
  'm': 'digipeater',// Mic-E repeater
  'n': 'digipeater',// node
  'o': 'emergency', // EOC
  'r': 'digipeater',// repeater
  's': 'vehicles',  // ship
  't': 'fixed',     // truck stop
  'u': 'vehicles',  // semi truck
  'v': 'vehicles',  // van
  'w': 'fixed',     // water station
  'y': 'fixed',     // Yagi at QTH
};

// Alternate table ('\\') code → bucket. Absent codes → other.
const ALTERNATE_BUCKET: Record<string, BucketKey> = {
  '!': 'emergency', // emergency
  '#': 'digipeater',// overlay digipeater
  '$': 'fixed',     // bank/ATM
  '%': 'fixed',     // power plant
  '&': 'igate',     // igate
  "'": 'emergency', // crash/incident site
  '(': 'weather',   // cloudy
  '*': 'weather',   // snow
  '+': 'fixed',     // church
  '-': 'fixed',     // house
  '8': 'digipeater',// network node
  ':': 'weather',   // hail
  ';': 'fixed',     // park/event
  '=': 'vehicles',  // railroad
  '>': 'vehicles',  // vehicle
  '?': 'fixed',     // info kiosk
  '@': 'weather',   // hurricane
  'B': 'weather',   // blowing snow
  'C': 'emergency', // coast guard
  'D': 'fixed',     // depot
  'E': 'weather',   // smoke
  'F': 'weather',   // freezing rain
  'G': 'weather',   // snow shower
  'H': 'weather',   // haze
  'I': 'weather',   // rain shower
  'J': 'weather',   // lightning
  'L': 'fixed',     // lighthouse
  'M': 'emergency', // MARS
  'P': 'fixed',     // parking
  'R': 'fixed',     // restaurant
  'T': 'weather',   // thunderstorm
  'U': 'weather',   // sunny
  'W': 'weather',   // NWS site
  'X': 'fixed',     // pharmacy
  '[': 'weather',   // wall cloud
  '^': 'vehicles',  // aircraft
  '_': 'weather',   // weather site
  '`': 'weather',   // rain
  'a': 'emergency', // ARRL/ARES/WinLink
  'b': 'weather',   // blowing dust/sand
  'c': 'emergency', // RACES/SATERN triangle
  'e': 'weather',   // sleet
  'f': 'weather',   // funnel cloud
  'g': 'weather',   // gale warning
  'h': 'fixed',     // store/hamfest
  'i': 'fixed',     // point of interest
  'j': 'fixed',     // work zone
  'k': 'vehicles',  // SUV
  'p': 'weather',   // partly cloudy
  'r': 'fixed',     // restrooms
  's': 'vehicles',  // ship/boat
  't': 'weather',   // tornado
  'u': 'vehicles',  // truck
  'v': 'vehicles',  // van
  'w': 'weather',   // flooding
  'x': 'emergency', // wreck/obstruction
  'y': 'weather',   // skywarn
  'z': 'emergency', // shelter (EmComm)
  '{': 'weather',   // fog
};

// Overlay combos ("<overlay><code>") whose bucket DIFFERS from the alternate-table
// base for that code. Combos not listed fall through to ALTERNATE_BUCKET[code].
// (e.g. I&, R&, W# already resolve correctly via the base; only D-STAR / C4FM
// repeaters drawn over the ARES 'a' symbol need an explicit digipeater override.)
const OVERLAY_BUCKET: Record<string, BucketKey> = {
  Da: 'digipeater', // D-STAR
  Ya: 'digipeater', // Yaesu C4FM repeater
};

function isOverlayChar(table: string): boolean {
  return table.length === 1 && /[0-9A-Z]/.test(table);
}

export function bucketForStation(ctx: StationBucketCtx): BucketKey {
  if (ctx.isWeather) return 'weather';

  const { symbolTable: table, symbolCode: code } = ctx;
  if (code.length !== 1) return 'other';

  if (table === '/') return PRIMARY_BUCKET[code] ?? 'other';
  if (table === '\\') return ALTERNATE_BUCKET[code] ?? 'other';

  if (isOverlayChar(table)) {
    const combo = OVERLAY_BUCKET[table + code];
    if (combo) return combo;
    return ALTERNATE_BUCKET[code] ?? 'other';
  }

  return 'other';
}
