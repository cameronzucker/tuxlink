// APRS symbol-table lookup (RX render).
//
// Maps the two-character APRS symbol identifier — a TABLE char and a CODE char —
// to a human name and a representative glyph for the Tac Chat map and legend.
//
// Sources (the project's prior-art-is-ground-truth rule): the base 94+94
// primary/alternate tables come from aprs.org/symbols/symbolsX.txt; overlay
// semantics (a non-"/"/"\\" table char drawn over an ALTERNATE symbol) come from
// aprs.org/symbols/symbols-new.txt. Names are clean human forms grounded in
// those tables; glyphs are a curated emoji set (no canonical per-symbol emoji
// exists — a bespoke sprite sheet is a documented fast-follow).
//
// RF-honesty: this only renders the symbol a station actually transmitted; it
// never infers or fabricates identity.

export interface AprsSymbol {
  /** Human-readable symbol name. */
  name: string;
  /** A representative emoji glyph for map/legend rendering. */
  glyph: string;
  /**
   * The overlay character drawn over an alternate-table symbol (APRS allows any
   * `0-9`/`A-Z` table char as an overlay), or `null` for the plain primary/
   * alternate tables. The UI can render this char on top of the base glyph.
   */
  overlay: string | null;
}

/** Every printable APRS symbol CODE char, `!` (0x21) … `~` (0x7E) — 94 in all. */
export const SYMBOL_CODES =
  '!"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~';

type SymbolEntry = { name: string; glyph: string };

/** Primary symbol table (TABLE char `/`) — mostly stations and vehicles. */
export const PRIMARY_SYMBOLS: Record<string, SymbolEntry> = {
  '!': { name: 'Police/Sheriff', glyph: '👮' },
  '"': { name: 'Reserved', glyph: '📍' },
  '#': { name: 'Digipeater', glyph: '📡' },
  '$': { name: 'Phone', glyph: '☎️' },
  '%': { name: 'DX cluster', glyph: '🌐' },
  '&': { name: 'HF gateway', glyph: '🌉' },
  '\'': { name: 'Small aircraft', glyph: '🛩️' },
  '(': { name: 'Mobile satellite station', glyph: '🛰️' },
  ')': { name: 'Wheelchair', glyph: '♿' },
  '*': { name: 'Snowmobile', glyph: '🛷' },
  '+': { name: 'Red Cross', glyph: '✚' },
  ',': { name: 'Boy Scouts', glyph: '⚜️' },
  '-': { name: 'House', glyph: '🏠' },
  '.': { name: 'X', glyph: '❌' },
  '/': { name: 'Red dot', glyph: '🔴' },
  '0': { name: 'Numbered circle', glyph: '⓿' },
  '1': { name: 'Numbered circle', glyph: '①' },
  '2': { name: 'Numbered circle', glyph: '②' },
  '3': { name: 'Numbered circle', glyph: '③' },
  '4': { name: 'Numbered circle', glyph: '④' },
  '5': { name: 'Numbered circle', glyph: '⑤' },
  '6': { name: 'Numbered circle', glyph: '⑥' },
  '7': { name: 'Numbered circle', glyph: '⑦' },
  '8': { name: 'Numbered circle', glyph: '⑧' },
  '9': { name: 'Numbered circle', glyph: '⑨' },
  ':': { name: 'Fire', glyph: '🔥' },
  ';': { name: 'Campground', glyph: '⛺' },
  '<': { name: 'Motorcycle', glyph: '🏍️' },
  '=': { name: 'Railroad engine', glyph: '🚂' },
  '>': { name: 'Car', glyph: '🚗' },
  '?': { name: 'File server', glyph: '🖥️' },
  '@': { name: 'Hurricane forecast', glyph: '🌀' },
  'A': { name: 'Aid station', glyph: '⛑️' },
  'B': { name: 'BBS', glyph: '💬' },
  'C': { name: 'Canoe', glyph: '🛶' },
  'D': { name: 'Depot', glyph: '🚉' },
  'E': { name: 'Eyeball', glyph: '👁️' },
  'F': { name: 'Farm vehicle', glyph: '🚜' },
  'G': { name: 'Grid square', glyph: '🔲' },
  'H': { name: 'Hotel', glyph: '🏨' },
  'I': { name: 'TCP/IP station', glyph: '🌐' },
  'J': { name: 'Undefined', glyph: '📍' },
  'K': { name: 'School', glyph: '🏫' },
  'L': { name: 'PC user', glyph: '💻' },
  'M': { name: 'MacAPRS', glyph: '🖥️' },
  'N': { name: 'NTS station', glyph: '📮' },
  'O': { name: 'Balloon', glyph: '🎈' },
  'P': { name: 'Police', glyph: '👮' },
  'Q': { name: 'Undefined', glyph: '📍' },
  'R': { name: 'Recreational vehicle', glyph: '🚐' },
  'S': { name: 'Space shuttle', glyph: '🚀' },
  'T': { name: 'SSTV', glyph: '📺' },
  'U': { name: 'Bus', glyph: '🚌' },
  'V': { name: 'ATV', glyph: '📺' },
  'W': { name: 'Weather service site', glyph: '🌦️' },
  'X': { name: 'Helicopter', glyph: '🚁' },
  'Y': { name: 'Yacht', glyph: '⛵' },
  'Z': { name: 'WinAPRS', glyph: '🖥️' },
  '[': { name: 'Person', glyph: '🧍' },
  '\\': { name: 'DF triangle', glyph: '🔺' },
  ']': { name: 'Post office', glyph: '📬' },
  '^': { name: 'Large aircraft', glyph: '✈️' },
  '_': { name: 'Weather station', glyph: '🌡️' },
  '`': { name: 'Dish antenna', glyph: '📡' },
  'a': { name: 'Ambulance', glyph: '🚑' },
  'b': { name: 'Bicycle', glyph: '🚲' },
  'c': { name: 'Incident command post', glyph: '🚨' },
  'd': { name: 'Fire department', glyph: '🧯' },
  'e': { name: 'Horse', glyph: '🐎' },
  'f': { name: 'Fire truck', glyph: '🚒' },
  'g': { name: 'Glider', glyph: '🛩️' },
  'h': { name: 'Hospital', glyph: '🏥' },
  'i': { name: 'Islands on the air', glyph: '🏝️' },
  'j': { name: 'Jeep', glyph: '🚙' },
  'k': { name: 'Truck', glyph: '🛻' },
  'l': { name: 'Laptop', glyph: '💻' },
  'm': { name: 'Mic-E repeater', glyph: '📻' },
  'n': { name: 'Node', glyph: '🔘' },
  'o': { name: 'EOC', glyph: '🏛️' },
  'p': { name: 'Rover', glyph: '🐕' },
  'q': { name: 'Grid square', glyph: '🔲' },
  'r': { name: 'Repeater', glyph: '📻' },
  's': { name: 'Ship', glyph: '🚢' },
  't': { name: 'Truck stop', glyph: '🛑' },
  'u': { name: 'Semi truck', glyph: '🚛' },
  'v': { name: 'Van', glyph: '🚐' },
  'w': { name: 'Water station', glyph: '🚰' },
  'x': { name: 'Unix station', glyph: '💠' },
  'y': { name: 'Yagi at QTH', glyph: '📡' },
  'z': { name: 'Undefined', glyph: '📍' },
  '{': { name: 'Undefined', glyph: '📍' },
  '|': { name: 'Reserved', glyph: '📍' },
  '}': { name: 'Undefined', glyph: '📍' },
  '~': { name: 'Reserved', glyph: '📍' },
};

