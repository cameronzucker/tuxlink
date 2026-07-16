// Frontend dock-state wire mirror (bd tuxlink-dmwte).
// Spec: docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md §3
// (wire contract), §5 (listen-first + reconcile), §6 (consent host).
//
// Rust owns the dock registry — this module is a VIEW, never an owner (spec
// §2). The types, SURFACE_WINDOW_LABEL table, and consentHostWindow are all
// copied/mirrored from the Rust-canonical shapes in
// src-tauri/src/dock/mod.rs, cross-checked by the shared parity fixture
// (dock-wire-fixture.json + dockParity.test.ts, spec §10).

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';

export type SurfaceId = 'routines' | 'tac_map' | 'aprs_chat';
export type DockMode = 'docked' | 'popped';

export interface DockSurfaces {
  routines: DockMode;
  tac_map: DockMode;
  aprs_chat: DockMode;
}

/** The `dock:changed` payload and `dock_state_get` return (spec §3) — a full
 * snapshot, never a delta: windows replace wholesale, so a missed event
 * self-heals at the next one. */
export interface DockSnapshot {
  surfaces: DockSurfaces;
  context: Record<SurfaceId, unknown | null>;
}

/**
 * The envelope shape every `DockSnapshot.context[surface]` slot carries (spec
 * §7 continuity token): `foreground` distinguishes a ⇤-style dock-back
 * (steal the pane / focus the mailbox onto this surface's state) from a
 * ✕/Ctrl+W/WM-close arrival (availability only — no pane theft); `state` is
 * the surface's own opaque continuity payload, shaped per-surface (Routines
 * carries `{ view, draft }`; tac_map and aprs_chat carry `null` — neither has
 * inline state worth restoring).
 *
 * Formalized here (review debt from Task 8, which left AppShell's routines-
 * arrival code with an ad-hoc inline `{ foreground?: boolean; state?: {...} |
 * null } | null` type of its own) so every consumer — Task 8's routines
 * arrival effect and Task 9's tac_map arrival effect alike — shares ONE
 * definition instead of restating the shape at each call site. Callers narrow
 * `state` to their own surface's payload shape at the point of use (`state`
 * stays `unknown` here deliberately — this type does not know per-surface
 * shapes).
 */
export type DockContextEnvelope = { foreground?: boolean; state?: unknown } | null | undefined;

/**
 * Wire table (spec §3) — copied verbatim from the spec, never derived: the
 * window-label form drops the surface id's underscore irregularly
 * (`tac_map` → `pop-tacmap`, `aprs_chat` → `pop-aprschat`).
 */
export const SURFACE_WINDOW_LABEL: Record<SurfaceId, string> = {
  routines: 'pop-routines',
  tac_map: 'pop-tacmap',
  aprs_chat: 'pop-aprschat',
};

/**
 * Consent-host resolution (spec §6). The Rust `consent_host_window` in
 * src-tauri/src/dock/mod.rs is CANONICAL (the backend must resolve the
 * hosting window anyway to fire the park notification); this mirrors it and
 * is cross-checked against the shared parity fixture, never bookkept
 * independently.
 */
export function consentHostWindow(s: DockSurfaces): 'main' | 'pop-routines' {
  return s.routines === 'popped' ? 'pop-routines' : 'main';
}

/**
 * Subscribes to the dock registry (spec §5). Subscription order is
 * mandatory: the `dock:changed` listener is registered and AWAITED before
 * `dock_state_get` is ever invoked — the launch-restoration window is
 * exactly where a get-then-subscribe gap loses a dock-back emit and strands
 * a permanent pathway to a nonexistent window (adrev R2-F5). A second,
 * reconcile `dock_state_get` follows the initial read, closing the
 * remaining gap between the listener settling and that first read landing.
 *
 * Returns `null` until the first snapshot lands (mount-time only — no
 * loading state is threaded beyond that single instant).
 *
 * TOCTOU guard (review-loop-3 F1): BOTH the initial get and the reconcile
 * get are in-flight concurrently with the live listener, so a real
 * `dock:changed` event can land AFTER either invoke is issued but BEFORE it
 * resolves. That event's payload is always at-least-as-new as any
 * `dock_state_get` response (Rust emits `dock:changed` synchronously with
 * every registry mutation, spec §3), so an `eventSeen` flag set by the
 * listener gates BOTH gets' `setSnapshot` calls — a stale get response,
 * initial or reconcile, must never clobber a newer event snapshot.
 */
export function useDockState(): DockSnapshot | null {
  const [snapshot, setSnapshot] = useState<DockSnapshot | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    // Set by the `dock:changed` handler below; gates the reconcile apply
    // (see the TOCTOU note in this function's doc comment above).
    let eventSeen = false;

    listen<DockSnapshot>('dock:changed', (event) => {
      eventSeen = true;
      if (!disposed) setSnapshot(event.payload);
    })
      .then(async (u) => {
        if (disposed) {
          u();
          return;
        }
        unlisten = u;

        const initial = await invoke<DockSnapshot>('dock_state_get');
        if (disposed) return;
        // Guarded by `eventSeen` for the same reason as the reconcile apply
        // below: a `dock:changed` event that landed while this initial get
        // was in flight is newer than the initial get's own response (Rust
        // emits it synchronously with every mutation), so applying the
        // initial get here would clobber that newer state with a stale one
        // (TOCTOU regression, review-loop-3 F1, initial-get leg). Skipping
        // is always safe — an event snapshot always originates from a
        // backend mutation and is complete on its own.
        if (!disposed && !eventSeen) setSnapshot(initial);

        // Reconcile read (spec §5): closes the gap between the listener
        // settling and the initial read landing. Guarded by `eventSeen` — a
        // `dock:changed` event that landed while this reconcile get was in
        // flight is newer than the reconcile's own response, so applying
        // the reconcile here would be a TOCTOU regression (review-loop-3
        // F1). Skip the apply when that happened; the event's own
        // `setSnapshot` call already carries the freshest state.
        const reconciled = await invoke<DockSnapshot>('dock_state_get');
        if (!disposed && !eventSeen) setSnapshot(reconciled);
      })
      .catch(() => {
        // No Tauri runtime (test/dev harness without a mock) — snapshot
        // stays null; callers treat null as "not yet known".
      });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  return snapshot;
}

/** Pop a surface out to its own OS window, or focus it if already live
 * (spec §3). `context` is the surface's opaque continuity token (spec §7). */
export function popOut(surface: SurfaceId, context?: unknown): Promise<void> {
  return invoke('surface_pop_out', { surface, context });
}

/** Dock a surface back inline (spec §3) — ⇤/✕/Ctrl+W all resolve here; the
 * foreground-vs-availability distinction (spec §5) is a main-window
 * presentation concern, not a wire-level one. */
export function dockBack(surface: SurfaceId, context?: unknown): Promise<void> {
  return invoke('surface_dock_back', { surface, context });
}

/** Focus (unminimize + raise + activate) a popped surface's window (spec §5,
 * "Focus semantics") — backs every visual-pathway affordance. */
export function focusSurface(surface: SurfaceId): Promise<void> {
  return invoke('surface_focus', { surface });
}
