// weatherGlyph — maps an NWS SFT tabular-forecast condition code ("Vryhot",
// "Ptcldy", …) to a glyph descriptor: an icon SHAPE (kind), a decoded
// plain-English label (tooltip + aria), and a sun ACCENT colour. The SVGs live
// in WeatherGlyph.tsx; this module is pure data + lookup so it is trivially
// testable. Anything not in the table resolves to null and the caller falls
// back to today's raw-text rendering — the grid never blanks on an unknown code.
//
// Scope: SFT tabular grid only. The ZFP zone product is free narrative text
// (no codes) and stays prose. SFT is daytime-only, so there are no night
// variants. Design: dev/scratch/2026-06-10-nws-weather-glyphs-mock.html (tuxlink-n6tp).

/// Sun colour for the sun-bearing shapes; also the heat semantics the legacy
/// `condClass` carried (vry → danger, hot → warn).
export type GlyphAccent = 'sun' | 'hot' | 'danger' | 'dim' | 'info';

/// Icon shape. Sunny/Hot/Vryhot share `sunny` and differ only by `accent`.
export type GlyphKind =
  | 'sunny'
  | 'mosunny'
  | 'ptcldy'
  | 'mocldy'
  | 'cloudy'
  | 'tstms'
  | 'rain'
  | 'showers'
  | 'drizzle'
  | 'snow'
  | 'frost'
  | 'windy'
  | 'fog'
  | 'haze'
  | 'dust'
  | 'smoke';

export interface WeatherGlyph {
  kind: GlyphKind;
  label: string; // decoded, plain-English ("Partly cloudy")
  accent: GlyphAccent;
}

const sun = (label: string, accent: GlyphAccent): WeatherGlyph => ({ kind: 'sunny', label, accent });
const g = (kind: GlyphKind, label: string, accent: GlyphAccent = 'dim'): WeatherGlyph => ({ kind, label, accent });

// Keyed by normalized code (lowercase, spaces + dots stripped). Aliases map the
// many NWS spellings (abbreviated + spelled-out) onto one descriptor. Grounded
// against the NWS SFT predominant-weather abbreviation set.
const TABLE: Record<string, WeatherGlyph> = {
  // clear → sun family
  sunny: sun('Sunny', 'sun'),
  clear: sun('Clear', 'sun'),
  fair: sun('Fair', 'sun'),
  mclear: sun('Mostly clear', 'sun'),
  hot: sun('Hot', 'hot'),
  vryhot: sun('Very hot', 'danger'),
  // sky-cover gradient
  mosunny: g('mosunny', 'Mostly sunny', 'sun'),
  mostlysunny: g('mosunny', 'Mostly sunny', 'sun'),
  ptsunny: g('ptcldy', 'Partly sunny'),
  partlysunny: g('ptcldy', 'Partly sunny'),
  ptcldy: g('ptcldy', 'Partly cloudy'),
  pcloudy: g('ptcldy', 'Partly cloudy'),
  partlycloudy: g('ptcldy', 'Partly cloudy'),
  mocldy: g('mocldy', 'Mostly cloudy'),
  mcloudy: g('mocldy', 'Mostly cloudy'),
  mostlycloudy: g('mocldy', 'Mostly cloudy'),
  cloudy: g('cloudy', 'Cloudy'),
  // thunder
  tstms: g('tstms', 'Thunderstorms', 'info'),
  tstrms: g('tstms', 'Thunderstorms', 'info'),
  scttstms: g('tstms', 'Scattered storms', 'info'),
  isotstms: g('tstms', 'Isolated storms', 'info'),
  // liquid precip
  rain: g('rain', 'Rain', 'info'),
  rnshwrs: g('showers', 'Rain showers', 'info'),
  showers: g('showers', 'Showers', 'info'),
  shwrs: g('showers', 'Showers', 'info'),
  drizzle: g('drizzle', 'Drizzle', 'info'),
  sprinkles: g('drizzle', 'Sprinkles', 'info'),
  frzrain: g('rain', 'Freezing rain', 'info'),
  frzdrzl: g('drizzle', 'Freezing drizzle', 'info'),
  // frozen precip
  snow: g('snow', 'Snow', 'info'),
  snoshwr: g('snow', 'Snow showers', 'info'),
  snowshwrs: g('snow', 'Snow showers', 'info'),
  flurries: g('snow', 'Flurries', 'info'),
  sleet: g('snow', 'Sleet', 'info'),
  rnsnow: g('snow', 'Rain and snow', 'info'),
  wintrymix: g('snow', 'Wintry mix', 'info'),
  blizzard: g('snow', 'Blizzard', 'info'),
  blgsnow: g('snow', 'Blowing snow', 'info'),
  blowingsnow: g('snow', 'Blowing snow', 'info'),
  frost: g('frost', 'Frost', 'info'),
  // wind / obstructions to visibility
  windy: g('windy', 'Windy'),
  breezy: g('windy', 'Breezy'),
  fog: g('fog', 'Fog'),
  patchyfog: g('fog', 'Patchy fog'),
  haze: g('haze', 'Haze'),
  hazy: g('haze', 'Haze'),
  smoke: g('smoke', 'Smoke'),
  dust: g('dust', 'Blowing dust'),
  blgdust: g('dust', 'Blowing dust'),
  blowingdust: g('dust', 'Blowing dust'),
};

/// Resolve an NWS condition code to its glyph, or null when unmapped (caller
/// falls back to raw text). Case-, whitespace- and dot-insensitive.
export function resolveGlyph(condition: string): WeatherGlyph | null {
  const key = condition.trim().toLowerCase().replace(/[\s.]+/g, '');
  if (!key) return null;
  return TABLE[key] ?? null;
}

/// Legacy heat-accent class for the fallback (unmapped) text path — parity with
/// the original `condClass` so unmapped codes still read vry=red / hot=orange.
export function conditionTextClass(condition: string): string {
  const c = condition.toLowerCase();
  if (c.startsWith('vry')) return 'cond vryhot';
  if (c.startsWith('hot')) return 'cond hot';
  return 'cond';
}