/** Alternate symbol table (TABLE char `\\`) — mostly objects and weather. */
export const ALTERNATE_SYMBOLS: Record<string, SymbolEntry> = {
  '!': { name: 'Emergency', glyph: '🆘' },
  '"': { name: 'Reserved', glyph: '📍' },
  '#': { name: 'Overlay digipeater', glyph: '📡' },
  '$': { name: 'Bank/ATM', glyph: '🏧' },
  '%': { name: 'Power plant', glyph: '⚡' },
  '&': { name: 'Igate', glyph: '🌐' },
  '\'': { name: 'Crash/incident site', glyph: '💥' },
  '(': { name: 'Cloudy', glyph: '☁️' },
  ')': { name: 'MODIS / Firenet', glyph: '🛰️' },
  '*': { name: 'Snow', glyph: '❄️' },
  '+': { name: 'Church', glyph: '⛪' },
  ',': { name: 'Girl Scouts', glyph: '⚜️' },
  '-': { name: 'House', glyph: '🏠' },
  '.': { name: 'Ambiguous', glyph: '❓' },
  '/': { name: 'Waypoint', glyph: '🚩' },
  '0': { name: 'Circle (VoIP)', glyph: '⭕' },
  '1': { name: 'Overlay circle', glyph: '⭕' },
  '2': { name: 'Overlay circle', glyph: '⭕' },
  '3': { name: 'Overlay circle', glyph: '⭕' },
  '4': { name: 'Overlay circle', glyph: '⭕' },
  '5': { name: 'Overlay circle', glyph: '⭕' },
  '6': { name: 'Overlay circle', glyph: '⭕' },
  '7': { name: 'Overlay circle', glyph: '⭕' },
  '8': { name: 'Network node', glyph: '🖧' },
  '9': { name: 'Overlay circle', glyph: '⭕' },
  ':': { name: 'Hail', glyph: '🧊' },
  ';': { name: 'Park/event', glyph: '🏞️' },
  '<': { name: 'Advisory', glyph: '🚩' },
  '=': { name: 'Railroad', glyph: '🚆' },
  '>': { name: 'Vehicle', glyph: '🚗' },
  '?': { name: 'Info kiosk', glyph: 'ℹ️' },
  '@': { name: 'Hurricane', glyph: '🌀' },
  'A': { name: 'DTMF box', glyph: '🔲' },
  'B': { name: 'Blowing snow', glyph: '🌬️' },
  'C': { name: 'Coast Guard', glyph: '⚓' },
  'D': { name: 'Depot', glyph: '🚉' },
  'E': { name: 'Smoke', glyph: '💨' },
  'F': { name: 'Freezing rain', glyph: '🌧️' },
  'G': { name: 'Snow shower', glyph: '🌨️' },
  'H': { name: 'Haze', glyph: '🌫️' },
  'I': { name: 'Rain shower', glyph: '🌦️' },
  'J': { name: 'Lightning', glyph: '🌩️' },
  'K': { name: 'Kenwood HT', glyph: '📻' },
  'L': { name: 'Lighthouse', glyph: '🗼' },
  'M': { name: 'MARS', glyph: '🎖️' },
  'N': { name: 'Navigation buoy', glyph: '🛟' },
  'O': { name: 'Rocket', glyph: '🚀' },
  'P': { name: 'Parking', glyph: '🅿️' },
  'Q': { name: 'Earthquake', glyph: '〰️' },
  'R': { name: 'Restaurant', glyph: '🍴' },
  'S': { name: 'Satellite', glyph: '🛰️' },
  'T': { name: 'Thunderstorm', glyph: '⛈️' },
  'U': { name: 'Sunny', glyph: '☀️' },
  'V': { name: 'VORTAC', glyph: '🧭' },
  'W': { name: 'NWS site', glyph: '🌐' },
  'X': { name: 'Pharmacy', glyph: '💊' },
  'Y': { name: 'Radio device', glyph: '📻' },
  'Z': { name: 'Undefined', glyph: '📍' },
  '[': { name: 'Wall cloud', glyph: '⛈️' },
  '\\': { name: 'GPS/navigation', glyph: '🛰️' },
  ']': { name: 'Undefined', glyph: '📍' },
  '^': { name: 'Aircraft', glyph: '✈️' },
  '_': { name: 'Weather site', glyph: '🌡️' },
  '`': { name: 'Rain', glyph: '🌧️' },
  'a': { name: 'ARRL/ARES/WinLink', glyph: '📡' },
  'b': { name: 'Blowing dust/sand', glyph: '🌬️' },
  'c': { name: 'RACES/SATERN triangle', glyph: '🔺' },
  'd': { name: 'DX spot', glyph: '📡' },
  'e': { name: 'Sleet', glyph: '🌨️' },
  'f': { name: 'Funnel cloud', glyph: '🌪️' },
  'g': { name: 'Gale warning', glyph: '🚩' },
  'h': { name: 'Store / hamfest', glyph: '🏪' },
  'i': { name: 'Point of interest', glyph: '📌' },
  'j': { name: 'Work zone', glyph: '🚧' },
  'k': { name: 'SUV', glyph: '🚙' },
  'l': { name: 'Area', glyph: '⬛' },
  'm': { name: 'Value sign', glyph: '🔢' },
  'n': { name: 'Triangle', glyph: '🔺' },
  'o': { name: 'Small circle', glyph: '⚪' },
  'p': { name: 'Partly cloudy', glyph: '⛅' },
  'q': { name: 'Undefined', glyph: '📍' },
  'r': { name: 'Restrooms', glyph: '🚻' },
  's': { name: 'Ship/boat', glyph: '🚢' },
  't': { name: 'Tornado', glyph: '🌪️' },
  'u': { name: 'Truck', glyph: '🚛' },
  'v': { name: 'Van', glyph: '🚐' },
  'w': { name: 'Flooding', glyph: '🌊' },
  'x': { name: 'Wreck/obstruction', glyph: '⚠️' },
  'y': { name: 'Skywarn', glyph: '🌪️' },
  'z': { name: 'Shelter', glyph: '🏠' },
  '{': { name: 'Fog', glyph: '🌫️' },
  '|': { name: 'TNC stream switch', glyph: '📍' },
  '}': { name: 'Undefined', glyph: '📍' },
  '~': { name: 'TNC stream switch', glyph: '📍' },
};

