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
//
// Tac Map pop-out (tuxlink-dmwte task 9, spec §5): the same header slot grows a
// ↗ pop-out affordance next to the Map toggle. Once popped, the whole slot
// SWAPS to a "Tac Map ↗ — in window" pathway (focuses the popped window) plus
// an adjacent "⇤ dock back" action — mirroring how the Routines pathway
// replaces its own affordance while popped (Global Constraints rule: a popped
// surface's docked-side control always resolves to focus-or-dock-back, never
// a dead toggle).

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
  /// Whether the Tac Map surface is currently popped to its own window
  /// (tuxlink-dmwte task 9, spec §5). When true, this control's ENTIRE
  /// rendering swaps from the Map toggle + pop-out button to the "in window"
  /// focus pathway + dock-back action below.
  mapPopped?: boolean;
  /// Pop the Tac Map out to its own window (spec §5, behavior 1). Renders a ↗
  /// button beside the Map toggle when provided and `mapPopped` is false;
  /// absent ⇒ no pop-out affordance.
  onPopOutMap?: () => void;
  /// Focus the already-popped Tac Map window — the "Tac Map ↗ — in window"
  /// control's click target while `mapPopped` is true.
  onFocusMap?: () => void;
  /// Dock the Tac Map back inline — the adjacent "⇤ dock back" control's click
  /// target while `mapPopped` is true.
  onDockBackMap?: () => void;
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
  mapPopped,
  onPopOutMap,
  onFocusMap,
  onDockBackMap,
}: AprsDockTabsProps) {
  // tuxlink-mxqjp: the map companion controls and the tab strip used to share
  // one flex-wrap row; at the dock's ~400px floor the wrap point was arbitrary
  // and Map/Pop out rendered as an orphaned tab-shaped row above the real tabs
  // (R2 operator report 2026-07-20). When map controls exist the split is now
  // INTENTIONAL: a surface bar (map pathway + × close) above a clean tab row.
  // Callers without map controls keep the original single row.
  const hasSurfaceBar = Boolean(mapPopped || onToggleMap);
  const closeButton = (
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
  );
  return (
    <div className={`aprs-dock-tabs ${hasSurfaceBar ? 'aprs-dock-tabs--rows' : ''}`} data-testid="aprs-dock-tabs">
      {hasSurfaceBar && (
        <div className="aprs-dock-surfacebar" data-testid="aprs-dock-surfacebar">
          {mapPopped ? (
            <div className="aprs-dock-map-popped" data-testid="aprs-map-popped-controls">
              <button
                type="button"
                className="aprs-dock-map-focus"
                data-testid="aprs-map-focus"
                title="Focus the Tac Map window"
                onClick={onFocusMap}
              >
                Tac Map ↗ — in window
              </button>
              <button
                type="button"
                className="aprs-dock-map-dockback"
                data-testid="aprs-map-dockback"
                aria-label="Dock the Tac Map back inline"
                title="Dock the Tac Map back inline"
                onClick={onDockBackMap}
              >
                ⇤ dock back
              </button>
            </div>
          ) : (
            onToggleMap && (
              <>
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
                {onPopOutMap && (
                  <button
                    type="button"
                    className="aprs-dock-map-popout"
                    data-testid="aprs-map-popout"
                    title="Open the Tac Map in its own window"
                    onClick={onPopOutMap}
                  >
                    {/* Text-labeled, never icon-only (spec §1 visual-pathway rule +
                        §5 text-labeled requirement); mirrors the map focus /
                        dock-back pathway controls. */}
                    <span className="aprs-dock-map-popout-glyph" aria-hidden="true">↗</span>
                    Pop out
                  </button>
                )}
              </>
            )
          )}
          {closeButton}
        </div>
      )}
      <div className="aprs-dock-tabrow" data-testid="aprs-dock-tabrow">
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
        {/* Station Data is associated with the APRS channel, so it sits adjacent
            to APRS Chat. Modem is an unrelated console and lives at the far right
            (operator decision 2026-06-18). */}
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
      {!hasSurfaceBar && closeButton}
      </div>
    </div>
  );
}
