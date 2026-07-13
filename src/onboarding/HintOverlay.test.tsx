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
