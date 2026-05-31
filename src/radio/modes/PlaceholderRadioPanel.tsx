// src/radio/modes/PlaceholderRadioPanel.tsx
//
// During P1, every mode mounts this placeholder. P2-P4 replace each
// mode's placeholder with its real implementation, one phase at a time.

import { RadioPanel } from '../RadioPanel';
import { panelTitle, type RadioPanelMode } from '../types';

export interface PlaceholderRadioPanelProps {
  mode: RadioPanelMode;
  onClose: () => void;
}

export function PlaceholderRadioPanel({
  mode, onClose,
}: PlaceholderRadioPanelProps) {
  return (
    <RadioPanel mode={mode} state="disconnected" onClose={onClose}>
      <section className="radio-panel-sec">
        <h5>{panelTitle(mode)}</h5>
        <p
          data-testid="radio-panel-placeholder"
          style={{ color: 'var(--text-faint, #94a3b8)', fontSize: 11 }}
        >
          {panelTitle(mode)} panel coming soon — replaced in a future
          implementation phase. The reading-pane / dock surface for this
          mode still works in the meantime.
        </p>
      </section>
    </RadioPanel>
  );
}
