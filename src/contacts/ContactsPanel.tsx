// ContactsPanel — THE address surface (tuxlink-je5d; unified by tuxlink-sbf03).
//
// Selected from the sidebar's Address → Contacts pseudo-folder — AND, since
// tuxlink-sbf03, the Favorites pseudo-folder (initialScope='favorites');
// AppShell swaps it in for BOTH the MessageList and the reading pane (grid
// lines 2→4 per AppShell.css). Contacts, Favorites, and Heard are SCOPES of
// this one list, not sibling features: Favorites = starred dials (with the
// retired FavoritesPanel's inline Connect), Heard = the not-saved-yet class
// (unconfirmed auto-observed stations + suggested-from-traffic callsigns).
//
//   ROSTER (~380px message-list footprint):
//     · a global search that scopes the WHOLE tree (groups containing a match
//       auto-expand) + the scope pills (All / ★ Favorites / Heard) + sort.
//     · collapsible GROUP sections — header = caret · name · member count ·
//       contained avatar stack (max 2 + "+N"); members render indented.
//     · a "Contacts" section — curated contacts referenced by no group.
//     · a "Heard — not saved" section — the one unsaved class, uniform row
//       anatomy (dashed avatar · provenance sub-line · "+ Save" · dismiss).
//     · EVERY row: avatar · callsign+name · reach dot · last-heard age · ★.
//   DETAIL (reading pane, polymorphic):
//     · member selected → ContactDetail (identity headline · connection
//       record · Reachability & connect rows with per-dial ★ (a starred dial
//       IS a Favorite) + Connect · groups · New message / Edit).
//     · group header selected → GroupManagement (rename · per-member remove ·
//       add by callsign/name · delete) — inline, no popup.
//
// Multi-select (Ctrl/Shift-click contact rows) raises a bulk bar with "Add to
// group" and "Remove". There is NO "Message all" — messaging is Compose /
// send-to-group, never a contacts-list verb.
//
// The model logic lives in contactTree.ts (pure, unit-tested); this file is a
// thin renderer + selection/collapse state over it.

import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import './ContactsPanel.css';
import type { Contact, Group, GroupMember, Suggestion } from './types';
import { useContacts } from './useContacts';
import {
  buildContactTree,
  groupsMatchingQuery,
  contactInitials,
  hasDisplayName,
  type GroupSection,
  type OutlineRow,
  type SortKey,
} from './contactTree';
import { ContactEditor, emptyContact } from './ContactEditor';
import { openComposeTo } from './composeTo';
import { ConnectionRecord } from '../favorites/ConnectionRecord';
import { useContactConnectionRecord } from './useContactConnectionRecord';
import { FAVORITES_QUERY_KEY } from '../favorites/useFavorites';
import { dialToNewFavorite, favoriteKey } from '../favorites/dialToFavorite';
import type { Favorite, FavoriteDial, StationsFile } from '../favorites/types';
import { connectPeerChannel, connectPeerEndpoint, radioModeForPeerTransport } from '../peers/connectPeer';
import {
  channelStatusLine,
  channelSummary,
  deriveRecentStatus,
  endpointStatusLine,
  recentStatusLine,
} from '../peers/recentStatus';
import type { Channel as ReachChannel, Endpoint as ReachEndpoint, Provenance } from './types';

/// Query key for the suggest-from-history rows. Distinct from the contacts file
/// key (`['contacts']`) so a Save can re-derive suggestions without forcing a
/// full contacts refetch.
const SUGGESTIONS_QUERY_KEY = ['contacts', 'suggestions'] as const;

/// What the reading pane shows. A member row selects a contact (by id) OR a raw
/// callsign (no Contact backing); a group header selects a group (by id).
type Selection =
  | { kind: 'none' }
  | { kind: 'contact'; id: string }
  | { kind: 'raw'; callsign: string }
  | { kind: 'suggestion'; callsign: string; messageCount: number }
  | { kind: 'group'; id: string };

/// The inline contact editor target (takes over the detail pane when open).
type EditorState = { kind: 'closed' } | { kind: 'new'; seed: Contact } | { kind: 'edit'; contact: Contact };

/// Roster scope (tuxlink-sbf03 consolidation): Contacts, Favorites, and Heard
/// are SCOPES of one list, not separate features. 'all' = groups + contacts +
/// heard; 'favorites' = starred dials (with inline Connect — the retired
/// FavoritesPanel's job); 'heard' = the not-yet-saved class only.
export type RosterScope = 'all' | 'favorites' | 'heard';

export interface ContactsPanelProps {
  /** Scope pre-selected at mount. The sidebar Favorites pseudo-folder opens
   *  THIS panel with 'favorites'; the Contacts pseudo-folder omits it ('all'). */
  initialScope?: RosterScope;
  /** RADIO-1: open + arm the matching modem for a starred dial's Connect
   *  (AppShell's handleFavoritesConnect — the exact handler the retired
   *  FavoritesPanel took). Omitted ⇒ Connect buttons are not rendered. */
  onConnectFavorite?: (dial: FavoriteDial) => void;
}

// ---------------------------------------------------------------------------
// Row-meta derivation (tuxlink-sbf03): every roster row carries the SAME
// right-edge meta — reach dot · last-heard age · ★. All from contact data the
// roster already holds; no extra queries.
// ---------------------------------------------------------------------------

/** Most-recent activity instant (ms) across channels + endpoints, or null. */
export function contactLastHeardMs(c: Contact): number | null {
  const times: number[] = [];
  for (const ch of c.channels ?? []) {
    const t = Date.parse(ch.last_seen);
    if (!Number.isNaN(t)) times.push(t);
  }
  for (const ep of c.endpoints ?? []) {
    const t = Date.parse(ep.last_seen);
    if (!Number.isNaN(t)) times.push(t);
  }
  return times.length > 0 ? Math.max(...times) : null;
}

/** Dot tone: activity within 6 h = good (green), within 7 d = stale (amber),
 *  older/never = dead (hollow). Same recency the `ago` label shows. */
export function reachTone(lastMs: number | null, nowMs: number): 'good' | 'stale' | 'dead' {
  if (lastMs === null) return 'dead';
  const age = nowMs - lastMs;
  if (age <= 6 * 3600_000) return 'good';
  if (age <= 7 * 86_400_000) return 'stale';
  return 'dead';
}

/** Compact age: "now", "35 m", "2 h", "3 d", "5 w"; em-dash for never. */
export function agoLabel(lastMs: number | null, nowMs: number): string {
  if (lastMs === null) return '—';
  const s = Math.max(0, Math.floor((nowMs - lastMs) / 1000));
  if (s < 90) return 'now';
  if (s < 3600) return `${Math.round(s / 60)} m`;
  if (s < 48 * 3600) return `${Math.round(s / 3600)} h`;
  if (s < 14 * 86_400) return `${Math.round(s / 86_400)} d`;
  return `${Math.round(s / (7 * 86_400))} w`;
}

/** Email pseudo-contact ("SMTP:addr" callsign): render an EMAIL chip + the
 *  display name, never the raw scheme string (tuxlink-sbf03). */
export function emailAddressOf(c: Pick<Contact, 'callsign'>): string | null {
  return c.callsign.startsWith('SMTP:') ? c.callsign.slice('SMTP:'.length) : null;
}

/** Base callsign for favorite↔contact linking ("N0DAJ-10" → "N0DAJ"). */
function baseCall(callsign: string): string {
  return callsign.split('-')[0].toUpperCase();
}

/** The starred favorites belonging to a contact — linked by `contact_id` when
 *  the favorite carries one, else by base-callsign match on the gateway. */
export function starredFavoritesOf(c: Contact, favorites: Favorite[]): Favorite[] {
  const base = baseCall(c.callsign);
  return favorites.filter(
    (f) => f.starred && (f.contact_id === c.id || baseCall(f.gateway) === base),
  );
}

