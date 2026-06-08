import { describe, it, expect } from 'vitest';

import type { Contact, Group } from './types';
import {
  GROUP_SENTINEL_PREFIX,
  formatChips,
  groupToken,
  matchRecipients,
  parseChips,
  resolveGroupMemberCallsigns,
  resolveGroupMemberCount,
} from './recipients';

// --- Fixtures ---------------------------------------------------------------

const CONTACTS: Contact[] = [
  {
    id: 'c1',
    name: 'Vera Knox',
    callsign: 'KE7VAR',
    email: 'ke7var@winlink.org',
    tactical: 'NCS-1',
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
  {
    id: 'c2',
    name: 'Walt Briggs',
    callsign: 'W6ABC',
    // no email, no tactical
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
  {
    id: 'c3',
    name: 'Dot Reyes',
    callsign: 'KD0RES',
    email: 'dot@example.com',
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
];

const GROUPS: Group[] = [
  {
    id: 'g1',
    name: 'ARES Net',
    members: [
      { type: 'contact', contact_id: 'c1' },
      { type: 'contact', contact_id: 'c2' },
      { type: 'raw', callsign: 'W7XYZ' },
    ],
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
  {
    // A group whose NAME collides with a typed recipient — see the
    // "typed name equal to a group name is NOT a group" test.
    id: 'g2',
    name: 'KE7VAR',
    members: [{ type: 'raw', callsign: 'KE7VAR' }],
    created_at: '2026-06-01T00:00:00+00:00',
    updated_at: '2026-06-01T00:00:00+00:00',
  },
];

// ============================================================================
// matchRecipients
// ============================================================================

describe('matchRecipients', () => {
  it('empty query yields no rows', () => {
    expect(matchRecipients('', CONTACTS, GROUPS)).toEqual([]);
    expect(matchRecipients('   ', CONTACTS, GROUPS)).toEqual([]);
  });

  it('matches a contact by name substring, case-insensitive', () => {
    const rows = matchRecipients('vera', CONTACTS, GROUPS);
    const inserts = rows.map((r) => r.insert);
    expect(inserts).toContain('KE7VAR');
  });

  it('matches a contact by callsign substring, case-insensitive', () => {
    const rows = matchRecipients('w6a', CONTACTS, GROUPS);
    expect(rows.some((r) => r.insert === 'W6ABC')).toBe(true);
  });

  it('matches a contact by email substring, case-insensitive', () => {
    const rows = matchRecipients('dot@', CONTACTS, GROUPS);
    expect(rows.some((r) => r.insert === 'dot@example.com')).toBe(true);
  });

  it('emits one row per usable address (callsign + email + tactical) — Codex#12', () => {
    const rows = matchRecipients('vera', CONTACTS, GROUPS);
    const inserts = rows.map((r) => r.insert);
    // Vera (c1) has callsign KE7VAR, email ke7var@winlink.org, tactical NCS-1.
    expect(inserts).toContain('KE7VAR');
    expect(inserts).toContain('ke7var@winlink.org');
    expect(inserts).toContain('NCS-1');
    // Each alternate is a separate, distinctly-keyed contact row.
    const contactRows = rows.filter((r) => r.kind === 'contact');
    expect(new Set(contactRows.map((r) => r.key)).size).toBe(contactRows.length);
  });

  it('a contact with an email yields BOTH a callsign row and an email row (Codex#12)', () => {
    const rows = matchRecipients('vera', CONTACTS, GROUPS);
    expect(rows.some((r) => r.insert === 'KE7VAR')).toBe(true);
    expect(rows.some((r) => r.insert === 'ke7var@winlink.org')).toBe(true);
  });

  it('a contact WITHOUT email/tactical yields only the callsign row', () => {
    const rows = matchRecipients('walt', CONTACTS, GROUPS).filter((r) => r.kind === 'contact');
    expect(rows).toHaveLength(1);
    expect(rows[0].insert).toBe('W6ABC');
  });

  it('includes a group row matching by group name; its insert is the group: sentinel', () => {
    const rows = matchRecipients('ares', CONTACTS, GROUPS);
    const groupRow = rows.find((r) => r.kind === 'group');
    expect(groupRow).toBeDefined();
    expect(groupRow!.insert).toBe(groupToken(GROUPS[0]));
    expect(groupRow!.insert).toBe(`${GROUP_SENTINEL_PREFIX}g1`);
  });

  it('returns an ordered list (groups before contacts is fine; deterministic for a stable input)', () => {
    const rows = matchRecipients('e7v', CONTACTS, GROUPS);
    // 'e7v' matches KE7VAR (contact c1, callsign+email) and group g2 ('KE7VAR').
    expect(rows.length).toBeGreaterThan(0);
    // stable: calling again yields the same order
    expect(matchRecipients('e7v', CONTACTS, GROUPS)).toEqual(rows);
  });
});

// ============================================================================
// parse / format round-trip (H5 sentinel)
// ============================================================================

describe('parseChips / formatChips', () => {
  it('parses a raw callsign token into a raw chip', () => {
    const chips = parseChips('W6ABC', GROUPS);
    expect(chips).toHaveLength(1);
    expect(chips[0]).toMatchObject({ kind: 'raw', token: 'W6ABC' });
  });

  it('parses a group: sentinel into a resolved group chip (H5)', () => {
    const chips = parseChips('group:g1', GROUPS);
    expect(chips).toHaveLength(1);
    expect(chips[0].kind).toBe('group');
    expect(chips[0].token).toBe('group:g1');
    expect(chips[0].group?.id).toBe('g1');
  });

  it('an unresolvable group: token becomes a visibly-distinct unknown-group chip, NOT dropped (H5)', () => {
    const chips = parseChips('group:deleted-id', GROUPS);
    expect(chips).toHaveLength(1);
    expect(chips[0].kind).toBe('unknown-group');
    // The raw token is preserved so the recipient is not silently lost.
    expect(chips[0].token).toBe('group:deleted-id');
  });

  it('a typed recipient whose TEXT equals a group NAME is a raw chip, NOT a group (H5)', () => {
    // 'KE7VAR' is both a contact callsign AND group g2's name. As a typed
    // token (no group: prefix) it must be a raw chip.
    const chips = parseChips('KE7VAR', GROUPS);
    expect(chips).toHaveLength(1);
    expect(chips[0].kind).toBe('raw');
    expect(chips[0].token).toBe('KE7VAR');
  });

  it('round-trips a mixed value through parse → format (semicolon string)', () => {
    const value = 'W6ABC; group:g1; dot@example.com';
    const chips = parseChips(value, GROUPS);
    expect(chips.map((c) => c.kind)).toEqual(['raw', 'group', 'raw']);
    // format re-serializes to the same set of tokens (whitespace-normalized).
    const reparsed = parseChips(formatChips(chips), GROUPS);
    expect(reparsed.map((c) => c.token)).toEqual(['W6ABC', 'group:g1', 'dot@example.com']);
  });

  it('empty / whitespace value parses to no chips', () => {
    expect(parseChips('', GROUPS)).toEqual([]);
    expect(parseChips('   ;  ; ', GROUPS)).toEqual([]);
  });

  it('formatChips of zero chips is the empty string', () => {
    expect(formatChips([])).toBe('');
  });
});

// ============================================================================
// resolveGroupMembers (M6 — count == eventual expansion length)
// ============================================================================

describe('resolveGroupMemberCallsigns / resolveGroupMemberCount', () => {
  it('resolves contact_id members to callsigns + passes raw members through', () => {
    const calls = resolveGroupMemberCallsigns(GROUPS[0], CONTACTS);
    expect(calls).toEqual(['KE7VAR', 'W6ABC', 'W7XYZ']);
    expect(resolveGroupMemberCount(GROUPS[0], CONTACTS)).toBe(3);
  });

  it('a deleted-contact member resolves to nothing; count reflects only survivors (M6)', () => {
    // Drop c2 (W6ABC) from the contacts list — simulating a deleted contact.
    const survivors = CONTACTS.filter((c) => c.id !== 'c2');
    const calls = resolveGroupMemberCallsigns(GROUPS[0], survivors);
    // KE7VAR (c1) + W7XYZ (raw) survive; the deleted c2 drops out.
    expect(calls).toEqual(['KE7VAR', 'W7XYZ']);
    // Count == eventual expansion length, so the chip label is honest.
    expect(resolveGroupMemberCount(GROUPS[0], survivors)).toBe(2);
  });

  it('a group with no resolvable members resolves to count 0 (no crash)', () => {
    const orphan: Group = {
      id: 'g9',
      name: 'Empties',
      members: [{ type: 'contact', contact_id: 'gone' }],
      created_at: '2026-06-01T00:00:00+00:00',
      updated_at: '2026-06-01T00:00:00+00:00',
    };
    expect(resolveGroupMemberCallsigns(orphan, CONTACTS)).toEqual([]);
    expect(resolveGroupMemberCount(orphan, CONTACTS)).toBe(0);
  });
});
