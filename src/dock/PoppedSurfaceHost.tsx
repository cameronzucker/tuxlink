// src/dock/PoppedSurfaceHost.tsx — the popped-window shell (spec §4).
// bd tuxlink-dmwte task 7. Mounted by App.tsx for every `/pop/<surface>`
// route. Owns: the title bar (⇤/min/max/✕), the close-intent round-trip that
// flushes the surface's continuity token before the backend's 1.5s liveness
// timeout fires (spec §3 "Close handling"), Ctrl+W (semantically the same
// close), the per-window theme apply + cross-window storage listener (spec
// §4 "Theme propagation", adrev R5-F9), the mini status strip (chrome option
// B), and — for Routines only — the always-mounted ConsentGate (spec §6).
import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { SurfaceId, DockSnapshot } from './dockState';
import { dockBack } from './dockState';
import { PopTitleBar } from './PopTitleBar';
import { SURFACE_REGISTRY } from './surfaceRegistry';
import { ConsentGate } from '../routines/ConsentGate';
import {
  applyColorScheme,
  loadColorScheme,
  COLOR_SCHEME_STORAGE_KEY,
  CUSTOM_THEME_STORAGE_KEY,
} from '../shell/colorScheme';
import './PoppedSurfaceHost.css';

export function PoppedSurfaceHost({ surface }: { surface: SurfaceId }) {
  const entry = SURFACE_REGISTRY[surface];

  // The continuity token's `context` half (spec §7) — fetched once at mount
  // from `dock_state_get`, the "destination host consumes it at mount" read
  // the spec describes. Null until loaded (and null if absent on the wire).
  const [context, setContext] = useState<unknown | null>(null);
  const [contextLoaded, setContextLoaded] = useState(false);
  useEffect(() => {
    let mounted = true;
    invoke<DockSnapshot>('dock_state_get')
      .then((snap) => {
        if (mounted) setContext(snap.context[surface] ?? null);
      })
      .catch(() => {
        // No Tauri runtime (test/dev harness) — stays null.
      })
      .finally(() => {
        if (mounted) setContextLoaded(true);
      });
    return () => {
      mounted = false;
    };
  }, [surface]);

  // The surface's live state-collector (registered via registerGetContext).
  // Surfaces with nothing to carry (tac_map, aprs_chat) never call it, so
  // this stays null and every dock-back for them carries state: null.
  const getContextRef = useRef<(() => unknown | null) | null>(null);
  const registerGetContext = useCallback((fn: () => unknown | null) => {
    getContextRef.current = fn;
  }, []);

  const collectState = useCallback((): unknown | null => {
    return getContextRef.current ? getContextRef.current() : null;
  }, []);

  /** ✕ / Ctrl+W / close-intent semantics — availability, not foreground
   *  (spec §3). */
  const runClose = useCallback(() => {
    void dockBack(surface, { foreground: false, state: collectState() });
  }, [surface, collectState]);

  /** ⇤ Dock back — foreground semantics (spec §4). */
  const runDockBack = useCallback(() => {
    void dockBack(surface, { foreground: true, state: collectState() });
  }, [surface, collectState]);

  // Close-intent round-trip (spec §3 "Close handling"): the backend catches
  // the WM's CloseRequested, calls prevent_close, and emits this event so the
  // webview can flush its continuity token before the 1.5s liveness timeout
  // falls through to a stateless dock-back. Belt-and-braces surface check —
  // a broadcast-emitting backend bug must never dock every window back.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<{ surface: SurfaceId }>('dock:close-intent', (event) => {
      if (event.payload.surface === surface) runClose();
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // No Tauri runtime (test/dev harness).
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [surface, runClose]);

  // Ctrl+W — semantically honest: close IS dock-back (spec §4).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'w') {
        e.preventDefault();
        runClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [runClose]);

  // Theme propagation (spec §4, adrev R5-F9): apply the stored scheme at
  // mount exactly as main.tsx does at boot, then re-apply on a `storage`
  // event touching EITHER key — the color scheme itself, or the custom-theme
  // token set (colorScheme.ts:57-58; scheme-key-only would leave popped
  // windows stale on a custom-theme edit while scheme stays 'custom').
  useEffect(() => {
    applyColorScheme(loadColorScheme());
    const onStorage = (e: StorageEvent) => {
      if (e.key === COLOR_SCHEME_STORAGE_KEY || e.key === CUSTOM_THEME_STORAGE_KEY) {
        // broadcast:false — we're responding to another window's change, not
        // originating one (tuxlink-och6 loop guard, mirrors useHelpTheme.ts).
        applyColorScheme(loadColorScheme(), { broadcast: false });
      }
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  const Component = entry.Component;
  const StatusStrip = entry.StatusStrip;

  return (
    <div className="pop-surface-host" data-testid={`pop-surface-host-${surface}`}>
      <PopTitleBar title={entry.title} onDockBack={runDockBack} onClose={runClose} />
      <div className="pop-surface-body">
        {/* Consent modal mounts here for Routines only (spec §6) — always
         *  mounted, self-managing, like AppShell's instance. Task 8 wires the
         *  gating prop that lets it reach out of this window when needed. */}
        {surface === 'routines' && <ConsentGate />}
        {contextLoaded && (
          <Component context={context} registerGetContext={registerGetContext} />
        )}
      </div>
      <StatusStrip />
    </div>
  );
}