/**
 * Enriched meanings for common overlay+code combinations, keyed `"<overlay><code>"`.
 * When a combo is listed its specific meaning wins; otherwise the base alternate
 * symbol's identity is kept and the overlay char is surfaced separately.
 */
const OVERLAY_MEANINGS: Record<string, SymbolEntry> = {
  'Aa': { name: 'ARES', glyph: '📡' },
  'Da': { name: 'D-STAR', glyph: '📡' },
  'Ga': { name: 'RSGB', glyph: '📡' },
  'Ra': { name: 'RACES', glyph: '📡' },
  'Sa': { name: 'SATERN', glyph: '📡' },
  'Wa': { name: 'WinLink', glyph: '📡' },
  'Ya': { name: 'Yaesu C4FM repeater', glyph: '📡' },
  'I&': { name: 'Igate', glyph: '🌐' },
  'R&': { name: 'Receive-only Igate', glyph: '🌐' },
  'P&': { name: 'PSKmail node', glyph: '🌐' },
  'T&': { name: 'TX Igate (1 hop)', glyph: '🌐' },
  'W&': { name: 'WIRES-X', glyph: '🌐' },
  'L&': { name: 'LoRa Igate', glyph: '🌐' },
  '2&': { name: 'TX Igate (2 hop)', glyph: '🌐' },
  '1#': { name: 'WIDE1-1 digipeater', glyph: '📡' },
  'A#': { name: 'Alternate-input digipeater', glyph: '📡' },
  'I#': { name: 'Igate digipeater', glyph: '📡' },
  'L#': { name: 'Path-trapping digipeater', glyph: '📡' },
  'S#': { name: 'SSn-N digipeater', glyph: '📡' },
  'X#': { name: 'Experimental digipeater', glyph: '📡' },
  'W#': { name: 'WIDEn-N digipeater', glyph: '📡' },
  'B>': { name: 'Battery EV', glyph: '🚗' },
  'H>': { name: 'Hybrid car', glyph: '🚗' },
  'S>': { name: 'Solar car', glyph: '🚗' },
  'T>': { name: 'Tesla', glyph: '🚗' },
  'S-': { name: 'Solar house', glyph: '🏠' },
  'W-': { name: 'Wind-powered house', glyph: '🏠' },
  'B-': { name: 'Off-grid house', glyph: '🏠' },
  'E-': { name: 'Emergency-power house', glyph: '🏠' },
  'O-': { name: 'Operator present', glyph: '🏠' },
  'N%': { name: 'Nuclear plant', glyph: '⚡' },
  'S%': { name: 'Solar plant', glyph: '☀️' },
  'W%': { name: 'Wind plant', glyph: '⚡' },
  'H%': { name: 'Hydro plant', glyph: '⚡' },
  'C%': { name: 'Coal plant', glyph: '⚡' },
  'E!': { name: 'ELT/EPIRB', glyph: '🆘' },
  'V!': { name: 'Volcanic eruption', glyph: '🌋' },
};

