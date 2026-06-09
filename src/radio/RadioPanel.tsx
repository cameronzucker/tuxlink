// src/radio/RadioPanel.tsx
//
// Shell for the right-hand radio panel. Per spec §3.2 + §4.2:
//   - 360 px wide when mounted; not shown otherwise
//   - header with state dot + mode title + close
//   - body renders mode-specific sections (passed as children)
//   - all sections always rendered (no collapsible by default)
//
// See docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md.

import type { ReactNode } from 'react';
import { panelTitle, type RadioPanelMode } from './types';
import './RadioPanel.css';

export type RadioPanelState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'disconnecting'
  | 'error';

export interface RadioPanelProps {
  mode: RadioPanelMode;
  state?: RadioPanelState;
  /** Optional sub-text in the header (peer / bandwidth / etc.). */
  sub?: string;
  /** Called when the operator clicks the close button. */
  onClose: () => void;
  /**
   * tuxlink-6jpf: RF dial modes (ARDOP / Packet / VARA) pass this to surface a
   * "Find a gateway" affordance in the panel chrome — the station finder is
   * needed at connect time, in the panel. Telnet (fixed CMS host) omits it.
   */
  onFindGateway?: () => void;
  /** Mode-specific section content. */
  children: ReactNode;
}

export function RadioPanel({
  mode, state = 'disconnected', sub, onClose, onFindGateway, children,
}: RadioPanelProps) {
  return (
    <aside className="radio-panel" data-testid="radio-panel-root">
      <header className="radio-panel-h">
        <span
          className="radio-panel-dot"
          data-testid="radio-panel-dot"
          data-state={state}
        />
        <span className="radio-panel-name" data-testid="radio-panel-title">
          MODEM · {panelTitle(mode)}
        </span>
        {sub && <span className="radio-panel-sub">{sub}</span>}
        <button
          type="button"
          className="radio-panel-close"
          data-testid="radio-panel-close"
          onClick={onClose}
          aria-label="Close radio panel"
        >
          ☓
        </button>
      </header>
      <div className="radio-panel-body">
        {onFindGateway && (
          <button
            type="button"
            className="radio-panel-find-gateway"
            data-testid="radio-panel-find-gateway"
            onClick={onFindGateway}
            title="Find a gateway near you"
          >
            🛰 Find a gateway…
          </button>
        )}
        {children}
      </div>
    </aside>
  );
}
