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
});
