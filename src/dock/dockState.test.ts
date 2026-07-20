// Tests for the frontend dock-state wire mirror (bd tuxlink-dmwte, spec §3/§5/§6).
//
// `consentHostWindow` mirrors the Rust-canonical `consent_host_window`
// (src-tauri/src/dock/mod.rs) — cross-checked against the shared fixture in
// dockParity.test.ts, this file only exercises its own two branches.
//
// `useDockState` must observe the listen-FIRST discipline (spec §5): the
// `dock:changed` listener registration is awaited BEFORE `dock_state_get` is
// ever invoked, closing the get-then-subscribe gap that would otherwise
// strand a permanent pathway to a nonexistent window (adrev R2-F5). A
// reconcile `dock_state_get` follows the initial read.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import {
  consentHostWindow,
  useDockState,
  popOut,
  dockBack,
  focusSurface,
  type DockSurfaces,
  type DockSnapshot,
} from './dockState';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

function surfaces(partial: Partial<DockSurfaces> = {}): DockSurfaces {
  return {
    routines: 'docked',
    tac_map: 'docked',
    aprs_chat: 'docked',
    elmer: 'docked',
    station_intelligence: 'docked',
    ...partial,
  };
}

function snapshot(partial: Partial<DockSurfaces> = {}): DockSnapshot {
  return {
    surfaces: surfaces(partial),
    context: {
      routines: null,
      tac_map: null,
      aprs_chat: null,
      elmer: null,
      station_intelligence: null,
    },
  };
}

describe('consentHostWindow', () => {
  it('resolves to main when Routines is docked', () => {
    expect(consentHostWindow(surfaces({ routines: 'docked' }))).toBe('main');
  });

  it('resolves to pop-routines when Routines is popped', () => {
    expect(consentHostWindow(surfaces({ routines: 'popped' }))).toBe('pop-routines');
  });
});

