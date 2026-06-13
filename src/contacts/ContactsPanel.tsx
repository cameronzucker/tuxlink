// ContactsPanel — the unified Contacts outline (tuxlink-je5d).
//
// Selected from the sidebar's Address → Contacts pseudo-folder; AppShell swaps
// it in for BOTH the MessageList and the reading pane (it spans grid lines 2→4
// per AppShell.css). It replaces the prior nested 286px master-detail with ONE
// outline (the roster) feeding a polymorphic reading-pane detail:
//
//   ROSTER (~380px message-list footprint):
//     · a global search that scopes the WHOLE tree (groups containing a match
//       auto-expand)
//     · collapsible GROUP sections — header = caret · name · member count ·
//       avatar stack; members render indented when expanded. The caret toggles
//       expand; the group NAME selects the group (→ group management).
//     · an "Ungrouped" section — contacts referenced by no group, plus the
//       suggested-from-traffic callsigns (each a "New"-tagged row with an inline
//       "Save" that creates a contact).
//   DETAIL (reading pane, polymorphic):
//     · member selected → ContactDetail (callsign headline · name · connection
//       record card · details · New message / Edit).
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
  | { kind: 'group'; id: string };

/// The inline contact editor target (takes over the detail pane when open).
type EditorState = { kind: 'closed' } | { kind: 'new'; seed: Contact } | { kind: 'edit'; contact: Contact };

export function ContactsPanel() {
  const qc = useQueryClient();
  const { contacts, groups, upsertContact, deleteContact, upsertGroup, deleteGroup } = useContacts();
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>('last-heard');
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

  const q = query.trim().toLowerCase();

  const tree = useMemo(
    () => buildContactTree({ contacts, groups, suggestions, query: q, sort }),
    [contacts, groups, suggestions, q, sort],
  );

  // Under a query, groups containing a match auto-expand (overrides manual
  // collapse). With no query, manual collapse state governs.
  const autoExpand = useMemo(
    () => groupsMatchingQuery({ contacts, groups, suggestions, query: q, sort }),
    [contacts, groups, suggestions, q, sort],
  );

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

        <div className="contacts-sort" data-testid="contacts-sort">
          <SortButton current={sort} value="last-heard" label="Last heard" onChange={setSort} />
          <SortButton current={sort} value="name" label="Name" onChange={setSort} />
          <SortButton current={sort} value="callsign" label="Callsign" onChange={setSort} />
        </div>

        <div className="contacts-tree" data-testid="contacts-tree">
          {tree.groups.map((section) => (
            <GroupSectionView
              key={section.group.id}
              section={section}
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
              <span className="contacts-ungrouped-label">Ungrouped</span>
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
                    onContactRowClick={onContactRowClick}
                    onSelectRaw={(callsign) => {
                      setSelected(new Set());
                      setSelection({ kind: 'raw', callsign });
                      setEditor({ kind: 'closed' });
                    }}
                    onSaveSuggestion={addSuggestion}
                  />
                ))}
              </ul>
            )}
          </section>
        </div>

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
            onNewMessage={() => void openComposeTo(selectedContact.callsign)}
            onEdit={() => setEditor({ kind: 'edit', contact: selectedContact })}
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

function SortButton({
  current,
  value,
  label,
  onChange,
}: {
  current: SortKey;
  value: SortKey;
  label: string;
  onChange: (v: SortKey) => void;
}) {
  return (
    <button
      type="button"
      className={`contacts-sort-btn${current === value ? ' contacts-sort-btn--active' : ''}`}
      data-testid={`contacts-sort-${value}`}
      aria-pressed={current === value}
      onClick={() => onChange(value)}
    >
      {label}
    </button>
  );
}

function GroupSectionView({
  section,
  expanded,
  onToggleCollapse,
  onSelectGroup,
  selection,
  multiSelected,
  onContactRowClick,
  onSelectRaw,
}: {
  section: GroupSection;
  expanded: boolean;
  onToggleCollapse: () => void;
  onSelectGroup: () => void;
  selection: Selection;
  multiSelected: Set<string>;
  onContactRowClick: (id: string, e: React.MouseEvent) => void;
  onSelectRaw: (callsign: string) => void;
}) {
  const { group, memberCount, rows } = section;
  const isSelected = selection.kind === 'group' && selection.id === group.id;
  // Up to three avatars in the stack — named members first (callsign-only rows
  // are avatar-less, so their initials would be noise).
  const stack = rows
    .filter((r): r is Extract<OutlineRow, { kind: 'contact' }> => r.kind === 'contact' && hasDisplayName(r.contact))
    .slice(0, 3);

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
  onContactRowClick,
  onSelectRaw,
  onSaveSuggestion,
}: {
  row: OutlineRow;
  selection: Selection;
  multiSelected: Set<string>;
  onContactRowClick: (id: string, e: React.MouseEvent) => void;
  onSelectRaw: (callsign: string) => void;
  onSaveSuggestion?: (callsign: string) => void;
}) {
  if (row.kind === 'suggestion') {
    return (
      <li className="contacts-row contacts-row--suggestion" data-testid={`suggestion-${row.callsign}`}>
        <span className="contacts-row-callsign">{row.callsign}</span>
        <span className="contacts-tag contacts-tag--new" data-testid={`suggestion-new-${row.callsign}`}>
          New
        </span>
        <span className="contacts-row-meta">
          {row.messageCount} {row.messageCount === 1 ? 'message' : 'messages'}
        </span>
        <button
          type="button"
          className="contacts-row-save"
          data-testid={`suggestion-add-${row.callsign}`}
          onClick={() => onSaveSuggestion?.(row.callsign)}
        >
          Save
        </button>
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
          <span className="contacts-row-callsign">{row.callsign}</span>
          <span className="contacts-row-name contacts-row-name--add">+ add name</span>
        </button>
      </li>
    );
  }

  // Contact row — callsign-first; avatar only for named contacts.
  const c = row.contact;
  const named = hasDisplayName(c);
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
        {named ? (
          <span className="contacts-avatar contacts-avatar--sm" aria-hidden="true">
            {contactInitials(c.name)}
          </span>
        ) : (
          <span className="contacts-avatar-placeholder" aria-hidden="true" />
        )}
        <span className="contacts-row-callsign">{c.callsign}</span>
        {named ? (
          <span className="contacts-row-name">{c.name}</span>
        ) : (
          <span className="contacts-row-name contacts-row-name--add">+ add name</span>
        )}
        {c.tactical && <span className="contacts-tag contacts-tag--tactical">{c.tactical}</span>}
      </button>
    </li>
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
  onNewMessage,
  onEdit,
}: {
  contact: Contact;
  groups: Group[];
  onNewMessage: () => void;
  onEdit: () => void;
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
            {contact.callsign}
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

/// A raw (callsign-only, group-member) detail — no Contact backing. Offers the
/// same connection record (keyed by callsign) plus a Save-as-contact and a New
/// message. Mirrors a suggestion row's affordances at detail scale.
function RawDetail({
  callsign,
  onNewMessage,
  onSave,
}: {
  callsign: string;
  onNewMessage: () => void;
  onSave: () => void;
}) {
  const { attempts, hint } = useContactConnectionRecord(callsign);
  return (
    <div className="contact-detail" data-testid="raw-detail">
      <div className="contact-detail-header">
        <div className="contact-detail-id">
          <h2 className="contact-detail-callsign">{callsign}</h2>
          <span className="contact-detail-name contact-detail-name--add">not saved</span>
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
