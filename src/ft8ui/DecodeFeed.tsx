// DecodeFeed.tsx — chronological FT-8 decode list (Task C11, plan
// tuxlink-b026z.4 §Strip feed). This is the strip's raw, per-decode feed —
// distinct from `LiveDecodesTab`'s station-centric aggregation
// ("two questions, two surfaces", spec §Rail). D1 wires this to
// `useFt8Listener().decodesRing`; the ring is a plain prop here so the
// component stays presentational/testable in isolation (LiveBandStrip,
// Task C7, is the eventual host — not built yet).
//
// Cap discipline (spec §Rail "Untrusted-input hardening" (c)): the ring holds
// up to 240 slots, each of which can carry ~30 decodes — an unbounded render
// would be thousands of DOM rows. The feed is HARD-CAPPED at
// `DECODE_FEED_CAP` (~200) rows, newest first, never unbounded.
//
// Untrusted-input hardening (shared with LiveDecodesTab.tsx): callsign /
// grid / message text arrives over the air and is rendered ONLY via React
// text-node children — never `dangerouslySetInnerHTML` — so React's own
// escaping is the sink-side guard; a hostile decode can render `<img
// src=x onerror=...>` as inert text but never as a live element. React keys
// are derived from the BACKEND-ASSIGNED `slotUtcMs` + in-slot decode index —
// NEVER from callsign/message/grid text — so a hostile or duplicated decode
// payload can never manufacture a colliding or markup-shaped key.

import { useMemo } from 'react';
import type { DecodeDto, SlotRecord } from './ft8Types';
import './DecodeFeed.css';

/** Hard cap on rendered rows — never unbounded (spec §Rail). */
export const DECODE_FEED_CAP = 200;

export interface DecodeFeedProps {
  /** `useFt8Listener().decodesRing` — oldest→newest, capped at 240 by the hook. */
  decodesRing: SlotRecord[];
}

/** One flattened, renderable decode row. `key` is backend-numeric only — see
 *  the untrusted-key discipline in the module doc above. */
interface DecodeFeedRow {
  key: string;
  slotUtcMs: number;
  snrDb: number;
  freqHz: number;
  message: string;
}

/**
 * Flatten decoded slots into individual decode rows, newest first, capped at
 * `DECODE_FEED_CAP`. Only `decoded`-outcome slots carry a `decodes` payload
 * (mirrors `LiveDecodesTab`'s aggregation — non-evidence outcomes, e.g.
 * `band-dead` / `failed` / `dropped-*` / `discarded`, never contribute rows).
 * Exported for direct unit testing.
 */
export function flattenDecodeFeed(ring: SlotRecord[]): DecodeFeedRow[] {
  const rows: DecodeFeedRow[] = [];
  for (const rec of ring) {
    if (rec.outcome.kind !== 'decoded') continue;
    rec.decodes.forEach((d: DecodeDto, i: number) => {
      rows.push({
        key: `${rec.slotUtcMs}-${i}`, // backend-numeric — never decode text
        slotUtcMs: rec.slotUtcMs,
        snrDb: d.snrDb,
        freqHz: d.freqHz,
        message: d.message,
      });
    });
  }
  // Array#sort is stable (ES2019+): same-slot decodes keep their original
  // in-slot order after the newest-first reorder.
  rows.sort((a, b) => b.slotUtcMs - a.slotUtcMs);
  return rows.slice(0, DECODE_FEED_CAP);
}

/** dB color tier — mirrors the approved v4 mock's good/mid/weak thresholds. */
function snrClass(snrDb: number): string {
  if (snrDb >= -10) return 'decode-feed__db--good';
  if (snrDb >= -18) return 'decode-feed__db--mid';
  return 'decode-feed__db--weak';
}

/** `+03` / `-13` — signed, zero-padded, matches the mock's dB column. */
function formatSnr(snrDb: number): string {
  const rounded = Math.round(snrDb);
  return `${rounded >= 0 ? '+' : '-'}${String(Math.abs(rounded)).padStart(2, '0')}`;
}

/** `HH:MM:SS` UTC — emcomm convention (mirrors MessageList's UTC-anchored date math). */
function utcLabel(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

export function DecodeFeed({ decodesRing }: DecodeFeedProps) {
  // D1 (C11 review note, deferred to the wiring task): before D1 this component
  // was never mounted, so re-flattening on every render cost nothing. It is now
  // fed the LIVE ring, which commits a new array on every FT-8 slot event — so
  // flatten only when the ring identity actually changes, not on every parent
  // re-render (the provider re-renders the whole panel on each slot).
  const rows = useMemo(() => flattenDecodeFeed(decodesRing), [decodesRing]);

  if (rows.length === 0) {
    return (
      <div className="decode-feed decode-feed--empty" data-testid="decode-feed-empty">
        No decodes yet.
      </div>
    );
  }

  return (
    <div className="decode-feed" data-testid="decode-feed">
      <table>
        <thead>
          <tr>
            <th>UTC</th>
            <th>dB</th>
            <th>Freq</th>
            <th>Message</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.key} data-testid={`decode-feed-row-${row.key}`}>
              <td className="decode-feed__utc">{utcLabel(row.slotUtcMs)}</td>
              <td className={`decode-feed__db ${snrClass(row.snrDb)}`}>{formatSnr(row.snrDb)}</td>
              <td className="decode-feed__freq">{String(Math.round(row.freqHz)).padStart(4, '0')}</td>
              {/* Plain text-node child — React escapes it. Untrusted radio
                  text (callsigns/grids embedded in the message) never reaches
                  the DOM as markup. */}
              <td className="decode-feed__message">{row.message}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