describe('useDockState', () => {
  let callOrder: string[];
  let resolveListen: (fn: () => void) => void;
  const unlistenFn = vi.fn();

  beforeEach(() => {
    mockInvoke.mockReset();
    mockListen.mockReset();
    unlistenFn.mockReset();
    callOrder = [];

    mockListen.mockImplementation(() => {
      callOrder.push('listen');
      return new Promise<() => void>((resolve) => {
        resolveListen = (fn) => resolve(fn);
      });
    });

    // Teardown pitfall (vitest): invoke mocks are called with NO args at
    // cleanup — the cmd === undefined branch must be inert, never throw.
    mockInvoke.mockImplementation((cmd?: string) => {
      if (cmd === undefined) return Promise.resolve();
      callOrder.push(`invoke:${cmd}`);
      return Promise.resolve(snapshot());
    });
  });

  it('awaits listen registration before invoking dock_state_get, then reconciles with one more read', async () => {
    renderHook(() => useDockState());

    // listen() fires synchronously on mount...
    expect(mockListen).toHaveBeenCalledWith('dock:changed', expect.any(Function));
    // ...but invoke must NOT fire until the listen promise resolves.
    expect(mockInvoke).not.toHaveBeenCalled();

    resolveListen(unlistenFn);
    await waitFor(() => expect(callOrder.length).toBeGreaterThanOrEqual(3));

    expect(callOrder).toEqual(['listen', 'invoke:dock_state_get', 'invoke:dock_state_get']);
  });

  it('is null until the first read lands, then reflects the snapshot', async () => {
    const { result } = renderHook(() => useDockState());
    expect(result.current).toBeNull();

    resolveListen(unlistenFn);
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current?.surfaces.routines).toBe('docked');
  });

  it('updates from a dock:changed event after the listener is live', async () => {
    let handler: ((e: { payload: DockSnapshot }) => void) | undefined;
    mockListen.mockImplementation((_event: string, cb: (e: { payload: DockSnapshot }) => void) => {
      handler = cb;
      return Promise.resolve(unlistenFn);
    });

    const { result } = renderHook(() => useDockState());
    await waitFor(() => expect(result.current).not.toBeNull());

    handler?.({ payload: snapshot({ routines: 'popped' }) });
    await waitFor(() => expect(result.current?.surfaces.routines).toBe('popped'));
  });

  it('a dock:changed event landing during the in-flight INITIAL get wins over the initial response (TOCTOU guard, both legs of review-loop-3 F1)', async () => {
    let handler: ((e: { payload: DockSnapshot }) => void) | undefined;
    mockListen.mockImplementation((_event: string, cb: (e: { payload: DockSnapshot }) => void) => {
      handler = cb;
      return Promise.resolve(unlistenFn);
    });

    // --- Leg 1: the event lands while the INITIAL get is still in flight ---
    // Both the initial and reconcile invokes resolve to the same pending
    // promise here — only the initial apply's guard is under test in this
    // leg, so the reconcile apply resolving alongside it is inert either way.
    let resolveInitialGet!: (v: DockSnapshot) => void;
    const initialGet = new Promise<DockSnapshot>((resolve) => {
      resolveInitialGet = resolve;
    });
    mockInvoke.mockImplementation((cmd?: string) => {
      if (cmd === undefined) return Promise.resolve();
      return initialGet;
    });

    const { result: initialLegResult, unmount: unmountInitialLeg } = renderHook(() => useDockState());
    expect(initialLegResult.current).toBeNull();

    // A real dock:changed event lands BEFORE the initial get resolves — it
    // is newer than anything the initial get can return.
    handler?.({ payload: snapshot({ routines: 'popped' }) });
    await waitFor(() => expect(initialLegResult.current?.surfaces.routines).toBe('popped'));

    // The initial get NOW resolves with a STALE ('docked') snapshot. It
    // must NOT clobber the event's newer 'popped' state.
    await act(async () => {
      resolveInitialGet(snapshot({ routines: 'docked' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(initialLegResult.current?.surfaces.routines).toBe('popped');
    unmountInitialLeg();

    // --- Leg 2: the event lands while the RECONCILE get is still in flight ---
    // `handler` is reassigned by the still-active `mockListen.mockImplementation`
    // above as soon as this leg's `renderHook` mounts and re-subscribes.
    let resolveFirstGet!: (v: DockSnapshot) => void;
    let resolveReconcileGet!: (v: DockSnapshot) => void;
    const firstGet = new Promise<DockSnapshot>((resolve) => {
      resolveFirstGet = resolve;
    });
    const reconcileGet = new Promise<DockSnapshot>((resolve) => {
      resolveReconcileGet = resolve;
    });
    let getCalls = 0;
    mockInvoke.mockImplementation((cmd?: string) => {
      if (cmd === undefined) return Promise.resolve();
      getCalls += 1;
      return getCalls === 1 ? firstGet : reconcileGet;
    });

    const { result } = renderHook(() => useDockState());

    // First get resolves with the docked baseline.
    resolveFirstGet(snapshot({ routines: 'docked' }));
    await waitFor(() => expect(result.current?.surfaces.routines).toBe('docked'));

    // A real dock:changed event lands while the reconcile get is still
    // in flight — it is NEWER than anything the reconcile can return.
    handler?.({ payload: snapshot({ routines: 'popped' }) });
    await waitFor(() => expect(result.current?.surfaces.routines).toBe('popped'));

    // The reconcile get NOW resolves with a STALE ('docked') snapshot. It
    // must NOT clobber the event's newer 'popped' state.
    await act(async () => {
      resolveReconcileGet(snapshot({ routines: 'docked' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(result.current?.surfaces.routines).toBe('popped');
  });
});

describe('popOut / dockBack / focusSurface', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd?: string) => (cmd === undefined ? Promise.resolve() : Promise.resolve(undefined)));
  });

  it('popOut invokes surface_pop_out with the surface and context', async () => {
    await popOut('tac_map', { foo: 'bar' });
    expect(mockInvoke).toHaveBeenCalledWith('surface_pop_out', { surface: 'tac_map', context: { foo: 'bar' } });
  });

  it('popOut omits context when not supplied', async () => {
    await popOut('routines');
    expect(mockInvoke).toHaveBeenCalledWith('surface_pop_out', { surface: 'routines', context: undefined });
  });

  it('dockBack invokes surface_dock_back with the surface and context', async () => {
    await dockBack('routines', { view: 'designer' });
    expect(mockInvoke).toHaveBeenCalledWith('surface_dock_back', { surface: 'routines', context: { view: 'designer' } });
  });

  it('focusSurface invokes surface_focus with just the surface', async () => {
    await focusSurface('aprs_chat');
    expect(mockInvoke).toHaveBeenCalledWith('surface_focus', { surface: 'aprs_chat' });
  });
});
