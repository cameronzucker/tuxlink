// ContactsPanel — the inline Contacts management surface (Task A8).
//
// Selected from the sidebar's Address → Contacts pseudo-folder; AppShell swaps
// it in for BOTH the MessageList and the reading pane (M8). It is a list+detail
// surface (NO popup window — design A.4):
//
//   LIST column (~286px):
//     · search (filters groups + people by name/callsign/email/tactical)
//     · "+ New" button (opens the ContactEditor)
//     · a "Suggested" affordance — one-click "+ Add" cards derived from
//       `contacts_suggestions`, each annotated with its message count
//     · a GROUPS section (blue avatars) ON TOP, then a PEOPLE section. People
//       use react-virtuoso (mirroring MessageList) for long lists; Groups are a
//       plain list.
//   DETAIL pane:
//     · avatar, name, primary callsign, the multi-address fields (email,
//       tactical, notes), and actions: "New message" (drops the contact's
//       primary callsign into Compose To) and "Edit".
//
// Suggestions are SUGGEST-ONLY: "+ Add" is one explicit click that calls
// `contact_upsert` with the callsign prefilled — contacts are NEVER auto-created.

import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { Virtuoso } from 'react-virtuoso';
import './ContactsPanel.css';
import type { Contact, Group, Suggestion } from './types';
import { useContacts } from './useContacts';
import { resolveGroupMemberCount } from './recipients';
import { ContactEditor, emptyContact } from './ContactEditor';
import { openComposeTo } from './composeTo';

