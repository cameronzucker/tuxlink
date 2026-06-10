// src/radio/sections/AuthDiagnosticBanner.test.tsx
//
// Tests for AuthDiagnosticBanner (tuxlink-7do4 Task 21, spec §4 + §8.2).
// All Tauri + browser APIs mocked; no jsdom Tauri context needed.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import type { AuthDiagnosticState } from '../../connections/useAuthDiagnostic';
import type { SessionLogEntry } from './SessionLogSection';

// ---------------------------------------------------------------------------
// Module-level mocks (hoisted above imports per vi.mock hoisting rules)
// ---------------------------------------------------------------------------

// useAuthDiagnostic — replaced entirely so we control state + callbacks.
const mockDismiss = vi.fn(() => Promise.resolve());
const mockTestCredentials = vi.fn(() => Promise.resolve());

let mockState: AuthDiagnosticState = {
  mode: null,
  attemptId: null,
  retryCount: 1,
  rawWireResponse: null,
  transportKind: null,
  postAuthExchangeStarted: false,
  testingInFlight: false,
  testRateLimit: { disabledUntil: null, circuitBroken: false, recentTestTimestamps: [] },
};

vi.mock('../../connections/useAuthDiagnostic', () => ({
  useAuthDiagnostic: () => ({
    state: mockState,
    dismiss: mockDismiss,
    testCredentials: mockTestCredentials,
  }),
}));

// useSessionLog — return empty entries by default; overridden per test.
let mockEntries: SessionLogEntry[] = [];

vi.mock('./useSessionLog', () => ({
  useSessionLog: () => ({ entries: mockEntries, clear: vi.fn() }),
}));

// @tauri-apps/api/core — capture invoke calls.
const mockInvoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => mockInvoke(cmd, args),
}));

// @tauri-apps/api/event — listen() needed by useSessionLog (mocked above, but
// the real module is still imported transitively in some paths).
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// @tauri-apps/plugin-shell — capture shellOpen calls.
const mockShellOpen = vi.fn((_url: string) => Promise.resolve());

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: (url: string) => mockShellOpen(url),
}));

// navigator.clipboard — jsdom doesn't implement it; stub out writeText.
const mockClipboardWrite = vi.fn((_text: string) => Promise.resolve());

// ---------------------------------------------------------------------------
// Subject under test (imported AFTER mocks are set up)
// ---------------------------------------------------------------------------

import { AuthDiagnosticBanner } from './AuthDiagnosticBanner';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stateWith(overrides: Partial<AuthDiagnosticState>): AuthDiagnosticState {
  return {
    mode: null,
    attemptId: 1,
    retryCount: 1,
    rawWireResponse: null,
    transportKind: null,
    postAuthExchangeStarted: false,
    testingInFlight: false,
    testRateLimit: { disabledUntil: null, circuitBroken: false, recentTestTimestamps: [] },
    ...overrides,
  };
}

function renderBanner() {
  return render(<AuthDiagnosticBanner />);
}

// ---------------------------------------------------------------------------
// Setup / teardown
// ---------------------------------------------------------------------------

beforeEach(() => {
  vi.clearAllMocks();
  mockState = stateWith({ mode: null });
  mockEntries = [];

  // Install clipboard stub on the global navigator.
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText: mockClipboardWrite },
    writable: true,
    configurable: true,
  });
});

// ---------------------------------------------------------------------------
// mode === null → renders nothing
// ---------------------------------------------------------------------------