export function ContactsPanel({ initialScope = 'all', onConnectFavorite }: ContactsPanelProps = {}) {
  const qc = useQueryClient();
  const { contacts, groups, upsertContact, deleteContact, confirmContact, upsertGroup, deleteGroup } =
    useContacts();
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>('last-heard');
  const [scope, setScope] = useState<RosterScope>(initialScope);
  const [selection, setSelection] = useState<Selection>({ kind: 'none' });
  const [editor, setEditor] = useState<EditorState>({ kind: 'closed' });
  // Manual collapse state — group ids the operator has explicitly collapsed.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // Multi-select set of contact ids (raw/suggestion rows are not multi-selectable).
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const suggestionsQuery = useQuery({
    queryKey: SUGGESTIONS_QUERY_KEY,
    queryFn: () => invoke<Suggestion[]>('contacts_suggestions'),
  });
  const suggestions = suggestionsQuery.data ?? [];

  // Favorites (tuxlink-sbf03): same query key FavoritesPanel used, so star
  // toggles from any surface invalidate consistently. Drives the row-meta ★,
  // the Favorites scope, and the detail pane's per-dial stars.
  const favoritesQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });
  const favorites = useMemo(() => favoritesQuery.data?.favorites ?? [], [favoritesQuery.data]);
  const starredFavorites = useMemo(() => favorites.filter((f) => f.starred), [favorites]);

  // Operator grid for a telnet-endpoint Connect's B2F locator (best-effort;
  // empty means the dial simply carries no locator). Mirrors StationFinderPanel.
  const gridQuery = useQuery({
    queryKey: ['config', 'grid'],
    queryFn: () => invoke<{ grid: string | null }>('config_read'),
  });
  const operatorGrid = gridQuery.data?.grid ?? '';

  const q = query.trim().toLowerCase();

  // The curated address book (`Confirmed`, plus any forward-compat tier) vs the
  // auto-observed "Recent" tier (`Unconfirmed`). The tree renders ONLY curated
  // contacts; the Recent section (spec §AMENDMENT pt. 7) renders the unconfirmed
  // ones below it, so an auto-created record never silently pollutes the roster.
  const curatedContacts = useMemo(
    () => contacts.filter((c) => (c.tier ?? 'confirmed') !== 'unconfirmed'),
    [contacts],
  );
  const recentContacts = useMemo(
    () =>
      contacts
        .filter((c) => (c.tier ?? 'confirmed') === 'unconfirmed')
        .filter((c) => recentMatchesQuery(c, q))
        .sort((a, b) => recentRecency(b) - recentRecency(a)),
    [contacts, q],
  );
  // Suggestions render in the Heard section (not the tree), so they filter on
  // the query here.
  const visibleSuggestions = useMemo(
    () => suggestions.filter((s) => !q || s.callsign.toLowerCase().includes(q)),
    [suggestions, q],
  );

  // The "Last heard" sort finally gets its data (tuxlink-sbf03: the map was
  // caller-supplied and NEVER passed, so the default sort silently didn't
  // sort). Keyed by uppercase callsign per LastHeardMap's contract.
  const lastHeard = useMemo(() => {
    const map: Record<string, number> = {};
    for (const c of contacts) {
      const t = contactLastHeardMs(c);
      if (t !== null) map[c.callsign.toUpperCase()] = t;
    }
    return map;
  }, [contacts]);

  // Suggestions no longer render inside Ungrouped — they are Heard-class rows
  // (tuxlink-sbf03), merged with the unconfirmed contacts below the roster.
  const tree = useMemo(
    () => buildContactTree({ contacts: curatedContacts, groups, suggestions: [], query: q, sort, lastHeard }),
    [curatedContacts, groups, q, sort, lastHeard],
  );

  // Under a query, groups containing a match auto-expand (overrides manual
  // collapse). With no query, manual collapse state governs.
  const autoExpand = useMemo(
    () => groupsMatchingQuery({ contacts: curatedContacts, groups, suggestions: [], query: q, sort }),
    [curatedContacts, groups, q, sort],
  );

  // ---- Favorites scope rows (tuxlink-sbf03) ----
  // A starred favorite joins to a contact by contact_id / base-callsign; the
  // remainder (pure gateway/CMS favorites with no contact) still get rows —
  // retiring FavoritesPanel must not orphan them.
  const favoriteRows = useMemo(() => {
    const rows: Array<{ favorite: Favorite; contact: Contact | null }> = [];
    for (const f of starredFavorites) {
      const c =
        contacts.find((k) => f.contact_id === k.id) ??
        contacts.find((k) => baseCall(k.callsign) === baseCall(f.gateway)) ??
        null;
      if (q) {
        const hay = [f.gateway, f.freq, f.band, c?.name, c?.callsign].filter(Boolean).join(' ').toLowerCase();
        if (!hay.includes(q)) continue;
      }
      rows.push({ favorite: f, contact: c });
    }
    rows.sort((a, b) => {
      const ta = a.contact ? contactLastHeardMs(a.contact) ?? -Infinity : -Infinity;
      const tb = b.contact ? contactLastHeardMs(b.contact) ?? -Infinity : -Infinity;
      return tb - ta || a.favorite.gateway.localeCompare(b.favorite.gateway);
    });
    return rows;
  }, [starredFavorites, contacts, q]);

  // Scope pill counts (whole-population, not query-filtered — the pill answers
  // "how many exist", the list answers "how many match").
  const heardCount =
    contacts.filter((c) => (c.tier ?? 'confirmed') === 'unconfirmed').length + suggestions.length;

  const isExpanded = (groupId: string): boolean =>
    q ? autoExpand.has(groupId) : !collapsed.has(groupId);

  const toggleCollapse = (groupId: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const selectedContact = useMemo(
    () => (selection.kind === 'contact' ? contacts.find((c) => c.id === selection.id) ?? null : null),
    [selection, contacts],
  );
  const selectedGroup = useMemo(
    () => (selection.kind === 'group' ? groups.find((g) => g.id === selection.id) ?? null : null),
    [selection, groups],
  );
  const selectedRawCallsign = selection.kind === 'raw' ? selection.callsign : null;
  const selectedSuggestion = selection.kind === 'suggestion' ? selection : null;

  const saveContact = async (c: Contact) => {
    await upsertContact(c);
    setEditor({ kind: 'closed' });
  };

  const addSuggestion = async (callsign: string) => {
    // One explicit click → create a contact with the callsign prefilled (name
    // defaults to the callsign so the row isn't blank). Then invalidate the
    // suggestions query so the just-saved row drops out (the backend re-derives
    // suggestions excluding saved contacts — without this a second click would
    // create a DUPLICATE, since each empty id gets a fresh uuid).
    await upsertContact({ ...emptyContact(callsign), name: callsign });
    await qc.invalidateQueries({ queryKey: SUGGESTIONS_QUERY_KEY });
  };

  // ---- Recent (unconfirmed-tier) row actions ----
  // Promote = one-click "+ Add" (spec §AMENDMENT pt. 7): flip tier via
  // contact_confirm so the row moves into the curated list. CURATION, not
  // identity authentication.
  const promoteContact = async (id: string) => {
    await confirmContact(id);
  };
  // Delete an auto-observed record (cascades its keyring endpoints backend-side).
  const deleteRecent = async (id: string) => {
    await deleteContact(id);
    if (selection.kind === 'contact' && selection.id === id) setSelection({ kind: 'none' });
  };
  // A Recent row IS a real Contact (has an id) → selectable into the detail
  // pane exactly like a curated row. `selectedContact` resolves it from the
  // full contacts list, so its ContactDetail (reachability + promote) shows.
  const selectRecent = (id: string) => {
    setSelected(new Set());
    setSelection({ kind: 'contact', id });
    setEditor({ kind: 'closed' });
  };

  // A suggestion row (mailbox correspondent, not yet saved) is selectable into
  // the detail pane like any other row — it was previously a dead body with a
  // lone "Save" button, so clicking a suggestion did nothing. The detail offers
  // New message + Save (add), carrying the message count for context.
  const selectSuggestion = (callsign: string, messageCount: number) => {
    setSelected(new Set());
    setSelection({ kind: 'suggestion', callsign, messageCount });
    setEditor({ kind: 'closed' });
  };

  // ---- per-dial star (tuxlink-sbf03) ----
  // ★ on a detail-pane dial = that dial is a starred Favorite (it appears in
  // the Favorites scope and the ribbon Connect targets). Find-or-create then
  // toggle — the StationFinderPanel save-favorite pattern, keyed the same way.
  const toggleDialStar = async (dial: FavoriteDial) => {
    const existing = favorites.find((f) => favoriteKey(f) === favoriteKey(dial));
    if (existing) {
      await invoke('favorite_star', { id: existing.id, starred: !existing.starred }).catch(() => {});
    } else {
      const created = await invoke<Favorite>('favorite_upsert', {
        favorite: dialToNewFavorite(dial),
      }).catch(() => null);
      if (created) await invoke('favorite_star', { id: created.id, starred: true }).catch(() => {});
    }
    await qc.invalidateQueries({ queryKey: FAVORITES_QUERY_KEY });
  };

  // ---- multi-select (Ctrl/Shift) over contact rows ----
  const toggleMultiSelect = (contactId: string, e: React.MouseEvent) => {
    const additive = e.ctrlKey || e.metaKey || e.shiftKey;
    setSelected((prev) => {
      const next = new Set(additive ? prev : []);
      if (next.has(contactId)) next.delete(contactId);
      else next.add(contactId);
      return next;
    });
  };

  const clearMultiSelect = () => setSelected(new Set());

  // A plain click on a contact row selects it for the detail pane AND, if a
  // modifier is held, toggles its membership in the multi-select set instead.
  const onContactRowClick = (contactId: string, e: React.MouseEvent) => {
    if (e.ctrlKey || e.metaKey || e.shiftKey) {
      toggleMultiSelect(contactId, e);
      return;
    }
    setSelected(new Set());
    setSelection({ kind: 'contact', id: contactId });
    setEditor({ kind: 'closed' });
  };

  const addSelectedToGroup = async (group: Group, selectedIds: string[]) => {
    const existing = new Set(
      group.members.filter((m) => m.type === 'contact').map((m) => (m as { contact_id: string }).contact_id),
    );
    const added: GroupMember[] = selectedIds
      .filter((id) => !existing.has(id))
      .map((id) => ({ type: 'contact', contact_id: id }));
    if (added.length === 0) {
      clearMultiSelect();
      return;
    }
    await upsertGroup({ ...group, members: [...group.members, ...added] });
    clearMultiSelect();
  };

  const addSelectedToNewGroup = async (name: string, selectedIds: string[]) => {
    const members: GroupMember[] = selectedIds.map((id) => ({ type: 'contact', contact_id: id }));
    await upsertGroup({ id: '', name, members, created_at: '', updated_at: '' });
    clearMultiSelect();
  };

  const removeSelectedContacts = async (selectedIds: string[]) => {
    for (const id of selectedIds) {
      // eslint-disable-next-line no-await-in-loop -- sequential delete keeps the
      // file write serialized; the count is small (operator-selected rows).
      await deleteContact(id);
    }
    if (selection.kind === 'contact' && selectedIds.includes(selection.id)) {
      setSelection({ kind: 'none' });
    }
    clearMultiSelect();
  };

  // The contact editor takes over the whole panel body when open (inline; no
  // popup). Mirrors the prior surface so the editor flow is unchanged.
  if (editor.kind === 'new' || editor.kind === 'edit') {
    const target = editor.kind === 'new' ? editor.seed : editor.contact;
    return (
      <div className="contacts-panel contacts-panel--editing" data-testid="contacts-panel">
        <ContactEditor
          contact={target}
          onSave={saveContact}
          onCancel={() => setEditor({ kind: 'closed' })}
        />
      </div>
    );
  }

  return (
    <div className="contacts-panel" data-testid="contacts-panel">
      {/* ROSTER — the single outline. */}
      <div className="contacts-roster" data-testid="contacts-roster">
        <div className="contacts-roster-toolbar">
          <input
            className="contacts-search"
            data-testid="contacts-search"
            type="text"
            placeholder="Search contacts"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
          <button
            type="button"
            className="contacts-new-btn"
            data-testid="contacts-new"
            onClick={() => setEditor({ kind: 'new', seed: emptyContact() })}
          >
            + New
          </button>
        </div>

        {/* Scope pills + sort — ONE always-present row (tuxlink-sbf03). The
            pills are the consolidation: Favorites and Heard are filters over
            this list, not sibling features. */}
        <div className="contacts-scopes" data-testid="contacts-scopes">
          <ScopePill
            label="All"
            count={contacts.length}
            active={scope === 'all'}
            testid="contacts-scope-all"
            onClick={() => setScope('all')}
          />
          <ScopePill
            label="★ Favorites"
            count={starredFavorites.length}
            active={scope === 'favorites'}
            testid="contacts-scope-favorites"
            onClick={() => setScope('favorites')}
          />
          <ScopePill
            label="Heard"
            count={heardCount}
            active={scope === 'heard'}
            testid="contacts-scope-heard"
            onClick={() => setScope('heard')}
          />
          <label className="contacts-sort" data-testid="contacts-sort">
            sort
            <select
              className="tux-select contacts-sort-select"
              data-testid="contacts-sort-select"
              value={sort}
              onChange={(e) => setSort(e.target.value as SortKey)}
            >
              <option value="last-heard">Last heard</option>
              <option value="name">Name</option>
              <option value="callsign">Callsign</option>
            </select>
          </label>
        </div>

        {scope === 'favorites' ? (
          <div className="contacts-tree" data-testid="contacts-tree">
            {favoriteRows.length === 0 ? (
              <p className="contacts-empty" data-testid="contacts-favorites-empty">
                No starred favorites yet — ★ a dial on a contact, or star a gateway from Find a Station.
              </p>
            ) : (
              <ul className="contacts-rows">
                {favoriteRows.map(({ favorite, contact }) => (
                  <FavoriteScopeRow
                    key={favorite.id}
                    favorite={favorite}
                    contact={contact}
                    selected={
                      contact !== null && selection.kind === 'contact' && selection.id === contact.id
                    }
                    onSelect={() => {
                      if (contact) {
                        setSelection({ kind: 'contact', id: contact.id });
                      } else {
                        setSelection({ kind: 'raw', callsign: favorite.gateway });
                      }
                      setEditor({ kind: 'closed' });
                    }}
                    onConnect={onConnectFavorite}
                  />
                ))}
              </ul>
            )}
          </div>
        ) : scope === 'heard' ? (
          <div className="contacts-tree" data-testid="contacts-tree">
            <HeardSection
              recentContacts={recentContacts}
              suggestions={visibleSuggestions}
              selection={selection}
              standalone
              onSelectRecent={selectRecent}
              onPromote={(id) => void promoteContact(id)}
              onDeleteRecent={(id) => void deleteRecent(id)}
              onSelectSuggestion={selectSuggestion}
              onSaveSuggestion={(cs) => void addSuggestion(cs)}
            />
          </div>
        ) : (
        <div className="contacts-tree" data-testid="contacts-tree">
          {tree.groups.map((section) => (
            <GroupSectionView
              key={section.group.id}
              section={section}
              favorites={starredFavorites}
              expanded={isExpanded(section.group.id)}
              onToggleCollapse={() => toggleCollapse(section.group.id)}
              onSelectGroup={() => {
                setSelection({ kind: 'group', id: section.group.id });
                setEditor({ kind: 'closed' });
              }}
              selection={selection}
              multiSelected={selected}
              onContactRowClick={onContactRowClick}
              onSelectRaw={(callsign) => {
                setSelected(new Set());
                setSelection({ kind: 'raw', callsign });
                setEditor({ kind: 'closed' });
              }}
            />
          ))}

          <section className="contacts-ungrouped" data-testid="contacts-ungrouped">
            <div className="contacts-ungrouped-head" data-testid="contacts-ungrouped-head">
              <span className="contacts-ungrouped-label">Contacts</span>
              <span className="contacts-ungrouped-count">{tree.ungrouped.length}</span>
            </div>
            {tree.ungrouped.length === 0 ? (
              <p className="contacts-empty" data-testid="contacts-ungrouped-empty">
                No ungrouped contacts
              </p>
            ) : (
              <ul className="contacts-rows">
                {tree.ungrouped.map((row) => (
                  <OutlineRowView
                    key={row.key}
                    row={row}
                    selection={selection}
                    multiSelected={selected}
                    favorites={starredFavorites}
                    onContactRowClick={onContactRowClick}
                    onSelectRaw={(callsign) => {
                      setSelected(new Set());
                      setSelection({ kind: 'raw', callsign });
                      setEditor({ kind: 'closed' });
                    }}
                    onSelectSuggestion={selectSuggestion}
                    onSaveSuggestion={addSuggestion}
                  />
                ))}
              </ul>
            )}
          </section>

          {/* HEARD — the one not-saved-yet class (tuxlink-sbf03): unconfirmed
              auto-observed stations AND suggested-from-traffic callsigns, one
              section, one row anatomy. Each row makes its OWN honest RF claim;
              the section makes none. Empty ⇒ hidden. */}
          <HeardSection
            recentContacts={recentContacts}
            suggestions={visibleSuggestions}
            selection={selection}
            onSelectRecent={selectRecent}
            onPromote={(id) => void promoteContact(id)}
            onDeleteRecent={(id) => void deleteRecent(id)}
            onSelectSuggestion={selectSuggestion}
            onSaveSuggestion={(cs) => void addSuggestion(cs)}
          />
        </div>
        )}

        {selected.size > 0 && (
          <BulkBar
            count={selected.size}
            groups={groups}
            onAddToGroup={(g) => void addSelectedToGroup(g, [...selected])}
            onAddToNewGroup={(name) => void addSelectedToNewGroup(name, [...selected])}
            onRemove={() => void removeSelectedContacts([...selected])}
            onClear={clearMultiSelect}
          />
        )}
      </div>

      {/* DETAIL — polymorphic reading pane. */}
      <div className="contacts-detail" data-testid="contacts-detail-pane">
        {selectedContact ? (
          <ContactDetail
            contact={selectedContact}
            groups={groups}
            operatorGrid={operatorGrid}
            favorites={favorites}
            onToggleStar={(dial) => void toggleDialStar(dial)}
            onNewMessage={() => void openComposeTo(selectedContact.callsign)}
            onEdit={() => setEditor({ kind: 'edit', contact: selectedContact })}
            onPromote={
              (selectedContact.tier ?? 'confirmed') === 'unconfirmed'
                ? () => void promoteContact(selectedContact.id)
                : undefined
            }
          />
        ) : selectedGroup ? (
          <GroupManagement
            group={selectedGroup}
            contacts={contacts}
            onSave={(g) => upsertGroup(g)}
            onDelete={async (id) => {
              await deleteGroup(id);
              setSelection({ kind: 'none' });
            }}
          />
        ) : selectedRawCallsign ? (
          <RawDetail
            callsign={selectedRawCallsign}
            onNewMessage={() => void openComposeTo(selectedRawCallsign)}
            onSave={() => void addSuggestion(selectedRawCallsign)}
          />
        ) : selectedSuggestion ? (
          <RawDetail
            callsign={selectedSuggestion.callsign}
            messageCount={selectedSuggestion.messageCount}
            onNewMessage={() => void openComposeTo(selectedSuggestion.callsign)}
            onSave={() => void addSuggestion(selectedSuggestion.callsign)}
          />
        ) : (
          <div className="contacts-detail-empty" data-testid="contacts-detail-empty">
            Select a contact or group to view details.
          </div>
        )}
      </div>
    </div>
  );
}

