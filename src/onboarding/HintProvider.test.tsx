// HintProvider tests (tuxlink-10bkw Task 5) — one `it` per behavior-contract
// bullet in task-5-brief.md. Conventions mirror ContactsPanel.test.tsx.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { HintProvider, useHints } from './HintProvider';

type ConfigRead = { onboarding_tour_completed: boolean; onboarding_tips_seen: string[] };
type PointAtEvent = { payload: { request_id: number; anchor_id: string } };
type PointAtListener = (event: PointAtEvent) => void;

/** Routes invoke() by command name; config_set_onboarding/ack default to resolving. */
function routeInvoke(configRead: ConfigRead, opts: { rejectSetOnboarding?: boolean } = {}) {
  vi.mocked(invoke).mockImplementation((async (cmd: string) => {
    if (cmd === 'config_read') return configRead;
    if (cmd === 'config_set_onboarding') {
      if (opts.rejectSetOnboarding) throw new Error('disk full');
      return undefined;
    }
    if (cmd === 'onboarding_point_at_ack') return undefined;
    return undefined;
  }) as typeof invoke);
}

/** Captures the onboarding:point-at listener callback registered via listen(). */
function captureListener(): { get: () => PointAtListener | undefined } {
  let captured: PointAtListener | undefined;
  vi.mocked(listen).mockImplementation((async (_evt: string, cb: PointAtListener) => {
    captured = cb;
    return () => {};
  }) as unknown as typeof listen);
  return { get: () => captured };
}

/** Gives a test fixture's `[data-tour-anchor]` div a non-zero rect. jsdom's
 *  default getBoundingClientRect() is all-zero for every element (no real
 *  layout engine); fixwave finding #1 now treats a zero rect the same as
 *  "anchor not found" (the confirmed live case is a `display:contents`
 *  wrapper), so tests exercising the "mounted and visible" path need a real
 *  fixture anchor to have a real-looking rect, same as HintOverlay.test.tsx's
 *  stubRect helper. */
function stubAnchorRect(anchorId: string): void {
  const el = document.querySelector(`[data-tour-anchor="${anchorId}"]`);
  if (!el) throw new Error(`stubAnchorRect: no element for anchor "${anchorId}"`);
  (el as HTMLElement).getBoundingClientRect = () =>
    ({
      top: 100, left: 50, width: 120, height: 40, bottom: 140, right: 170, x: 50, y: 100,
      toJSON() { return this; },
    }) as DOMRect;
}

function Probe({ anchors = [] as string[] }: { anchors?: string[] }) {
  const hints = useHints();
  return (
    <div>
      <pre data-testid="active">{JSON.stringify(hints.active)}</pre>
      <span data-testid="overlay-active">{String(hints.overlayActive)}</span>
      <button onClick={hints.startTour}>startTour</button>
      <button onClick={hints.advance}>advance</button>
      <button onClick={hints.back}>back</button>
      <button onClick={hints.skipTour}>skipTour</button>
      <button onClick={hints.declineOffer}>declineOffer</button>
      <button onClick={hints.dismissSingle}>dismissSingle</button>
      <button onClick={hints.abandonSingle}>abandonSingle</button>
      <button onClick={() => hints.requestFirstOpenTip('find-a-station')}>requestTip</button>
      {anchors.map((a) => (
        <div key={a} data-tour-anchor={a} />
      ))}
    </div>
  );
}

function activeOf(): unknown {
  return JSON.parse(screen.getByTestId('active').textContent!);
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  vi.mocked(listen).mockImplementation(async () => () => {});
});

