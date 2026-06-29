/**
 * elmerEvents — TypeScript mirror of src-tauri/src/elmer/events.rs.
 *
 * Event-name constants and payload interfaces MUST match the Rust serde contract
 * exactly. The Rust enum uses `#[serde(tag = "kind", rename_all = "camelCase")]`,
 * so the discriminant is `event.payload.kind`. The `Outcome` variant emits
 * `outcomeKind` (not `kind`) for the outcome's category to avoid a name collision
 * with the tag field — confirmed in events.rs line 59.
 *
 * DO NOT rename these constants or interface fields without updating events.rs.
 */

// ---------------------------------------------------------------------------
// Event-name constants (Rust ↔ TypeScript contract)
// ---------------------------------------------------------------------------

/** Streaming text token from the model (role + text snippet). */
export const EV_TURN = 'elmer-turn' as const;

/** A tool was called or returned (tool name + status string). */
export const EV_CHIP = 'elmer-chip' as const;

/** The run reached a terminal outcome (outcomeKind string + optional detail). */
export const EV_OUTCOME = 'elmer-outcome' as const;

// ---------------------------------------------------------------------------
// Payload interfaces (mirror ElmerEvent Rust enum variants, camelCase)
// ---------------------------------------------------------------------------

/**
 * A text token (or full assistant/user/system turn) from the model.
 * Emitted on EV_TURN.
 */
export interface ElmerTurnPayload {
  kind: 'turn';
  /** 'user' | 'assistant' | 'system' */
  role: string;
  /** The text content of the turn. */
  text: string;
}

/**
 * A tool was invoked or returned.
 * Emitted on EV_CHIP.
 */
export interface ElmerChipPayload {
  kind: 'chip';
  /** MCP tool name, e.g. 'find_stations'. */
  tool: string;
  /** Human-readable status, e.g. 'calling' | 'ok' | 'denied'. */
  status: string;
}

/**
 * The run loop exited with a terminal outcome.
 * Emitted on EV_OUTCOME.
 *
 * IMPORTANT: the category field is `outcomeKind`, NOT `kind` — the Rust
 * serde tag uses `"kind"` as the discriminant for the enum variant ("outcome"),
 * and `outcome_kind` (→ camelCase `outcomeKind`) carries the terminal state.
 *
 * Possible outcomeKind values: 'done' | 'cancelled' | 'needsOperator' |
 * 'toolDenied' | 'offline' | 'error' (the Rust side may emit others; treat
 * any unknown value as a generic error state in the UI).
 */
export interface ElmerOutcomePayload {
  kind: 'outcome';
  /** Terminal state category. */
  outcomeKind: string;
  /** Operator-facing detail string (already redacted at the session layer). */
  detail: string;
}

/** Union of all Elmer event payloads. Switch on `payload.kind`. */
export type ElmerPayload = ElmerTurnPayload | ElmerChipPayload | ElmerOutcomePayload;
