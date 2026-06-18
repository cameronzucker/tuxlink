// src/aprs/aprsDecode.ts
//
// Frontend APRS info-field decoder for the chat-feed "monitor" line (bug #2,
// tuxlink-hzwc). The backend surfaces every non-message frame's raw info field
// in the feed (the tuxlink-8tz1 diagnostic). Raw it reads as gibberish
// (`@182019z3608.17N/11114.52W_311/004g014t097...`); mature clients like
// APRSIS-32 render a readable summary, so we do too.
//
// SCOPE: this decodes the INFO FIELD only — symbol, comment, weather run,
// status text, telemetry. Exact coordinates (and a Mic-E report's
// destination-encoded lat/lon) are NOT shown on the feed line: the map already
// plots the real fix (RF-honest, no duplicate/derived coordinate here). The feed
// line is for "what kind of traffic + what it says", not navigation.
//
// Pure + deterministic → unit-tested directly against real on-air captures.

import { lookupAprsSymbol } from './aprsSymbols';

/// Broad classification of a decoded frame, used by the feed for an icon + label.
export type AprsPacketCategory =
  | 'position'
  | 'weather'
  | 'telemetry'
  | 'status'
  | 'object'
  | 'item'
  | 'mice'
  | 'message'
  | 'unknown';

export interface DecodedPacket {
  category: AprsPacketCategory;
  /// A readable one-line summary for the feed row. Never empty.
  summary: string;
}

/// Strip a leading `ddd/sss` course/speed and any `/A=dddddd` altitude token from
/// a position comment so they don't leak into the readable text. PHG/RNG/DFS
/// radio-range tokens are left in place (they read acceptably as text).
function cleanComment(comment: string): string {
  let c = comment.replace(/^[0-9]{3}\/[0-9]{3}/, '');
  c = c.replace(/\/A=[0-9]{6}/, '');
  return c.trim();
}

/// Decode the APRS weather data run (after the symbol/timestamp) into a compact
/// present-only summary. Ham-conventional units (°F / mph / % / hPa / in).
function decodeWeather(run: string): string {
  const parts: string[] = [];
  const num = (re: RegExp): number | null => {
    const m = re.exec(run);
    return m ? Number(m[1]) : null;
  };
  const temp = num(/t(-?[0-9]{1,3})/);
  if (temp !== null) parts.push(`${temp}°F`);
  // Wind direction/speed is the leading `ddd/sss` of the run.
  const wind = /^([0-9]{3})\/([0-9]{3})/.exec(run);
  if (wind) parts.push(`wind ${Number(wind[2])} mph @${Number(wind[1])}°`);
  const gust = num(/g([0-9]{1,3})/);
  if (gust !== null) parts.push(`gust ${gust} mph`);
  let hum = num(/h([0-9]{2})/);
  if (hum !== null) {
    if (hum === 0) hum = 100; // APRS h00 == 100%
    parts.push(`hum ${hum}%`);
  }
  const baro = num(/b([0-9]{5})/);
  if (baro !== null) parts.push(`${(baro / 10).toFixed(1)} hPa`);
  const rain = num(/r([0-9]{3})/);
  if (rain !== null) parts.push(`rain ${(rain / 100).toFixed(2)} in`);
  return parts.length ? `Weather: ${parts.join(', ')}` : 'Weather report';
}

/// Locate the symbol (table+code) and comment in an uncompressed/compressed
/// position body (everything after the DTI and any timestamp). Returns null when
/// the body is too short to carry a position.
function positionFields(body: string): { table: string; code: string; comment: string } | null {
  const first = body[0];
  if (first === undefined) return null;
  if ((first >= '0' && first <= '9') || first === ' ') {
    // Uncompressed: 8-char lat, 1 table, 9-char lon, 1 code, then comment.
    if (body.length < 19) return null;
    return { table: body[8], code: body[18], comment: body.slice(19) };
  }
  // Compressed: 1 table, 4 lat, 4 lon, 1 code, 2 cs, 1 type, then comment.
  if (body.length < 13) return null;
  return { table: body[0], code: body[9], comment: body.slice(13) };
}

