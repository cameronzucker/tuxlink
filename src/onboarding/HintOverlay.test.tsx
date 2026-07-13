// HintOverlay component test (tuxlink-10bkw Task 6). Drives a real
// <HintProvider> + a fake anchored button through the public useHints() API
// (mirrors the harness pattern in HintProvider.test.tsx) and asserts the
// overlay's geometry, a11y, and focus contract from task-6-brief.md.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { invoke } from '@tauri-apps/api/core';
import { HintProvider, useHints } from './HintProvider';
import { HintOverlay } from './HintOverlay';

/** A stable, known rect for the fake anchor button. */
const ANCHOR_RECT: Partial<DOMRect> = { top: 100, left: 50, width: 120, height: 40, bottom: 140, right: 170 };

function stubRect(el: Element, rect: Partial<DOMRect>) {
  el.getBoundingClientRect = () => ({
    top: 0, left: 0, width: 0, height: 0, bottom: 0, right: 0, x: 0, y: 0,
    toJSON() { return this; },
    ...rect,
  }) as DOMRect;
}

/** Drives the provider via its public actions, mirroring Probe in HintProvider.test.tsx. */
function Harness({ onAnchorClick, showMailboxAnchor = false }: { onAnchorClick: () => void; showMailboxAnchor?: boolean }) {
  const hints = useHints();
  return (
    <div>
      <button data-tour-anchor="ribbon-connect" onClick={onAnchorClick}>
        Connect
      </button>
      {showMailboxAnchor && <div data-tour-anchor="mailbox" />}
      <button onClick={hints.startTour}>startTour</button>
      <button onClick={hints.advance}>advance</button>
      <button onClick={hints.back}>back</button>
      <button onClick={hints.skipTour}>skipTour</button>
      <HintOverlay />
    </div>
  );
}

/** A tip-only harness (no tour): 'find-a-station' has fallback:'skip', so it
 *  drives the auto-skip path exercised by fixwave findings #3/#5. */
function TipHarness({ showAnchor = true }: { showAnchor?: boolean }) {
  const hints = useHints();
  return (
    <div>
      {showAnchor && <div data-tour-anchor="find-a-station" />}
      <button onClick={() => hints.requestFirstOpenTip('find-a-station')}>requestTip</button>
      <HintOverlay />
    </div>
  );
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { onboarding_tour_completed: true, onboarding_tips_seen: [] };
    return undefined;
  });
});

