// bd tuxlink-mfssz: ElmerPopped wrapper contract — token seeding, live
// getContext, and the two cross-window menu-verb intents. ElmerPane itself is
// stubbed (its own suite covers the pane); this suite pins the seam between
// the pane and the dock framework.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import type { ElmerPaneProps } from '../elmer/ElmerPane';

const h = vi.hoisted(() => ({
  paneProps: [] as unknown[],
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  mockDockBack: vi.fn(async () => {}),
}));

vi.mock('../elmer/ElmerPane', () => ({
  ElmerPane: (props: unknown) => {
    h.paneProps.push(props);
    const p = props as ElmerPaneProps;
    return (
      <div data-testid="elmer-pane-stub" data-expand-model={String(p.expandModel === true)}>
        <button
          data-testid="stub-report"
          onClick={() =>
            p.onConversationChange?.({
              items: [{ kind: 'turn', id: 'live-0', role: 'user', text: 'live state' }],
              running: false,
              context: null,
            })
          }
        />
      </div>
    );
  },
}));

vi.mock('../security/useEgressArm', () => ({
  useEgressArm: () => ({
    status: undefined,
    arm: vi.fn(),
    disarm: vi.fn(),
    rearm: vi.fn(),
    busy: false,
    error: null,
  }),
}));

vi.mock('./dockState', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./dockState')>();
  return { ...actual, dockBack: h.mockDockBack };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (channel: string, handler: (event: { payload: unknown }) => void) => {
    h.listeners.set(channel, handler);
    return () => h.listeners.delete(channel);
  }),
}));

import { SURFACE_REGISTRY } from './surfaceRegistry';

const seedToken = {
  items: [{ kind: 'turn', id: 'seed-0', role: 'user', text: 'seeded' }],
  running: true,
  context: { promptTokens: 5, numCtx: 1000 },
};

function renderElmerPopped(context: unknown) {
  const Component = SURFACE_REGISTRY.elmer.Component;
  const getContextRef = { current: null as (() => unknown | null) | null };
  const registerGetContext = (fn: () => unknown | null) => {
    getContextRef.current = fn;
  };
  const utils = render(<Component context={context} registerGetContext={registerGetContext} />);
  return { ...utils, getContextRef };
}

async function fireIntent(intent: string, surface = 'elmer') {
  await waitFor(() => expect(h.listeners.has('dock:intent')).toBe(true));
  h.listeners.get('dock:intent')!({ payload: { surface, intent } });
}

describe('ElmerPopped (bd tuxlink-mfssz)', () => {
  beforeEach(() => {
    h.paneProps.length = 0;
    h.listeners.clear();
    vi.clearAllMocks();
  });

  it('seeds ElmerPane from a valid token and starts getContext on it', () => {
    const { getContextRef } = renderElmerPopped(seedToken);
    const p = h.paneProps[0] as ElmerPaneProps;
    expect(p.popped).toBe(true);
    expect(p.onPopOut).toBeUndefined();
    expect(p.initialConversation).toEqual(seedToken);
    expect(getContextRef.current!()).toEqual(seedToken);
  });

  it('rejects a malformed token whole (null seed, null getContext)', () => {
    const { getContextRef } = renderElmerPopped({ items: [{ kind: 'mystery', id: 'x' }] });
    const p = h.paneProps[0] as ElmerPaneProps;
    expect(p.initialConversation).toBeNull();
    expect(getContextRef.current!()).toBeNull();
  });

  it('getContext reports the LIVE token after the pane reports a change', () => {
    const { getContextRef } = renderElmerPopped(seedToken);
    fireEvent.click(screen.getByTestId('stub-report'));
    expect(getContextRef.current!()).toEqual({
      items: [{ kind: 'turn', id: 'live-0', role: 'user', text: 'live state' }],
      running: false,
      context: null,
    });
  });

  it("dock_back intent flushes THIS window's live token with foreground semantics", async () => {
    renderElmerPopped(seedToken);
    fireEvent.click(screen.getByTestId('stub-report'));
    await fireIntent('dock_back');
    await waitFor(() =>
      expect(h.mockDockBack).toHaveBeenCalledWith('elmer', {
        foreground: true,
        state: {
          items: [{ kind: 'turn', id: 'live-0', role: 'user', text: 'live state' }],
          running: false,
          context: null,
        },
      }),
    );
  });

  it('open_model intent remounts the pane with expandModel, re-seeded from the live token', async () => {
    renderElmerPopped(seedToken);
    fireEvent.click(screen.getByTestId('stub-report'));
    await fireIntent('open_model');
    await waitFor(() =>
      expect(screen.getByTestId('elmer-pane-stub').dataset.expandModel).toBe('true'),
    );
    const latest = h.paneProps[h.paneProps.length - 1] as ElmerPaneProps;
    expect(latest.expandModel).toBe(true);
    expect(latest.initialConversation).toEqual({
      items: [{ kind: 'turn', id: 'live-0', role: 'user', text: 'live state' }],
      running: false,
      context: null,
    });
  });

  it("another surface's intent is ignored", async () => {
    renderElmerPopped(seedToken);
    await fireIntent('dock_back', 'routines');
    expect(h.mockDockBack).not.toHaveBeenCalled();
    expect(screen.getByTestId('elmer-pane-stub').dataset.expandModel).toBe('false');
  });
});
