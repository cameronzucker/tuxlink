// AprsLayersPanel (tuxlink-8fjx) — collapsible station-category filter for the
// APRS map. Presentational: the parent owns the enabled-set + collapse state
// (usePersistedBucketFilter) and the live counts; this renders the control and
// reports intent. Collapsed = a single button so the map is unobstructed;
// expanded = a master "All" row plus one checkbox row per bucket with a live count.

import { BUCKETS, type BucketKey } from './stationBuckets';
import './AprsLayersPanel.css';

export interface AprsLayersPanelProps {
  enabled: Set<BucketKey>;
  counts: Record<BucketKey, number>;
  total: number;
  collapsed: boolean;
  onToggleBucket: (key: BucketKey) => void;
  onToggleAll: (on: boolean) => void;
  onToggleCollapsed: () => void;
  /** Optional Winlink layer toggle row — rendered only when the callback is provided. */
  winlinkOn?: boolean;
  onToggleWinlink?: () => void;
}

export function AprsLayersPanel({
  enabled,
  counts,
  total,
  collapsed,
  onToggleBucket,
  onToggleAll,
  onToggleCollapsed,
  winlinkOn,
  onToggleWinlink,
}: AprsLayersPanelProps) {
  if (collapsed) {
    return (
      <button
        type="button"
        className="aprs-layers-toggle"
        data-testid="aprs-layers-toggle"
        aria-label="Show map layers filter"
        onClick={onToggleCollapsed}
      >
        <span aria-hidden="true">☰</span> Layers
      </button>
    );
  }

  const allOn = enabled.size === BUCKETS.length;

  return (
    <div className="aprs-layers-panel" data-testid="aprs-layers-panel" role="group" aria-label="Map station filter">
      <div className="aprs-layers-panel__head">
        <span className="aprs-layers-panel__title">Show on map</span>
        <button
          type="button"
          className="aprs-layers-panel__collapse"
          data-testid="aprs-layers-collapse"
          aria-label="Collapse layers filter"
          onClick={onToggleCollapsed}
        >
          ✕
        </button>
      </div>

      <label className="aprs-layers-panel__row aprs-layers-panel__row--all">
        <input
          type="checkbox"
          data-testid="aprs-layers-all"
          checked={allOn}
          onChange={() => onToggleAll(!allOn)}
        />
        <span className="aprs-layers-panel__name">All stations</span>
        <span className="aprs-layers-panel__count">{total}</span>
      </label>

      {BUCKETS.map((m) => (
        <label
          key={m.key}
          className="aprs-layers-panel__row"
          data-testid={`aprs-layers-row-${m.key}`}
        >
          <input
            type="checkbox"
            data-testid={`aprs-layers-check-${m.key}`}
            checked={enabled.has(m.key)}
            onChange={() => onToggleBucket(m.key)}
          />
          <span className="aprs-layers-panel__name">
            <span className="aprs-layers-panel__glyph" aria-hidden="true">{m.glyph}</span>
            {m.label}
          </span>
          <span
            className={`aprs-layers-panel__count${counts[m.key] === 0 ? ' aprs-layers-panel__count--zero' : ''}`}
            data-testid={`aprs-layers-count-${m.key}`}
          >
            {counts[m.key]}
          </span>
        </label>
      ))}

      {onToggleWinlink && (
        <label className="aprs-layers-panel__row" data-testid="winlink-layer-toggle">
          <input type="checkbox" checked={!!winlinkOn} onChange={onToggleWinlink} />
          <span className="aprs-layers-panel__name">
            <span className="aprs-layers-panel__glyph" aria-hidden="true">◆</span> Winlink links
          </span>
        </label>
      )}
    </div>
  );
}
