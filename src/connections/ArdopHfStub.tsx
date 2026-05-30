// src/connections/ArdopHfStub.tsx
// Reading-pane stub shown when the operator has selected Winlink (CMS) → ARDOP
// HF in the sidebar. The actual dial UI lives in the right-hand ArdopDock
// (mounted by AppShell when the modem isn't 'stopped' — and conditionally even
// when the modem IS stopped, see Phase 4 design), so the reading-pane just
// directs the operator there.
//
// Reuses the `.reading-pane` class so it slots into the same grid column as
// MessageView, TelnetCmsPanel, and PacketConnectionPanel.

export function ArdopHfStub() {
  return (
    <div className="reading-pane ardop-hf-stub" data-testid="ardop-hf-stub">
      <p style={{ color: 'var(--text-faint)', padding: '14px 16px' }}>
        ARDOP HF is configured. Use the <strong>modem dock on the right</strong> to dial a target station.
      </p>
    </div>
  );
}
