// Label-truth permutations for the Recent/detail honest status (Task T-F).
// The operator dictated these semantics at the mock gate — LITERALNESS is the
// point: a success is "heard" (incoming) or "reached" (outgoing); no success is
// "dialed · not reached yet". Nothing here may say "worked" or imply identity
// verification, and a FAILED attempt (last_seen bumped, last_ok null) must NEVER
// read as a success.
//
// Review Finding 1: the flavor keys on `last_ok_direction` (captured atomically
// WITH the success), NEVER on `Channel.direction` (mutated by failures too) —
// the verb must be as literally true as the timestamp.

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
    last_ok_direction: null,
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
      channels: [
        channel({
          direction: 'outgoing',
          last_ok: '2026-07-11T09:00:00-07:00',
          last_ok_direction: 'outgoing',
        }),
      ],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('reached');
    expect(recentStatusLine(status, NOW)).toBe('reached 3 h ago · VARA HF · 7.101 MHz');
  });

  it('an INCOMING channel success reads "heard …" (dialed into my station)', () => {
    const c = contact({
      origin: 'incoming',
      channels: [
        channel({
          direction: 'incoming',
          last_ok: '2026-07-11T11:30:00-07:00',
          last_ok_direction: 'incoming',
        }),
      ],
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

  it('an incoming success then an outgoing FAILED dial on the same channel stays "heard" (Finding 1)', () => {
    // The store mutates `direction` on every observation, so after the failed
    // outgoing dial the channel reads direction='outgoing' — but the 3h-ago
    // success was a HEARD. The verb keys on last_ok_direction and stays true.
    const c = contact({
      origin: 'incoming',
      channels: [
        channel({
          direction: 'outgoing', // failure-mutated
          counts: { ok: 1, fail: 1 },
          last_seen: '2026-07-11T11:55:00-07:00', // the failed dial
          last_ok: '2026-07-11T09:00:00-07:00', // the incoming success
          last_ok_direction: 'incoming',
        }),
      ],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('heard');
    expect(recentStatusLine(status, NOW)).toBe('heard 3 h ago · VARA HF · 7.101 MHz');
  });

  it('an outgoing success then an incoming FAILED attempt on the same channel stays "reached" (Finding 1)', () => {
    const c = contact({
      origin: 'outgoing',
      channels: [
        channel({
          direction: 'incoming', // failure-mutated
          counts: { ok: 1, fail: 1 },
          last_seen: '2026-07-11T11:55:00-07:00',
          last_ok: '2026-07-11T09:00:00-07:00',
          last_ok_direction: 'outgoing',
        }),
      ],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('reached');
    expect(recentStatusLine(status, NOW)).toBe('reached 3 h ago · VARA HF · 7.101 MHz');
  });

  it('a success with NO recorded direction degrades to the neutral "connected", never a guess', () => {
    // Pre-Finding-1 record shape: last_ok present, last_ok_direction absent →
    // null. The verb must not guess from `direction` or the record's origin.
    const c = contact({
      origin: 'incoming',
      channels: [
        channel({
          direction: 'outgoing',
          last_ok: '2026-07-11T09:00:00-07:00',
          last_ok_direction: null,
        }),
      ],
    });
    const status = deriveRecentStatus(c);
    expect(status.kind).toBe('connected');
    expect(recentStatusLine(status, NOW)).toBe('connected 3 h ago · VARA HF · 7.101 MHz');
  });

  it('the MOST RECENT success wins its flavor (a newer incoming beats an older outgoing)', () => {
    const c = contact({
      origin: 'outgoing',
      channels: [
        channel({
          direction: 'outgoing',
          last_ok: '2026-07-10T09:00:00-07:00',
          last_ok_direction: 'outgoing',
        }),
        channel({
          direction: 'incoming',
          target_callsign: 'W6ABC',
          last_ok: '2026-07-11T11:00:00-07:00',
          last_ok_direction: 'incoming',
        }),
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
    const dirs: (Direction | null)[] = ['incoming', 'outgoing', 'unknown', null];
    for (const last_ok_direction of dirs) {
      for (const last_ok of ['2026-07-11T11:00:00-07:00', null]) {
        const line = recentStatusLine(
          deriveRecentStatus(contact({ channels: [channel({ last_ok, last_ok_direction })] })),
          NOW,
        );
        expect(line).not.toMatch(/worked|verified/i);
      }
    }
  });
});

describe('channelStatusLine / endpointStatusLine — per-row', () => {
  it('channelStatusLine: success → verb+ago (keyed on last_ok_direction), no success → dialed-not-reached', () => {
    expect(
      channelStatusLine(
        channel({ last_ok: '2026-07-11T11:00:00-07:00', last_ok_direction: 'outgoing' }),
        NOW,
      ),
    ).toBe('reached 1 h ago');
    // Failure-mutated `direction` cannot flip the verb (Finding 1, per-row form).
    expect(
      channelStatusLine(
        channel({
          direction: 'outgoing',
          last_ok: '2026-07-11T11:00:00-07:00',
          last_ok_direction: 'incoming',
        }),
        NOW,
      ),
    ).toBe('heard 1 h ago');
    expect(channelStatusLine(channel({ last_ok: null }), NOW)).toBe('dialed · not reached yet');
  });

  it('endpointStatusLine mirrors channelStatusLine, provenance-flavored', () => {
    const prov: Provenance = 'observed-incoming';
    expect(endpointStatusLine(endpoint({ provenance: prov, last_ok: '2026-07-11T11:00:00-07:00' }), NOW)).toBe(
      'heard 1 h ago',
    );
  });
});
