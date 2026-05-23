// src/connections/StubPanel.tsx
// Stub pane shown when a {sessionType, protocol} combination hasn't been
// built yet. Reuses the `reading-pane` class so it slots cleanly into the
// same layout as TelnetCmsPanel and PacketConnectionPanel.

import { SESSION_TYPES } from './sessionTypes';
import type { SessionTypeId, ProtocolId } from './sessionTypes';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface StubPanelProps {
  sessionType: SessionTypeId;
  protocol: ProtocolId;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function StubPanel({ sessionType, protocol }: StubPanelProps) {
  const sessionEntry = SESSION_TYPES.find((s) => s.id === sessionType);
  const protocolEntry = sessionEntry?.protocols.find((p) => p.id === protocol);

  const sessTypeLabel = sessionEntry?.label ?? sessionType;
  const protoLabel = protocolEntry?.label ?? protocol;

  return (
    <div className="reading-pane stub-panel" data-testid="stub-panel-root">
      <h2 className="stub-panel-title">
        {sessTypeLabel} · {protoLabel}
      </h2>
      <p className="stub-panel-body">
        This combination is not yet built. Coming soon.
      </p>
    </div>
  );
}