describe('HintOverlay', () => {
  it('renders 4 panels + 1 blocker over the anchor hole once the tour starts', async () => {
    const onAnchorClick = vi.fn();
    render(
      <HintProvider>
        <Harness onAnchorClick={onAnchorClick} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    stubRect(screen.getByText('Connect'), ANCHOR_RECT);

    fireEvent.click(screen.getByText('startTour'));

    const panels = await waitFor(() => {
      const els = screen.getAllByTestId('hint-overlay-panel');
      expect(els).toHaveLength(4);
      return els;
    });
    expect(panels).toHaveLength(4);

    const blocker = screen.getByTestId('hint-overlay-blocker');
    // The blocker sits exactly over the (padded) anchor hole.
    expect(blocker.style.top).toBe('92px'); // 100 - 8 padding
    expect(blocker.style.left).toBe('42px'); // 50 - 8 padding
    expect(blocker.style.width).toBe('136px'); // 120 + 16 padding
    expect(blocker.style.height).toBe('56px'); // 40 + 16 padding
  });

  it('the popover has dialog a11y attributes wired to real title/body ids', async () => {
    render(
      <HintProvider>
        <Harness onAnchorClick={() => {}} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    stubRect(screen.getByText('Connect'), ANCHOR_RECT);
    fireEvent.click(screen.getByText('startTour'));

    const popover = await waitFor(() => screen.getByTestId('hint-overlay-popover'));
    expect(popover).toHaveAttribute('role', 'dialog');
    expect(popover).toHaveAttribute('aria-modal', 'false');
    const labelledBy = popover.getAttribute('aria-labelledby');
    const describedBy = popover.getAttribute('aria-describedby');
    expect(labelledBy).toBeTruthy();
    expect(describedBy).toBeTruthy();
    expect(document.getElementById(labelledBy!)).toHaveTextContent('Connect');
    expect(document.getElementById(describedBy!)?.textContent).toBeTruthy();
    expect(screen.getByTestId('hint-overlay-live')).toHaveTextContent('Step 1 of 5: Connect');
  });

  it('focus lands on Next when the tour opens and returns to the anchor after Skip tour', async () => {
    render(
      <HintProvider>
        <Harness onAnchorClick={() => {}} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    const anchorBtn = screen.getByText('Connect');
    stubRect(anchorBtn, ANCHOR_RECT);
    anchorBtn.focus();
    expect(document.activeElement).toBe(anchorBtn);

    fireEvent.click(screen.getByText('startTour'));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId('hint-overlay-next')));

    fireEvent.click(screen.getByText('skipTour'));
    await waitFor(() => expect(screen.queryByTestId('hint-overlay-popover')).toBeNull());
    expect(document.activeElement).toBe(anchorBtn);
  });

  it('fallback "center" (contacts) renders a centered popover with no panels once "mailbox" auto-skips', async () => {
    render(
      <HintProvider>
        <Harness onAnchorClick={() => {}} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    stubRect(screen.getByText('Connect'), ANCHOR_RECT);

    fireEvent.click(screen.getByText('startTour'));
    await waitFor(() => expect(screen.getByTestId('hint-overlay-next')).toBeInTheDocument());

    // Step 1 ('mailbox') has NO anchor rendered here and fallback:'skip' — the
    // overlay must auto-advance past it to step 2 ('contacts', fallback:'center'),
    // which ALSO has no anchor rendered here, so it renders centered.
    fireEvent.click(screen.getByTestId('hint-overlay-next'));

    await waitFor(() => {
      expect(screen.getByTestId('hint-overlay-live')).toHaveTextContent('Step 3 of 5: Contacts');
    });
    expect(screen.queryAllByTestId('hint-overlay-panel')).toHaveLength(0);
    expect(screen.queryByTestId('hint-overlay-blocker')).toBeNull();
    const popover = screen.getByTestId('hint-overlay-popover');
    expect(popover).toBeInTheDocument();
  });

  // Reviewer proof-gap #1 (Task 6 review): the collision-flip/clamp math was
  // never exercised with real geometry — jsdom reports offsetWidth/Height = 0
  // and the `|| 320` / `|| 160` fallbacks masked whether the measured popover
  // size was used at all. Stub the prototype getters to values that DIFFER
  // from the fallbacks (400×200, not 320×160) so these assertions can only
  // pass if the real measurement feeds the math.
  describe('collision flip + horizontal clamp (measured geometry)', () => {
    const POP_W = 400;
    const POP_H = 200;
    let origW: PropertyDescriptor | undefined;
    let origH: PropertyDescriptor | undefined;

    beforeEach(() => {
      origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
      origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
      Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => POP_W,
      });
      Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get: () => POP_H,
      });
      // vitest runs a beforeEach-returned function as the paired teardown.
      return () => {
        if (origW) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', origW);
        if (origH) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', origH);
      };
    });

    it('flips the popover ABOVE an anchor near the viewport bottom edge', async () => {
      render(
        <HintProvider>
          <Harness onAnchorClick={() => {}} />
        </HintProvider>,
      );
      await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
      // Anchor hugging the bottom: below-placement top would be
      // anchorTop + 40 + 12, and +200 of measured popover overflows
      // innerHeight − 8 margin → flip above.
      const anchorTop = window.innerHeight - 68; // 700 at jsdom's default 768
      stubRect(screen.getByText('Connect'), {
        top: anchorTop, left: 50, width: 120, height: 40,
        bottom: anchorTop + 40, right: 170,
      });
      fireEvent.click(screen.getByText('startTour'));

      const popover = await waitFor(() => screen.getByTestId('hint-overlay-popover'));
      const expectedTop = Math.max(8, anchorTop - POP_H - 12);
      await waitFor(() => expect(popover.style.top).toBe(`${expectedTop}px`));
      // The load-bearing property: it sits ABOVE the anchor, not below it…
      expect(parseFloat(popover.style.top)).toBeLessThan(anchorTop);
      // …and it used the MEASURED 200px height, not the 160px fallback (which
      // would have produced anchorTop − 172 instead of anchorTop − 212).
      expect(expectedTop).toBe(anchorTop - 200 - 12);
    });

    it('clamps the popover horizontally at the right edge (8px viewport margin held)', async () => {
      render(
        <HintProvider>
          <Harness onAnchorClick={() => {}} />
        </HintProvider>,
      );
      await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
      // Anchor hugging the right edge: left-aligned placement would put the
      // measured 400px popover past innerWidth − 8 → clamp left.
      const anchorLeft = window.innerWidth - 84; // 940 at jsdom's default 1024
      stubRect(screen.getByText('Connect'), {
        top: 100, left: anchorLeft, width: 120, height: 40,
        bottom: 140, right: anchorLeft + 120,
      });
      fireEvent.click(screen.getByText('startTour'));

      const popover = await waitFor(() => screen.getByTestId('hint-overlay-popover'));
      const expectedLeft = window.innerWidth - POP_W - 8;
      await waitFor(() => expect(popover.style.left).toBe(`${expectedLeft}px`));
      // Clamped inside the viewport, honoring the 8px margin on both sides,
      // with the MEASURED 400px width (the 320px fallback would land 80px
      // further right).
      expect(parseFloat(popover.style.left)).toBeGreaterThanOrEqual(8);
      expect(parseFloat(popover.style.left) + POP_W).toBeLessThanOrEqual(window.innerWidth - 8);
    });
  });

  it('clicking the blocker does not click the anchored button underneath', async () => {
    const onAnchorClick = vi.fn();
    render(
      <HintProvider>
        <Harness onAnchorClick={onAnchorClick} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    stubRect(screen.getByText('Connect'), ANCHOR_RECT);
    fireEvent.click(screen.getByText('startTour'));

    const blocker = await waitFor(() => screen.getByTestId('hint-overlay-blocker'));
    fireEvent.click(blocker);
    expect(onAnchorClick).not.toHaveBeenCalled();
  });

  // Fixwave finding #1: a zero-rect anchor (present in the DOM, but laid out
  // with no box — jsdom's default getBoundingClientRect() for any element,
  // matching the live RadioDrawer `display:contents` shape at desktop widths)
  // must be treated as anchor-missing: the entry's declared fallback engages
  // instead of a top-left-corner spotlight.
  it('fixwave #1: a zero-rect anchor engages the center fallback with no panels/blocker', async () => {
    render(
      <HintProvider>
        <Harness onAnchorClick={() => {}} />
      </HintProvider>,
    );
    await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
    // Deliberately NOT calling stubRect — the anchor keeps jsdom's default
    // all-zero rect, exactly the display:contents shape.

    fireEvent.click(screen.getByText('startTour'));

    const popover = await waitFor(() => screen.getByTestId('hint-overlay-popover'));
    expect(popover).toHaveClass('hint-overlay__popover--center');
    expect(screen.queryAllByTestId('hint-overlay-panel')).toHaveLength(0);
    expect(screen.queryByTestId('hint-overlay-blocker')).toBeNull();
  });

  // Fixwave findings #2/#3: Enter/Space activation on the popover's own
  // buttons must not double-fire, must not be swallowed, and consumed keys
  // (Escape, and Enter/Arrows outside the popover) must not leak past the
  // overlay to a listener underneath it.
  describe('fixwave #2/#3: keyboard policy — no double-advance, native activation preserved, handled keys consumed', () => {
    it('Enter on the focused Next button does not double-advance', async () => {
      render(
        <HintProvider>
          <Harness onAnchorClick={() => {}} />
        </HintProvider>,
      );
      await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
      stubRect(screen.getByText('Connect'), ANCHOR_RECT);
      fireEvent.click(screen.getByText('startTour'));

      const nextBtn = await waitFor(() => screen.getByTestId('hint-overlay-next'));
      await waitFor(() => expect(document.activeElement).toBe(nextBtn));
      expect(screen.getByTestId('hint-overlay-live')).toHaveTextContent('Step 1 of 5');

      // Dispatched directly ON the focused button, so e.target === nextBtn —
      // the capture-phase handler must see it's inside the popover dialog
      // and do nothing (no preventDefault, no synthesized advance()).
      const evEnter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
      const preventEnter = vi.spyOn(evEnter, 'preventDefault');
      nextBtn.dispatchEvent(evEnter);
      expect(preventEnter).not.toHaveBeenCalled();
      // Not advanced by our own handler — jsdom doesn't synthesize the
      // native "Enter-on-button triggers click" behavior a real browser
      // would have produced from the untouched event above.
      expect(screen.getByTestId('hint-overlay-live')).toHaveTextContent('Step 1 of 5');

      // Simulate that one real native follow-up click a browser would fire.
      // Lands on "Step 3 of 5: Contacts", not step 2 — step 1 ('mailbox') has
      // no anchor rendered in this harness and fallback:'skip', so the
      // UNRELATED auto-skip effect (HintOverlay.tsx) advances past it
      // automatically, exactly like the existing "fallback center" test
      // above. The point under test is that this ONE click produces exactly
      // ONE user-initiated advance (mailbox -> contacts), not two.
      fireEvent.click(nextBtn);
      await waitFor(() => expect(screen.getByTestId('hint-overlay-live')).toHaveTextContent('Step 3 of 5: Contacts'));
    });

    it('Space on the focused Skip button is not swallowed', async () => {
      render(
        <HintProvider>
          <Harness onAnchorClick={() => {}} />
        </HintProvider>,
      );
      await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
      stubRect(screen.getByText('Connect'), ANCHOR_RECT);
      fireEvent.click(screen.getByText('startTour'));

      const skipBtn = await waitFor(() => screen.getByTestId('hint-overlay-skip'));
      skipBtn.focus();
      expect(document.activeElement).toBe(skipBtn);

      const evSpace = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
      const preventSpace = vi.spyOn(evSpace, 'preventDefault');
      const stopSpace = vi.spyOn(evSpace, 'stopPropagation');
      skipBtn.dispatchEvent(evSpace);
      expect(preventSpace).not.toHaveBeenCalled();
      expect(stopSpace).not.toHaveBeenCalled();

      // Simulate the native Space-activation click this untouched event
      // would produce in a real browser.
      fireEvent.click(skipBtn);
      await waitFor(() => expect(screen.queryByTestId('hint-overlay-popover')).toBeNull());
    });

    it('Escape is consumed and does not reach a document-level listener underneath the overlay', async () => {
      render(
        <HintProvider>
          <TipHarness />
        </HintProvider>,
      );
      const anchorDiv = document.querySelector('[data-tour-anchor="find-a-station"]')!;
      stubRect(anchorDiv, ANCHOR_RECT);
      // Fires whether config_read has resolved yet or not — pre-hydration it
      // queues (finding #6) and shows once hydration drains the queue;
      // post-hydration it shows immediately. Either way the popover appears.
      fireEvent.click(screen.getByText('requestTip'));
      await waitFor(() => expect(screen.getByTestId('hint-overlay-popover')).toBeInTheDocument());

      const docListener = vi.fn();
      document.addEventListener('keydown', docListener);
      try {
        const evEsc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
        document.body.dispatchEvent(evEsc);

        await waitFor(() => expect(screen.queryByTestId('hint-overlay-popover')).toBeNull());
        expect(docListener).not.toHaveBeenCalled();
      } finally {
        document.removeEventListener('keydown', docListener);
      }
    });
  });

  // Fixwave finding #4: the dim panels must swallow clicks themselves rather
  // than let them fall through to a live app control underneath.
  describe('fixwave #4: backdrop panels swallow clicks', () => {
    const HINT_OVERLAY_CSS = import.meta.glob('./HintOverlay.css', {
      eager: true,
      query: '?raw',
      import: 'default',
    }) as Record<string, string>;
    const css = HINT_OVERLAY_CSS['./HintOverlay.css'];

    it('the panel rule is pointer-events:auto, not none', () => {
      expect(css).toMatch(/\.hint-overlay__panel\s*{[^}]*pointer-events:\s*auto;/);
      expect(css).not.toMatch(/\.hint-overlay__panel\s*{[^}]*pointer-events:\s*none;/);
    });

    it('clicking a panel does not reach an underlying control spy', async () => {
      const onAnchorClick = vi.fn();
      render(
        <HintProvider>
          <Harness onAnchorClick={onAnchorClick} />
        </HintProvider>,
      );
      await waitFor(() => expect(screen.getByText('Connect')).toBeInTheDocument());
      stubRect(screen.getByText('Connect'), ANCHOR_RECT);
      fireEvent.click(screen.getByText('startTour'));

      const panels = await waitFor(() => {
        const els = screen.getAllByTestId('hint-overlay-panel');
        expect(els).toHaveLength(4);
        return els;
      });
      fireEvent.click(panels[0]);
      expect(onAnchorClick).not.toHaveBeenCalled();
    });
  });

  // Fixwave finding #5: the overlay's auto-skip path (fallback:'skip', anchor
  // missing at fire time) must abandon the single hint WITHOUT persisting —
  // see HintProvider.test.tsx's dedicated abandonSingle contract test for the
  // "stays eligible for a future request" half of this finding.
  it('fixwave #5: an auto-skipped tip does not persist tips_seen', async () => {
    render(
      <HintProvider>
        <TipHarness showAnchor={false} />
      </HintProvider>,
    );
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('config_read'));
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByText('requestTip'));
    await waitFor(() => expect(screen.queryByTestId('hint-overlay-popover')).toBeNull());

    expect(invoke).not.toHaveBeenCalledWith('config_set_onboarding', expect.anything());
  });
});
