// GroupEditor — the New / Edit distribution-group form (Task A8b).
//
// One controlled form reused for BOTH entry points:
//   1. "+ New group" in ContactsPanel  → blank name, empty member list.
//   2. "Edit" on a selected group       → name + members prefilled.
//
// Name is REQUIRED (Save is disabled until it is non-empty). Members are managed
// inline (no popup, no native <select> — WebKitGTK + project design constraint):
//
//   - Add a member by searching contacts and PICKING one → stores a
//     GroupMember{ type:'contact', contact_id } (the Locked rule: a contact_id
//     when added from a contact, so a later contact edit propagates to the group).
//   - Add a member by TYPING a callsign and committing (Enter / + Add) with no
//     contact picked → stores a GroupMember{ type:'raw', callsign } (a frozen
//     literal — the Locked rule's raw branch).
//   - Each member row resolves its display form: a contact member shows the
//     contact's name + callsign; a DELETED-contact member (contact_id no longer
//     resolves) renders DISTINCTLY ("unknown / removed contact") and is still
//     removable — it is NEVER silently dropped and NEVER crashes (M6 / the
//     M6/H5 "don't silently drop" posture).
//   - Remove a member via the row's X.
//
// Save routes through `onSave(group)` (the ContactsPanel wires this to
// `upsertGroup` → `group_upsert`; an empty id means the backend stamps it).
// Delete (edit mode only) routes through `onDelete(id)` → `group_delete`.

import { useMemo, useState } from 'react';
import './GroupEditor.css';
import type { Contact, Group, GroupMember } from './types';

export interface GroupEditorProps {
  /// The group being edited. For a NEW group pass a seed with an empty `id` and
  /// an empty `members` list (see `emptyGroup`).
  group: Group;
  /// The current address book — used to resolve contact members to their display
  /// form and to drive the member-add contact search.
  contacts: Contact[];
  /// Persist the assembled group (name required, id empty for a new group).
  onSave: (group: Group) => Promise<void> | void;
  /// Delete this group (edit mode only). Not rendered for a new group.
  onDelete: (id: string) => Promise<void> | void;
  /// Discard without saving.
  onCancel: () => void;
}

/// An empty Group seed — id/timestamps left blank for the backend to stamp.
export function emptyGroup(): Group {
  return { id: '', name: '', members: [], created_at: '', updated_at: '' };
}

/// A stable key for a member, used for React keys + the remove-button testid.
/// `contact:<id>` or `raw:<callsign>`.
function memberKey(m: GroupMember): string {
  return m.type === 'contact' ? `contact:${m.contact_id}` : `raw:${m.callsign}`;
}

/// Is this member already present? Contact members dedup by contact_id; raw
/// members dedup by exact callsign (SSID-bearing, case-sensitive — SSID is
/// identity, never normalized away).
function hasMember(members: GroupMember[], candidate: GroupMember): boolean {
  return members.some((m) => memberKey(m) === memberKey(candidate));
}