describe('mode null', () => {
  it('renders nothing when mode is null', () => {
    mockState = stateWith({ mode: null });
    renderBanner();
    expect(screen.queryByTestId('diag-banner')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// role + aria-live present on banner root
// ---------------------------------------------------------------------------

describe('a11y attributes', () => {
  it('has role="alert" and aria-live="polite" on banner root', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: null });
    renderBanner();
    const banner = screen.getByTestId('diag-banner');
    expect(banner).toHaveAttribute('role', 'alert');
    expect(banner).toHaveAttribute('aria-live', 'polite');
  });
});

// ---------------------------------------------------------------------------
// Mode 1 — network_unreachable
// ---------------------------------------------------------------------------

describe('Mode 1: network_unreachable', () => {
  it('renders DNS-specific copy when transportKind=dns', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: 'dns' });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain("Couldn't find the Winlink server's address");
  });

  it('renders TLS-specific copy when transportKind=tls_handshake', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: 'tls_handshake' });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain("Couldn't negotiate TLS");
  });

  it('renders TCP-refused copy when transportKind=tcp_refused', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: 'tcp_refused' });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain('refused the connection');
  });

  it('renders fallback copy when transportKind=null', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: null });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain("Couldn't reach the Winlink server");
  });

  it('only affordance is "Copy log for help"', () => {
    mockState = stateWith({ mode: 'network_unreachable', transportKind: null });
    renderBanner();
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('diag-switch-cmsz-btn')).toBeNull();
    expect(screen.queryByTestId('diag-test-credentials-btn')).toBeNull();
    expect(screen.queryByTestId('diag-reenter-password-btn')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Mode 2 — client_rejected
// ---------------------------------------------------------------------------

describe('Mode 2: client_rejected', () => {
  beforeEach(() => {
    mockState = stateWith({ mode: 'client_rejected' });
    mockInvoke.mockResolvedValue(undefined);
    renderBanner();
  });

  it('renders headline and body copy', () => {
    expect(screen.getByTestId('diag-title').textContent).toContain("Tuxlink isn't on the Winlink server's allowlist");
  });

  it('has "Switch to cms-z (dev)" button', () => {
    expect(screen.getByTestId('diag-switch-cmsz-btn')).toBeInTheDocument();
  });

  it('"Switch to cms-z" calls config_set_connect with correct args', async () => {
    fireEvent.click(screen.getByTestId('diag-switch-cmsz-btn'));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('config_set_connect', {
        host: 'cms-z.winlink.org',
        transport: 'Telnet',
      });
    });
  });

  it('has "Open issue tracker" link button', () => {
    expect(screen.getByTestId('diag-issue-tracker-btn')).toBeInTheDocument();
  });

  it('"Open issue tracker" calls shellOpen with github issues URL', async () => {
    fireEvent.click(screen.getByTestId('diag-issue-tracker-btn'));
    await waitFor(() => {
      expect(mockShellOpen).toHaveBeenCalledWith(
        'https://github.com/cameronzucker/tuxlink/issues/new/choose',
      );
    });
  });

  it('has "Copy log for help"', () => {
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Mode 3 — password_rejected
// ---------------------------------------------------------------------------

describe('Mode 3: password_rejected', () => {
  // NOTE: No renderBanner() in beforeEach — several tests need custom mockState
  // before rendering. Each test renders its own instance to avoid duplicate
  // testid collisions from a beforeEach render + an in-test render.
  beforeEach(() => {
    mockState = stateWith({ mode: 'password_rejected' });
    mockInvoke.mockResolvedValue({ callsign: 'N0CALL' });
  });

  it('renders headline copy', () => {
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain("wasn't accepted");
  });

  it('has "Re-enter password" button', () => {
    renderBanner();
    expect(screen.getByTestId('diag-reenter-password-btn')).toBeInTheDocument();
  });

  it('inline form hidden initially', () => {
    renderBanner();
    expect(screen.queryByTestId('diag-password-form')).toBeNull();
  });

  it('"Re-enter password" click opens the inline form', () => {
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    expect(screen.getByTestId('diag-password-form')).toBeInTheDocument();
  });

  it('"Cancel" button closes the form', () => {
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    expect(screen.getByTestId('diag-password-form')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('diag-password-cancel'));
    expect(screen.queryByTestId('diag-password-form')).toBeNull();
  });

  it('Escape key inside password input closes the form', () => {
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    const input = screen.getByTestId('diag-password-input');
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByTestId('diag-password-form')).toBeNull();
  });

  it('Enter submits: calls config_read then credentials_write_password', async () => {
    mockInvoke
      .mockResolvedValueOnce({ callsign: 'N0CALL' })  // config_read
      .mockResolvedValueOnce(undefined);               // credentials_write_password

    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    const input = screen.getByTestId('diag-password-input');
    fireEvent.change(input, { target: { value: 'MyP@ss1!' } });
    const form = screen.getByTestId('diag-password-form');
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('config_read', undefined);
      expect(mockInvoke).toHaveBeenCalledWith('credentials_write_password', {
        callsign: 'N0CALL',
        password: 'MyP@ss1!',
      });
    });
  });

  it('password NOT retained in React state after submit (input cleared in finally)', async () => {
    mockInvoke
      .mockResolvedValueOnce({ callsign: 'N0CALL' })
      .mockResolvedValueOnce(undefined);

    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    const input = screen.getByTestId('diag-password-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'secret' } });
    expect(input.value).toBe('secret');

    fireEvent.submit(screen.getByTestId('diag-password-form'));

    // After submit completes, the form closes (onClose) — input unmounts.
    // Verify the form is gone (password can't exist in DOM).
    await waitFor(() => {
      expect(screen.queryByTestId('diag-password-form')).toBeNull();
    });
  });

  it('keyring error shows inline error without closing form', async () => {
    mockInvoke
      .mockResolvedValueOnce({ callsign: 'N0CALL' })  // config_read
      .mockRejectedValueOnce(new Error('locked'));      // credentials_write_password

    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reenter-password-btn'));
    const input = screen.getByTestId('diag-password-input');
    fireEvent.change(input, { target: { value: 'bad' } });
    fireEvent.submit(screen.getByTestId('diag-password-form'));

    await waitFor(() => {
      expect(screen.getByTestId('diag-password-error').textContent).toContain('Keyring unavailable');
    });
    // Form stays open
    expect(screen.getByTestId('diag-password-form')).toBeInTheDocument();
  });

  it('has "Check this password works" button when not rate-limited', () => {
    renderBanner();
    expect(screen.getByTestId('diag-test-credentials-btn')).toBeInTheDocument();
  });

  it('"Check this password works" calls testCredentials', async () => {
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-test-credentials-btn'));
    await waitFor(() => {
      expect(mockTestCredentials).toHaveBeenCalledOnce();
    });
  });

  it('"Check this password works" shows Testing… indicator when testingInFlight', () => {
    mockState = stateWith({ mode: 'password_rejected', testingInFlight: true });
    renderBanner();
    expect(screen.getByTestId('diag-testing-indicator')).toBeInTheDocument();
    expect(screen.queryByTestId('diag-test-credentials-btn')).toBeNull();
  });

  it('"Check this password works" disabled when rate-limited (disabledUntil in future)', () => {
    mockState = stateWith({
      mode: 'password_rejected',
      testRateLimit: {
        disabledUntil: Date.now() + 5000,
        circuitBroken: false,
        recentTestTimestamps: [],
      },
    });
    renderBanner();
    const btn = screen.getByTestId('diag-test-credentials-btn');
    expect(btn).toBeDisabled();
  });

  it('"Check this password works" disabled when circuit-broken', () => {
    mockState = stateWith({
      mode: 'password_rejected',
      testRateLimit: {
        disabledUntil: null,
        circuitBroken: true,
        recentTestTimestamps: [],
      },
    });
    renderBanner();
    expect(screen.getByTestId('diag-test-credentials-btn')).toBeDisabled();
  });

  it('has "Reset on winlink.org" link button', () => {
    renderBanner();
    expect(screen.getByTestId('diag-reset-password-btn')).toBeInTheDocument();
  });

  it('"Reset on winlink.org" calls shellOpen with password reset URL', async () => {
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-reset-password-btn'));
    await waitFor(() => {
      expect(mockShellOpen).toHaveBeenCalledWith('https://winlink.org/user/password-recovery');
    });
  });

  it('has "Copy log for help"', () => {
    renderBanner();
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Mode 4 — callsign_rejected
// ---------------------------------------------------------------------------

describe('Mode 4: callsign_rejected', () => {
  beforeEach(() => {
    mockState = stateWith({ mode: 'callsign_rejected' });
    mockInvoke.mockResolvedValue(undefined);
    renderBanner();
  });

  it('renders headline copy', () => {
    expect(screen.getByTestId('diag-title').textContent).toContain("didn't accept your callsign");
  });

  it('has "Verify on winlink.org" button (PRIMARY)', () => {
    expect(screen.getByTestId('diag-verify-callsign-btn')).toBeInTheDocument();
  });

  it('"Verify on winlink.org" is positioned BEFORE "Try a different callsign" (R4 #2)', () => {
    const verify = screen.getByTestId('diag-verify-callsign-btn');
    const change = screen.getByTestId('diag-change-callsign-btn');
    // compareDocumentPosition: 4 = DOCUMENT_POSITION_FOLLOWING (verify precedes change)
    expect(verify.compareDocumentPosition(change) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('"Verify on winlink.org" calls shellOpen with account URL', async () => {
    fireEvent.click(screen.getByTestId('diag-verify-callsign-btn'));
    await waitFor(() => {
      expect(mockShellOpen).toHaveBeenCalledWith('https://winlink.org/user/account');
    });
  });

  it('has "Try a different callsign" button', () => {
    expect(screen.getByTestId('diag-change-callsign-btn')).toBeInTheDocument();
  });

  it('"Try a different callsign" calls wizard_reopen with step=callsign', async () => {
    fireEvent.click(screen.getByTestId('diag-change-callsign-btn'));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('wizard_reopen', { step: 'callsign' });
    });
  });

  it('has "Copy log for help"', () => {
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });

  it('no inline password form or test-credentials button', () => {
    expect(screen.queryByTestId('diag-password-form')).toBeNull();
    expect(screen.queryByTestId('diag-test-credentials-btn')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Mode 5 — session_dropped_after_auth
// ---------------------------------------------------------------------------

describe('Mode 5: session_dropped_after_auth', () => {
  beforeEach(() => {
    mockState = stateWith({ mode: 'session_dropped_after_auth' });
    renderBanner();
  });

  it('renders headline copy', () => {
    expect(screen.getByTestId('diag-title').textContent).toContain('Login succeeded');
  });

  it('has "Check this password works" button', () => {
    expect(screen.getByTestId('diag-test-credentials-btn')).toBeInTheDocument();
  });

  it('calls testCredentials when clicked', async () => {
    fireEvent.click(screen.getByTestId('diag-test-credentials-btn'));
    await waitFor(() => {
      expect(mockTestCredentials).toHaveBeenCalledOnce();
    });
  });

  it('has "Copy log for help"', () => {
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });

  it('no inline password form', () => {
    expect(screen.queryByTestId('diag-reenter-password-btn')).toBeNull();
    expect(screen.queryByTestId('diag-password-form')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Mode 6 — temporary_server_unavailability
// ---------------------------------------------------------------------------

describe('Mode 6: temporary_server_unavailability', () => {
  beforeEach(() => {
    mockState = stateWith({ mode: 'temporary_server_unavailability' });
    renderBanner();
  });

  it('renders headline copy', () => {
    expect(screen.getByTestId('diag-title').textContent).toContain('temporarily unavailable');
  });

  it('has "Copy log for help" ONLY — no retry affordance', () => {
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('diag-test-credentials-btn')).toBeNull();
    expect(screen.queryByTestId('diag-switch-cmsz-btn')).toBeNull();
    expect(screen.queryByTestId('diag-verify-callsign-btn')).toBeNull();
    expect(screen.queryByTestId('diag-change-callsign-btn')).toBeNull();
    expect(screen.queryByTestId('diag-reenter-password-btn')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Uncategorized mode
// ---------------------------------------------------------------------------

describe('Uncategorized mode', () => {
  beforeEach(() => {
    mockState = stateWith({ mode: 'uncategorized' });
    mockInvoke.mockResolvedValue(undefined);
    renderBanner();
  });

  it('renders headline copy', () => {
    expect(screen.getByTestId('diag-title').textContent).toContain('Connection failed');
  });

  it('has "Try a different callsign"', () => {
    expect(screen.getByTestId('diag-change-callsign-btn')).toBeInTheDocument();
  });

  it('has "Copy log for help"', () => {
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });

  it('no test-credentials or password form', () => {
    expect(screen.queryByTestId('diag-test-credentials-btn')).toBeNull();
    expect(screen.queryByTestId('diag-reenter-password-btn')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Wire response toggle
// ---------------------------------------------------------------------------

describe('Wire response toggle', () => {
  it('toggle not rendered when rawWireResponse is null', () => {
    mockState = stateWith({ mode: 'password_rejected', rawWireResponse: null });
    renderBanner();
    expect(screen.queryByTestId('diag-raw-toggle')).toBeNull();
  });

  it('toggle rendered when rawWireResponse is non-null, collapsed by default', () => {
    mockState = stateWith({ mode: 'password_rejected', rawWireResponse: 'SID: [WL2K-3]' });
    renderBanner();
    expect(screen.getByTestId('diag-raw-toggle')).toBeInTheDocument();
    expect(screen.queryByTestId('diag-raw-content')).toBeNull();
  });

  it('clicking toggle expands the wire response', () => {
    mockState = stateWith({ mode: 'password_rejected', rawWireResponse: 'SID: [WL2K-3]' });
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-raw-toggle'));
    expect(screen.getByTestId('diag-raw-content')).toBeInTheDocument();
    expect(screen.getByTestId('diag-raw-content').textContent).toBe('SID: [WL2K-3]');
  });

  it('clicking toggle again collapses the wire response', () => {
    mockState = stateWith({ mode: 'password_rejected', rawWireResponse: 'SID: [WL2K-3]' });
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-raw-toggle'));
    fireEvent.click(screen.getByTestId('diag-raw-toggle'));
    expect(screen.queryByTestId('diag-raw-content')).toBeNull();
  });

  it('wire response is plain text — no HTML rendering (React text-escaping)', () => {
    const evilPayload = '<script>alert(1)</script>';
    mockState = stateWith({ mode: 'password_rejected', rawWireResponse: evilPayload });
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-raw-toggle'));
    const pre = screen.getByTestId('diag-raw-content');
    // Text content should be the raw string — NOT executed or parsed as HTML.
    expect(pre.textContent).toBe(evilPayload);
    // The DOM must not contain a <script> element as a real script tag.
    expect(pre.querySelector('script')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Dismiss button
// ---------------------------------------------------------------------------

describe('Dismiss button', () => {
  it('calls dismiss() when the × button is clicked', async () => {
    mockState = stateWith({ mode: 'network_unreachable' });
    renderBanner();
    fireEvent.click(screen.getByTestId('diag-dismiss'));
    await waitFor(() => {
      expect(mockDismiss).toHaveBeenCalledOnce();
    });
  });
});

// ---------------------------------------------------------------------------
// Retry counter
// ---------------------------------------------------------------------------

describe('Retry counter', () => {
  it('no counter suffix when retryCount === 1', () => {
    mockState = stateWith({ mode: 'password_rejected', retryCount: 1 });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).not.toContain('attempt');
  });

  it('appends "(2nd attempt)" when retryCount === 2', () => {
    mockState = stateWith({ mode: 'password_rejected', retryCount: 2 });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain('(2nd attempt)');
  });

  it('appends "(3rd attempt)" when retryCount === 3', () => {
    mockState = stateWith({ mode: 'password_rejected', retryCount: 3 });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain('(3rd attempt)');
  });

  it('appends "(4th attempt)" when retryCount === 4', () => {
    mockState = stateWith({ mode: 'password_rejected', retryCount: 4 });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain('(4th attempt)');
  });

  it('appends "(11th attempt)" when retryCount === 11', () => {
    mockState = stateWith({ mode: 'password_rejected', retryCount: 11 });
    renderBanner();
    expect(screen.getByTestId('diag-title').textContent).toContain('(11th attempt)');
  });
});

// ---------------------------------------------------------------------------
// Copy log for help
// ---------------------------------------------------------------------------

describe('Copy log for help', () => {
  it('calls navigator.clipboard.writeText with session log + wire response', async () => {
    mockEntries = [
      { ts: '19:00:00', level: 'info', message: 'Connecting to cms.winlink.org' },
    ];
    mockState = stateWith({ mode: 'network_unreachable', rawWireResponse: 'ERR 500' });
    renderBanner();

    fireEvent.click(screen.getByTestId('diag-copy-log-btn'));

    await waitFor(() => {
      expect(mockClipboardWrite).toHaveBeenCalledOnce();
      const written: string = mockClipboardWrite.mock.calls[0][0] as string;
      expect(written).toContain('Connecting to cms.winlink.org');
      expect(written).toContain('ERR 500');
    });
  });

  it('shows confirmation message after successful copy', async () => {
    mockState = stateWith({ mode: 'network_unreachable' });
    renderBanner();

    fireEvent.click(screen.getByTestId('diag-copy-log-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('diag-copy-confirmation')).toBeInTheDocument();
    });
    expect(screen.getByTestId('diag-copy-confirmation').textContent).toContain(
      'sensitive tokens redacted',
    );
    expect(screen.getByTestId('diag-copy-confirmation').textContent).toContain(
      'github.com/cameronzucker/tuxlink/issues',
    );
  });

  it('shows copy button (not confirmation) when clipboard fails', async () => {
    mockClipboardWrite.mockRejectedValueOnce(new Error('NotAllowedError'));
    mockState = stateWith({ mode: 'network_unreachable' });
    renderBanner();

    await act(async () => {
      fireEvent.click(screen.getByTestId('diag-copy-log-btn'));
    });

    // Confirmation should NOT appear when clipboard fails; button stays.
    expect(screen.queryByTestId('diag-copy-confirmation')).toBeNull();
    expect(screen.getByTestId('diag-copy-log-btn')).toBeInTheDocument();
  });
});
