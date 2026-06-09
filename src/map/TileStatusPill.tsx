/**
 * TileStatusPill — pure, presentational provenance pill for the active tile
 * source. Maps a {@link TileSourceStatus} to the §8.5 status-pill text and
 * exposes the zoom-cap reason as a hover title.
 *
 * OWNERSHIP (tuxlink-dyop plan, Phase 8.3): dyop ships this component
 * STANDALONE because the `TileSourceStatus` → display-string mapping is
 * dyop-domain logic (it mirrors the §8.5 source-state table verbatim). The
 * shared-toolbar unit (tuxlink-a1cc) PLACES this component in the toolbar; it
 * MUST NOT reimplement the mapping. Consume `<TileStatusPill status={...}
 * zoomCapReason={...} />` directly.
 *
 * Pure: props-driven, NO `invoke`, NO network, NO effects. The status comes
 * from `getTileSourceStatus()` upstream (a hook or the map shell), not here.
 *
 * §8.5 source-state → pill text (verbatim):
 *   bundled      → "z{zoom} · bundled"
 *   lan-live     → "z{zoom} · LAN live"
 *   lan-cached   → "z{zoom} · LAN cached as of {humanized cachedAt}"
 *   partial      → "z{zoom} · LAN live (partial)"
 *   unreachable  → "tiles unreachable — bundled"
 *   incompatible → "incompatible tile source — expected EPSG:4326"
 */
import type { TileSourceStatus } from './tileSource';

export interface TileStatusPillProps {
  status: TileSourceStatus;
  /** Human-readable reason the zoom ceiling is where it is (hover title). */
  zoomCapReason: string;
}

/** Humanize an ISO-8601 cache timestamp for the `lan-cached` pill. */
function humanizeCachedAt(cachedAt: string | null): string {
  if (!cachedAt) return 'unknown';
  const d = new Date(cachedAt);
  if (Number.isNaN(d.getTime())) return cachedAt;
  return d.toLocaleString();
}

/** Map a status to its §8.5 pill text. */
function pillText(status: TileSourceStatus): string {
  const { kind, zoom } = status;
  switch (kind) {
    case 'bundled':
      return `z${zoom} · bundled`;
    case 'lan-live':
      return `z${zoom} · LAN live`;
    case 'lan-cached':
      return `z${zoom} · LAN cached as of ${humanizeCachedAt(status.cachedAt)}`;
    case 'partial':
      return `z${zoom} · LAN live (partial)`;
    case 'unreachable':
      return 'tiles unreachable — bundled';
    case 'incompatible':
      return 'incompatible tile source — expected EPSG:4326';
    default: {
      // Exhaustiveness guard: a new StatusKind must be handled above.
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}

export function TileStatusPill({ status, zoomCapReason }: TileStatusPillProps) {
  return (
    <span
      className="tux-tile-status-pill"
      data-testid="tile-status-pill"
      data-kind={status.kind}
      title={zoomCapReason}
    >
      {pillText(status)}
    </span>
  );
}