const UNKNOWN: AprsSymbol = { name: 'Unknown', glyph: '📍', overlay: null };

function isPrintableCode(code: string): boolean {
  if (code.length !== 1) return false;
  const o = code.charCodeAt(0);
  return o >= 0x21 && o <= 0x7e;
}

/**
 * Resolve an APRS symbol from its TABLE char and CODE char.
 *
 * - `table === '/'` → primary table.
 * - `table === '\\'` → alternate table.
 * - any other single `0-9`/`A-Z` char → overlay: the ALTERNATE symbol for `code`
 *   with `table` as the overlay char (enriched where a common meaning is known).
 *
 * Malformed input yields a non-throwing `Unknown` sentinel so the map always has
 * something renderable.
 */
export function lookupAprsSymbol(table: string, code: string): AprsSymbol {
  if (!isPrintableCode(code)) return UNKNOWN;

  if (table === '/') {
    const e = PRIMARY_SYMBOLS[code];
    return e ? { ...e, overlay: null } : UNKNOWN;
  }
  if (table === '\\') {
    const e = ALTERNATE_SYMBOLS[code];
    return e ? { ...e, overlay: null } : UNKNOWN;
  }

  // Overlay: a single alphanumeric table char drawn over the alternate symbol.
  if (table.length === 1 && /[0-9A-Z]/.test(table)) {
    const enriched = OVERLAY_MEANINGS[table + code];
    if (enriched) return { ...enriched, overlay: table };
    const base = ALTERNATE_SYMBOLS[code];
    if (base) return { ...base, overlay: table };
  }

  return UNKNOWN;
}
