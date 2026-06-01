import { useState } from 'react';
import type { FormComposeProps } from '../forms';

export function PositionForm({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const set = (id: string, v: string) => {
    setValues((s) => {
      const next = { ...s, [id]: v };
      onChange?.(next);
      return next;
    });
  };
  const required = ['thetime', 'lat', 'lon'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => { if (canSubmit) onSubmit(values); };
  return (
    <form className="position-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <label>Time <input value={values.thetime ?? ''} onChange={(e) => set('thetime', e.target.value)} maxLength={20} placeholder="HH:MMZ" required /></label>
      <label>Latitude <input value={values.lat ?? ''} onChange={(e) => set('lat', e.target.value)} maxLength={20} placeholder="DD.DDDDDD" required /></label>
      <label>Longitude <input value={values.lon ?? ''} onChange={(e) => set('lon', e.target.value)} maxLength={20} placeholder="DDD.DDDDDD" required /></label>
      <label>Comment <textarea value={values.message ?? ''} onChange={(e) => set('message', e.target.value)} rows={3} maxLength={200} /></label>
      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="position-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
