// src/elmer/elmerToken.ts — the Elmer continuity token's `state` shape,
// defined ONCE here (bd tuxlink-mfssz, mirroring routinesToken.ts). The full
// wire context per surface is an ENVELOPE `{ foreground, state }` (dockState
// spec §5); this module owns the `state` half for Elmer.
//
// Why the token carries the conversation: ElmerItem[] is frontend useState —
// NOT backend-hydrated (AC-5: the backend transcript belongs to ElmerSession
// and is not readable back), and the backend emits only assistant turns
// (session.rs), so user turns exist ONLY in the window that sent them. A
// popped/docked-back Elmer window can therefore never rebuild its scrollback
// from events; the token is the only carrier. Streaming buffers are NOT
// carried — `app.emit` broadcasts EV_DELTA/EV_TURN to every window, so an
// in-flight turn re-attaches live (the committed EV_TURN carries full text).
//
// Deliberately dependency-light (type-only imports + structural guards) so
// AppShell can import the guard without pulling the Elmer chunk eagerly.
import type { ElmerItem } from './useElmer';

/** The Elmer continuity token's `state` half. Travels inside the
 *  `{ foreground, state }` envelope on every pop-out / dock-back. */
export interface ElmerTokenState {
  /** The full conversation scrollback (turns, chips, attributions, errors). */
  items: ElmerItem[];
  /** True when a run was in flight at token-flush time. Seeds the receiving
   *  window's single-flight guard so a second send can't double-run while the
   *  in-flight turn's events stream in (they broadcast to all windows). */
  running?: boolean;
  /** Latest context-meter snapshot, so the meter doesn't blank on a window
   *  move. Null/absent when no EV_CONTEXT has arrived yet. */
  context?: { promptTokens: number; numCtx: number | null } | null;
}

/** Structural check for one {@link ElmerItem}. Field-level laxity is
 *  deliberate: unknown extra fields pass (forward compatibility), but the
 *  discriminant + fields the renderers dereference must be present. */
function isElmerItem(value: unknown): value is ElmerItem {
  if (!value || typeof value !== 'object') return false;
  const v = value as { kind?: unknown; id?: unknown };
  if (typeof v.id !== 'string') return false;
  switch (v.kind) {
    case 'turn': {
      const t = value as { role?: unknown; text?: unknown };
      return typeof t.role === 'string' && typeof t.text === 'string';
    }
    case 'chip': {
      const c = value as { tool?: unknown; status?: unknown };
      return typeof c.tool === 'string' && typeof c.status === 'string';
    }
    case 'attribution':
      return typeof (value as { model?: unknown }).model === 'string';
    case 'error':
      return typeof (value as { outcomeKind?: unknown }).outcomeKind === 'string';
    default:
      return false;
  }
}

/** True when `value` is a well-formed {@link ElmerTokenState}. Used to
 *  validate a token's `state` before seeding a surface from it — a malformed
 *  or absent token falls back to an empty conversation rather than crashing.
 *  A token with ANY malformed item is rejected whole (a partial scrollback
 *  would silently misrepresent the conversation). */
export function isElmerTokenState(value: unknown): value is ElmerTokenState {
  if (!value || typeof value !== 'object') return false;
  const v = value as { items?: unknown; running?: unknown; context?: unknown };
  if (!Array.isArray(v.items) || !v.items.every(isElmerItem)) return false;
  if (v.running !== undefined && typeof v.running !== 'boolean') return false;
  if (v.context !== undefined && v.context !== null) {
    const c = v.context as { promptTokens?: unknown; numCtx?: unknown };
    if (typeof c.promptTokens !== 'number') return false;
    if (c.numCtx !== null && typeof c.numCtx !== 'number') return false;
  }
  return true;
}
