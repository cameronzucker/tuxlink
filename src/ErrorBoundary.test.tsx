// ErrorBoundary (tuxlink-52h6) — the app-wide catch-all that stops a single
// unguarded throw (e.g. maplibre map construction) from unmounting the entire
// React root to a blank window. See the 0.60.0 blank-location-screen incident.
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';

const { mockReport } = vi.hoisted(() => ({ mockReport: vi.fn() }));
vi.mock('./frontendErrorLog', () => ({
  reportFrontendError: mockReport,
  installGlobalErrorForwarding: vi.fn(),
}));

import { ErrorBoundary } from './ErrorBoundary';

afterEach(cleanup);

function Boom(): never {
  throw new Error('kaboom');
}

describe('ErrorBoundary', () => {
  it('renders the children when nothing throws', () => {
    render(
      <ErrorBoundary>
        <div data-testid="ok">healthy</div>
      </ErrorBoundary>,
    );
    expect(screen.getByTestId('ok')).toBeInTheDocument();
  });

  it('renders the default recovery fallback (not a blank tree) when a child throws', () => {
    // React logs the caught error to console.error — silence it so the suite
    // output stays pristine.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>,
    );
    // The throwing child is gone, and a real recovery surface is shown instead
    // of an empty tree (the 0.60.0 blank window).
    expect(screen.getByTestId('error-boundary-fallback')).toBeInTheDocument();
    expect(screen.queryByText('kaboom')).not.toBeInTheDocument();
    spy.mockRestore();
  });

  it('renders a custom fallback when one is provided (local degradation)', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(
      <ErrorBoundary fallback={<div data-testid="map-unavailable">Map unavailable</div>}>
        <Boom />
      </ErrorBoundary>,
    );
    expect(screen.getByTestId('map-unavailable')).toBeInTheDocument();
    // The default fallback is NOT used when a custom one is supplied.
    expect(screen.queryByTestId('error-boundary-fallback')).not.toBeInTheDocument();
    spy.mockRestore();
  });

  it('forwards the caught error to the structured log (tuxlink-4b96)', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    mockReport.mockClear();
    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>,
    );
    expect(mockReport).toHaveBeenCalledWith(
      'react-error-boundary',
      'kaboom',
      expect.stringContaining('Component stack:'),
    );
    spy.mockRestore();
  });
});
