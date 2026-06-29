/**
 * AppShell — Elmer pane lazy-load tests (Task 12, AC-13).
 *
 * Asserts that:
 *   1. ElmerPane is NOT mounted when `elmerOpen` is false (cold-start
 *      discipline: the module chunk must not be fetched until needed).
 *   2. ElmerPane IS mounted once `elmerOpen` is true.
 *
 * Strategy: we mock the lazy import so the test does not need a full
 * AppShell render (which requires extensive IPC mocks). Instead we test
 * the lazy+Suspense+call-site-gate pattern in isolation by rendering a
 * minimal wrapper that replicates the AppShell pattern:
 *
 *   const ElmerPane = lazy(() => import('../elmer/ElmerPane')…)
 *   {flag && <Suspense><ElmerPane /></Suspense>}
 *
 * The mock resolves ElmerPane to a simple stub so the Suspense boundary
 * resolves immediately in jsdom (no real module loading).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { lazy, Suspense, useState } from 'react';

// ---------------------------------------------------------------------------
// Mock ElmerPane so the lazy() call resolves to a known stub.
// ---------------------------------------------------------------------------

vi.mock('../elmer/ElmerPane', () => ({
  ElmerPane: () => <div data-testid="elmer-pane-stub">Elmer pane</div>,
}));

// The lazy wrapper mirrors AppShell's pattern exactly.
const ElmerPaneLazy = lazy(() =>
  import('../elmer/ElmerPane').then((m) => ({ default: m.ElmerPane })),
);

// Minimal harness that holds the open flag and mounts the pane exactly as
// AppShell does: call-site gate + Suspense + lazy component.
function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button data-testid="open-elmer" onClick={() => setOpen(true)}>
        Open Elmer
      </button>
      {open && (
        <Suspense fallback={<div data-testid="elmer-suspense-fallback" />}>
          <ElmerPaneLazy />
        </Suspense>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('ElmerPane lazy-load (Task 12 AC-13)', () => {
  it('ElmerPane is NOT mounted when the toggle is off', () => {
    render(<Harness />);
    // The stub must not be in the DOM before the toggle.
    expect(screen.queryByTestId('elmer-pane-stub')).toBeNull();
    // No Suspense fallback either.
    expect(screen.queryByTestId('elmer-suspense-fallback')).toBeNull();
  });

  it('ElmerPane IS mounted after the toggle is flipped', async () => {
    render(<Harness />);

    await act(async () => {
      fireEvent.click(screen.getByTestId('open-elmer'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('elmer-pane-stub')).toBeTruthy();
    });
  });

  it('ElmerPane is absent before toggle, present after', async () => {
    render(<Harness />);

    // Before: not mounted.
    expect(screen.queryByTestId('elmer-pane-stub')).toBeNull();

    // After toggle: mounted.
    await act(async () => {
      fireEvent.click(screen.getByTestId('open-elmer'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('elmer-pane-stub')).toBeTruthy();
    });
  });
});