// ===========================================================================
// Roster sub-components
// ===========================================================================

/** One scope pill: label + population count; the active pill carries the
 *  accent (tuxlink-sbf03 — Favorites/Heard are FILTERS of one list). */
function ScopePill({
  label,
  count,
  active,
  testid,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  testid: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`contacts-scope${active ? ' contacts-scope--active' : ''}`}
      data-testid={testid}
      aria-pressed={active}
      onClick={onClick}
    >
      {label} <b>{count}</b>
    </button>
  );
}

/** The uniform right-edge row meta: reach dot · last-heard age · ★. */
function RowMeta({ contact, favorites }: { contact: Contact; favorites: Favorite[] }) {
  const last = contactLastHeardMs(contact);
  const now = Date.now();
  const starred = starredFavoritesOf(contact, favorites).length > 0;
  return (
    <span className="contacts-row-meta-right" data-testid={`row-meta-${contact.id}`}>
      <span className={`contacts-reach-dot contacts-reach-dot--${reachTone(last, now)}`} aria-hidden="true" />
      <span className="contacts-row-ago">{agoLabel(last, now)}</span>
      {starred && (
        <span className="contacts-row-star" data-testid={`row-star-${contact.id}`} aria-label="starred favorite">
          ★
        </span>
      )}
    </span>
  );
}

