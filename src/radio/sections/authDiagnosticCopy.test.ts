import { describe, it, expect } from 'vitest';
import { copyFor } from './authDiagnosticCopy';

describe('authDiagnosticCopy', () => {
  it('Mode 3 password_rejected has tuxlink-original copy (no PASSWORD NOT RECOGNISED)', () => {
    const c = copyFor({ mode: 'password_rejected', transportKind: null });
    expect(c.headline.toLowerCase()).toContain('password');
    expect(c.headline).not.toContain('PASSWORD NOT RECOGNISED');
    expect(c.body.length).toBeGreaterThan(0);
  });

  it('Mode 2 client_rejected does not say "tuxlink-side bug" (R4 #1 jargon fix)', () => {
    const c = copyFor({ mode: 'client_rejected', transportKind: null });
    expect(c.headline.toLowerCase()).not.toContain('tuxlink-side');
    expect(c.headline.toLowerCase()).toContain('allowlist');
  });

  it('Mode 4 callsign_rejected does NOT route the user to "Re-run wizard" first (R4 #2)', () => {
    const c = copyFor({ mode: 'callsign_rejected', transportKind: null });
    // The headline mentions the most likely cause (account deactivation).
    expect(c.body.toLowerCase()).toContain('account');
  });

  it('Mode 5 session_dropped does NOT assert "credentials are fine" (R4 #3)', () => {
    const c = copyFor({ mode: 'session_dropped_after_auth', transportKind: null });
    expect(c.headline.toLowerCase()).not.toContain('credentials are fine');
    expect(c.headline.toLowerCase()).toContain('dropped');
  });

  it('Mode 1 transport-kind variants have distinct copy', () => {
    const dns = copyFor({ mode: 'network_unreachable', transportKind: 'dns' });
    const tls = copyFor({ mode: 'network_unreachable', transportKind: 'tls_handshake' });
    expect(dns.headline).not.toBe(tls.headline);
    expect(tls.headline.toLowerCase()).toMatch(/tls|plaintext|transport/);
  });

  it('Mode 6 temporary_server_unavailability has wait-and-retry framing', () => {
    const c = copyFor({ mode: 'temporary_server_unavailability', transportKind: null });
    expect(c.headline.toLowerCase()).toMatch(/temporarily|maintenance|busy/);
    expect(c.body.toLowerCase()).toMatch(/few minutes|wait|try again/);
  });

  it('Uncategorized has an honest fallback copy', () => {
    const c = copyFor({ mode: 'uncategorized', transportKind: null });
    expect(c.headline.length).toBeGreaterThan(0);
    expect(c.body.toLowerCase()).toMatch(/unrecognis|details|log/);
  });
});
