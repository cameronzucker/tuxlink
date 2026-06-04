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

import './ListenSection.css';

export interface ListenArmButtonProps {
  armed: boolean;
  /** Remaining TTL in minutes when armed; ignored when disarmed. May be
   *  null until the parent computes a value. */
  minutesRemaining: number | null;
  /** Optional armed-state label. Defaults to "ARMED". */
  armedLabel?: string;
  /** Disabled while an in-flight arm/disarm call is settling. */
  busy?: boolean;
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
  busy = false,
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
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-bad"
            data-testid={`${testIdPrefix}-disarm-btn`}
            disabled={busy}
            onClick={onDisarm}
          >
            {busy ? 'Disarming…' : 'Disarm'}
          </button>
        ) : (
          <button
            type="button"
            className="radio-panel-btn radio-panel-btn-primary"
            data-testid={`${testIdPrefix}-arm-btn`}
            disabled={busy}
            onClick={onArm}
          >
            {busy ? 'Arming…' : 'Arm listener'}
          </button>
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
      </div>
      {helpText && (
        <p className="radio-panel-help" data-testid={`${testIdPrefix}-help`}>
          {helpText}
        </p>
      )}
    </>
  );
}
