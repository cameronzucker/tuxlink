/**
 * EgressArmControl — the operator ARM surface for agent send-authority
 * (MCP phase 3.6). Lives in the dashboard ribbon next to the APRS + Connection
 * indicators (AppShell wires it in).
 *
 * Without this, nothing the MCP agent does can egress: this is the
 * operator-present surface for the egress gate. The operator arms send-authority
 * for a bounded window and SEES its state at a glance.
 *
 * States (plain-language, WLE-litmus: forgiving + legible, never cryptic):
 *   - Disarmed  → "Agent send: OFF" + duration presets to arm.
 *   - Armed     → "Agent send: ON" + a live ticking countdown + Disarm.
 *   - Tainted   → "Agent send: LOCKED" — session tainted, authority locked.
 *
 * Presentational: state + actions come from useEgressArm (AppShell owns the
 * hook instance), mirroring DashboardRibbon's data-down convention. The live
 * 1-second countdown lives in a scoped subtree (like ClockCell) so the tick
 * does not repaint the rest of the ribbon.
 */

import { memo, useEffect, useState } from 'react';
import {
  EGRESS_DURATION_PRESETS,
  formatEgressRemaining,
  type EgressStatusDto,
} from '../security/egressTypes';

/**
 * Live countdown cell. Seeds from the polled remaining-seconds and ticks down
 * locally each second; re-seeds whenever a fresh poll changes the value (so a
 * re-arm or clock drift is corrected). Scoped so only this text node repaints.
 */
function CountdownCell({ remainingSecs }: { remainingSecs: number }) {
  const [secs, setSecs] = useState(remainingSecs);

  // Re-seed when the polled value changes (re-arm, disarm-then-arm, drift fix).
  useEffect(() => {
    setSecs(remainingSecs);
  }, [remainingSecs]);

  useEffect(() => {
    const id = setInterval(() => {
      setSecs((s) => (s > 0 ? s - 1 : 0));
    }, 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <span className="egress-countdown" data-testid="egress-countdown">
      {formatEgressRemaining(secs)} left
    </span>
  );
}

export interface EgressArmControlProps {
  /** Live egress-grant snapshot from useEgressArm. */
  status: EgressStatusDto;
  /** Arm send-authority for the chosen duration (seconds). */
  onArm: (durationSecs: number) => void;
  /** Disarm send-authority immediately. */
  onDisarm: () => void;
  /** True while an arm/disarm round-trip is in flight (disables controls). */
  busy?: boolean;
  /** Last arm/disarm error, or null. Surfaced inline so a failed arm is visible
   *  (an operator must never believe authority is armed when it is not). */
  error?: string | null;
}

export const EgressArmControl = memo(function EgressArmControl({
  status,
  onArm,
  onDisarm,
  busy,
  error,
}: EgressArmControlProps) {
  const { armed, armedRemainingSecs, tainted } = status;

  // Taint is terminal: send-authority is locked regardless of arm state.
  // Surface it first so the operator is never misled by a stale "ON".
  const dotClass = tainted ? 'tx' : armed ? '' : 'idle';
  const stateLabel = tainted ? 'LOCKED' : armed ? 'ON' : 'OFF';

  return (
    <div className="dash-item dash-egress" data-testid="egress-arm-control">
      <div className="dash-label">Agent send</div>
      <div
        className="dash-egress-row"
        data-testid="egress-state"
        data-armed={armed}
        data-tainted={tainted}
      >
        <span
          className={`dash-status-dot ${dotClass}`}
          aria-hidden="true"
        />
        <span className="dash-egress-state">{stateLabel}</span>

        {tainted ? (
          // Terminal locked state — no arm/disarm affordance; the session must
          // be restarted to clear taint. Plain-language so the operator knows
          // why the agent can't send (no cryptic error code).
          <span className="dash-egress-locked" data-testid="egress-locked">
            session tainted — restart to re-enable
          </span>
        ) : armed ? (
          <>
            <CountdownCell remainingSecs={armedRemainingSecs} />
            <button
              type="button"
              className="egress-disarm-button"
              data-testid="egress-disarm"
              disabled={busy}
              onClick={onDisarm}
            >
              Disarm
            </button>
          </>
        ) : (
          <span
            className="egress-presets"
            role="group"
            aria-label="Arm agent send-authority for a bounded window"
            data-testid="egress-presets"
          >
            {EGRESS_DURATION_PRESETS.map((preset) => (
              <button
                key={preset.secs}
                type="button"
                className="egress-arm-button"
                data-testid={`egress-arm-${preset.secs}`}
                disabled={busy}
                onClick={() => onArm(preset.secs)}
                title={`Arm agent send-authority for ${preset.label}`}
              >
                {preset.label}
              </button>
            ))}
          </span>
        )}
      </div>
      {error && (
        <div className="dash-egress-error" role="alert" data-testid="egress-error">
          {error}
        </div>
      )}
    </div>
  );
});
