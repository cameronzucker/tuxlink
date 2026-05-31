import { useState } from 'react';
import type { FormComposeProps } from '../forms';

export function Ics213Form({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
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
  return (
    <form className="ics213-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <label>Incident Name <input value={values.inc_name ?? ''} onChange={(e) => set('inc_name', e.target.value)} maxLength={30} /></label>
      <label>To (Name and Position) <input value={values.to_name ?? ''} onChange={(e) => set('to_name', e.target.value)} maxLength={60} required /></label>
      <label>From (Name and Position) <input value={values.fm_name ?? ''} onChange={(e) => set('fm_name', e.target.value)} maxLength={60} required /></label>
      <label>Subject <input value={values.subjectline ?? ''} onChange={(e) => set('subjectline', e.target.value)} maxLength={50} required /></label>
      <label>Date <input value={values.mdate ?? ''} onChange={(e) => set('mdate', e.target.value)} type="date" required /></label>
      <label>Time <input value={values.mtime ?? ''} onChange={(e) => set('mtime', e.target.value)} placeholder="HH:MMZ" required /></label>
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