function decodePosition(body: string): DecodedPacket {
  const fields = positionFields(body);
  if (!fields) return { category: 'position', summary: 'Position report' };
  const { table, code } = fields;
  const sym = lookupAprsSymbol(table, code);
  const comment = cleanComment(fields.comment);
  // A weather-symbol position (`_`) carries a WX run in its comment.
  if (code === '_') {
    return { category: 'weather', summary: decodeWeather(fields.comment) };
  }
  const name = sym.name === 'Unknown' ? 'Position' : sym.name;
  return { category: 'position', summary: comment ? `${name}: ${comment}` : name };
}

function decodeMice(info: string): DecodedPacket {
  // Mic-E info: DTI + 3 lon + 3 speed/course + 1 symbol code + 1 symbol table id,
  // then status/telemetry. Coordinates live in the AX.25 destination (not here),
  // so the feed shows the symbol identity only — the map plots the real fix.
  if (info.length >= 9) {
    const sym = lookupAprsSymbol(info[8], info[7]);
    if (sym.name !== 'Unknown') return { category: 'mice', summary: `Mic-E position (${sym.name})` };
  }
  return { category: 'mice', summary: 'Mic-E position' };
}

function decodeObjectOrItem(info: string): DecodedPacket {
  if (info[0] === ';') {
    // Object: 9-char name + state flag + 7-char timestamp + position.
    const name = info.slice(1, 10).replace(/ +$/, '');
    const alive = info[10] !== '_';
    const tail = positionFields(info.slice(17));
    const comment = tail ? cleanComment(tail.comment) : '';
    const label = alive ? 'Object' : 'Object (killed)';
    return { category: 'object', summary: `${label} ${name}${comment ? `: ${comment}` : ''}`.trim() };
  }
  // Item: 3–9 char name terminated by `!` (live) or `_` (killed).
  const m = /^\)([\s\S]{3,9}?)[!_]/.exec(info);
  const name = m ? m[1] : '';
  return { category: 'item', summary: `Item ${name}`.trim() };
}

function decodeTelemetry(info: string): DecodedPacket {
  // T#SEQ,a1,a2,a3,a4,a5,bbbbbbbb
  const body = info.slice(2);
  const fields = body.split(',');
  const seqRaw = fields[0] ?? '';
  const seqNum = Number(seqRaw);
  const seq = Number.isFinite(seqNum) && seqRaw.trim() !== '' ? `seq ${seqNum}` : 'seq —';
  const analog = fields.slice(1, 6).filter((f) => f !== '');
  return {
    category: 'telemetry',
    summary: analog.length ? `Telemetry · ${seq} · ${analog.join(', ')}` : `Telemetry · ${seq}`,
  };
}

/**
 * Decode a raw APRS info field into a readable feed summary. Never throws; an
 * unrecognized frame falls back to its trimmed raw text (never empty).
 */
export function decodeAprsInfo(info: string): DecodedPacket {
  const dti = info[0];
  switch (dti) {
    case '!':
    case '=':
      return decodePosition(info.slice(1));
    case '/':
    case '@':
      // 7-char timestamp follows the DTI on these forms.
      return decodePosition(info.slice(8));
    case '`':
    case "'":
      return decodeMice(info);
    case '>':
      return { category: 'status', summary: info.slice(1).trim() || 'status' };
    case ';':
    case ')':
      return decodeObjectOrItem(info);
    case '_':
      // Positionless weather: DTI + 8-char MDHM timestamp + WX run.
      return { category: 'weather', summary: decodeWeather(info.slice(9)) };
    case ':':
      return { category: 'message', summary: info.slice(11).replace(/\{.*$/, '').trim() || 'message' };
    default:
      if (info.startsWith('T#')) return decodeTelemetry(info);
      return { category: 'unknown', summary: info.trim() || '(empty)' };
  }
}
