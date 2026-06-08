// Unit tests for expandGroupsAndDedup — Task A6 send-time recipient expansion.
//
// Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → Task A6.
//
// expandGroupsAndDedup is the CORRECTNESS-CRITICAL transform that turns the
// raw chip tokens (group:<id> sentinels + literal recipients) into the actual
// callsign list that reaches the wire. The adversarial safety properties under
// test:
//   - H5  — NO `group:<id>` sentinel survives expansion (a group resolves to
//           its members; an unresolvable group token is DROPPED, never passed
//           through as a literal `group:<id>` string).
//   - H6/M5 — wire-key dedup: dedup key = trim → strip a trailing
//           `@winlink.org` (case-insensitive) → UPPERCASE, PRESERVING SSID. A
//           non-`@winlink.org` SMTP address is a distinct identity. First
//           occurrence's display form is kept.
//   - M6  — a deleted-contact group member resolves to nothing (no crash, no
//           empty token); surviving members expand normally.

import { describe, expect, it } from 'vitest';
import { expandGroupsAndDedup, splitAddrs } from './useDraft';
import type { Contact, Group } from '../contacts/types';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const contact = (id: string, callsign: string, extra: Partial<Contact> = {}): Contact => ({
  id,
  name: callsign,
  callsign,
  created_at: '2026-06-07T00:00:00Z',
  updated_at: '2026-06-07T00:00:00Z',
  ...extra,
});

const W6ABC = contact('c-w6abc', 'W6ABC');
const W7DEF = contact('c-w7def', 'W7DEF');
const KE6GLA = contact('c-ke6gla', 'KE6GLA-7'); // SSID-bearing identity

// A group with two contact-id members + one raw member.
const ARES: Group = {
  id: 'g-ares',
  name: 'ARES Net',
  members: [
    { type: 'contact', contact_id: 'c-w6abc' },
    { type: 'contact', contact_id: 'c-w7def' },
    { type: 'raw', callsign: 'W9XYZ' },
  ],
  created_at: '2026-06-07T00:00:00Z',
  updated_at: '2026-06-07T00:00:00Z',
};

// A group whose only members are contact-ids, one of which is deleted.
const PARTIAL: Group = {
  id: 'g-partial',
  name: 'Partial',
  members: [
    { type: 'contact', contact_id: 'c-w6abc' }, // survives
    { type: 'contact', contact_id: 'c-deleted' }, // deleted — resolves to nothing
  ],
  created_at: '2026-06-07T00:00:00Z',
  updated_at: '2026-06-07T00:00:00Z',
};

const CONTACTS = [W6ABC, W7DEF, KE6GLA];
const GROUPS = [ARES, PARTIAL];

// ---------------------------------------------------------------------------
// Group expansion (H5)
// ---------------------------------------------------------------------------

