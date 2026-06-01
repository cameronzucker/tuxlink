import { useState } from 'react';
import type { FormComposeProps } from '../forms';

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
  return (
    <form className="bulletin-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <label>Precedence Level <input value={values.level ?? ''} onChange={(e) => set('level', e.target.value)} maxLength={20} required /></label>
      <label>Subject <input value={values.subjectline ?? ''} onChange={(e) => set('subjectline', e.target.value)} maxLength={80} required /></label>
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
