// src/aprs/AprsDockTabs.tsx
//
// The shared right-dock tab switcher: [ APRS chat | Modem ] with a close control.
// The dock hosts the APRS chat (default tenant) or the modem console; these tabs
// flip between them. The Modem tab is disabled when no connection/modem panel is
// available. The close button dismisses the whole APRS dock surface — when no
// radio session is active that frees the reading pane to the window edge
// (tuxlink-iehg: the chat opened one-way before, with no way to close it).

import './AprsDockTabs.css';

export type DockTab = 'aprs' | 'modem';

export interface AprsDockTabsProps {
  active: DockTab;
  unread: number;
  /// Whether the Modem tab can be selected (a radio panel mode is available).
  modemEnabled: boolean;
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
  onSelect,
  onClose,
  mapOpen,
  onToggleMap,
}: AprsDockTabsProps) {
  return (
    <div className="aprs-dock-tabs" role="tablist" data-testid="aprs-dock-tabs">
      <button
        type="button"
        role="tab"
        aria-selected={active === 'aprs'}
        className={`aprs-dock-tab ${active === 'aprs' ? 'is-active' : ''}`}
        data-testid="aprs-dock-tab-aprs"
        onClick={() => onSelect('aprs')}
      >
        APRS chat
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
      {onToggleMap && (
        <button
          type="button"
          className={`aprs-dock-maptoggle ${mapOpen ? 'is-active' : ''}`}
          data-testid="aprs-map-toggle"
          aria-pressed={Boolean(mapOpen)}
          title={mapOpen ? 'Hide the heard-positions map' : 'Show heard stations on a map'}
          onClick={onToggleMap}
        >
          Map
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
