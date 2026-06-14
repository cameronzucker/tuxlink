// tuxlink-4b96 — frontend errors forward into the structured log.
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

import { reportFrontendError, installGlobalErrorForwarding } from './frontendErrorLog';

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined);
});

describe('reportFrontendError', () => {
  it('forwards source/message/stack to the log_frontend_error command', () => {
    reportFrontendError('react-error-boundary', 'kaboom', 'at Foo (x.tsx:1)');
    expect(mockInvoke).toHaveBeenCalledWith('log_frontend_error', {
      source: 'react-error-boundary',
      message: 'kaboom',
      stack: 'at Foo (x.tsx:1)',
    });
  });

  it('sends stack:null when omitted and never throws if invoke rejects', () => {
    mockInvoke.mockRejectedValue(new Error('not in a tauri webview'));
    expect(() => reportFrontendError('window.error', 'oops')).not.toThrow();
    expect(mockInvoke).toHaveBeenCalledWith('log_frontend_error', {
      source: 'window.error',
      message: 'oops',
      stack: null,
    });
  });
});

describe('installGlobalErrorForwarding', () => {
  it('forwards an uncaught window error to the log', () => {
    installGlobalErrorForwarding();
    window.dispatchEvent(new ErrorEvent('error', { message: 'boom', error: new Error('boom') }));
    expect(mockInvoke).toHaveBeenCalledWith(
      'log_frontend_error',
      expect.objectContaining({ source: 'window.error', message: 'boom' }),
    );
  });

  it('forwards an unhandled promise rejection to the log', () => {
    installGlobalErrorForwarding();
    const evt = new Event('unhandledrejection') as PromiseRejectionEvent;
    Object.defineProperty(evt, 'reason', { value: new Error('rejected'), configurable: true });
    window.dispatchEvent(evt);
    expect(mockInvoke).toHaveBeenCalledWith(
      'log_frontend_error',
      expect.objectContaining({ source: 'unhandledrejection', message: 'rejected' }),
    );
  });
});
