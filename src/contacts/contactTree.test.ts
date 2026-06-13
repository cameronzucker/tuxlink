// contactTree tests (tuxlink-je5d) — the pure outline model.
//
// Covers: group sections built from members[], a member appearing under TWO
// groups (groups are labels not folders), raw vs contact members, ungrouped
// derivation (unreferenced contacts + suggestions), deleted-contact member
// drop, the search filter + group auto-expand, and sort-within-section.

import { describe, it, expect } from 'vitest';
import { buildContactTree, groupsMatchingQuery, hasDisplayName } from './contactTree';
import type { Contact, Group, Suggestion } from './types';

const NOW = '2026-06-07T00:00:00Z';

function contact(id: string, callsign: string, name = ''): Contact {
  return { id, name, callsign, created_at: NOW, updated_at: NOW };
}

function group(id: string, name: string, members: Group['members']): Group {
  return { id, name, members, created_at: NOW, updated_at: NOW };
}

const ALICE = contact('c-alice', 'W6ABC', 'Alice Operator');
const BOB = contact('c-bob', 'KE7XYZ', 'Bob Relay');
const CARA = contact('c-cara', 'N0CARA', 'Cara Field');

describe('buildContactTree — sections', () => {
  it('builds a group section with contact + raw member rows in declared order', () => {
    const g = group('g1', 'ARES', [
      { type: 'contact', contact_id: 'c-alice' },
      { type: 'raw', callsign: 'KG7VLT' },
    ]);
    const tree = buildContactTree({ contacts: [ALICE], groups: [g], suggestions: [] });

    expect(tree.groups).toHaveLength(1);
    const section = tree.groups[0];
    expect(section.memberCount).toBe(2);
    const kinds = section.rows.map((r) => r.kind);
    expect(kinds).toContain('contact');
    expect(kinds).toContain('raw');
    const raw = section.rows.find((r) => r.kind === 'raw');
    expect(raw?.callsign).toBe('KG7VLT');
  });

  it('lists groups alphabetically by name', () => {
    const g1 = group('g1', 'SKYWARN', []);
    const g2 = group('g2', 'ARES', []);
    const tree = buildContactTree({ contacts: [], groups: [g1, g2], suggestions: [] });
    expect(tree.groups.map((s) => s.group.name)).toEqual(['ARES', 'SKYWARN']);
  });

  it('a contact in TWO groups appears under BOTH (groups are labels, not folders)', () => {
    const g1 = group('g1', 'ARES', [{ type: 'contact', contact_id: 'c-alice' }]);
    const g2 = group('g2', 'County EOC', [{ type: 'contact', contact_id: 'c-alice' }]);
    const tree = buildContactTree({ contacts: [ALICE], groups: [g1, g2], suggestions: [] });

    const aresRow = tree.groups[0].rows.find((r) => r.callsign === 'W6ABC');
    const eocRow = tree.groups[1].rows.find((r) => r.callsign === 'W6ABC');
    expect(aresRow).toBeDefined();
    expect(eocRow).toBeDefined();
    // Distinct React keys (namespaced by group id).
    expect(aresRow?.key).not.toEqual(eocRow?.key);
    // And Alice is NOT in Ungrouped (she is referenced by a group).
    expect(tree.ungrouped.find((r) => r.callsign === 'W6ABC')).toBeUndefined();
  });

  it('drops a deleted-contact member from rows AND keeps the count honest', () => {
    const g = group('g1', 'ARES', [
      { type: 'contact', contact_id: 'c-alice' },
      { type: 'contact', contact_id: 'c-gone' }, // no such contact
    ]);
    const tree = buildContactTree({ contacts: [ALICE], groups: [g], suggestions: [] });
    expect(tree.groups[0].rows).toHaveLength(1);
    expect(tree.groups[0].memberCount).toBe(1);
  });
});

describe('buildContactTree — ungrouped derivation', () => {
  it('puts contacts referenced by no group into Ungrouped', () => {
    const g = group('g1', 'ARES', [{ type: 'contact', contact_id: 'c-alice' }]);
    const tree = buildContactTree({ contacts: [ALICE, BOB], groups: [g], suggestions: [] });
    const ungroupedCalls = tree.ungrouped.map((r) => r.callsign);
    expect(ungroupedCalls).toContain('KE7XYZ'); // Bob — no group
    expect(ungroupedCalls).not.toContain('W6ABC'); // Alice — grouped
  });

  it('dissolves suggestions into Ungrouped as not-yet-saved rows', () => {
    const suggestions: Suggestion[] = [{ callsign: 'AE7PT', message_count: 3 }];
    const tree = buildContactTree({ contacts: [], groups: [], suggestions });
    const sug = tree.ungrouped.find((r) => r.kind === 'suggestion');
    expect(sug).toBeDefined();
    expect(sug?.callsign).toBe('AE7PT');
    if (sug?.kind === 'suggestion') expect(sug.messageCount).toBe(3);
  });

  it('a raw group member does NOT remove a same-callsign contact from Ungrouped', () => {
    // A raw member references no contact id, so it cannot "consume" a contact.
    const g = group('g1', 'ARES', [{ type: 'raw', callsign: 'KE7XYZ' }]);
    const tree = buildContactTree({ contacts: [BOB], groups: [g], suggestions: [] });
    expect(tree.ungrouped.find((r) => r.callsign === 'KE7XYZ' && r.kind === 'contact')).toBeDefined();
  });
});

