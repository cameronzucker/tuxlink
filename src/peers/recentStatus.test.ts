// Label-truth permutations for the Recent/detail honest status (Task T-F).
// The operator dictated these semantics at the mock gate — LITERALNESS is the
// point: a success is "heard" (incoming) or "reached" (outgoing); no success is
// "dialed · not reached yet". Nothing here may say "worked" or imply identity
// verification, and a FAILED attempt (last_seen bumped, last_ok null) must NEVER
// read as a success.

import { describe, it, expect } from 'vitest';
import {
  deriveRecentStatus,
  recentStatusLine,
  channelStatusLine,
  endpointStatusLine,
} from './recentStatus';
import type { Channel, Contact, Endpoint, Direction, Provenance } from '../contacts/types';

// A fixed "now" so relativeAgo is deterministic.
const NOW = new Date('2026-07-11T12:00:00-07:00');

function channel(over: Partial<Channel> = {}): Channel {
  return {
    transport: 'vara-hf',
    target_callsign: 'W6ABC-7',
    via: [],
    freq_hz: 7_101_000,
    bandwidth: null,
    direction: 'outgoing',
    counts: { ok: 0, fail: 0 },
    last_seen: '2026-07-11T09:00:00-07:00',
    last_ok: null,
    ...over,
  };
}

function endpoint(over: Partial<Endpoint> = {}): Endpoint {
  return {
    id: 'e1',
    host: 'peer.example',
    port: 8772,
    provenance: 'operator',
    last_seen: '2026-07-11T09:00:00-07:00',
    last_ok: null,
    ...over,
  };
}

function contact(over: Partial<Contact> = {}): Contact {
  return {
    id: 'c1',
    name: '',
    callsign: 'W6ABC-7',
    tier: 'unconfirmed',
    origin: 'outgoing',
    channels: [],
    endpoints: [],
    created_at: '2026-07-11T08:00:00-07:00',
    updated_at: '2026-07-11T09:00:00-07:00',
    ...over,
  };
}

describe('deriveRecentStatus / recentStatusLine — label truth', () => {
  it('an OUTGOING channel success reads "reached …" with a transport/freq summary', () => {
    const c = contact({
      channels: [channel({ direction: 'outgoing', last_ok: '2026-07-11T09:00:00-07:00' })],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('reached');
    expect(recentStatusLine(status, NOW)).toBe('reached 3 h ago · VARA HF · 7.101 MHz');
  });

  it('an INCOMING channel success reads "heard …" (dialed into my station)', () => {
    const c = contact({
      origin: 'incoming',
      channels: [channel({ direction: 'incoming', last_ok: '2026-07-11T11:30:00-07:00' })],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('heard');
    expect(recentStatusLine(status, NOW)).toBe('heard 30 min ago · VARA HF · 7.101 MHz');
  });

  it('attempts with NO success read "dialed · not reached yet" — a fail is never a success', () => {
    // last_seen is bumped (a fail happened) but last_ok is null.
    const c = contact({
      channels: [channel({ counts: { ok: 0, fail: 2 }, last_ok: null })],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('dialed-not-reached');
    expect(recentStatusLine(status, NOW)).toBe('dialed · not reached yet');
  });

  it('the MOST RECENT success wins its flavor (a newer incoming beats an older outgoing)', () => {
    const c = contact({
      origin: 'outgoing',
      channels: [
        channel({ direction: 'outgoing', last_ok: '2026-07-10T09:00:00-07:00' }),
        channel({ direction: 'incoming', target_callsign: 'W6ABC', last_ok: '2026-07-11T11:00:00-07:00' }),
      ],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('heard');
    expect(recentStatusLine(status, NOW)).toBe('heard 1 h ago · VARA HF · 7.101 MHz');
  });

  it('a telnet endpoint success maps provenance → flavor (operator=reached, observed=heard)', () => {
    const opStatus = deriveRecentStatus(
      contact({ channels: [], endpoints: [endpoint({ provenance: 'operator', last_ok: '2026-07-11T11:00:00-07:00' })] }),
    );
    expect(opStatus.kind).toBe('reached');
    const obsStatus = deriveRecentStatus(
      contact({
        origin: 'incoming',
        channels: [],
        endpoints: [endpoint({ provenance: 'observed-incoming', last_ok: '2026-07-11T11:00:00-07:00' })],
      }),
    );
    expect(obsStatus.kind).toBe('heard');
  });

  it('a contact with no reachability rows is "none" (empty line)', () => {
    const status = deriveRecentStatus(contact({ channels: [], endpoints: [] }));
    expect(status.kind).toBe('none');
    expect(recentStatusLine(status, NOW)).toBe('');
  });

  it('never emits "worked" or "verified" for any permutation', () => {
    const perms: Direction[] = ['incoming', 'outgoing', 'unknown'];
    for (const direction of perms) {
      for (const last_ok of ['2026-07-11T11:00:00-07:00', null]) {
        const line = recentStatusLine(
          deriveRecentStatus(contact({ channels: [channel({ direction, last_ok })] })),
          NOW,
        );
        expect(line).not.toMatch(/worked|verified/i);
      }
    }
  });
});

describe('channelStatusLine / endpointStatusLine — per-row', () => {
  it('channelStatusLine: success → verb+ago, no success → dialed-not-reached', () => {
    expect(channelStatusLine(channel({ direction: 'outgoing', last_ok: '2026-07-11T11:00:00-07:00' }), 'outgoing', NOW)).toBe(
      'reached 1 h ago',
    );
    expect(channelStatusLine(channel({ last_ok: null }), 'outgoing', NOW)).toBe('dialed · not reached yet');
  });

  it('endpointStatusLine mirrors channelStatusLine, provenance-flavored', () => {
    const prov: Provenance = 'observed-incoming';
    expect(endpointStatusLine(endpoint({ provenance: prov, last_ok: '2026-07-11T11:00:00-07:00' }), 'incoming', NOW)).toBe(
      'heard 1 h ago',
    );
  });
});
