// src/aprs/AprsDockTabs.tsx
//
// The shared right-dock tab row: [ Map · APRS Chat | Modem ] with a close control.
// The dock hosts the APRS chat (default tenant) or the modem console; the two
// TABS flip between them. The Modem tab is disabled when no connection/modem
// panel is available. The close button dismisses the whole APRS dock surface —
// when no radio session is active that frees the reading pane to the window edge
// (tuxlink-iehg: the chat opened one-way before, with no way to close it).
//
// Map is NOT a third exclusive tab — it is a COMPANION TOGGLE pinned to the LEFT
// (operator decision 2026-06-15). Toggling it splits the heard-positions map into
// the reading pane BESIDE whichever tenant (chat or modem) is currently active,
// so the map can ride along with either. It carries its own on/off state
// (aria-pressed), distinct from the mutually-exclusive chat⇄modem tabs.

import './AprsDockTabs.css';

export type DockTab = 'aprs' | 'modem' | 'stations';

export interface AprsDockTabsProps {
  active: DockTab;
  unread: number;
  /// Whether the Modem tab can be selected (a radio panel mode is available).
  modemEnabled: boolean;
  /// Count of stations currently heard emitting weather/telemetry — shown as a
  /// live count on the Station Data tab (tuxlink-2phz). Omit/0 ⇒ no count shown.
  stationCount?: number;
  /// Pop the Station Data panel out into its own window (the second-window
  /// pattern shared with compose/help). Renders a pop-out control when provided.
  onPopOut?: () => void;
  onSelect: (tab: DockTab) => void;
  /// Dismiss the APRS dock surface (sets `aprsOpen=false` in AppShell). With no
  /// radio session active this collapses the dock and frees the reading pane.
  onClose: () => void;
  /// Whether the heard-positions map is currently expanded into the reading pane
  /// (tuxlink-6vgt). Omit to hide the Map toggle entirely.
  mapOpen?: boolean;
  /// Toggle the heard-positions map open/closed. When set, a "Map" control is
  /// rendered in the dock header; absent ⇒ no toggle (e.g. legacy callers).
  onToggleMap?: () => void;
}

export function AprsDockTabs({
  active,
  unread,
  modemEnabled,
  stationCount = 0,
  onPopOut,
  onSelect,
  onClose,
  mapOpen,
  onToggleMap,
}: AprsDockTabsProps) {
  return (
    <div className="aprs-dock-tabs" data-testid="aprs-dock-tabs">
      {onToggleMap && (
        <button
          type="button"
          className={`aprs-dock-maptoggle ${mapOpen ? 'is-active' : ''}`}
          data-testid="aprs-map-toggle"
          aria-pressed={Boolean(mapOpen)}
          title={
            mapOpen
              ? 'Hide the heard-positions map'
              : 'Show heard stations on a map beside the chat or modem'
          }
          onClick={onToggleMap}
        >
          <span className="aprs-dock-map-glyph" aria-hidden="true">⊞</span>
          Map
        </button>
      )}
      <div className="aprs-dock-tabgroup" role="tablist" aria-label="Dock view">
        <button
          type="button"
          role="tab"
          aria-selected={active === 'aprs'}
          className={`aprs-dock-tab ${active === 'aprs' ? 'is-active' : ''}`}
          data-testid="aprs-dock-tab-aprs"
          onClick={() => onSelect('aprs')}
        >
          APRS Chat
          {unread > 0 && active !== 'aprs' && (
            <span className="aprs-dock-tab-badge" data-testid="aprs-dock-tab-aprs-unread">{unread}</span>
          )}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={active === 'modem'}
          className={`aprs-dock-tab ${active === 'modem' ? 'is-active' : ''}`}
          data-testid="aprs-dock-tab-modem"
          disabled={!modemEnabled}
          onClick={() => onSelect('modem')}
        >
          Modem
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={active === 'stations'}
          className={`aprs-dock-tab ${active === 'stations' ? 'is-active' : ''}`}
          data-testid="aprs-dock-tab-stations"
          onClick={() => onSelect('stations')}
        >
          Station Data
          {stationCount > 0 && (
            <span className="aprs-dock-tab-count" data-testid="aprs-dock-tab-stations-count">{stationCount}</span>
          )}
        </button>
      </div>
      {onPopOut && (
        <button
          type="button"
          className="aprs-dock-popout"
          data-testid="aprs-dock-popout"
          aria-label="Open station data in its own window"
          title="Open station data in its own window"
          onClick={onPopOut}
        >
          <span className="aprs-dock-popout-glyph" aria-hidden="true">⤢</span>
        </button>
      )}
      <button
        type="button"
        className="aprs-dock-close"
        data-testid="aprs-dock-close"
        aria-label="Close APRS chat"
        title="Close APRS chat"
        onClick={onClose}
      >
        ×
      </button>
    </div>
  );
}
