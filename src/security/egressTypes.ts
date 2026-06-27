/**
 * egressTypes — TS mirror of the Rust egress-grant command shapes.
 *
 * Backend: src-tauri/src/ui_core/security_commands.rs (EgressStatusDto +
 * egress_arm / egress_disarm / egress_status). Commands registered in
 * src-tauri/src/lib.rs. The operator arms/disarms agent send-authority here;
 * the GUI renders the live state (this is MCP phase 3.6, the operator ARM
 * surface for the egress gate).
 *
 * Mirror convention follows src/shell/useStatus.ts (DTOs mirror the Rust
 * serde `rename_all = "camelCase"` shape — the Rust struct uses snake_case
 * fields rendered to camelCase on the wire).
 */

/** Serializable snapshot of the egress-grant state (mirrors EgressStatusDto). */
export interface EgressStatusDto {
  /** True while send-authority is armed (i.e. the grant has not yet expired). */
  armed: boolean;
  /** Seconds remaining on the current grant; 0 when disarmed/expired. */
  armedRemainingSecs: number;
  /** True once the session is tainted — send-authority is locked until a fresh
   *  session, regardless of arm state. */
  tainted: boolean;
}

/** Arm-duration presets the operator can pick from (seconds). */
export interface EgressDurationPreset {
  label: string;
  secs: number;
}

/** Sensible presets for delegating send-authority for a bounded window. */
export const EGRESS_DURATION_PRESETS: readonly EgressDurationPreset[] = [
  { label: '15 min', secs: 15 * 60 },
  { label: '1 hour', secs: 60 * 60 },
  { label: '4 hours', secs: 4 * 60 * 60 },
] as const;

/** Disarmed baseline used before the first status poll resolves. */
export const EGRESS_STATUS_DISARMED: EgressStatusDto = {
  armed: false,
  armedRemainingSecs: 0,
  tainted: false,
};

/**
 * Format a remaining-seconds count as a compact countdown.
 *  - < 1 hour: "MM:SS"
 *  - >= 1 hour: "H:MM:SS"
 * Pure — the prime unit-test target (mirrors useStatus.ts's pure-formatter
 * convention).
 */
export function formatEgressRemaining(totalSecs: number): string {
  const s = Math.max(0, Math.floor(totalSecs));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const seconds = s % 60;
  const pad = (n: number) => n.toString().padStart(2, '0');
  if (hours > 0) {
    return `${hours}:${pad(minutes)}:${pad(seconds)}`;
  }
  return `${pad(minutes)}:${pad(seconds)}`;
}
