import { useEffect, useState } from 'react';
import type { FormComposeProps } from '../forms';
import { listSlots, upsertSlot, deleteSlot, type FormDraftSlot } from '../../compose/FormDraftLibrary';

// FormDraftLibrary integration:
//   Saveable fields: to_name, fm_name, subjectline, message, inc_name, approved_name,
//   approved_postitle, isexercise.
//   NOT saved: mdate, mtime (auto-filled to now at compose time; a stale slot datetime
//   would be confusing and incorrect).
const ICS213_FORM_ID = 'ICS213_Initial';

// The saveable field keys for ICS-213 slots.
const ICS213_SAVEABLE = [
  'to_name', 'fm_name', 'subjectline', 'message',
  'inc_name', 'approved_name', 'approved_postitle', 'isexercise',
] as const;

export function Ics213Form({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  // Default Date + Time to "now" (UTC). The vast majority of ICS-213 traffic is
  // composed for the moment of composition; pre-filling saves the operator a
  // step and avoids the broken native `<input type="date">` picker on WebKitGTK
  // (tabbing to accept today's auto-fill doesn't bind; click-off behavior is
  // buggy). initialValues — including draft-restored state — overrides defaults.
  const [values, setValues] = useState<Record<string, string>>(() => {
    const now = new Date();
    const today = now.toISOString().slice(0, 10);
    const hh = String(now.getUTCHours()).padStart(2, '0');
    const mm = String(now.getUTCMinutes()).padStart(2, '0');
    return { mdate: today, mtime: `${hh}:${mm}Z`, ...initialValues };
  });
  const set = (id: string, v: string) => {
    setValues((s) => {
      const next = { ...s, [id]: v };
      onChange?.(next);
      return next;
    });
  };
  const required = ['to_name', 'fm_name', 'subjectline', 'mdate', 'mtime', 'message'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => {
    if (canSubmit) onSubmit(values);
  };

  // FormDraftLibrary slot state.
  const [slots, setSlots] = useState<FormDraftSlot[]>([]);
  const [selectedSlotId, setSelectedSlotId] = useState<string>('');

  // Load saved slots on mount. Error → empty list (non-fatal).
  useEffect(() => {
    listSlots(ICS213_FORM_ID).then(setSlots).catch(() => setSlots([]));
  }, []);

  function applySlot(slotId: string) {
    setSelectedSlotId(slotId);
    if (!slotId) return;
    const slot = slots.find((s) => s.slot_id === slotId);
    if (!slot) return;
    // Apply only saveable fields; leave volatile mdate/mtime at current values.
    setValues((prev) => {
      const merged: Record<string, string> = { ...prev };
      for (const key of ICS213_SAVEABLE) {
        if (key in slot.payload && typeof slot.payload[key] === 'string') {
          merged[key] = slot.payload[key] as string;
        }
      }
      onChange?.(merged);
      return merged;
    });
  }

  async function saveSlot() {
    const label = window.prompt('Name this slot (e.g. "Net Check-In Template"):');
    if (!label) return;
    // "Save as slot…" always creates a new slot — no slotId passed even when
    // a slot is selected. Update-in-place is a P3 follow-up.
    const payload: Record<string, string> = {};
    for (const key of ICS213_SAVEABLE) {
      payload[key] = values[key] ?? '';
    }
    const newSlot = await upsertSlot({
      formId: ICS213_FORM_ID,
      label,
      payload,
    });
    setSlots((prev) => [...prev, newSlot]);
    setSelectedSlotId(newSlot.slot_id);
  }

  async function removeSlot() {
    if (!selectedSlotId) return;
    if (!window.confirm('Delete this saved slot?')) return;
    await deleteSlot(selectedSlotId);
    setSlots((prev) => prev.filter((s) => s.slot_id !== selectedSlotId));
    setSelectedSlotId('');
  }

  return (
    <form className="ics213-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      {/* ── Saved slots toolbar ── */}
      <div className="form-slot-toolbar" data-testid="slot-toolbar">
        <label htmlFor="ics213-slot-select">Saved slots:</label>
        <select
          id="ics213-slot-select"
          value={selectedSlotId}
          onChange={(e) => applySlot(e.target.value)}
        >
          <option value="">— None —</option>
          {slots.map((s) => (
            <option key={s.slot_id} value={s.slot_id}>{s.label}</option>
          ))}
        </select>
        <button type="button" onClick={saveSlot} data-testid="slot-save-btn">
          Save as slot…
        </button>
        {selectedSlotId && (
          <button type="button" onClick={removeSlot} data-testid="slot-delete-btn">
            Delete
          </button>
        )}
      </div>
      <label>Incident Name <input value={values.inc_name ?? ''} onChange={(e) => set('inc_name', e.target.value)} maxLength={30} /></label>
      <label>To (Name and Position) <input value={values.to_name ?? ''} onChange={(e) => set('to_name', e.target.value)} maxLength={60} required /></label>
      <label>From (Name and Position) <input value={values.fm_name ?? ''} onChange={(e) => set('fm_name', e.target.value)} maxLength={60} required /></label>
      <label>Subject <input value={values.subjectline ?? ''} onChange={(e) => set('subjectline', e.target.value)} maxLength={50} required /></label>
      <label>Date <input value={values.mdate ?? ''} onChange={(e) => set('mdate', e.target.value)} placeholder="YYYY-MM-DD" maxLength={10} required /></label>
      <label>Time <input value={values.mtime ?? ''} onChange={(e) => set('mtime', e.target.value)} placeholder="HH:MMZ" maxLength={6} required /></label>
      <label>Message <textarea value={values.message ?? ''} onChange={(e) => set('message', e.target.value)} rows={6} required /></label>
      <label>Approved by <input value={values.approved_name ?? ''} onChange={(e) => set('approved_name', e.target.value)} maxLength={60} /></label>
      <label>Position/Title <input value={values.approved_postitle ?? ''} onChange={(e) => set('approved_postitle', e.target.value)} maxLength={60} /></label>
      <label><input type="checkbox" checked={values.isexercise === '** THIS IS AN EXERCISE **'} onChange={(e) => set('isexercise', e.target.checked ? '** THIS IS AN EXERCISE **' : '')} /> Is exercise</label>
      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="ics213-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
