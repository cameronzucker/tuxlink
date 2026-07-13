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

    // ArrowRight: passes through AND advances the tour.
    const evArrow = new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true });
    const preventArrow = vi.spyOn(evArrow, 'preventDefault');
    act(() => window.dispatchEvent(evArrow));
    expect(preventArrow).not.toHaveBeenCalled();
    await waitFor(() => expect(activeOf()).toEqual({ kind: 'tour', stepIndex: 1 }));

    // Escape: passes through AND skips (persists + clears active).
    const evEsc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    const preventEsc = vi.spyOn(evEsc, 'preventDefault');
    act(() => window.dispatchEvent(evEsc));
    expect(preventEsc).not.toHaveBeenCalled();
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
});
