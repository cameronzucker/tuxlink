import { useState } from 'react';
import { useModemStatus } from './useModemStatus';
import './ArdopDock.css';

export function ArdopDock() {
  const { status } = useModemStatus();
  const [target, setTarget] = useState('');

  return (
    <aside className="ardop-dock" data-testid="ardop-dock-root">
      <header className="ardop-dock-h">
        <span className="ardop-dock-state-dot" data-state={status.state} />
        <span className="ardop-dock-name">MODEM · ARDOP HF</span>
        <span className="ardop-dock-sub">ardopcf · :8515</span>
      </header>

      {status.state === 'stopped' && (
        <section className="ardop-dock-section">
          <div className="ardop-dock-section-h">Target station</div>
          <label className="ardop-dock-field">
            Target callsign
            <input
              className="ardop-dock-input"
              data-testid="ardop-target"
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="W7RMS-10"
            />
          </label>
          <button
            className="ardop-dock-btn ardop-dock-btn-primary"
            disabled={target.trim() === ''}
            // onClick wired in Task 6.2 (consent modal flow)
          >
            Connect
          </button>
        </section>
      )}
    </aside>
  );
}
