/**
 * EgressArmControl — the operator ARM surface for agent send-authority
 * (MCP phase 3.6). A compact ribbon chip shows the state at a glance; the
 * arm/disarm actions live in a click-to-open popover so the dashboard ribbon
 * stays uncrowded.
 *
 * States (plain-language, WLE-litmus: forgiving + legible, never cryptic):
 *   - Disarmed → chip "Agent send: OFF"; popover offers duration presets.
 *   - Armed    → chip "Agent send: ON" + a live ticking countdown; popover
 *                offers Disarm.
 *   - Tainted  → chip "Agent send: LOCKED"; popover explains the session is
 *                tainted and authority is locked until restart.
 *
 * Presentational: state + actions come from useEgressArm (AppShell owns the
 * hook instance). The popover reuses IdentitySwitcher's mechanism: measured
 * anchor coords, createPortal to <body> (position:fixed) so it escapes the
 * ribbon's stacking context, Esc-to-close, and document mousedown
 * outside-click-to-close. The live 1-second countdown lives in a scoped
 * subtree (CountdownCell) so the tick does not repaint the rest of the ribbon.
 */

import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
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
  const dotClass = tainted ? 'tx' : armed ? '' : 'idle';
  const stateLabel = tainted ? 'LOCKED' : armed ? 'ON' : 'OFF';

  const [open, setOpen] = useState(false);
  const chipRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<{ top: number; left: number } | null>(null);

  // Anchor the portaled popover under the chip; re-measure on resize.
  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    function measure() {
      const r = chipRef.current?.getBoundingClientRect();
      if (r) setCoords({ top: r.bottom + 6, left: r.left });
    }
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [open]);

  // Click-outside closes (the popover is portaled out of the chip subtree).
  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      const t = e.target as Node;
      if (!chipRef.current?.contains(t) && !popRef.current?.contains(t)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  return (
    <div className="dash-item dash-egress" data-testid="egress-arm-control">
      <button
        type="button"
        ref={chipRef}
        className="dash-egress-chip"
        data-testid="egress-chip"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={`dash-status-dot ${dotClass}`} aria-hidden="true" />
        <span className="dash-egress-label">Agent send</span>
        <span
          className="dash-egress-state"
          data-testid="egress-state"
          data-armed={armed}
          data-tainted={tainted}
        >
          {stateLabel}
        </span>
        {armed && !tainted && <CountdownCell remainingSecs={armedRemainingSecs} />}
        <span className="dash-egress-caret" aria-hidden="true">
          {open ? '▴' : '▾'}
        </span>
      </button>

      {open &&
        coords &&
        createPortal(
          <div
            ref={popRef}
            className="egress-arm-popover"
            data-testid="egress-popover"
            role="dialog"
            aria-label="Agent send authority"
            tabIndex={-1}
            style={{ top: coords.top, left: coords.left }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setOpen(false);
            }}
          >
            <div className="egress-pop-title">Agent send authority</div>

            {tainted ? (
              <div className="dash-egress-locked" data-testid="egress-locked">
                Session tainted — restart Tuxlink to re-enable agent send.
              </div>
            ) : armed ? (
              <button
                type="button"
                className="egress-disarm-button"
                data-testid="egress-disarm"
                disabled={busy}
                onClick={onDisarm}
              >
                Disarm now
              </button>
            ) : (
              <>
                <div className="egress-arm-label">Arm send-authority for:</div>
                <div
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
                </div>
                <div className="egress-pop-help">
                  While armed, an MCP agent may transmit or change settings. Disarms automatically
                  when the timer ends.
                </div>
              </>
            )}

            {error && (
              <div className="dash-egress-error" role="alert" data-testid="egress-error">
                {error}
              </div>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
});
