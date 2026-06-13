// ErrorBoundary (tuxlink-52h6) — the app's catch-all for render/lifecycle/effect
// throws. React has no built-in: without a boundary, a single unguarded
// exception (the 0.60.0 case was `new maplibregl.Map()` failing during the
// location step's map mount) unmounts the ENTIRE root tree, leaving the bare
// body background — the "dark blue empty window" the operator saw.
//
// A boundary converts that into a contained, recoverable surface: the default
// fallback is a reload-to-recover screen at the app root; a `fallback` prop lets
// a caller degrade a single subtree locally (e.g. a map panel shows "unavailable"
// while the rest of the screen keeps working).
//
// Error boundaries must be class components — there is no hook equivalent.

import { Component, type ErrorInfo, type ReactNode } from 'react';

export interface ErrorBoundaryProps {
  children: ReactNode;
  /** Rendered in place of the children after a caught error. Omitted → the
   *  default app-level recovery screen. Supply this to degrade one subtree
   *  locally instead of replacing the whole view. */
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Surface the trace for the WebKitGTK inspector / tauri stdout. This is the
    // only record of WHAT threw, since the UI no longer crashes to a blank
    // window that an operator would otherwise screenshot.
    // eslint-disable-next-line no-console
    console.error('Uncaught error boundary capture:', error, info.componentStack);
  }

  render(): ReactNode {
    if (!this.state.hasError) return this.props.children;
    if (this.props.fallback !== undefined) return this.props.fallback;
    return (
      <div
        role="alert"
        data-testid="error-boundary-fallback"
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: '1rem',
          minHeight: '100vh',
          padding: '2rem',
          textAlign: 'center',
        }}
      >
        <h1 style={{ margin: 0 }}>Something went wrong</h1>
        <p style={{ margin: 0, maxWidth: '32rem' }}>
          Tuxlink hit an unexpected error and stopped this screen. Reload to continue.
          Your settings and messages are unaffected.
        </p>
        <button type="button" onClick={() => window.location.reload()}>
          Reload Tuxlink
        </button>
      </div>
    );
  }
}
