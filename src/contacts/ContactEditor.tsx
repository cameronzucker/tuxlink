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
// This is an inline form, never a popup window (WebKitGTK + project design
// constraint). The dial-transport <select> follows the app's `tux-select`
// convention (ContactsPanel's sort select).
//
// "Radio dials" (tuxlink-6vn4x / the Peers fix): the operator's MANUAL
// channels — transport + frequency — are editable here. Observed channels
// (minted by on-air activity) are NEVER shown or touched by this form; they
// pass through to onSave untouched.

import { useState } from 'react';
import './ContactEditor.css';
import type { Channel, ChannelTransport, Contact } from './types';

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

/// The transports a manual dial can carry — the exact wire tags from
/// `ChannelTransport`, minus `'unknown'` (nothing dialable there).
export const DIAL_TRANSPORTS: readonly ChannelTransport[] = ['packet', 'ardop', 'vara-hf', 'vara-fm'];

const DIAL_TRANSPORT_LABEL: Record<string, string> = {
  packet: 'Packet',
  ardop: 'ARDOP',
  'vara-hf': 'VARA HF',
  'vara-fm': 'VARA FM',
};

/// Parse an operator-entered dial frequency into integer Hz.
///   "7.1035"  → 7_103_500  (MHz — the display unit)
///   "7103500" → 7_103_500  (no dot AND > 100 kHz ⇒ tolerate raw Hz paste)
///   "146"     → 146_000_000 (no dot but ≤ 100 kHz as a number ⇒ still MHz)
/// Returns null for empty / non-numeric / non-positive input.
export function parseDialFreq(input: string): number | null {
  const t = input.trim();
  if (t.length === 0) return null;
  const n = Number(t);
  if (!Number.isFinite(n) || n <= 0) return null;
  if (!t.includes('.') && n > 100_000) return Math.round(n);
  return Math.round(n * 1_000_000);
}

/// Display form of a stored freq_hz: MHz, trailing-zero-free ("7.1035").
function formatDialFreq(freqHz: number | null): string {
  if (freqHz == null) return '';
  return String(freqHz / 1_000_000);
}

/// One editable dial row. `original` is the pre-existing manual Channel this
/// row seeds from (kept so an UNCHANGED dial round-trips verbatim — its
/// counts/last_ok history survives the edit).
interface DialRow {
  transport: ChannelTransport;
  freqText: string;
  original: Channel | null;
}

/// A fresh manual Channel for a dial the operator just entered (or changed).
function manualChannel(transport: ChannelTransport, freqHz: number, targetCallsign: string): Channel {
  return {
    transport,
    target_callsign: targetCallsign,
    via: [],
    freq_hz: freqHz,
    bandwidth: null,
    direction: 'unknown',
    counts: { ok: 0, fail: 0 },
    last_seen: '',
    last_ok: null,
    last_ok_direction: null,
    source: 'manual',
  };
}

export function ContactEditor({ contact, onSave, onCancel }: ContactEditorProps) {
  const [name, setName] = useState(contact.name ?? '');
  const [callsign, setCallsign] = useState(contact.callsign ?? '');
  const [email, setEmail] = useState(contact.email ?? '');
  const [tactical, setTactical] = useState(contact.tactical ?? '');
  const [notes, setNotes] = useState(contact.notes ?? '');
  const [dials, setDials] = useState<DialRow[]>(() =>
    (contact.channels ?? [])
      .filter((ch) => ch.source === 'manual')
      .map((ch) => ({ transport: ch.transport, freqText: formatDialFreq(ch.freq_hz), original: ch })),
  );
  const [saving, setSaving] = useState(false);

  const callsignValid = callsign.trim().length > 0;
  const isNew = contact.id.trim().length === 0;

  const setDial = (index: number, patch: Partial<DialRow>) => {
    setDials((prev) => prev.map((d, i) => (i === index ? { ...d, ...patch } : d)));
  };
  const removeDial = (index: number) => {
    setDials((prev) => prev.filter((_, i) => i !== index));
  };
  const addDial = () => {
    setDials((prev) => [...prev, { transport: 'vara-hf', freqText: '', original: null }]);
  };

  const submit = async () => {
    if (!callsignValid || saving) return;
    const trimmedCallsign = callsign.trim();
    // Observed channels pass through UNTOUCHED; manual rows are rebuilt from
    // the dial editor. A dial whose transport+freq are unchanged round-trips
    // its original Channel (history preserved); a new/changed dial is a fresh
    // manual Channel. Rows with an unparseable/empty frequency are dropped —
    // a dial without a frequency is not a dial.
    const observed = (contact.channels ?? []).filter((ch) => ch.source !== 'manual');
    const manual: Channel[] = [];
    for (const d of dials) {
      const freqHz = parseDialFreq(d.freqText);
      if (freqHz == null) continue;
      if (d.original && d.original.transport === d.transport && d.original.freq_hz === freqHz) {
        // Round-trip the original row (history preserved) but ALWAYS retarget
        // it at the CURRENT callsign — a callsign edit must not leave the dial
        // connecting to the previous station (Codex adrev 2026-07-18 P2).
        manual.push({ ...d.original, target_callsign: trimmedCallsign });
      } else {
        manual.push(manualChannel(d.transport, freqHz, trimmedCallsign));
      }
    }
    setSaving(true);
    try {
      await onSave({
        ...contact,
        // SSID-bearing identity — trim whitespace only, never strip the SSID.
        callsign: trimmedCallsign,
        name: name.trim(),
        email: opt(email),
        tactical: opt(tactical),
        notes: opt(notes),
        channels: [...observed, ...manual],
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

      <div className="contact-editor-row contact-editor-dials" data-testid="editor-dials">
        <span>Radio dials</span>
        {dials.map((d, i) => (
          <div className="contact-editor-dial" data-testid={`editor-dial-${i}`} key={i}>
            <select
              className="tux-select contact-editor-dial-transport"
              data-testid={`editor-dial-transport-${i}`}
              aria-label="Dial transport"
              value={d.transport}
              onChange={(e) => setDial(i, { transport: e.target.value as ChannelTransport })}
            >
              {DIAL_TRANSPORTS.map((t) => (
                <option key={t} value={t}>
                  {DIAL_TRANSPORT_LABEL[t] ?? t}
                </option>
              ))}
            </select>
            <input
              className="contact-editor-dial-freq"
              data-testid={`editor-dial-freq-${i}`}
              aria-label="Dial frequency (MHz)"
              value={d.freqText}
              onChange={(e) => setDial(i, { freqText: e.target.value })}
              placeholder="7.1035"
              inputMode="decimal"
              autoComplete="off"
              spellCheck={false}
            />
            <span className="contact-editor-dial-unit" aria-hidden="true">
              MHz
            </span>
            <button
              type="button"
              className="contact-editor-dial-remove"
              data-testid={`editor-dial-remove-${i}`}
              aria-label="Remove dial"
              title="Remove dial"
              onClick={() => removeDial(i)}
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          className="contact-editor-dial-add"
          data-testid="editor-dial-add"
          onClick={addDial}
        >
          ＋ add dial
        </button>
      </div>

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
