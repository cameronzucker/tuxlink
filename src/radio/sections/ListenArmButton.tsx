// src/radio/sections/ListenArmButton.tsx
//
// Shared Arm/Disarm button + status pill used in each transport's Listen
// section. The button toggles between primary (Arm) and bad (Disarm)
// depending on `armed`; the pill shows ARMED + remaining minutes when
// armed, or "disarmed" when idle.
//
// Each transport's listener calls `<transport>_listen()` to arm and
// `<transport>_set_listen({ enabled: false })` to disarm. The parent
// supplies those handlers via `onArm` + `onDisarm`. The countdown
// minutes is informational only — the backend's arms record is the
// authoritative TTL.

import { Button } from '../../controls';
import './ListenSection.css';

export interface ListenArmButtonProps {
  armed: boolean;
  /** Remaining TTL in minutes when armed; ignored when disarmed. May be
   *  null until the parent computes a value. */
  minutesRemaining: number | null;
  /** Optional armed-state label. Defaults to "ARMED". */
  armedLabel?: string;
  /** The identity the armed listener answers as — its OWN identity bound at
   *  arm time (the active identity's presented label captured when the arm
   *  call resolved). Rendered as a small monospace pill beside the status
   *  ONLY when the listener is armed AND this is a non-empty string. The
   *  Phase-6 backend invariant keeps an armed listener bound to its arm-time
   *  identity even if the operator later switches the active identity; this
   *  prop carries that bound identity so the badge never drifts to the
   *  globally-active one. Null/undefined → no badge. */
  boundIdentity?: string | null;
  /** TRUE while an in-flight arm/disarm call is settling — drives BOTH the
   *  disabled attribute AND the transient "Arming…" / "Disarming…" label. */
  busy?: boolean;
  /** TRUE when the button should be greyed out due to a precondition that is
   *  NOT an in-flight call (e.g. the VARA listener requires the transport to
   *  be Open before arm is meaningful). Drives the disabled attribute but
   *  leaves the label at the steady "Arm listener" / "Disarm" copy, so the
   *  operator doesn't see "Arming…" describing a process that never started
   *  (tuxlink-tccc). Pair with `helpText` to explain WHY it's disabled. */
  disabled?: boolean;
  /** Optional help text rendered beneath the button. */
  helpText?: string;
  onArm: () => void;
  onDisarm: () => void;
  testIdPrefix: string;
}

export function ListenArmButton({
  armed,
  minutesRemaining,
  armedLabel = 'ARMED',
  boundIdentity,
  busy = false,
  disabled = false,
  helpText,
  onArm,
  onDisarm,
  testIdPrefix,
}: ListenArmButtonProps) {
  return (
    <>
      <div
        style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}
      >
        {armed ? (
          <Button
            tone="danger" emphasis="soft" size="md"
            data-testid={`${testIdPrefix}-disarm-btn`}
            disabled={busy || disabled}
            onClick={onDisarm}
          >
            {busy ? 'Disarming…' : 'Disarm'}
          </Button>
        ) : (
          <Button
            tone="primary" emphasis="soft" size="md"
            data-testid={`${testIdPrefix}-arm-btn`}
            disabled={busy || disabled}
            onClick={onArm}
          >
            {busy ? 'Arming…' : 'Arm listener'}
          </Button>
        )}
        <span
          className={`listen-status ${armed ? 'armed' : 'disarmed'}`}
          data-testid={`${testIdPrefix}-status`}
        >
          {armed
            ? minutesRemaining !== null
              ? `${armedLabel} · ${minutesRemaining} min`
              : armedLabel
            : 'disarmed'}
        </span>
        {armed && boundIdentity ? (
          <span
            className="listener-identity-badge"
            data-testid="listener-identity-badge"
            title="Identity this listener answers as (bound at arm time)"
          >
            {boundIdentity}
          </span>
        ) : null}
      </div>
      {helpText && (
        <p className="radio-panel-help" data-testid={`${testIdPrefix}-help`}>
          {helpText}
        </p>
      )}
    </>
  );
}
