import { gridToLatLon } from '../forms/position/maidenhead';
import type { RecentGatewayPin } from './recentGateways';

export interface WinlinkPin {
  gateway: string;
  lat: number;
  lon: number;
  tierClass: string;
  isLive: boolean;
}

/** Strip SSID suffix (chars after `-`) and uppercase the base callsign. */
const base = (call: string) => call.split('-')[0]!.toUpperCase();

/**
 * Map a list of recent-gateway rows to map pins with CSS tier classes.
 *
 * Drops:
 *   - rows with no `grid` field
 *   - rows whose `grid` string does not parse (gridToLatLon returns null)
 *
 * tierClass priority:
 *   live (matches livePeer, SSID-stripped, case-insensitive) → `winlink-pin--live`
 *   outcome === 'failed'                                      → `winlink-pin--failed`
 *   reached within 1h of nowMs                               → `winlink-pin--reached`
 *   otherwise (reached but older than 1h)                    → `winlink-pin--stale`
 */
export function toWinlinkPins(
  rows: RecentGatewayPin[],
  opts: { livePeer: string | null; nowMs: number },
): WinlinkPin[] {
  const live = opts.livePeer ? base(opts.livePeer) : null;
  const out: WinlinkPin[] = [];

  for (const r of rows) {
    if (!r.grid) continue;
    const ll = gridToLatLon(r.grid);
    if (!ll) continue;

    const isLive = live !== null && base(r.gateway) === live;

    let tierClass: string;
    if (isLive) {
      tierClass = 'winlink-pin--live';
    } else if (r.outcome === 'failed') {
      tierClass = 'winlink-pin--failed';
    } else {
      const ageMs = opts.nowMs - Date.parse(r.last_attempt_at);
      tierClass = ageMs <= 3_600_000 ? 'winlink-pin--reached' : 'winlink-pin--stale';
    }

    out.push({ gateway: r.gateway, lat: ll.lat, lon: ll.lon, tierClass, isLive });
  }

  return out;
}
