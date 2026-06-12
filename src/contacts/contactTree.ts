// contactTree.ts — the pure model behind the Contacts outline (tuxlink-je5d).
//
// The outline is ONE list: collapsible group sections (their members rendered
// inline) followed by an "Ungrouped" section. This module owns the model logic
// — building sections from contacts + groups + suggestions, deriving the
// ungrouped set, sorting, and the search filter (with group auto-expand). It is
// pure (no React, no Tauri) so it is unit-testable in isolation and so the
// component stays a thin renderer over it.
//
// Load-bearing decisions:
//   - Groups are LABELS, not folders. A contact referenced by two groups
//     appears under BOTH — membership lives on `Group.members[]`, so a member
//     row is derived per (group, member) pair, never moved.
//   - A group member is `{ type:'contact', contact_id }` or `{ type:'raw',
//     callsign }`. A contact member resolves to its Contact; a deleted-contact
//     member resolves to nothing (dropped — mirrors resolveGroupMemberCallsigns
//     in recipients.ts so counts stay honest). A raw member renders as a
//     callsign-only row with no backing Contact.
//   - Ungrouped = contacts referenced by NO group's contact members, PLUS the
//     suggested-from-traffic callsigns (each a not-yet-saved row).
//   - Search filters the whole tree. A group whose name OR any member matches
//     stays visible and AUTO-EXPANDS; a contact/raw row matches on its own
//     callsign/name/email/tactical. Ungrouped rows filter individually.

import type { Contact, Group, Suggestion } from './types';

/// How rows within a section (and the suggestions) are ordered. `last-heard` is
/// the recency default; the component supplies the per-callsign last-heard
/// instant (from the connection record) when it has one, else rows fall back to
/// the name/callsign tiebreak. Groups themselves always list alphabetically.
export type SortKey = 'last-heard' | 'name' | 'callsign';

/// A single identity row in the outline. A `contact` row carries the resolved
/// Contact; a `raw` row carries only a callsign (a typed group member with no
/// contact backing); a `suggestion` row is a not-yet-saved callsign heard in
/// traffic (rendered with a "New" tag + inline Save).
export type OutlineRow =
  | { kind: 'contact'; key: string; callsign: string; contact: Contact }
  | { kind: 'raw'; key: string; callsign: string }
  | { kind: 'suggestion'; key: string; callsign: string; messageCount: number };

/// A collapsible group section: its header (name + resolved member count + the
/// member callsigns for the avatar stack) and its member rows.
export interface GroupSection {
  group: Group;
  /// Resolved member count == rows.length (deleted-contact members already
  /// dropped) — the honest count shown in the header.
  memberCount: number;
  rows: OutlineRow[];
}

/// The whole outline model: the group sections (alphabetical by name) and the
/// ungrouped section (unreferenced contacts + suggestions).
export interface ContactTree {
  groups: GroupSection[];
  ungrouped: OutlineRow[];
}

/// Optional per-callsign last-heard instant (epoch ms), keyed by UPPERCASE
/// callsign. Supplied by the component from connection-record data when it has
/// any; absent callsigns sort last under `last-heard`.
export type LastHeardMap = Record<string, number>;

export interface BuildTreeOptions {
  contacts: Contact[];
  groups: Group[];
  suggestions: Suggestion[];
  /// Trimmed, already-lowercased search query, or '' for no filter.
  query?: string;
  sort?: SortKey;
  lastHeard?: LastHeardMap;
}

/// A stable comparator over rows for the chosen sort. `last-heard` orders by the
/// supplied last-heard instant DESC (most-recent first; unknown callsigns last),
/// tie-broken by display name then callsign. `name` / `callsign` are ascending,
/// case-insensitive. Suggestion rows have no Contact, so their display name is
/// the callsign.
function rowSortValue(row: OutlineRow): { name: string; callsign: string } {
  if (row.kind === 'contact') {
    const name = (row.contact.name ?? '').trim() || row.callsign;
    return { name: name.toLowerCase(), callsign: row.callsign.toLowerCase() };
  }
  return { name: row.callsign.toLowerCase(), callsign: row.callsign.toLowerCase() };
}

function makeRowComparator(sort: SortKey, lastHeard: LastHeardMap) {
  return (a: OutlineRow, b: OutlineRow): number => {
    if (sort === 'last-heard') {
      const ta = lastHeard[a.callsign.toUpperCase()] ?? -Infinity;
      const tb = lastHeard[b.callsign.toUpperCase()] ?? -Infinity;
      if (ta !== tb) return tb - ta; // most-recent first
    }
    const va = rowSortValue(a);
    const vb = rowSortValue(b);
    if (sort === 'callsign') {
      if (va.callsign !== vb.callsign) return va.callsign < vb.callsign ? -1 : 1;
      return va.name < vb.name ? -1 : va.name > vb.name ? 1 : 0;
    }
    // 'name' and the last-heard tiebreak both fall through to name → callsign.
    if (va.name !== vb.name) return va.name < vb.name ? -1 : 1;
    return va.callsign < vb.callsign ? -1 : va.callsign > vb.callsign ? 1 : 0;
  };
}

/// Does a contact match the query on any addressable field or its name?
function contactMatches(c: Contact, q: string): boolean {
  if (!q) return true;
  const hay = [c.name, c.callsign, c.email, c.tactical]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
  return hay.includes(q);
}