/** Avatar for every row (tuxlink-sbf03 uniform anatomy): initials from the
 *  display name, else the callsign's first two characters. `heard` renders the
 *  dashed not-saved variant. */
function RowAvatar({ contact, heard = false }: { contact: Pick<Contact, 'callsign' | 'name'>; heard?: boolean }) {
  const text = hasDisplayName(contact as Contact)
    ? contactInitials(contact.name)
    : contact.callsign.replace(/^SMTP:/, '').slice(0, 2).toUpperCase();
  return (
    <span className={`contacts-avatar contacts-avatar--sm${heard ? ' contacts-avatar--heard' : ''}`} aria-hidden="true">
      {text}
    </span>
  );
}

function GroupSectionView({
  section,
  expanded,
  onToggleCollapse,
  onSelectGroup,
  selection,
  multiSelected,
  favorites,
  onContactRowClick,
  onSelectRaw,
}: {
  section: GroupSection;
  expanded: boolean;
  onToggleCollapse: () => void;
  onSelectGroup: () => void;
  selection: Selection;
  multiSelected: Set<string>;
  favorites: Favorite[];
  onContactRowClick: (id: string, e: React.MouseEvent) => void;
  onSelectRaw: (callsign: string) => void;
}) {
  const { group, memberCount, rows } = section;
  const isSelected = selection.kind === 'group' && selection.id === group.id;
  // Up to three avatars in the stack — named members first (callsign-only rows
  // are avatar-less, so their initials would be noise).
  const named = rows.filter(
    (r): r is Extract<OutlineRow, { kind: 'contact' }> => r.kind === 'contact' && hasDisplayName(r.contact),
  );
  // Max TWO avatars + a contained "+N" overflow chip — the open-ended stack
  // clipped at the roster edge (tuxlink-sbf03 survey render).
  const stack = named.slice(0, 2);
  const overflow = memberCount - stack.length;

  return (
    <section className="contacts-group" data-testid={`group-section-${group.id}`}>
      <div className={`contacts-group-head${isSelected ? ' contacts-group-head--selected' : ''}`}>
        <button
          type="button"
          className="contacts-caret"
          data-testid={`group-caret-${group.id}`}
          aria-label={expanded ? 'Collapse group' : 'Expand group'}
          aria-expanded={expanded}
          onClick={onToggleCollapse}
        >
          {expanded ? '▾' : '▸'}
        </button>
        <button
          type="button"
          className="contacts-group-name"
          data-testid={`group-name-${group.id}`}
          onClick={onSelectGroup}
        >
          {group.name}
        </button>
        <span className="contacts-group-count" data-testid={`group-count-${group.id}`}>
          {memberCount}
        </span>
        <span className="contacts-avatar-stack" aria-hidden="true">
          {stack.map((r) => (
            <span key={r.key} className="contacts-avatar contacts-avatar--sm">
              {contactInitials(r.contact.name)}
            </span>
          ))}
          {overflow > 0 && (
            <span className="contacts-avatar contacts-avatar--sm contacts-avatar--more">+{overflow}</span>
          )}
        </span>
      </div>

      {expanded && (
        <ul className="contacts-rows contacts-rows--indented">
          {rows.length === 0 ? (
            <li className="contacts-empty contacts-empty--member" data-testid={`group-empty-${group.id}`}>
              No members
            </li>
          ) : (
            rows.map((row) => (
              <OutlineRowView
                key={row.key}
                row={row}
                selection={selection}
                multiSelected={multiSelected}
                favorites={favorites}
                onContactRowClick={onContactRowClick}
                onSelectRaw={onSelectRaw}
              />
            ))
          )}
        </ul>
      )}
    </section>
  );
}