/// Up-to-two-character initials for the round avatar. Prefers the name; falls
/// back to the callsign so a name-less contact still shows something.
function initials(c: { name?: string; callsign: string }): string {
  const src = (c.name ?? '').trim() || c.callsign;
  const parts = src.split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

const matchesQuery = (c: Contact, q: string): boolean => {
  if (!q) return true;
  const hay = [c.name, c.callsign, c.email, c.tactical]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
  return hay.includes(q);
};

const groupMatchesQuery = (g: Group, q: string): boolean =>
  !q || g.name.toLowerCase().includes(q);

/// Editor-target discriminator: closed, a brand-new contact, or an existing one.
type EditorState =
  | { kind: 'closed' }
  | { kind: 'new'; seed: Contact }
  | { kind: 'edit'; contact: Contact };

/// Query key for the suggest-from-history cards. Distinct from the contacts
/// file key (`['contacts']`) so `addSuggestion` can re-derive suggestions
/// without forcing a full contacts refetch — though `upsertContact` invalidates
/// `['contacts']` independently.
const SUGGESTIONS_QUERY_KEY = ['contacts', 'suggestions'] as const;

export function ContactsPanel() {
  const qc = useQueryClient();
  const { contacts, groups, upsertContact } = useContacts();
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState>({ kind: 'closed' });

  // Suggest-from-history cards. Suggestions exclude already-saved + own callsign
  // server-side (A3); fetched HERE so the panel owns the query (and re-runs as
  // the mailbox / contacts change via the default cache invalidation).
  const suggestionsQuery = useQuery({
    queryKey: SUGGESTIONS_QUERY_KEY,
    queryFn: () => invoke<Suggestion[]>('contacts_suggestions'),
  });
  const suggestions = suggestionsQuery.data ?? [];

  const q = query.trim().toLowerCase();
  const visibleGroups = useMemo(
    () => groups.filter((g) => groupMatchesQuery(g, q)),
    [groups, q],
  );
  const visiblePeople = useMemo(
    () => contacts.filter((c) => matchesQuery(c, q)),
    [contacts, q],
  );

  const selected = useMemo(
    () => contacts.find((c) => c.id === selectedId) ?? null,
    [contacts, selectedId],
  );

  const saveContact = async (c: Contact) => {
    await upsertContact(c);
    setEditor({ kind: 'closed' });
  };

  const addSuggestion = async (s: Suggestion) => {
    // One explicit click → create a contact with the callsign prefilled. Never
    // auto-created. The name defaults to the callsign so the row isn't blank.
    await upsertContact({ ...emptyContact(s.callsign), name: s.callsign });
    // Re-derive suggestions: the backend `derive_suggestions` excludes existing
    // contacts, so the just-added card disappears. Without this, the stale card
    // stays visible and a second click creates a DUPLICATE contact (each empty
    // id gets a fresh uuid → callsign-based dedup never fires at this layer).
    await qc.invalidateQueries({ queryKey: SUGGESTIONS_QUERY_KEY });
  };

  // Editor takes over the whole panel body when open (inline; no popup).
  if (editor.kind !== 'closed') {
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
      {/* LIST column */}
      <div className="contacts-list" data-testid="contacts-list">
        <div className="contacts-list-toolbar">
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

        {/* Suggested affordance — one-click "+ Add" cards (suggest-only). */}
        {suggestions.length > 0 && (
          <section className="contacts-section contacts-suggested" data-testid="contacts-suggested">
            <h4 className="contacts-section-label" data-testid="contacts-suggested-heading">
              Suggested
            </h4>
            <ul className="contacts-suggestion-list">
              {suggestions.map((s) => (
                <li
                  key={s.callsign}
                  className="contacts-suggestion"
                  data-testid={`suggestion-${s.callsign}`}
                >
                  <div className="contacts-suggestion-text">
                    <span className="contacts-suggestion-callsign">{s.callsign}</span>
                    <span className="contacts-suggestion-count">
                      exchanged {s.message_count} {s.message_count === 1 ? 'message' : 'messages'} with{' '}
                      {s.callsign}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="contacts-suggestion-add"
                    data-testid={`suggestion-add-${s.callsign}`}
                    onClick={() => void addSuggestion(s)}
                  >
                    + Add
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        {/* GROUPS — on top (A.4). Plain list, blue avatars. */}
        <section className="contacts-section contacts-groups">
          <h4 className="contacts-section-label" data-testid="contacts-groups-heading">
            Groups
          </h4>
          {visibleGroups.length === 0 ? (
            <p className="contacts-empty" data-testid="contacts-groups-empty">
              No groups
            </p>
          ) : (
            <ul className="contacts-group-list">
              {visibleGroups.map((g) => (
                <li key={g.id} className="contacts-group-row" data-testid={`group-row-${g.id}`}>
                  <span className="contacts-avatar contacts-avatar--group" aria-hidden="true">
                    {initials({ name: g.name, callsign: g.name })}
                  </span>
                  <span className="contacts-row-name">{g.name}</span>
                  <span className="contacts-row-sub">
                    {resolveGroupMemberCount(g, contacts)} members
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* PEOPLE — below Groups. Virtualized for long lists. */}
        <section className="contacts-section contacts-people">
          <h4 className="contacts-section-label" data-testid="contacts-people-heading">
            People
          </h4>
          {visiblePeople.length === 0 ? (
            <p className="contacts-empty" data-testid="contacts-people-empty">
              No contacts
            </p>
          ) : (
            <div className="contacts-people-list" data-testid="contacts-people-list">
              <Virtuoso
                data={visiblePeople}
                computeItemKey={(_i, c) => c.id}
                itemContent={(_i, c) => (
                  <button
                    type="button"
                    className={`contacts-person-row${
                      c.id === selectedId ? ' contacts-person-row--selected' : ''
                    }`}
                    data-testid={`person-row-${c.id}`}
                    onClick={() => setSelectedId(c.id)}
                  >
                    <span className="contacts-avatar" aria-hidden="true">
                      {initials(c)}
                    </span>
                    <span className="contacts-row-name">{c.name || c.callsign}</span>
                    <span className="contacts-row-sub">{c.callsign}</span>
                  </button>
                )}
              />
            </div>
          )}
        </section>
      </div>

      {/* DETAIL pane */}
      <div className="contacts-detail" data-testid="contacts-detail-pane">
        {selected ? (
          <ContactDetail
            contact={selected}
            onNewMessage={() => void openComposeTo(selected.callsign)}
            onEdit={() => setEditor({ kind: 'edit', contact: selected })}
          />
        ) : (
          <div className="contacts-detail-empty" data-testid="contacts-detail-empty">
            Select a contact to view details.
          </div>
        )}
      </div>
    </div>
  );
}

function ContactDetail({
  contact,
  onNewMessage,
  onEdit,
}: {
  contact: Contact;
  onNewMessage: () => void;
  onEdit: () => void;
}) {
  return (
    <div className="contact-detail" data-testid="contact-detail">
      <div className="contact-detail-header">
        <span className="contacts-avatar contacts-avatar--lg" aria-hidden="true">
          {initials(contact)}
        </span>
        <div className="contact-detail-id">
          <h2 className="contact-detail-name">{contact.name || contact.callsign}</h2>
          <span className="contact-detail-callsign">{contact.callsign}</span>
        </div>
      </div>

      <dl className="contact-detail-fields">
        {contact.email && (
          <>
            <dt>Email</dt>
            <dd data-testid="contact-detail-email">{contact.email}</dd>
          </>
        )}
        {contact.tactical && (
          <>
            <dt>Tactical</dt>
            <dd data-testid="contact-detail-tactical">{contact.tactical}</dd>
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
        <button
          type="button"
          className="contact-detail-btn contact-detail-btn-primary"
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
