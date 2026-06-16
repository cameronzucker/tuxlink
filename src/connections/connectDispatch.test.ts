// src/connections/connectDispatch.test.ts
//
// tuxlink-ypz3 (3b): the ribbon/status-bar Connect path (connectFor) must record
// its empirical outcome into the per-mode Recent list, exactly like the in-panel
// Connect buttons do via useFavorites.recordAttempt. Before this fix connectFor
// invoked the transport but NEVER recorded, so the status-bar Connect — the
// PRIMARY connect surface since vu97 (pane closed) — left Recent permanently
// empty for ARDOP / VARA / packet.
//
// These tests pin: (1) a successful ribbon connect records `reached`; (2) an
// on-air failure records `failed` and still rejects; (3) a pre-air bail (missing
// target, VARA transport-open failure, VARA "session not open") records NOTHING;
// (4) telnet-CMS records nothing (no Recent surface).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { connectFor, writeLastTarget, MissingTargetError } from './connectDispatch';
import type { ConnectionKey } from './sessionTypes';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockInvoke = vi.mocked(invoke);

// Route invoke per command: `fail` (a command name) rejects; everything else
// (including favorite_record_attempt) resolves. The record call is fire-and-
// forget inside connectFor, so we assert on the synchronous call, not its await.
function routeInvoke(fail?: { cmd: string; message: string }) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (fail && cmd === fail.cmd) return Promise.reject(new Error(fail.message));
    return Promise.resolve(undefined);
  });
}

function recordCalls() {
  return mockInvoke.mock.calls.filter((c) => c[0] === 'favorite_record_attempt');
}

const ARDOP: ConnectionKey = { sessionType: 'cms', protocol: 'ardop-hf' };
const VARA: ConnectionKey = { sessionType: 'cms', protocol: 'vara-hf' };
const PACKET: ConnectionKey = { sessionType: 'cms', protocol: 'packet' };
const TELNET: ConnectionKey = { sessionType: 'cms', protocol: 'telnet' };

beforeEach(() => {
  localStorage.clear();
  mockInvoke.mockReset();
});

describe('connectFor — records the ribbon Connect outcome into Recent (3b)', () => {
  it('ARDOP success records a `reached` attempt for the dialed target', async () => {
    writeLastTarget('ardop-hf', 'W1ABC-10');
    routeInvoke();
    await connectFor(ARDOP);
    const calls = recordCalls();
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toMatchObject({
      dial: { mode: 'ardop-hf', gateway: 'W1ABC-10' },
      outcome: 'reached',
    });
    // tsLocal is an offset-bearing local ISO string, not a Z-suffixed UTC one.
    expect((calls[0][1] as { tsLocal: string }).tsLocal).toMatch(/[+-]\d{2}:\d{2}$/);
  });

  it('ARDOP: link reached then exchange failure records BOTH reached and failed', async () => {
    // modem_ardop_connect resolves (link up = on-air reached); the on-air B2F
    // exchange then throws (failed). Two distinct empirical facts, matching
    // ArdopRadioPanel (reached at connected-* + failed in the exchange catch).
    writeLastTarget('ardop-hf', 'W1ABC-10');
    routeInvoke({ cmd: 'modem_ardop_b2f_exchange', message: 'no answer' });
    await expect(connectFor(ARDOP)).rejects.toThrow('no answer');
    const outcomes = recordCalls().map((c) => (c[1] as { outcome: string }).outcome);
    expect(outcomes).toEqual(['reached', 'failed']);
  });

  it('ARDOP preflight (modem_ardop_connect) failure records NOTHING — pre-air, never dialed', async () => {
    // A connect rejection is PRE-AIR (missing identity / unconfigured audio /
    // busy guard / spawn-init) — no RF attempt happened, so Recent must stay
    // clean. The ARDOP panel records nothing for these Start failures either
    // (Codex ypz3 P2).
    writeLastTarget('ardop-hf', 'W1ABC-10');
    routeInvoke({ cmd: 'modem_ardop_connect', message: 'no playback device configured' });
    await expect(connectFor(ARDOP)).rejects.toThrow('no playback device configured');
    expect(recordCalls()).toHaveLength(0);
  });

  it('Packet success records `reached`; failure records `failed` and rejects', async () => {
    writeLastTarget('packet', 'N0CALL-7');
    routeInvoke();
    await connectFor(PACKET);
    expect(recordCalls()[0][1]).toMatchObject({
      dial: { mode: 'packet', gateway: 'N0CALL-7' },
      outcome: 'reached',
    });

    mockInvoke.mockReset();
    routeInvoke({ cmd: 'packet_connect', message: 'link failure' });
    await expect(connectFor(PACKET)).rejects.toThrow('link failure');
    expect(recordCalls()[0][1]).toMatchObject({
      dial: { mode: 'packet', gateway: 'N0CALL-7' },
      outcome: 'failed',
    });
  });

  it('VARA success records `reached` (after the pre-air session open)', async () => {
    writeLastTarget('vara-hf', 'KK6XYZ');
    routeInvoke();
    await connectFor(VARA);
    expect(recordCalls()[0][1]).toMatchObject({
      dial: { mode: 'vara-hf', gateway: 'KK6XYZ' },
      outcome: 'reached',
    });
  });

  it('VARA on-air exchange failure records `failed` and rejects', async () => {
    writeLastTarget('vara-hf', 'KK6XYZ');
    routeInvoke({ cmd: 'modem_vara_b2f_exchange', message: 'timeout' });
    await expect(connectFor(VARA)).rejects.toThrow('timeout');
    expect(recordCalls()[0][1]).toMatchObject({ outcome: 'failed' });
  });

  it('VARA "session not open" pre-air bail records NOTHING (never went on-air)', async () => {
    writeLastTarget('vara-hf', 'KK6XYZ');
    routeInvoke({ cmd: 'modem_vara_b2f_exchange', message: 'session not open' });
    await expect(connectFor(VARA)).rejects.toThrow('session not open');
    expect(recordCalls()).toHaveLength(0);
  });

  it('VARA transport-open failure (pre-air) records NOTHING', async () => {
    writeLastTarget('vara-hf', 'KK6XYZ');
    routeInvoke({ cmd: 'vara_open_session', message: 'connection refused' });
    await expect(connectFor(VARA)).rejects.toThrow('connection refused');
    expect(recordCalls()).toHaveLength(0);
  });

  it('a missing target throws MissingTargetError BEFORE any record or invoke', async () => {
    routeInvoke();
    await expect(connectFor(ARDOP)).rejects.toBeInstanceOf(MissingTargetError);
    expect(recordCalls()).toHaveLength(0);
    // No transport invoke either — the guard precedes every backend call.
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('telnet-CMS records nothing (no Recent surface; isManualOnly)', async () => {
    routeInvoke();
    await connectFor(TELNET);
    expect(mockInvoke).toHaveBeenCalledWith('cms_connect');
    expect(recordCalls()).toHaveLength(0);
  });
});
