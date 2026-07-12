/**
 * deriveUiState.ts — the total `ServiceAxisDto` (+ `SlotPhaseDto`, blocked
 * reason, configured-device) → `Ft8UiState` mapping (Task B2, plan
 * tuxlink-b026z.4 §Frontend data layer).
 *
 * This is the AUTHORITATIVE derivation. `Ft8UiState` / `Ft8Flags` are declared
 * in `./ft8Types` (Task B1's contract file) and imported here, not redefined.
 *
 * Design:
 *   - `deriveState` switches on `snapshot.service.axis` first — a discriminated
 *     union switch, so TypeScript enforces exhaustiveness over all 6
 *     `ServiceAxisDto` variants at compile time (adding a 7th axis without
 *     updating this function is a type error, not a silent gap).
 *   - The three phase rows (`waiting-first-slot` | `band-dead` | `decoding`)
 *     are returned ONLY from inside the `case 'listening':` branch — this is a
 *     STRUCTURAL guard (the phase-row code is physically unreachable from any
 *     other axis), not a runtime conditional that could be bypassed by a
 *     future edit. A stopped/blocked/transitional/yielded service can never
 *     render a phase row.
 *   - Inside `case 'blocked':`, `capture-wedged` and `device-absent` are named
 *     explicitly (first-match-wins); every other current-or-future blocked
 *     reason (`wsjtx-absent`, `unsupported-sample-rate`,
 *     `needs-device-selection`, and any reason added later) falls through to
 *     the `'needs-setup'` default — the spec's "blocked → needs a fix before
 *     listening can proceed" default, so the mapping stays total even if
 *     `BlockedReasonDto` grows a member this function doesn't yet name.
 *   - `flags` are computed independently of `state` — a straight pass-through
 *     of `snapshot.flags` (the `HealthFlagsDto` provenance/health fields),
 *     never gated on which state was chosen. They are an overlay, not a
 *     replacement.
 */

import type { Ft8Flags, Ft8Snapshot, Ft8UiState } from './ft8Types';

/**
 * Total mapping: every reachable combination of `service.axis` (+ blocked
 * `reason`, + `slotPhase` when listening, + whether a device is configured)
 * maps to a defined `Ft8UiState`. Pure — no I/O, no side effects.
 */
export function deriveUiState(snapshot: Ft8Snapshot): { state: Ft8UiState; flags: Ft8Flags } {
  return { state: deriveState(snapshot), flags: deriveFlags(snapshot) };
}

function deriveState(snapshot: Ft8Snapshot): Ft8UiState {
  const { service, slotPhase, configuredDeviceName } = snapshot;

  switch (service.axis) {
    case 'stopped':
      // Any stale slotPhase (including a leftover 'decoded' from before the
      // service stopped) is irrelevant here: a stopped service is 'off', not
      // 'decoding'. The phase fields are simply never consulted in this arm.
      return 'off';

    case 'starting':
    case 'stopping':
      return 'transitional';

    case 'yielded':
      return 'yielded';

    case 'blocked':
      switch (service.reason) {
        case 'capture-wedged':
          return 'wedged';
        case 'device-absent':
          // A configured device that's now absent is a regression (lost);
          // no device configured at all is an unfinished setup step.
          return configuredDeviceName !== null ? 'device-lost' : 'needs-setup';
        case 'wsjtx-absent':
        case 'unsupported-sample-rate':
        case 'needs-device-selection':
        default:
          return 'needs-setup';
      }

    case 'listening':
      // Phase rows are reachable ONLY from this branch — the axis guard is
      // structural, not a runtime `if`.
      switch (slotPhase) {
        case 'decoded':
          return 'decoding';
        case 'band-dead':
          return 'band-dead';
        case 'waiting-first-slot':
        default:
          return 'waiting-first-slot';
      }
  }
}

function deriveFlags(snapshot: Ft8Snapshot): Ft8Flags {
  return {
    clockUnsynced: snapshot.flags.clockUnsynced,
    catFixedBand: snapshot.flags.catFixedBand,
    jt9Degraded: snapshot.flags.jt9Degraded,
  };
}
