// src/aprs/AprsDockTabs.tsx
//
// The shared right-dock tab switcher: [ APRS chat | Modem ]. The dock hosts the
// APRS chat (default tenant) or the modem console; these tabs flip between them.
// The Modem tab is disabled when no connection/modem panel is available.

import './AprsDockTabs.css';

export type DockTab = 'aprs' | 'modem';

export interface AprsDockTabsProps {
  active: DockTab;
  unread: number;
  /// Whether the Modem tab can be selected (a radio panel mode is available).
  modemEnabled: boolean;
  onSelect: (tab: DockTab) => void;
}

export function AprsDockTabs({ active, unread, modemEnabled, onSelect }: AprsDockTabsProps) {
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
    </div>
  );
}
