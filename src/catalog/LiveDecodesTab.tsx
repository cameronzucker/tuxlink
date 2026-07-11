// Live decodes tab (plan tuxlink-b026z.4 Task C5, spec §Rail "Live decodes
// tab"): a STATION-centric aggregation over the FT8 decode ring, distinct
// from the chronological raw feed the strip (Task C7/C11) owns — "two
// questions, two surfaces". One row per distinct callsign heard inside the
// 10-minute evidence window: call · grid · best SNR · count · mi·brg, sorted
// by recency (most recently heard first).
//
// Untrusted-input hardening (spec §Rail, load-bearing): callsigns, grids, and
// message text arrive over the air and are NEVER trusted.
//   - Callsign/grid text renders via plain React text nodes (child
//     expressions), never `dangerouslySetInnerHTML` — React escapes them.
//   - A row with no grid heard yet is NON-INTERACTIVE and renders "—" for
//     grid/mi/brg — never a fake/guessed location.
//   - A row WITH a grid string is interactive, but the click handler routes
//     through the NULL-GUARDED `gridToLatLon` (already rejects malformed /
//     out-of-range Maidenhead text) and skips the pan on `null` — a garbage
//     over-the-air "grid" (wrong length, bad chars, even markup-shaped text)
//     never reaches the map and never throws.

import { useMemo } from 'react';
import { gridToLatLon, type LatLon } from '../forms/position/maidenhead';
import { bearingFromGrids } from './StationRail';
import { distanceFromGrids, kmToMi } from './distance';
import type { SlotRecord } from '../ft8ui/ft8Types';

export interface LiveDecodesTabProps {
  /** `useFt8Listener().decodesRing` — oldest→newest, capped at 240 (Task B1). */
  decodesRing: SlotRecord[];
  /** The operator's grid, for the mi·brg column. Empty ⇒ that column shows "—". */
  operatorGrid: string;
  /** Pan the map to a row's grid-derived coordinate. Omitted ⇒ a click still
   *  computes but has nowhere to act (never throws either way). */
  onPanTo?: (ll: LatLon) => void;
  /** Injectable "now" for deterministic tests; defaults to `Date.now()`. */
  nowMs?: number;
}

/** Aggregation window: 10 minutes (matches B3's §Openness window). Duplicated
 *  here rather than imported — C5's scope is `StationRail.tsx` +
 *  `LiveDecodesTab.tsx` only; it does not modify `deriveBandActivity.ts`. */
const LIVE_WINDOW_MS = 600_000;

export interface LiveDecodeRow {
  call: string;
  /** `null` until a decode carrying this station's grid is heard. */
  grid: string | null;
  bestSnrDb: number;
  count: number;
  /** Band tag from the most recent decode attributed to this call. */
  band: string;
  lastSlotUtcMs: number;
}

/**
 * Station-centric aggregation over evidence (`decoded`) slots in the window.
 * `ring` is oldest→newest (the hook's contract), so a forward scan naturally
 * makes the LAST write for a callsign the most recent — this is exactly the
 * "a later CQ carrying the grid upgrades the row in place" behavior: an
 * earlier null-grid decode never clobbers a later real one, and a later grid
 * always wins over an earlier one.
 */
export function aggregateLiveDecodes(ring: SlotRecord[], nowMs: number): LiveDecodeRow[] {
  const lowerBound = nowMs - LIVE_WINDOW_MS;
  const rows = new Map<string, LiveDecodeRow>();

  for (const rec of ring) {
    if (rec.slotUtcMs < lowerBound || rec.slotUtcMs > nowMs) continue;
    if (rec.outcome.kind !== 'decoded') continue; // only `decoded` slots carry decode payloads
    for (const decode of rec.decodes) {
      const call = decode.fromCall;
      if (!call) continue; // no attributable station — nothing to key the row on
      const existing = rows.get(call);
      if (existing) {
        existing.count += 1;
        existing.bestSnrDb = Math.max(existing.bestSnrDb, decode.snrDb);
        existing.lastSlotUtcMs = rec.slotUtcMs;
        existing.band = rec.band;
        if (decode.grid) existing.grid = decode.grid; // upgrade-in-place; never clobber with null
      } else {
        rows.set(call, {
          call,
          grid: decode.grid,
          bestSnrDb: decode.snrDb,
          count: 1,
          band: rec.band,
          lastSlotUtcMs: rec.slotUtcMs,
        });
      }
    }
  }

  return Array.from(rows.values()).sort((a, b) => b.lastSlotUtcMs - a.lastSlotUtcMs);
}

export function LiveDecodesTab({ decodesRing, operatorGrid, onPanTo, nowMs }: LiveDecodesTabProps) {
  const rows = useMemo(
    () => aggregateLiveDecodes(decodesRing, nowMs ?? Date.now()),
    [decodesRing, nowMs],
  );

  const handleRowClick = (grid: string | null) => {
    if (!grid) return; // no grid heard yet — the row has no onClick at all, belt-and-suspenders
    const ll = gridToLatLon(grid); // NULL-GUARDED: malformed/garbage grid → null, never throws
    if (!ll) return; // skip the pan — never feed NaN/garbage to the map
    onPanTo?.(ll);
  };

  if (rows.length === 0) {
    return (
      <div className="station-finder__railpane station-finder__ld station-finder__ld--empty" data-testid="live-decodes-empty">
        No decodes heard in the last 10 minutes.
      </div>
    );
  }

  return (
    <div className="station-finder__railpane station-finder__ld" data-testid="live-decodes-tab">
      <div className="station-finder__ld-head">
        <span>Call</span>
        <span>Grid</span>
        <span>SNR</span>
        <span>Count</span>
        <span>mi · brg</span>
      </div>
      {rows.map((row) => {
        const interactive = row.grid != null;
        const distKm = row.grid && operatorGrid ? distanceFromGrids(operatorGrid, row.grid) : null;
        const distMi = distKm != null ? Math.round(kmToMi(distKm)) : null;
        const brg = row.grid && operatorGrid ? bearingFromGrids(operatorGrid, row.grid) : null;
        const distLabel = distMi != null && brg != null ? `${distMi} mi · ${Math.round(brg)}°` : '—';
        return (
          <div
            key={row.call}
            className={`station-finder__ld-row${interactive ? ' is-clickable' : ''}`}
            data-testid={`ld-row-${row.call}`}
            role={interactive ? 'button' : undefined}
            tabIndex={interactive ? 0 : undefined}
            aria-disabled={interactive ? undefined : true}
            onClick={interactive ? () => handleRowClick(row.grid) : undefined}
            onKeyDown={
              interactive
                ? (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      handleRowClick(row.grid);
                    }
                  }
                : undefined
            }
          >
            <span className="station-finder__ld-call">
              {row.call}
              <span className="station-finder__ld-band">{row.band}</span>
            </span>
            <span className="station-finder__ld-grid">{row.grid ?? '—'}</span>
            <span className="station-finder__ld-snr">{Math.round(row.bestSnrDb)} dB</span>
            <span className="station-finder__ld-count">{row.count}</span>
            <span className="station-finder__ld-dist">{distLabel}</span>
          </div>
        );
      })}
    </div>
  );
}
