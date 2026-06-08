// ContactEditor — the New / Edit contact form (Task A8).
//
// One controlled form reused for THREE entry points:
//   1. "+ New" in ContactsPanel       → all fields blank.
//   2. "Edit" on a selected contact   → fields prefilled from the contact.
//   3. "Add to contacts" on a message  → callsign prefilled from the sender (G1).
//
// Callsign is REQUIRED (Save is disabled until it is non-empty); name / email /
// tactical / notes are optional. Save routes through `contact_upsert` (the Rust
// command stamps id + created_at/updated_at when the id is empty, so a NEW
// contact carries an empty id and the backend assigns it — see
// `src-tauri/src/contacts/commands.rs::stamp_contact`). The callsign is the
// SSID-bearing primary identity and is NEVER stripped.
//
// No native <select>; this is an inline form, never a popup window (WebKitGTK +
// project design constraint).

import { useState } from 'react';
import './ContactEditor.css';
import type { Contact } from './types';

export interface ContactEditorProps {
  /// The contact being edited. For a NEW contact pass a seed with an empty `id`
  /// (and optionally a prefilled callsign for the add-from-sender flow, G1).
  contact: Contact;
  /// Persist the assembled contact (callsign required). Returns the upsert
  /// promise so the editor can close after it resolves.
  onSave: (contact: Contact) => Promise<void> | void;
  /// Discard without saving.
  onCancel: () => void;
}

/// An empty Contact seed — the id/timestamps are left blank for the backend to
/// stamp. `callsign` may be pre-seeded (G1 add-from-sender) before mount.
export function emptyContact(callsign = ''): Contact {
  return {
    id: '',
    name: '',
    callsign,
    email: undefined,
    tactical: undefined,
    notes: undefined,
    created_at: '',
    updated_at: '',
  };
}

/// Coalesce a trimmed string field: '' → undefined (so optional fields are
/// genuinely absent on the wire, not empty strings).
function opt(s: string): string | undefined {
  const t = s.trim();
  return t.length > 0 ? t : undefined;
}

export function ContactEditor({ contact, onSave, onCancel }: ContactEditorProps) {
  const [name, setName] = useState(contact.name ?? '');
  const [callsign, setCallsign] = useState(contact.callsign ?? '');
  const [email, setEmail] = useState(contact.email ?? '');
  const [tactical, setTactical] = useState(contact.tactical ?? '');
  const [notes, setNotes] = useState(contact.notes ?? '');
  const [saving, setSaving] = useState(false);

  const callsignValid = callsign.trim().length > 0;
  const isNew = contact.id.trim().length === 0;

  const submit = async () => {
    if (!callsignValid || saving) return;
    setSaving(true);
    try {
      await onSave({
        ...contact,
        // SSID-bearing identity — trim whitespace only, never strip the SSID.
        callsign: callsign.trim(),
        name: name.trim(),
        email: opt(email),
        tactical: opt(tactical),
        notes: opt(notes),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      className="contact-editor"
      data-testid="contact-editor"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h3 className="contact-editor-title">{isNew ? 'New contact' : 'Edit contact'}</h3>

      <label className="contact-editor-row">
        <span>Callsign *</span>
        <input
          data-testid="editor-callsign"
          value={callsign}
          onChange={(e) => setCallsign(e.target.value)}
          placeholder="W6ABC-7"
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      <label className="contact-editor-row">
        <span>Name</span>
        <input
          data-testid="editor-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Full name"
          autoComplete="off"
        />
      </label>

      <label className="contact-editor-row">
        <span>Email</span>
        <input
          data-testid="editor-email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="callsign@winlink.org"
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      <label className="contact-editor-row">
        <span>Tactical</span>
        <input
          data-testid="editor-tactical"
          value={tactical}
          onChange={(e) => setTactical(e.target.value)}
          placeholder="NET-CONTROL"
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      <label className="contact-editor-row">
        <span>Notes</span>
        <textarea
          data-testid="editor-notes"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          rows={3}
        />
      </label>

      <div className="contact-editor-actions">
        <button
          type="button"
          className="contact-editor-btn"
          data-testid="editor-cancel"
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="contact-editor-btn contact-editor-btn-primary"
          data-testid="editor-save"
          disabled={!callsignValid || saving}
        >
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </form>
  );
}