function OutlineRowView({
  row,
  selection,
  multiSelected,
  favorites,
  onContactRowClick,
  onSelectRaw,
  onSelectSuggestion,
  onSaveSuggestion,
}: {
  row: OutlineRow;
  selection: Selection;
  multiSelected: Set<string>;
  favorites: Favorite[];
  onContactRowClick: (id: string, e: React.MouseEvent) => void;
  onSelectRaw: (callsign: string) => void;
  onSelectSuggestion?: (callsign: string, messageCount: number) => void;
  onSaveSuggestion?: (callsign: string) => void;
}) {
  if (row.kind === 'suggestion') {
    const isSelected = selection.kind === 'suggestion' && selection.callsign === row.callsign;
    return (
      <li className="contacts-row-li">
        <div
          className={`contacts-row contacts-row--suggestion${isSelected ? ' contacts-row--selected' : ''}`}
        >
          <button
            type="button"
            className="contacts-suggestion-main"
            data-testid={`suggestion-${row.callsign}`}
            onClick={() => onSelectSuggestion?.(row.callsign, row.messageCount)}
          >
            <span className="contacts-row-callsign">{row.callsign}</span>
            <span className="contacts-tag contacts-tag--new" data-testid={`suggestion-new-${row.callsign}`}>
              New
            </span>
            <span className="contacts-row-meta">
              {row.messageCount} {row.messageCount === 1 ? 'message' : 'messages'}
            </span>
          </button>
          <button
            type="button"
            className="contacts-row-save"
            data-testid={`suggestion-add-${row.callsign}`}
            onClick={() => onSaveSuggestion?.(row.callsign)}
          >
            Save
          </button>
        </div>
      </li>
    );
  }

  if (row.kind === 'raw') {
    const isSelected = selection.kind === 'raw' && selection.callsign === row.callsign;
    return (
      <li className="contacts-row-li">
        <button
          type="button"
          className={`contacts-row contacts-row--raw${isSelected ? ' contacts-row--selected' : ''}`}
          data-testid={`raw-row-${row.callsign}`}
          onClick={() => onSelectRaw(row.callsign)}
        >
          <RowAvatar contact={{ callsign: row.callsign, name: '' }} heard />
          <span className="contacts-row-callsign">{row.callsign}</span>
          <span className="contacts-row-name contacts-row-name--add">+ add name</span>
        </button>
      </li>
    );
  }

  // Contact row — the uniform anatomy (tuxlink-sbf03): avatar (always) ·
  // callsign+name · reach dot · last-heard · ★. Email pseudo-contacts render
  // an EMAIL chip + name, never the raw SMTP: string.
  const c = row.contact;
  const named = hasDisplayName(c);
  const email = emailAddressOf(c);
  const isSelected = selection.kind === 'contact' && selection.id === c.id;
  const isMulti = multiSelected.has(c.id);
  return (
    <li className="contacts-row-li">
      <button
        type="button"
        className={`contacts-row contacts-row--contact${isSelected ? ' contacts-row--selected' : ''}${
          isMulti ? ' contacts-row--multi' : ''
        }`}
        data-testid={`contact-row-${c.id}`}
        aria-pressed={isMulti}
        onClick={(e) => onContactRowClick(c.id, e)}
      >
        <RowAvatar contact={c} />
        {email ? (
          <>
            <span className="contacts-idkind" data-testid={`contact-email-chip-${c.id}`}>
              EMAIL
            </span>
            <span className="contacts-row-callsign contacts-row-callsign--name">
              {named ? c.name : email}
            </span>
            {named && <span className="contacts-row-name">{email}</span>}
          </>
        ) : (
          <>
            <span className="contacts-row-callsign">{c.callsign}</span>
            {named ? (
              <span className="contacts-row-name">{c.name}</span>
            ) : (
              <span className="contacts-row-name contacts-row-name--add">+ add name</span>
            )}
          </>
        )}
        {c.tactical && <span className="contacts-tag contacts-tag--tactical">{c.tactical}</span>}
        <RowMeta contact={c} favorites={favorites} />
      </button>
    </li>
  );
}

// ===========================================================================
// Recent section (auto-observed, Unconfirmed-tier contacts)
// ===========================================================================

/** Most-recent ACTIVITY instant (ms) across a contact's channels + endpoints —
 *  `last_seen` (ok OR fail), since Recent orders by recency of contact
 *  regardless of outcome. Falls back to `updated_at`. */
function recentRecency(c: Contact): number {
  const times: number[] = [];
  for (const ch of c.channels ?? []) {
    const t = Date.parse(ch.last_seen);
    if (!Number.isNaN(t)) times.push(t);
  }
  for (const ep of c.endpoints ?? []) {
    const t = Date.parse(ep.last_seen);
    if (!Number.isNaN(t)) times.push(t);
  }
  if (times.length > 0) return Math.max(...times);
  const u = Date.parse(c.updated_at);
  return Number.isNaN(u) ? 0 : u;
}

/** Recent-row search: matches callsign or grid (unconfirmed rows are usually
 *  nameless). Empty query matches all. */
function recentMatchesQuery(c: Contact, q: string): boolean {
  if (!q) return true;
  const hay = [c.callsign, c.grid?.value].filter(Boolean).join(' ').toLowerCase();
  return hay.includes(q);
}

/** Provenance labels for endpoint rows. CURATION vocabulary — "operator-added"
 *  vs "observed"; NEVER "verified" (which would imply identity authentication,
 *  spec §AMENDMENT pt. 3). */
const PROVENANCE_LABEL: Record<Provenance, string> = {
  operator: 'operator-added',
  'observed-incoming': 'observed',
  unknown: 'unknown',
};

/** The Heard section (tuxlink-sbf03): ONE not-saved-yet class — unconfirmed
 *  auto-observed stations (RF/telnet observations) and suggested-from-traffic
 *  callsigns — in the uniform row anatomy (dashed avatar), each with a
 *  one-click promote and dismiss. `standalone` renders the rows without the
 *  section head (the Heard SCOPE is the section). Empty ⇒ hidden. */