export function GroupEditor({ group, contacts, onSave, onDelete, onCancel }: GroupEditorProps) {
  const [name, setName] = useState(group.name ?? '');
  const [members, setMembers] = useState<GroupMember[]>(group.members ?? []);
  const [search, setSearch] = useState('');
  const [saving, setSaving] = useState(false);

  const isNew = group.id.trim().length === 0;
  const nameValid = name.trim().length > 0;

  // Contact-search options for the member picker. A small case-insensitive
  // substring match on name / callsign / email / tactical, excluding contacts
  // already added as members (so the dropdown only offers new picks).
  const q = search.trim().toLowerCase();
  const options = useMemo(() => {
    if (q.length === 0) return [];
    return contacts.filter((c) => {
      if (hasMember(members, { type: 'contact', contact_id: c.id })) return false;
      const hay = [c.name, c.callsign, c.email, c.tactical]
        .filter(Boolean)
        .join(' ')
        .toLowerCase();
      return hay.includes(q);
    });
  }, [contacts, members, q]);

  const addContact = (contact: Contact) => {
    const m: GroupMember = { type: 'contact', contact_id: contact.id };
    setMembers((prev) => (hasMember(prev, m) ? prev : [...prev, m]));
    setSearch('');
  };

  // Commit the typed text as a RAW callsign member (the Locked "typed → raw"
  // branch). Trimmed; whitespace-only is ignored.
  const addRaw = () => {
    const callsign = search.trim();
    if (callsign.length === 0) return;
    const m: GroupMember = { type: 'raw', callsign };
    setMembers((prev) => (hasMember(prev, m) ? prev : [...prev, m]));
    setSearch('');
  };

  const removeMember = (key: string) => {
    setMembers((prev) => prev.filter((m) => memberKey(m) !== key));
  };

  const onSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      // Enter with exactly one matching contact picks it; otherwise commit raw.
      if (options.length === 1) {
        addContact(options[0]);
      } else {
        addRaw();
      }
    } else if (e.key === 'Escape') {
      setSearch('');
    }
  };

  const submit = async () => {
    if (!nameValid || saving) return;
    setSaving(true);
    try {
      await onSave({ ...group, name: name.trim(), members });
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      className="group-editor"
      data-testid="group-editor"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h3 className="group-editor-title">{isNew ? 'New group' : 'Edit group'}</h3>

      <label className="group-editor-row">
        <span>Name *</span>
        <input
          data-testid="group-editor-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="ARES — Multnomah Co."
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      {/* Member management. */}
      <div className="group-editor-row">
        <span>Members</span>

        <ul className="group-member-list" data-testid="group-member-list">
          {members.length === 0 && (
            <li className="group-member-empty" data-testid="group-member-empty">
              No members yet — search a contact or type a callsign below.
            </li>
          )}
          {members.map((m) => {
            const key = memberKey(m);
            return (
              <MemberRow
                key={key}
                member={m}
                memberKey={key}
                contacts={contacts}
                onRemove={() => removeMember(key)}
              />
            );
          })}
        </ul>

        <div className="group-member-add">
          <input
            data-testid="group-member-search"
            className="group-member-search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={onSearchKeyDown}
            placeholder="Search a contact or type a callsign"
            autoComplete="off"
            spellCheck={false}
          />
          <button
            type="button"
            className="group-member-add-btn"
            data-testid="group-member-add-raw"
            onClick={addRaw}
            disabled={search.trim().length === 0}
            title="Add the typed text as a raw callsign"
          >
            + Add
          </button>

          {options.length > 0 && (
            <ul className="group-member-options" data-testid="group-member-options">
              {options.map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    className="group-member-option"
                    data-testid={`member-option-${c.id}`}
                    onClick={() => addContact(c)}
                  >
                    <span className="group-member-option-name">{c.name || c.callsign}</span>
                    <span className="group-member-option-callsign">{c.callsign}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <div className="group-editor-actions">
        {!isNew && (
          <button
            type="button"
            className="group-editor-btn group-editor-btn-danger"
            data-testid="group-editor-delete"
            onClick={() => void onDelete(group.id)}
          >
            Delete
          </button>
        )}
        <span className="group-editor-actions-spacer" />
        <button
          type="button"
          className="group-editor-btn"
          data-testid="group-editor-cancel"
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="group-editor-btn group-editor-btn-primary"
          data-testid="group-editor-save"
          disabled={!nameValid || saving}
        >
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </form>
  );
}

/// A single member row. A contact member resolves `contact_id` → name+callsign;
/// a deleted-contact member (unresolvable id) renders DISTINCTLY and stays
/// removable (M6). A raw member shows its literal callsign.
function MemberRow({
  member,
  memberKey: key,
  contacts,
  onRemove,
}: {
  member: GroupMember;
  memberKey: string;
  contacts: Contact[];
  onRemove: () => void;
}) {
  let primary: string;
  let secondary: string | null;
  let unknown = false;

  if (member.type === 'raw') {
    primary = member.callsign;
    secondary = 'raw callsign';
  } else {
    const c = contacts.find((x) => x.id === member.contact_id);
    if (c) {
      primary = c.name || c.callsign;
      secondary = c.callsign;
    } else {
      // Deleted contact — show distinctly, never silently drop (M6).
      unknown = true;
      primary = '(unknown / removed contact)';
      secondary = null;
    }
  }

  return (
    <li
      className={`group-member-row${unknown ? ' group-member-row--unknown' : ''}`}
      data-testid={`member-row-${key}`}
    >
      <span className="group-member-primary">{primary}</span>
      {secondary && <span className="group-member-secondary">{secondary}</span>}
      <button
        type="button"
        className="group-member-remove"
        data-testid={`member-remove-${key}`}
        onClick={onRemove}
        aria-label="Remove member"
        title="Remove member"
      >
        ×
      </button>
    </li>
  );
}