describe('expandGroupsAndDedup — group expansion (H5)', () => {
  it('expands a group:<id> sentinel to its member callsigns', () => {
    const out = expandGroupsAndDedup(['group:g-ares'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC', 'W7DEF', 'W9XYZ']);
  });

  it('resolves contact-id members to current callsigns and passes raw members through', () => {
    const out = expandGroupsAndDedup(['group:g-ares'], CONTACTS, GROUPS);
    // contact-id members resolved via the live contacts list, raw member literal.
    expect(out).toContain('W6ABC');
    expect(out).toContain('W7DEF');
    expect(out).toContain('W9XYZ');
  });

  it('NEVER lets a literal group:<id> token reach the wire (H5 safety property)', () => {
    const out = expandGroupsAndDedup(['group:g-ares', 'W6XYZ'], CONTACTS, GROUPS);
    expect(out.some((t) => t.startsWith('group:'))).toBe(false);
  });

  it('drops an unresolvable group:<id> token rather than leaking it as a literal (H5)', () => {
    // An unknown group id must NOT appear as a literal `group:<unknown>` on the
    // wire. It expands to nothing and is dropped.
    const out = expandGroupsAndDedup(['group:g-does-not-exist', 'W6ABC'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC']);
    expect(out.some((t) => t.startsWith('group:'))).toBe(false);
  });

  it('expands a group whose member contact was deleted to the survivors (M6)', () => {
    const out = expandGroupsAndDedup(['group:g-partial'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC']); // c-deleted resolves to nothing, dropped
    expect(out).not.toContain('');
  });
});

// ---------------------------------------------------------------------------
// Literal passthrough
// ---------------------------------------------------------------------------

describe('expandGroupsAndDedup — literal passthrough', () => {
  it('keeps a plain callsign that matches no group/contact unchanged', () => {
    const out = expandGroupsAndDedup(['W6XYZ'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6XYZ']);
  });

  it('preserves a non-@winlink.org SMTP recipient verbatim', () => {
    const out = expandGroupsAndDedup(['someone@example.com'], CONTACTS, GROUPS);
    expect(out).toEqual(['someone@example.com']);
  });

  it('returns an empty array for an empty input', () => {
    expect(expandGroupsAndDedup([], CONTACTS, GROUPS)).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Wire-key dedup (H6/M5)
// ---------------------------------------------------------------------------

describe('expandGroupsAndDedup — wire-key dedup (H6/M5)', () => {
  it('dedups a bare callsign against its @winlink.org email form (one entry)', () => {
    const out = expandGroupsAndDedup(['W6ABC', 'w6abc@winlink.org'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC']); // first occurrence's display form kept
  });

  it('keeps the FIRST occurrence display form when the email is listed first', () => {
    const out = expandGroupsAndDedup(['w6abc@winlink.org', 'W6ABC'], CONTACTS, GROUPS);
    expect(out).toEqual(['w6abc@winlink.org']);
  });

  it('dedups case-insensitively on the bare callsign', () => {
    const out = expandGroupsAndDedup(['W6ABC', 'w6abc'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC']);
  });

  it('treats W6ABC and W6ABC-7 as DISTINCT (SSID is identity, M5)', () => {
    const out = expandGroupsAndDedup(['W6ABC', 'W6ABC-7'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC', 'W6ABC-7']);
  });

  it('treats a non-winlink SMTP address as distinct from the bare callsign (M5)', () => {
    const out = expandGroupsAndDedup(['W6ABC', 'w6abc@gmail.com'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC', 'w6abc@gmail.com']);
  });

  it('does NOT strip the @winlink.org SSID when normalizing the email form', () => {
    // W6ABC-7 and w6abc-7@winlink.org are the SAME wire identity (SSID kept).
    const out = expandGroupsAndDedup(['W6ABC-7', 'w6abc-7@winlink.org'], CONTACTS, GROUPS);
    expect(out).toEqual(['W6ABC-7']);
  });

  it('dedups within an expanded group (no double-send of a repeated member)', () => {
    const dupeGroup: Group = {
      ...ARES,
      id: 'g-dupe',
      members: [
        { type: 'raw', callsign: 'W6ABC' },
        { type: 'contact', contact_id: 'c-w6abc' }, // also W6ABC
      ],
    };
    const out = expandGroupsAndDedup(['group:g-dupe'], CONTACTS, [...GROUPS, dupeGroup]);
    expect(out).toEqual(['W6ABC']);
  });
});

// ---------------------------------------------------------------------------
// Cc seeded from expanded To (Codex#6)
// ---------------------------------------------------------------------------

describe('expandGroupsAndDedup — Cc seeding from To', () => {
  it('removes a Cc recipient already present in the expanded To when seeded', () => {
    const to = expandGroupsAndDedup(['group:g-ares'], CONTACTS, GROUPS);
    // Cc contains W6ABC (already in To via the group) + a fresh recipient.
    const cc = expandGroupsAndDedup(['w6abc@winlink.org', 'NEW1'], CONTACTS, GROUPS, to);
    expect(cc).toEqual(['NEW1']); // W6ABC dropped — already in To
  });

  it('keeps Cc recipients not present in To when seeded', () => {
    const to = expandGroupsAndDedup(['W6ABC'], CONTACTS, GROUPS);
    const cc = expandGroupsAndDedup(['W7DEF', 'W8GHI'], CONTACTS, GROUPS, to);
    expect(cc).toEqual(['W7DEF', 'W8GHI']);
  });

  it('seeds on the wire key, not the display form (winlink-email To dedups bare Cc)', () => {
    const to = expandGroupsAndDedup(['w6abc@winlink.org'], CONTACTS, GROUPS);
    const cc = expandGroupsAndDedup(['W6ABC'], CONTACTS, GROUPS, to);
    expect(cc).toEqual([]);
  });

  it('without a seed, Cc dedups only within itself', () => {
    const cc = expandGroupsAndDedup(['W6ABC', 'w6abc@winlink.org'], CONTACTS, GROUPS);
    expect(cc).toEqual(['W6ABC']);
  });
});

// ---------------------------------------------------------------------------
// Integration with splitAddrs (the real Compose call shape)
// ---------------------------------------------------------------------------

describe('expandGroupsAndDedup — splitAddrs integration', () => {
  it('expands the To string the way Compose builds the send payload', () => {
    const out = expandGroupsAndDedup(
      splitAddrs('group:g-ares; W8SOLO'),
      CONTACTS,
      GROUPS,
    );
    expect(out).toEqual(['W6ABC', 'W7DEF', 'W9XYZ', 'W8SOLO']);
  });
});