describe('HintProvider', () => {
  it('bullet 1: mount reads config_read; false -> offer, true -> null', async () => {
    routeInvoke({ onboarding_tour_completed: false, onboarding_tips_seen: [] });
    const { unmount } = render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'offer' }));
    unmount();

    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('config_read'));
    expect(activeOf()).toBeNull();
  });

  it('bullet 2: decline/skip/finish persist tourCompleted:true + current tipsSeen; rejection still advances state', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    // declineOffer persists.
    routeInvoke({ onboarding_tour_completed: false, onboarding_tips_seen: ['aprs'] });
    const { unmount } = render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'offer' }));
    fireEvent.click(screen.getByText('declineOffer'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('config_set_onboarding', {
        tourCompleted: true,
        tipsSeen: ['aprs'],
      });
    });
    expect(activeOf()).toBeNull();
    unmount();

    // skipTour, with the backend write rejecting: local state still advances.
    routeInvoke({ onboarding_tour_completed: false, onboarding_tips_seen: [] }, { rejectSetOnboarding: true });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'offer' }));
    fireEvent.click(screen.getByText('startTour'));
    expect(activeOf()).toEqual({ kind: 'tour', stepIndex: 0 });
    fireEvent.click(screen.getByText('skipTour'));
    expect(activeOf()).toBeNull(); // advanced despite the invoke rejection below
    await waitFor(() => expect(errSpy).toHaveBeenCalled());

    errSpy.mockRestore();
  });

  it('bullet 3: requestFirstOpenTip shows iff unseen + idle; busy suppresses without consuming', async () => {
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    const { unmount } = render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toBeNull());
    fireEvent.click(screen.getByText('requestTip'));
    await waitFor(() => {
      const active = activeOf() as { kind: string; entry: { id: string } };
      expect(active.kind).toBe('single');
      expect(active.entry.id).toBe('find-a-station');
    });
    unmount();

    // Busy (offer active): request is suppressed, not consumed.
    routeInvoke({ onboarding_tour_completed: false, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'offer' }));
    fireEvent.click(screen.getByText('requestTip'));
    expect(activeOf()).toEqual({ kind: 'offer' }); // unchanged — suppressed
    fireEvent.click(screen.getByText('declineOffer'));
    await waitFor(() => expect(activeOf()).toBeNull());
    fireEvent.click(screen.getByText('requestTip')); // still eligible — was never consumed
    await waitFor(() => {
      const active = activeOf() as { kind: string };
      expect(active.kind).toBe('single');
    });
  });

  it('bullet 4: dismissSingle persists markTipSeen for a tip; persists nothing for a point-at', async () => {
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    const { unmount } = render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toBeNull());
    fireEvent.click(screen.getByText('requestTip'));
    await waitFor(() => expect((activeOf() as { kind: string }).kind).toBe('single'));
    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByText('dismissSingle'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('config_set_onboarding', {
        tourCompleted: true,
        tipsSeen: ['find-a-station'],
      });
    });
    expect(activeOf()).toBeNull();
    unmount();

    // point-at path: dismissing persists nothing.
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe anchors={['contacts']} />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    stubAnchorRect('contacts');
    act(() => listener.get()!({ payload: { request_id: 1, anchor_id: 'contacts' } }));
    await waitFor(() => expect((activeOf() as { source: string }).source).toBe('point-at'));
    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByText('dismissSingle'));
    expect(activeOf()).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith('config_set_onboarding', expect.anything());
  });

  it('bullet 5: capture-phase keydown swallows non-tour keys while overlay active; passes through otherwise', async () => {
    routeInvoke({ onboarding_tour_completed: false, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'offer' }));

    // Offer card never blocks typing.
    const evOffer = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
    const preventOffer = vi.spyOn(evOffer, 'preventDefault');
    act(() => window.dispatchEvent(evOffer));
    expect(preventOffer).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText('startTour'));
    await waitFor(() => expect(screen.getByTestId('overlay-active').textContent).toBe('true'));

    // Non-tour key while overlay active: swallowed.
    const evSwallow = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
    const preventSwallow = vi.spyOn(evSwallow, 'preventDefault');
    const stopSwallow = vi.spyOn(evSwallow, 'stopPropagation');
    act(() => window.dispatchEvent(evSwallow));
    expect(preventSwallow).toHaveBeenCalled();
    expect(stopSwallow).toHaveBeenCalled();

    // ArrowRight: consumed (fixwave finding #3 — a handled key must not also
    // leak to a listener underneath the overlay) AND advances the tour.
    const evArrow = new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true });
    const preventArrow = vi.spyOn(evArrow, 'preventDefault');
    const stopArrow = vi.spyOn(evArrow, 'stopPropagation');
    act(() => window.dispatchEvent(evArrow));
    expect(preventArrow).toHaveBeenCalled();
    expect(stopArrow).toHaveBeenCalled();
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'tour', stepIndex: 1 }));

    // Escape: consumed AND skips (persists + clears active) — kept global
    // regardless of focus location, per the finding #2/#3 fix.
    const evEsc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    const preventEsc = vi.spyOn(evEsc, 'preventDefault');
    const stopEsc = vi.spyOn(evEsc, 'stopPropagation');
    act(() => window.dispatchEvent(evEsc));
    expect(preventEsc).toHaveBeenCalled();
    expect(stopEsc).toHaveBeenCalled();
    await waitFor(() => expect(activeOf()).toBeNull());

    // Inactive: no-op passthrough.
    const evInactive = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
    const preventInactive = vi.spyOn(evInactive, 'preventDefault');
    act(() => window.dispatchEvent(evInactive));
    expect(preventInactive).not.toHaveBeenCalled();
  });

  it('bullet 6: point-at acks unknown-anchor / anchor-unmounted / overlay-busy / shown', async () => {
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe anchors={['find-a-station']} />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    await waitFor(() => expect(activeOf()).toBeNull());

    // Unknown anchor.
    act(() => listener.get()!({ payload: { request_id: 1, anchor_id: 'nope' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 1, outcome: 'unknown-anchor' }),
      );
    });
    const unknownCall = vi
      .mocked(invoke)
      .mock.calls.find((c) => c[0] === 'onboarding_point_at_ack' && (c[1] as { requestId: number }).requestId === 1);
    expect((unknownCall?.[1] as { validIds: string[] }).validIds).toEqual(
      expect.arrayContaining(['find-a-station', 'contacts']),
    );

    // Known anchor, but not mounted in the DOM -> anchor-unmounted.
    act(() => listener.get()!({ payload: { request_id: 2, anchor_id: 'contacts' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 2, outcome: 'anchor-unmounted', openHint: expect.any(String) }),
      );
    });

    // Known + mounted, idle -> shown.
    stubAnchorRect('find-a-station');
    act(() => listener.get()!({ payload: { request_id: 3, anchor_id: 'find-a-station' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 3, outcome: 'shown' }),
      );
    });
    await waitFor(() => {
      const active = activeOf() as { kind: string; source: string };
      expect(active.kind).toBe('single');
      expect(active.source).toBe('point-at');
    });

    // Tour running -> overlay-busy (a mounted, valid anchor is blocked by the tour).
    fireEvent.click(screen.getByText('startTour'));
    await waitFor(() => expect(screen.getByTestId('overlay-active').textContent).toBe('true'));
    act(() => listener.get()!({ payload: { request_id: 4, anchor_id: 'find-a-station' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 4, outcome: 'overlay-busy' }),
      );
    });
  });

  it('bullet 6 precedence: overlay-busy wins over anchor-unmounted while a tour is active', async () => {
    // point_at for a known-but-UNMOUNTED anchor while the tour is running must
    // ack overlay-busy, NOT anchor-unmounted — openHint navigation guidance is
    // actively wrong while a modal tour is capturing the UI.
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    await waitFor(() => expect(activeOf()).toBeNull());

    fireEvent.click(screen.getByText('startTour'));
    await waitFor(() => expect(screen.getByTestId('overlay-active').textContent).toBe('true'));

    // 'compose' is a known registry entry with no data-tour-anchor rendered here.
    act(() => listener.get()!({ payload: { request_id: 7, anchor_id: 'compose' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 7, outcome: 'overlay-busy' }),
      );
    });
    expect(invoke).not.toHaveBeenCalledWith(
      'onboarding_point_at_ack',
      expect.objectContaining({ requestId: 7, outcome: 'anchor-unmounted' }),
    );
  });

  // tuxlink-10bkw Task 9: menu chrome anchors (MENU_POINT_AT_ENTRIES) are
  // point-at-only — they must resolve through the same lookup as tour/tip
  // entries, including the "menu closed -> anchor-unmounted with openHint"
  // and "unknown-anchor validIds" paths.
  it('Task 9: menu item point-at acks anchor-unmounted with the menu openHint while the menu is closed', async () => {
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    await waitFor(() => expect(activeOf()).toBeNull());

    // 'menu:help:replay_tour' is a real MENU_POINT_AT_ENTRIES anchor, but no
    // DOM node carries it here — the menu-item element only exists in the DOM
    // while its parent menu is open (MenuBar renders it conditionally).
    act(() => listener.get()!({ payload: { request_id: 10, anchor_id: 'menu:help:replay_tour' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({
          requestId: 10,
          outcome: 'anchor-unmounted',
          openHint: 'Open the Help menu first — this entry lives inside it.',
        }),
      );
    });
    expect(activeOf()).toBeNull();
  });

  it('Task 9: unknown-anchor validIds includes menu chrome ids alongside tour/tip ids', async () => {
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    await waitFor(() => expect(activeOf()).toBeNull());

    act(() => listener.get()!({ payload: { request_id: 11, anchor_id: 'still-nope' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 11, outcome: 'unknown-anchor' }),
      );
    });
    const call = vi
      .mocked(invoke)
      .mock.calls.find(
        (c) => c[0] === 'onboarding_point_at_ack' && (c[1] as { requestId: number }).requestId === 11,
      );
    expect((call?.[1] as { validIds: string[] }).validIds).toEqual(
      expect.arrayContaining(['menu:tools', 'menu:help', 'menu:help:replay_tour', 'find-a-station']),
    );
  });

  // Fixwave finding #5: abandonSingle() (the auto-skip-fallback path) must
  // clear the active hint WITHOUT persisting tips_seen — suppressed, not
  // consumed — unlike dismissSingle() (the user-clicked "Got it" path, which
  // DOES persist for a tip; see bullet 4 above).
  it('fixwave #5: abandonSingle clears without persisting; the tip stays eligible for a future request', async () => {
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );
    await waitFor(() => expect(activeOf()).toBeNull());

    fireEvent.click(screen.getByText('requestTip'));
    await waitFor(() => {
      const active = activeOf() as { kind: string; entry: { id: string } };
      expect(active.kind).toBe('single');
      expect(active.entry.id).toBe('find-a-station');
    });

    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByText('abandonSingle'));
    expect(activeOf()).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith('config_set_onboarding', expect.anything());

    // Still eligible: requesting the same tip again shows it — tipsSeen was
    // never mutated by the abandon.
    fireEvent.click(screen.getByText('requestTip'));
    await waitFor(() => {
      const active = activeOf() as { kind: string; entry: { id: string } };
      expect(active.kind).toBe('single');
      expect(active.entry.id).toBe('find-a-station');
    });
  });

  // Fixwave finding #1: an anchor that IS in the DOM but lays out with a
  // zero-size rect (the live case is RadioDrawer's `display:contents` root at
  // desktop widths, anchor "radio-dock") must be treated as anchor-missing —
  // point-at acks anchor-unmounted, not shown.
  it('fixwave #1: a zero-rect anchor acks anchor-unmounted, not shown', async () => {
    const listener = captureListener();
    routeInvoke({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
    render(
      <HintProvider>
        <Probe anchors={['contacts']} />
      </HintProvider>,
    );
    await waitFor(() => expect(listener.get()).toBeDefined());
    await waitFor(() => expect(activeOf()).toBeNull());

    // 'contacts' IS in the DOM (Probe rendered it) but jsdom's default
    // getBoundingClientRect() is all-zero — exactly the display:contents
    // shape. Deliberately do NOT call stubAnchorRect here.
    act(() => listener.get()!({ payload: { request_id: 20, anchor_id: 'contacts' } }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'onboarding_point_at_ack',
        expect.objectContaining({ requestId: 20, outcome: 'anchor-unmounted', openHint: expect.any(String) }),
      );
    });
    expect(invoke).not.toHaveBeenCalledWith(
      'onboarding_point_at_ack',
      expect.objectContaining({ requestId: 20, outcome: 'shown' }),
    );
    expect(activeOf()).toBeNull();
  });

  // Fixwave finding #6: requestFirstOpenTip called before config_read
  // resolves must NOT consult the initialState placeholder
  // (tourCompleted:true, tipsSeen:[]) — it queues, and is re-checked against
  // the REAL config once hydration completes.
  it('fixwave #6: requestFirstOpenTip before hydration queues instead of showing, and drains against the real config', async () => {
    // Scenario A: the request arrives before config_read resolves; the
    // resolved tipsSeen is the "everything seen" sentinel — the queued
    // request must stay suppressed once hydration completes, exactly as a
    // synchronous post-hydration request would be.
    let resolveA!: (v: ConfigRead) => void;
    const configA = new Promise<ConfigRead>((res) => {
      resolveA = res;
    });
    vi.mocked(invoke).mockImplementation((async (cmd: string) => {
      if (cmd === 'config_read') return configA;
      return undefined;
    }) as typeof invoke);
    const { unmount } = render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );

    fireEvent.click(screen.getByText('requestTip'));
    expect(activeOf()).toBeNull(); // queued, not shown — no premature display

    await act(async () => {
      resolveA({ onboarding_tour_completed: true, onboarding_tips_seen: ['*'] });
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(activeOf()).toBeNull(); // drained against the sentinel -> still nothing
    unmount();

    // Scenario B: same before-hydration request, but the resolved tipsSeen is
    // empty — the queued request DOES show once hydration completes.
    let resolveB!: (v: ConfigRead) => void;
    const configB = new Promise<ConfigRead>((res) => {
      resolveB = res;
    });
    vi.mocked(invoke).mockImplementation((async (cmd: string) => {
      if (cmd === 'config_read') return configB;
      return undefined;
    }) as typeof invoke);
    render(
      <HintProvider>
        <Probe />
      </HintProvider>,
    );

    fireEvent.click(screen.getByText('requestTip'));
    expect(activeOf()).toBeNull();

    await act(async () => {
      resolveB({ onboarding_tour_completed: true, onboarding_tips_seen: [] });
      await Promise.resolve();
      await Promise.resolve();
    });
    await waitFor(() => {
      const active = activeOf() as { kind: string; entry: { id: string } };
      expect(active.kind).toBe('single');
      expect(active.entry.id).toBe('find-a-station');
    });
  });
});