function HeardSection({
  recentContacts,
  suggestions,
  selection,
  standalone = false,
  onSelectRecent,
  onPromote,
  onDeleteRecent,
  onSelectSuggestion,
  onSaveSuggestion,
}: {
  recentContacts: Contact[];
  suggestions: Suggestion[];
  selection: Selection;
  standalone?: boolean;
  onSelectRecent: (id: string) => void;
  onPromote: (id: string) => void;
  onDeleteRecent: (id: string) => void;
  onSelectSuggestion: (callsign: string, messageCount: number) => void;
  onSaveSuggestion: (callsign: string) => void;
}) {
  const total = recentContacts.length + suggestions.length;
  if (total === 0) {
    return standalone ? (
      <p className="contacts-empty" data-testid="contacts-heard-empty">
        Nothing heard yet — stations you hear (or that message you) appear here to save.
      </p>
    ) : null;
  }
  return (
    <section className="contacts-recent" data-testid="contacts-heard">
      {!standalone && (
        <div className="contacts-ungrouped-head" data-testid="contacts-heard-head">
          <span className="contacts-ungrouped-label">Heard — not saved</span>
          <span className="contacts-ungrouped-count">{total}</span>
        </div>
      )}
      <ul className="contacts-rows">
        {recentContacts.map((c) => (
          <HeardRowView
            key={c.id}
            contact={c}
            selected={selection.kind === 'contact' && selection.id === c.id}
            onSelect={() => onSelectRecent(c.id)}
            onPromote={() => onPromote(c.id)}
            onDelete={() => onDeleteRecent(c.id)}
          />
        ))}
        {suggestions.map((s) => {
          const isSelected = selection.kind === 'suggestion' && selection.callsign === s.callsign;
          return (
            <li className="contacts-row-li" key={`sugg-${s.callsign}`}>
              <div
                className={`contacts-row contacts-row--suggestion${isSelected ? ' contacts-row--selected' : ''}`}
              >
                <button
                  type="button"
                  className="contacts-suggestion-main"
                  data-testid={`suggestion-${s.callsign}`}
                  onClick={() => onSelectSuggestion(s.callsign, s.message_count)}
                >
                  <RowAvatar contact={{ callsign: s.callsign, name: '' }} heard />
                  <span className="contacts-heard-who">
                    <span className="contacts-row-callsign">{s.callsign}</span>
                    <span className="contacts-heard-sub">
                      {s.message_count} {s.message_count === 1 ? 'message' : 'messages'} in traffic
                    </span>
                  </span>
                </button>
                <button
                  type="button"
                  className="contacts-row-save"
                  data-testid={`suggestion-add-${s.callsign}`}
                  onClick={() => onSaveSuggestion(s.callsign)}
                >
                  + Save
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** A Heard (Unconfirmed) row in the uniform anatomy: dashed avatar · callsign ·
 *  honest provenance sub-line + one-click "+ Save" (promote) and dismiss. The
 *  row body selects the contact into the detail pane. */
function HeardRowView({
  contact,
  selected,
  onSelect,
  onPromote,
  onDelete,
}: {
  contact: Contact;
  selected: boolean;
  onSelect: () => void;
  onPromote: () => void;
  onDelete: () => void;
}) {
  const statusLine = recentStatusLine(deriveRecentStatus(contact));
  return (
    <li className="contacts-row-li">
      <div className={`contacts-row contacts-row--recent${selected ? ' contacts-row--selected' : ''}`}>
        <button
          type="button"
          className="contacts-recent-main"
          data-testid={`recent-row-${contact.id}`}
          onClick={onSelect}
        >
          <RowAvatar contact={contact} heard />
          <span className="contacts-heard-who">
            <span className="contacts-row-callsign">{contact.callsign}</span>
            <span className="contacts-heard-sub" data-testid={`recent-status-${contact.id}`}>
              {[statusLine, contact.grid?.value].filter(Boolean).join(' · ') || 'observed'}
            </span>
          </span>
        </button>
        <button
          type="button"
          className="contacts-row-save"
          data-testid={`recent-add-${contact.id}`}
          title="Save to contacts"
          onClick={onPromote}
        >
          + Save
        </button>
        <button
          type="button"
          className="contacts-recent-delete"
          data-testid={`recent-delete-${contact.id}`}
          aria-label="Dismiss"
          title="Dismiss"
          onClick={onDelete}
        >
          ×
        </button>
      </div>
    </li>
  );
}

/** A Favorites-scope row (tuxlink-sbf03 — the retired FavoritesPanel's job in
 *  the uniform anatomy): avatar · callsign+name · the starred dial's summary
 *  sub-line · a permanent Connect (ribbon parity, one click). */
function FavoriteScopeRow({
  favorite,
  contact,
  selected,
  onSelect,
  onConnect,
}: {
  favorite: Favorite;
  contact: Contact | null;
  selected: boolean;
  onSelect: () => void;
  onConnect?: (dial: FavoriteDial) => void;
}) {
  const label = MODE_LABELS[favorite.mode] ?? favorite.mode;
  const status = contact ? recentStatusLine(deriveRecentStatus(contact)) : null;
  const sub = [
    `★ ${label}`,
    favorite.freq || null,
    favorite.band || null,
    favorite.transport || null,
    status,
  ]
    .filter(Boolean)
    .join(' · ');
  const rowContact: Pick<Contact, 'callsign' | 'name'> = contact ?? {
    callsign: favorite.gateway,
    name: '',
  };
  return (
    <li className="contacts-row-li">
      <div className={`contacts-row contacts-row--favorite${selected ? ' contacts-row--selected' : ''}`}>
        <button
          type="button"
          className="contacts-recent-main"
          data-testid={`favorite-row-${favorite.id}`}
          onClick={onSelect}
        >
          <RowAvatar contact={rowContact} />
          <span className="contacts-heard-who">
            <span className="contacts-row-callsign">
              {contact?.callsign ?? favorite.gateway}
              {contact && hasDisplayName(contact) && (
                <span className="contacts-row-name">{contact.name}</span>
              )}
            </span>
            <span className="contacts-heard-sub">{sub}</span>
          </span>
        </button>
        {onConnect && (
          <button
            type="button"
            className="contacts-row-connect"
            data-testid={`favorite-connect-${favorite.id}`}
            title={`Connect to ${favorite.gateway}`}
            onClick={() =>
              onConnect({
                mode: favorite.mode,
                gateway: favorite.gateway,
                freq: favorite.freq,
                transport: favorite.transport,
                band: favorite.band,
                grid: favorite.grid,
                contact_id: favorite.contact_id,
              })
            }
          >
            Connect
          </button>
        )}
      </div>
    </li>
  );
}

/** Display labels for a favorite's RadioMode (FavoritesPanel's MODE_ORDER,
 *  carried over when that panel retired into the Favorites scope). */
const MODE_LABELS: Record<string, string> = {
  'vara-hf': 'VARA HF',
  'vara-fm': 'VARA FM',
  'ardop-hf': 'ARDOP HF',
  packet: 'Packet',
  telnet: 'Telnet',
};

// ===========================================================================
// Reachability block (contact detail, Task T-F Part 2)
// ===========================================================================

/** Kebab-case bandwidth → display text (empty when absent). */
function bandwidthText(bw: ReachChannel['bandwidth']): string {
  if (!bw) return '';
  if (bw.kind === 'hz') return `${bw.hz} Hz`;
  if (bw.kind === 'wide') return 'wide';
  if (bw.kind === 'narrow') return 'narrow';
  return '';
}

/** One RF channel row: transport · freq · target callsign, via/bandwidth/honest
 *  status, and a Connect that dispatches the Task-23a p2p seam (never a
 *  reimplemented dial, never a CMS fallback). */
function ReachChannelRow({
  contact,
  channel,
  index,
  favorites,
  onToggleStar,
}: {
  contact: Contact;
  channel: ReachChannel;
  index: number;
  favorites: Favorite[];
  onToggleStar: (dial: FavoriteDial) => void;
}) {
  const protocol = radioModeForPeerTransport(channel.transport);
  // The channel as a Favorite dial (tuxlink-sbf03): ★ = this dial appears in
  // the Favorites scope + the ribbon Connect targets. Starless when the
  // transport has no tuxlink modem (nothing could ever dial it).
  const dial: FavoriteDial | null = protocol
    ? {
        mode: protocol,
        gateway: channel.target_callsign,
        freq: channel.freq_hz != null ? (channel.freq_hz / 1000).toFixed(1) : undefined,
        contact_id: contact.id,
      }
    : null;
  const starred =
    dial !== null && favorites.some((f) => f.starred && favoriteKey(f) === favoriteKey(dial));
  const sub = [
    channel.via.length > 0 ? `via ${channel.via.join(', ')}` : '',
    bandwidthText(channel.bandwidth),
    channelStatusLine(channel),
  ]
    .filter(Boolean)
    .join(' · ');
  return (
    <div className="contact-reach-row" data-testid={`reach-channel-${contact.id}-${index}`}>
      <div className="contact-reach-info">
        <div className="contact-reach-primary">
          {channelSummary(channel)} · {channel.target_callsign}
        </div>
        <div className="contact-reach-sub" data-testid={`reach-channel-status-${contact.id}-${index}`}>
          {sub}
        </div>
      </div>
      {dial !== null && (
        <button
          type="button"
          className={`contact-dial-star${starred ? ' contact-dial-star--on' : ''}`}
          data-testid={`reach-channel-star-${contact.id}-${index}`}
          aria-pressed={starred}
          title={starred ? 'Unstar — remove from Favorites' : 'Star — add to Favorites + ribbon Connect'}
          onClick={() => onToggleStar(dial)}
        >
          {starred ? '★' : '☆'}
        </button>
      )}
      <button
        type="button"
        className="contact-detail-btn"
        data-testid={`reach-channel-connect-${contact.id}-${index}`}
        disabled={!protocol}
        title={
          !protocol ? 'No tuxlink modem for this transport' : `Connect to ${channel.target_callsign}`
        }
        onClick={() => protocol && connectPeerChannel(channel)}
      >
        Connect →
      </button>
    </div>
  );
}

/** One telnet endpoint row. host:port IS shown — this is the OPERATOR's UI
 *  (only the AGENT surface must never see the address). Connect dispatches the
 *  operator telnet p2p dial (the click is RADIO-1 consent). */
function ReachEndpointRow({
  contact,
  endpoint,
  operatorGrid,
}: {
  contact: Contact;
  endpoint: ReachEndpoint;
  operatorGrid: string;
}) {
  const sub = [PROVENANCE_LABEL[endpoint.provenance], endpointStatusLine(endpoint)]
    .filter(Boolean)
    .join(' · ');
  return (
    <div className="contact-reach-row" data-testid={`reach-endpoint-${endpoint.id}`}>
      <div className="contact-reach-info">
        <div className="contact-reach-primary">
          telnet · {endpoint.host}:{endpoint.port}
        </div>
        <div className="contact-reach-sub" data-testid={`reach-endpoint-status-${endpoint.id}`}>
          {sub}
        </div>
      </div>
      <button
        type="button"
        className="contact-detail-btn"
        data-testid={`reach-endpoint-connect-${endpoint.id}`}
        title={`Connect to ${contact.callsign} over telnet`}
        onClick={() => connectPeerEndpoint(contact.callsign, endpoint, operatorGrid, contact.id)}
      >
        Connect →
      </button>
    </div>
  );
}

/** The reachability block: RF channel rows + telnet endpoint rows. Hidden when
 *  the contact carries neither (spec: heard stations appear when they happen). */
function ReachabilityBlock({
  contact,
  operatorGrid,
  favorites,
  onToggleStar,
}: {
  contact: Contact;
  operatorGrid: string;
  favorites: Favorite[];
  onToggleStar: (dial: FavoriteDial) => void;
}) {
  const channels = contact.channels ?? [];
  const endpoints = contact.endpoints ?? [];
  if (channels.length === 0 && endpoints.length === 0) return null;
  return (
    <section className="contact-reach-card" data-testid="contact-reachability">
      <h3 className="contact-card-label">Reachability &amp; connect</h3>
      {channels.map((ch, i) => (
        <ReachChannelRow
          key={`ch-${i}`}
          contact={contact}
          channel={ch}
          index={i}
          favorites={favorites}
          onToggleStar={onToggleStar}
        />
      ))}
      {endpoints.map((ep) => (
        <ReachEndpointRow key={ep.id} contact={contact} endpoint={ep} operatorGrid={operatorGrid} />
      ))}
    </section>
  );
}

function BulkBar({
  count,
  groups,
  onAddToGroup,
  onAddToNewGroup,
  onRemove,
  onClear,
}: {
  count: number;
  groups: Group[];
  onAddToGroup: (group: Group) => void;
  onAddToNewGroup: (name: string) => void;
  onRemove: () => void;
  onClear: () => void;
}) {
  const [picking, setPicking] = useState(false);
  const [newName, setNewName] = useState('');
  const [confirmingRemove, setConfirmingRemove] = useState(false);

  return (
    <div className="contacts-bulk-bar" data-testid="contacts-bulk-bar">
      <span className="contacts-bulk-count" data-testid="contacts-bulk-count">
        {count} selected
      </span>
      <div className="contacts-bulk-acts">
        <button
          type="button"
          className="contacts-bulk-btn"
          data-testid="contacts-bulk-add-to-group"
          onClick={() => {
            setPicking((v) => !v);
            setConfirmingRemove(false);
          }}
        >
          Add to group
        </button>
        {!confirmingRemove ? (
          <button
            type="button"
            className="contacts-bulk-btn contacts-bulk-btn--danger"
            data-testid="contacts-bulk-remove"
            onClick={() => {
              setConfirmingRemove(true);
              setPicking(false);
            }}
          >
            Remove
          </button>
        ) : (
          <button
            type="button"
            className="contacts-bulk-btn contacts-bulk-btn--danger"
            data-testid="contacts-bulk-remove-confirm"
            onClick={onRemove}
          >
            Remove {count}? — confirm
          </button>
        )}
        <button
          type="button"
          className="contacts-bulk-btn contacts-bulk-btn--ghost"
          data-testid="contacts-bulk-clear"
          onClick={onClear}
        >
          Clear
        </button>
      </div>

      {picking && (
        <div className="contacts-bulk-picker" data-testid="contacts-bulk-picker">
          <ul className="contacts-bulk-group-list">
            {groups.map((g) => (
              <li key={g.id}>
                <button
                  type="button"
                  className="contacts-bulk-group"
                  data-testid={`contacts-bulk-group-${g.id}`}
                  onClick={() => {
                    onAddToGroup(g);
                    setPicking(false);
                  }}
                >
                  {g.name}
                </button>
              </li>
            ))}
          </ul>
          <form
            className="contacts-bulk-newgroup"
            onSubmit={(e) => {
              e.preventDefault();
              const name = newName.trim();
              if (name.length === 0) return;
              onAddToNewGroup(name);
              setNewName('');
              setPicking(false);
            }}
          >
            <input
              className="contacts-bulk-newgroup-input"
              data-testid="contacts-bulk-newgroup-input"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="New group name"
              autoComplete="off"
              spellCheck={false}
            />
            <button
              type="submit"
              className="contacts-bulk-btn"
              data-testid="contacts-bulk-newgroup-add"
              disabled={newName.trim().length === 0}
            >
              + Create
            </button>
          </form>
        </div>
      )}
    </div>
  );
}

// ===========================================================================
// Detail sub-components
// ===========================================================================

function ContactDetail({
  contact,
  groups,
  operatorGrid,
  favorites,
  onToggleStar,
  onNewMessage,
  onEdit,
  onPromote,
}: {
  contact: Contact;
  groups: Group[];
  operatorGrid: string;
  favorites: Favorite[];
  onToggleStar: (dial: FavoriteDial) => void;
  onNewMessage: () => void;
  onEdit: () => void;
  /// Present ONLY for an Unconfirmed contact — the one-click promote ("+ Add
  /// to contacts"). Absent ⇒ the contact is already curated.
  onPromote?: () => void;
}) {
  const named = hasDisplayName(contact);
  const { attempts, hint } = useContactConnectionRecord(contact.callsign);
  // The groups this contact belongs to (by contact_id membership).
  const memberOf = groups.filter((g) =>
    g.members.some((m) => m.type === 'contact' && m.contact_id === contact.id),
  );

  return (
    <div className="contact-detail" data-testid="contact-detail">
      <div className="contact-detail-header">
        {named && (
          <span className="contacts-avatar contacts-avatar--lg" aria-hidden="true">
            {contactInitials(contact.name)}
          </span>
        )}
        <div className="contact-detail-id">
          <h2 className="contact-detail-callsign" data-testid="contact-detail-callsign">
            {emailAddressOf(contact) ? (
              <>
                <span className="contacts-idkind contacts-idkind--lg">EMAIL</span>
                {hasDisplayName(contact) ? contact.name : emailAddressOf(contact)}
              </>
            ) : (
              contact.callsign
            )}
            {starredFavoritesOf(contact, favorites).length > 0 && (
              <span className="contact-detail-star" title="Has starred Favorite dials">
                ★
              </span>
            )}
          </h2>
          {named ? (
            <span className="contact-detail-name">{contact.name}</span>
          ) : (
            <span className="contact-detail-name contact-detail-name--add">+ add name</span>
          )}
        </div>
      </div>

      {/* Connection record card — rendered regardless; the empty state is the
          honest "no connection attempts yet" surface. */}
      <section className="contact-record-card" data-testid="contact-record-card">
        <h3 className="contact-card-label">Connection record</h3>
        <ConnectionRecord attempts={attempts} hint={hint} />
      </section>

      {/* Reachability (Task T-F Part 2): live RF/telnet rows with Connect —
          hidden when the contact has neither. */}
      <ReachabilityBlock
        contact={contact}
        operatorGrid={operatorGrid}
        favorites={favorites}
        onToggleStar={onToggleStar}
      />

      <dl className="contact-detail-fields">
        {contact.tactical && (
          <>
            <dt>Tactical</dt>
            <dd data-testid="contact-detail-tactical">{contact.tactical}</dd>
          </>
        )}
        {contact.email && (
          <>
            <dt>Email</dt>
            <dd data-testid="contact-detail-email">{contact.email}</dd>
          </>
        )}
        {memberOf.length > 0 && (
          <>
            <dt>Groups</dt>
            <dd data-testid="contact-detail-groups">{memberOf.map((g) => g.name).join(', ')}</dd>
          </>
        )}
        {contact.notes && (
          <>
            <dt>Notes</dt>
            <dd data-testid="contact-detail-notes">{contact.notes}</dd>
          </>
        )}
      </dl>

      <div className="contact-detail-actions">
        {onPromote && (
          <button
            type="button"
            className="contact-detail-btn contact-detail-btn-primary"
            data-testid="contact-promote"
            title="Add this recent station to your contacts"
            onClick={onPromote}
          >
            + Add to contacts
          </button>
        )}
        {/* One primary per detail: promote (when present) IS the tier action,
            so New message demotes to a plain button on unconfirmed contacts. */}
        <button
          type="button"
          className={`contact-detail-btn${onPromote ? '' : ' contact-detail-btn-primary'}`}
          data-testid="contact-new-message"
          onClick={onNewMessage}
        >
          New message
        </button>
        <button
          type="button"
          className="contact-detail-btn"
          data-testid="contact-edit"
          onClick={onEdit}
        >
          Edit
        </button>
      </div>
    </div>
  );
}

/// A raw (callsign-only, group-member) detail — no Contact backing. Offers the
/// same connection record (keyed by callsign) plus a Save-as-contact and a New
/// message. Mirrors a suggestion row's affordances at detail scale.
function RawDetail({
  callsign,
  messageCount,
  onNewMessage,
  onSave,
}: {
  callsign: string;
  /** When set (a suggestion), show "N messages" instead of the bare "not
   *  saved" label — the correspondent has mailbox history worth surfacing. */
  messageCount?: number;
  onNewMessage: () => void;
  onSave: () => void;
}) {
  const { attempts, hint } = useContactConnectionRecord(callsign);
  const subtitle =
    messageCount != null
      ? `not saved · ${messageCount} ${messageCount === 1 ? 'message' : 'messages'}`
      : 'not saved';
  return (
    <div className="contact-detail" data-testid="raw-detail">
      <div className="contact-detail-header">
        <div className="contact-detail-id">
          <h2 className="contact-detail-callsign">{callsign}</h2>
          <span className="contact-detail-name contact-detail-name--add">{subtitle}</span>
        </div>
      </div>

      <section className="contact-record-card">
        <h3 className="contact-card-label">Connection record</h3>
        <ConnectionRecord attempts={attempts} hint={hint} />
      </section>

      <div className="contact-detail-actions">
        <button
          type="button"
          className="contact-detail-btn contact-detail-btn-primary"
          data-testid="raw-new-message"
          onClick={onNewMessage}
        >
          New message
        </button>
        <button
          type="button"
          className="contact-detail-btn"
          data-testid="raw-save-contact"
          onClick={onSave}
        >
          Save as contact
        </button>
      </div>
    </div>
  );
}

/// GroupManagement — the inline group-management detail (no popup). Editable
/// name, per-member remove, add-by-callsign/name, and delete. All mutations
/// route through `onSave` (group_upsert) / `onDelete` (group_delete).
export function GroupManagement({
  group,
  contacts,
  onSave,
  onDelete,
}: {
  group: Group;
  contacts: Contact[];
  onSave: (group: Group) => Promise<void> | void;
  onDelete: (id: string) => Promise<void> | void;
}) {
  const [name, setName] = useState(group.name);
  const [search, setSearch] = useState('');
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  // Reseed the name field when the selected group changes (id is the identity).
  const [seenId, setSeenId] = useState(group.id);
  if (seenId !== group.id) {
    setSeenId(group.id);
    setName(group.name);
    setSearch('');
    setConfirmingDelete(false);
  }

  const byId = new Map(contacts.map((c) => [c.id, c]));
  const nameTrimmed = name.trim();
  const nameValid = nameTrimmed.length > 0;
  const nameDirty = nameTrimmed !== group.name.trim();

  const memberKeyOf = (m: GroupMember): string =>
    m.type === 'contact' ? `contact:${m.contact_id}` : `raw:${m.callsign}`;

  const hasMember = (candidate: GroupMember): boolean =>
    group.members.some((m) => memberKeyOf(m) === memberKeyOf(candidate));

  const saveName = () => {
    if (!nameValid || !nameDirty) return;
    void onSave({ ...group, name: nameTrimmed });
  };

  const removeMember = (key: string) => {
    void onSave({ ...group, members: group.members.filter((m) => memberKeyOf(m) !== key) });
  };

  // Contact-search options for the add input, excluding already-added members.
  const q = search.trim().toLowerCase();
  const options =
    q.length === 0
      ? []
      : contacts.filter((c) => {
          if (hasMember({ type: 'contact', contact_id: c.id })) return false;
          const hay = [c.name, c.callsign, c.email, c.tactical].filter(Boolean).join(' ').toLowerCase();
          return hay.includes(q);
        });

  const addContactMember = (contact: Contact) => {
    const m: GroupMember = { type: 'contact', contact_id: contact.id };
    if (hasMember(m)) {
      setSearch('');
      return;
    }
    void onSave({ ...group, members: [...group.members, m] });
    setSearch('');
  };

  const addRawMember = () => {
    const callsign = search.trim();
    if (callsign.length === 0) return;
    const m: GroupMember = { type: 'raw', callsign };
    if (hasMember(m)) {
      setSearch('');
      return;
    }
    void onSave({ ...group, members: [...group.members, m] });
    setSearch('');
  };

  return (
    <div className="group-management" data-testid="group-management">
      <div className="group-management-head">
        <input
          className="group-management-name"
          data-testid="group-management-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Group name"
          autoComplete="off"
          spellCheck={false}
        />
        <button
          type="button"
          className="group-management-rename"
          data-testid="group-management-rename"
          disabled={!nameValid || !nameDirty}
          onClick={saveName}
        >
          Rename
        </button>
      </div>

      <h3 className="contact-card-label">Members</h3>
      <ul className="group-management-members" data-testid="group-management-members">
        {group.members.length === 0 && (
          <li className="contacts-empty" data-testid="group-management-empty">
            No members — add one below.
          </li>
        )}
        {group.members.map((m) => {
          const key = memberKeyOf(m);
          let primary: string;
          let secondary: string | null;
          if (m.type === 'raw') {
            primary = m.callsign;
            secondary = 'raw callsign';
          } else {
            const c = byId.get(m.contact_id);
            if (c) {
              primary = c.callsign;
              secondary = hasDisplayName(c) ? c.name : null;
            } else {
              primary = '(removed contact)';
              secondary = null;
            }
          }
          return (
            <li className="group-management-member" data-testid={`gm-member-${key}`} key={key}>
              <span className="group-management-member-primary">{primary}</span>
              {secondary && <span className="group-management-member-secondary">{secondary}</span>}
              <button
                type="button"
                className="group-management-member-remove"
                data-testid={`gm-remove-${key}`}
                aria-label="Remove member"
                title="Remove member"
                onClick={() => removeMember(key)}
              >
                ×
              </button>
            </li>
          );
        })}
      </ul>

      <div className="group-management-add">
        <input
          className="group-management-add-input"
          data-testid="group-management-add-input"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              if (options.length === 1) addContactMember(options[0]);
              else addRawMember();
            }
          }}
          placeholder="Add by callsign or name"
          autoComplete="off"
          spellCheck={false}
        />
        <button
          type="button"
          className="group-management-add-btn"
          data-testid="group-management-add-raw"
          disabled={search.trim().length === 0}
          onClick={addRawMember}
        >
          + Add
        </button>
        {options.length > 0 && (
          <ul className="group-management-options" data-testid="group-management-options">
            {options.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  className="group-management-option"
                  data-testid={`gm-option-${c.id}`}
                  onClick={() => addContactMember(c)}
                >
                  <span className="group-management-option-callsign">{c.callsign}</span>
                  {hasDisplayName(c) && (
                    <span className="group-management-option-name">{c.name}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="group-management-actions">
        {!confirmingDelete ? (
          <button
            type="button"
            className="group-management-delete"
            data-testid="group-management-delete"
            onClick={() => setConfirmingDelete(true)}
          >
            Delete group
          </button>
        ) : (
          <button
            type="button"
            className="group-management-delete"
            data-testid="group-management-delete-confirm"
            onClick={() => void onDelete(group.id)}
          >
            Delete {group.name}? — confirm
          </button>
        )}
      </div>
    </div>
  );
}