describe('buildContactTree — search filter + auto-expand', () => {
  const g1 = group('g1', 'ARES District 7', [{ type: 'contact', contact_id: 'c-alice' }]);
  const g2 = group('g2', 'County EOC', [{ type: 'contact', contact_id: 'c-bob' }]);

  it('keeps a group whose NAME matches and all its members', () => {
    const tree = buildContactTree({
      contacts: [ALICE, BOB],
      groups: [g1, g2],
      suggestions: [],
      query: 'ares',
    });
    expect(tree.groups.map((s) => s.group.name)).toEqual(['ARES District 7']);
    expect(tree.groups[0].rows).toHaveLength(1);
  });

  it('keeps a group whose MEMBER matches even if the name does not', () => {
    const tree = buildContactTree({
      contacts: [ALICE, BOB],
      groups: [g1, g2],
      suggestions: [],
      query: 'ke7', // Bob's callsign — only County EOC has him
    });
    expect(tree.groups.map((s) => s.group.name)).toEqual(['County EOC']);
  });

  it('groupsMatchingQuery returns the ids of groups that should auto-expand', () => {
    const ids = groupsMatchingQuery({
      contacts: [ALICE, BOB],
      groups: [g1, g2],
      suggestions: [],
      query: 'ke7',
    });
    expect(ids.has('g2')).toBe(true);
    expect(ids.has('g1')).toBe(false);
  });

  it('returns an empty auto-expand set with no query', () => {
    const ids = groupsMatchingQuery({ contacts: [ALICE], groups: [g1], suggestions: [] });
    expect(ids.size).toBe(0);
  });

  it('filters ungrouped contacts and suggestions by the query', () => {
    const tree = buildContactTree({
      contacts: [CARA],
      groups: [],
      suggestions: [{ callsign: 'AE7PT', message_count: 1 }],
      query: 'cara',
    });
    expect(tree.ungrouped.map((r) => r.callsign)).toEqual(['N0CARA']);
  });
});

describe('buildContactTree — sort', () => {
  it('sorts rows within a section by name (case-insensitive ascending)', () => {
    const g = group('g1', 'ARES', [
      { type: 'contact', contact_id: 'c-bob' },
      { type: 'contact', contact_id: 'c-alice' },
      { type: 'contact', contact_id: 'c-cara' },
    ]);
    const tree = buildContactTree({
      contacts: [ALICE, BOB, CARA],
      groups: [g],
      suggestions: [],
      sort: 'name',
    });
    expect(tree.groups[0].rows.map((r) => r.callsign)).toEqual(['W6ABC', 'KE7XYZ', 'N0CARA']);
  });

  it('sorts by callsign when sort = callsign', () => {
    const tree = buildContactTree({
      contacts: [ALICE, BOB, CARA],
      groups: [],
      suggestions: [],
      sort: 'callsign',
    });
    // K < N < W
    expect(tree.ungrouped.map((r) => r.callsign)).toEqual(['KE7XYZ', 'N0CARA', 'W6ABC']);
  });

  it('last-heard orders by the supplied instant DESC, unknown callsigns last', () => {
    const tree = buildContactTree({
      contacts: [ALICE, BOB, CARA],
      groups: [],
      suggestions: [],
      sort: 'last-heard',
      lastHeard: { KE7XYZ: 2000, W6ABC: 1000 }, // N0CARA unknown
    });
    expect(tree.ungrouped.map((r) => r.callsign)).toEqual(['KE7XYZ', 'W6ABC', 'N0CARA']);
  });
});

describe('hasDisplayName', () => {
  it('is false for an empty name', () => {
    expect(hasDisplayName(contact('x', 'W6ABC', ''))).toBe(false);
  });
  it('is false when the name equals the callsign (case-insensitive)', () => {
    expect(hasDisplayName(contact('x', 'W6ABC', 'w6abc'))).toBe(false);
  });
  it('is true for a distinct name', () => {
    expect(hasDisplayName(contact('x', 'W6ABC', 'Alice'))).toBe(true);
  });
});
