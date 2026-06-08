// recipients.ts — pure helpers backing RecipientInput (Task A5).
//
// These functions own the *value-string ↔ chips* mapping, the autocomplete
// match list, and the group-resolution / member-count logic. They are pure
// (no React, no Tauri) so A6 (Compose send-time group expansion) can reuse the
// SAME resolution logic — guaranteeing the chip's displayed member count equals
// the eventual expansion length (M6).
//
// Adversarial-hardened decisions encoded here:
//   - H5  — a group is represented in the value string by the `group:<id>`
//           sentinel. A typed recipient whose TEXT equals a group NAME is NOT a
//           group (only the sentinel is). An unresolvable `group:<id>` (deleted
//           group) becomes a visibly-distinct "unknown-group" chip — the raw
//           token is preserved so recipients are never silently dropped.
//   - M6  — `resolveGroupMemberCount` uses the SAME resolution as send-time
//           expansion, so a deleted-contact member is excluded from BOTH.
//   - Codex#12 — `matchRecipients` emits a row per usable contact address
//           (primary callsign + email + tactical when present), each separately
//           pickable.

import type { Contact, Group } from './types';

/// The value-string prefix that marks a token as a group reference (H5). The id
/// that follows is the group's uuid (no spaces, no `;`).
export const GROUP_SENTINEL_PREFIX = 'group:';

/// The value-string token for a group chip (H5): `group:<id>`.
export function groupToken(group: Group): string {
  return `${GROUP_SENTINEL_PREFIX}${group.id}`;
}

/// Is this raw token a group sentinel? (Does not assert the group resolves.)
function isGroupToken(token: string): boolean {
  return token.startsWith(GROUP_SENTINEL_PREFIX);
}

/// Extract the group id from a `group:<id>` token. Returns '' for the empty id.
function groupIdOf(token: string): string {
  return token.slice(GROUP_SENTINEL_PREFIX.length);
}

// ---------------------------------------------------------------------------
// Chips — the display model parsed from the value string.
// ---------------------------------------------------------------------------

/// A single recipient chip rendered by RecipientInput.
///   - `raw`           — a literal callsign / email / tactical the operator typed
///                       or picked. `token` is the verbatim text.
///   - `group`         — a resolved distribution group. `token` is the
///                       `group:<id>` sentinel; `group` is the resolved Group.
///   - `unknown-group` — a `group:<id>` sentinel whose group is not in the
///                       current groups list (e.g. the group was deleted).
///                       `token` is preserved verbatim so the recipient is not
///                       silently dropped (H5).
export interface Chip {
  kind: 'raw' | 'group' | 'unknown-group';
  /// The exact value-string token this chip serializes back to.
  token: string;
  /// Present iff `kind === 'group'`.
  group?: Group;
}

/// Parse a semicolon-separated value string into display chips. Empty/blank
/// tokens are dropped. A `group:<id>` token resolves against `groups`; an
/// unresolvable one yields an `unknown-group` chip (H5) rather than vanishing.
export function parseChips(value: string, groups: Group[]): Chip[] {
  return value
    .split(';')
    .map((t) => t.trim())
    .filter((t) => t.length > 0)
    .map((token): Chip => {
      if (isGroupToken(token)) {
        const id = groupIdOf(token);
        const group = groups.find((g) => g.id === id);
        return group ? { kind: 'group', token, group } : { kind: 'unknown-group', token };
      }
      return { kind: 'raw', token };
    });
}

/// Serialize chips back to a semicolon-separated value string (the inverse of
/// `parseChips`, modulo whitespace normalization). Used to feed `onChange`.
export function formatChips(chips: Chip[]): string {
  return chips.map((c) => c.token).join('; ');
}

// ---------------------------------------------------------------------------
// Group resolution (M6) — shared with A6 send-time expansion.
// ---------------------------------------------------------------------------

/// Resolve a group's members to callsigns, in declared order:
///   - `{ type: 'contact', contact_id }` → that contact's callsign IF the
///     contact still exists; a deleted contact resolves to nothing (dropped).
///   - `{ type: 'raw', callsign }`        → the literal callsign.
/// This is the SAME logic A6 uses at send time, so the resolved length here
/// equals the eventual expansion length (M6).
export function resolveGroupMemberCallsigns(group: Group, contacts: Contact[]): string[] {
  const out: string[] = [];
  for (const m of group.members) {
    if (m.type === 'contact') {
      const c = contacts.find((x) => x.id === m.contact_id);
      if (c) out.push(c.callsign);
      // A deleted contact resolves to nothing — intentionally dropped (M6).
    } else {
      out.push(m.callsign);
    }
  }
  return out;
}

/// The resolved member count == the eventual send-time expansion length (M6).
/// A deleted-contact member is excluded, so the chip label stays honest.
export function resolveGroupMemberCount(group: Group, contacts: Contact[]): number {
  return resolveGroupMemberCallsigns(group, contacts).length;
}

// ---------------------------------------------------------------------------
// Autocomplete match rows.
// ---------------------------------------------------------------------------

/// A row in the autocomplete dropdown. `insert` is what gets committed into the
/// value string when the row is chosen — a raw recipient for contacts, or the
/// `group:<id>` sentinel for groups.
export interface MatchRow {
  /// Stable unique key for React + focus tracking.
  key: string;
  kind: 'contact' | 'group';
  /// The primary display text (callsign / email / tactical, or group name).
  label: string;
  /// Secondary context shown muted (contact name + which address form; or the
  /// resolved member count for a group).
  sublabel: string;
  /// The token committed into the value string when this row is selected.
  insert: string;
}

const contains = (haystack: string | undefined, needle: string): boolean =>
  !!haystack && haystack.toLowerCase().includes(needle);

/// Build the ordered autocomplete rows for `query`. Substring match
/// (case-insensitive) on contact name / callsign / email / tactical and on
/// group name. A matching contact emits a row PER usable address (primary
/// callsign + email + tactical when present) so the operator can pick the
/// Winlink-email or tactical form (Codex#12). Groups are listed first.
/// Empty/blank query → [].
export function matchRecipients(query: string, contacts: Contact[], groups: Group[]): MatchRow[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [];

  const rows: MatchRow[] = [];

  // Groups first (a small, high-signal set).
  for (const g of groups) {
    if (contains(g.name, q)) {
      const count = resolveGroupMemberCount(g, contacts);
      rows.push({
        key: `group:${g.id}`,
        kind: 'group',
        label: g.name,
        sublabel: `${count} ${count === 1 ? 'member' : 'members'}`,
        insert: groupToken(g),
      });
    }
  }

  // Contacts. A contact matches if ANY of its addressable fields (or its name)
  // contains the query; once matched, EVERY present address is offered as its
  // own pickable row (Codex#12).
  for (const c of contacts) {
    const matched =
      contains(c.name, q) ||
      contains(c.callsign, q) ||
      contains(c.email, q) ||
      contains(c.tactical, q);
    if (!matched) continue;

    rows.push({
      key: `contact:${c.id}:callsign`,
      kind: 'contact',
      label: c.callsign,
      sublabel: `${c.name} · callsign`,
      insert: c.callsign,
    });
    if (c.email) {
      rows.push({
        key: `contact:${c.id}:email`,
        kind: 'contact',
        label: c.email,
        sublabel: `${c.name} · email`,
        insert: c.email,
      });
    }
    if (c.tactical) {
      rows.push({
        key: `contact:${c.id}:tactical`,
        kind: 'contact',
        label: c.tactical,
        sublabel: `${c.name} · tactical`,
        insert: c.tactical,
      });
    }
  }

  return rows;
}