/// Does an outline row match the query? Contact rows match on their Contact;
/// raw + suggestion rows match on the callsign alone.
function rowMatches(row: OutlineRow, q: string): boolean {
  if (!q) return true;
  if (row.kind === 'contact') return contactMatches(row.contact, q);
  return row.callsign.toLowerCase().includes(q);
}

/// Resolve a group's members to outline rows in declared order. A contact
/// member resolves to its Contact (a deleted-contact member is dropped); a raw
/// member becomes a callsign-only row. Keys are namespaced by group id so the
/// same contact under two groups yields two distinct React keys.
function resolveGroupRows(group: Group, byId: Map<string, Contact>): OutlineRow[] {
  const rows: OutlineRow[] = [];
  for (const m of group.members) {
    if (m.type === 'contact') {
      const c = byId.get(m.contact_id);
      if (c) {
        rows.push({
          kind: 'contact',
          key: `g:${group.id}:c:${c.id}`,
          callsign: c.callsign,
          contact: c,
        });
      }
      // Deleted-contact member → dropped (count stays honest; mirrors M6).
    } else {
      rows.push({
        kind: 'raw',
        key: `g:${group.id}:raw:${m.callsign}`,
        callsign: m.callsign,
      });
    }
  }
  return rows;
}

/// Build the outline model. Groups list alphabetically by name; rows within
/// each group and within Ungrouped are sorted by `sort`. When `query` is set,
/// non-matching rows are dropped, a group survives iff its name matches OR it
/// retains ≥1 matching member, and (the component reads `autoExpandGroupIds`
/// from `groupsMatchingQuery`) matching groups auto-expand.
export function buildContactTree(opts: BuildTreeOptions): ContactTree {
  const { contacts, groups, suggestions } = opts;
  const q = (opts.query ?? '').trim().toLowerCase();
  const sort = opts.sort ?? 'last-heard';
  const lastHeard = opts.lastHeard ?? {};
  const cmp = makeRowComparator(sort, lastHeard);

  const byId = new Map(contacts.map((c) => [c.id, c]));

  // Contact ids referenced by ANY group's contact members → the "grouped" set.
  // Raw members do NOT consume a contact from Ungrouped (they reference no
  // contact id); a callsign that exists both as a raw group member and as a
  // saved contact still surfaces the contact under Ungrouped if no group
  // references it BY contact_id.
  const groupedContactIds = new Set<string>();
  for (const g of groups) {
    for (const m of g.members) {
      if (m.type === 'contact') groupedContactIds.add(m.contact_id);
    }
  }

  // Group sections — alphabetical by name. Each resolves its member rows, then
  // (under a query) drops non-matching member rows; a group whose NAME matches
  // keeps all its rows (the whole group is a hit).
  const nameSorted = [...groups].sort((a, b) =>
    a.name.toLowerCase() < b.name.toLowerCase()
      ? -1
      : a.name.toLowerCase() > b.name.toLowerCase()
        ? 1
        : 0,
  );

  const groupSections: GroupSection[] = [];
  for (const g of nameSorted) {
    const allRows = resolveGroupRows(g, byId);
    const nameHit = !q || g.name.toLowerCase().includes(q);
    const rows = nameHit ? allRows : allRows.filter((r) => rowMatches(r, q));
    if (q && !nameHit && rows.length === 0) continue; // group has no hit at all
    groupSections.push({
      group: g,
      memberCount: allRows.length, // honest count is over ALL resolved members
      rows: [...rows].sort(cmp),
    });
  }

  // Ungrouped contacts — saved contacts referenced by no group (by contact_id).
  const ungroupedContacts: OutlineRow[] = contacts
    .filter((c) => !groupedContactIds.has(c.id))
    .filter((c) => contactMatches(c, q))
    .map((c) => ({
      kind: 'contact' as const,
      key: `u:c:${c.id}`,
      callsign: c.callsign,
      contact: c,
    }));

  // Suggested-from-traffic callsigns dissolve into Ungrouped as not-yet-saved
  // rows. The backend already excludes saved contacts + the own callsign.
  const suggestionRows: OutlineRow[] = suggestions
    .filter((s) => !q || s.callsign.toLowerCase().includes(q))
    .map((s) => ({
      kind: 'suggestion' as const,
      key: `u:sug:${s.callsign}`,
      callsign: s.callsign,
      messageCount: s.message_count,
    }));

  const ungrouped = [...ungroupedContacts, ...suggestionRows].sort(cmp);

  return { groups: groupSections, ungrouped };
}

/// The set of group ids that should auto-expand for the current query: a group
/// whose name matches OR that retains ≥1 matching member after filtering. With
/// no query this is empty (the component keeps its own manual collapse state).
export function groupsMatchingQuery(opts: BuildTreeOptions): Set<string> {
  const q = (opts.query ?? '').trim().toLowerCase();
  if (!q) return new Set();
  const tree = buildContactTree({ ...opts, query: q });
  return new Set(tree.groups.map((s) => s.group.id));
}

/// Up-to-two-character initials for a named contact's avatar disc. Prefers the
/// name; never called for callsign-only rows (those render avatar-less).
export function contactInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

/// Does this contact have a usable display name (one distinct from its
/// callsign)? Drives both the avatar (named-only) and the name-vs-"+ add name"
/// subtitle. Case-insensitive callsign compare so "W6ABC" name == "w6abc"
/// callsign counts as no-name.
export function hasDisplayName(c: Contact): boolean {
  const name = (c.name ?? '').trim();
  if (name.length === 0) return false;
  return name.toLowerCase() !== c.callsign.trim().toLowerCase();
}
