import { useEffect, useState } from 'react';
import type { FormComposeProps } from '../forms';
import { listSlots, upsertSlot, deleteSlot, type FormDraftSlot } from '../../compose/FormDraftLibrary';

// FormDraftLibrary integration:
//   Saveable fields: level, subjectline, name (recipient), from_name, message, title.
//   NOT saved: bullnr (bulletin number — unique per bulletin, not a template field),
//              activitydatetime1 (always operator-picked for the moment of the bulletin).
const BULLETIN_FORM_ID = 'Bulletin_Initial';

const BULLETIN_SAVEABLE = [
  'level', 'subjectline', 'name', 'from_name', 'message', 'title',
] as const;

export function BulletinForm({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const set = (id: string, v: string) => {
    setValues((s) => {
      const next = { ...s, [id]: v };
      onChange?.(next);
      return next;
    });
  };
  const required = ['level', 'subjectline', 'bullnr', 'name', 'from_name', 'activitydatetime1', 'message'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => { if (canSubmit) onSubmit(values); };

  // FormDraftLibrary slot state.
  const [slots, setSlots] = useState<FormDraftSlot[]>([]);
  const [selectedSlotId, setSelectedSlotId] = useState<string>('');

  // Load saved slots on mount. Error → empty list (non-fatal).
  useEffect(() => {
    listSlots(BULLETIN_FORM_ID).then(setSlots).catch(() => setSlots([]));
  }, []);

  function applySlot(slotId: string) {
    setSelectedSlotId(slotId);
    if (!slotId) return;
    const slot = slots.find((s) => s.slot_id === slotId);
    if (!slot) return;
    // Apply only saveable fields; leave bullnr and activitydatetime1 as-is.
    setValues((prev) => {
      const merged: Record<string, string> = { ...prev };
      for (const key of BULLETIN_SAVEABLE) {
        if (key in slot.payload && typeof slot.payload[key] === 'string') {
          merged[key] = slot.payload[key] as string;
        }
      }
      onChange?.(merged);
      return merged;
    });
  }

  async function saveSlot() {
    const label = window.prompt('Name this slot (e.g. "Weekly Net Bulletin"):');
    if (!label) return;
    // "Save as slot…" always creates a new slot — no slotId passed even when
    // a slot is selected. Update-in-place is a P3 follow-up.
    const payload: Record<string, string> = {};
    for (const key of BULLETIN_SAVEABLE) {
      payload[key] = values[key] ?? '';
    }
    const newSlot = await upsertSlot({
      formId: BULLETIN_FORM_ID,
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
    <form className="bulletin-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      {/* ── Saved slots toolbar ── */}
      <div className="form-slot-toolbar" data-testid="slot-toolbar">
        <label htmlFor="bulletin-slot-select">Saved slots:</label>
        <select
          id="bulletin-slot-select"
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
      <label>Precedence Level <input value={values.level ?? ''} onChange={(e) => set('level', e.target.value)} maxLength={20} required /></label>
      <label>Form subject <input value={values.subjectline ?? ''} onChange={(e) => set('subjectline', e.target.value)} maxLength={80} required /></label>
      <label>Bulletin # <input value={values.bullnr ?? ''} onChange={(e) => set('bullnr', e.target.value)} maxLength={10} required /></label>
      <label>Title <input value={values.title ?? ''} onChange={(e) => set('title', e.target.value)} maxLength={60} /></label>
      <label>For (Recipient) <input value={values.name ?? ''} onChange={(e) => set('name', e.target.value)} maxLength={60} required /></label>
      <label>Bulletin From <input value={values.from_name ?? ''} onChange={(e) => set('from_name', e.target.value)} maxLength={60} required /></label>
      <label>Date/Time <input value={values.activitydatetime1 ?? ''} onChange={(e) => set('activitydatetime1', e.target.value)} maxLength={30} required /></label>
      <label>Message <textarea value={values.message ?? ''} onChange={(e) => set('message', e.target.value)} rows={6} maxLength={4000} required /></label>
      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="bulletin-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
