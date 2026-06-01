import { useState } from 'react';
import type { FormComposeProps } from '../forms';

interface CategoryRowProps {
  label: string;
  prefix: string;
  values: Record<string, string>;
  set: (id: string, v: string) => void;
  nameField?: string; // for "Other" categories that have a user-named label
}

function CategoryRow({ label, prefix, values, set, nameField }: CategoryRowProps) {
  return (
    <fieldset className="damage-category">
      {nameField ? (
        <legend>
          <input
            aria-label={`${label} category name`}
            value={values[nameField] ?? ''}
            onChange={(e) => set(nameField, e.target.value)}
            placeholder={label}
            maxLength={40}
          />
          {' — Counts'}
        </legend>
      ) : (
        <legend>{label} — Counts</legend>
      )}
      <label>Affected <input value={values[`aff${prefix}`] ?? ''} onChange={(e) => set(`aff${prefix}`, e.target.value)} maxLength={10} /></label>
      <label>Minor <input value={values[`min${prefix}`] ?? ''} onChange={(e) => set(`min${prefix}`, e.target.value)} maxLength={10} /></label>
      <label>Major <input value={values[`maj${prefix}`] ?? ''} onChange={(e) => set(`maj${prefix}`, e.target.value)} maxLength={10} /></label>
      <label>Totaled <input value={values[`des${prefix}`] ?? ''} onChange={(e) => set(`des${prefix}`, e.target.value)} maxLength={10} /></label>
      <label>Total # <input value={values[`total${prefix}`] ?? ''} onChange={(e) => set(`total${prefix}`, e.target.value)} maxLength={10} /></label>
      <label>Costs <input value={values[`dollar${prefix}`] ?? ''} onChange={(e) => set(`dollar${prefix}`, e.target.value)} maxLength={20} /></label>
    </fieldset>
  );
}

const FIXED_CATEGORIES: Array<{ label: string; n: number }> = [
  { label: 'Houses', n: 1 },
  { label: 'Apartment Complex', n: 2 },
  { label: 'Mobile Homes', n: 3 },
  { label: 'Residential High Rise', n: 4 },
  { label: 'Commercial High Rise', n: 5 },
  { label: 'Public Buildings', n: 6 },
  { label: 'Small Business', n: 7 },
  { label: 'Factories/Industrial', n: 8 },
  { label: 'Roads', n: 9 },
  { label: 'Bridges', n: 10 },
  { label: 'Electrical Distribution', n: 11 },
  { label: 'Schools', n: 12 },
];

export function DamageAssessmentForm({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const set = (id: string, v: string) => {
    setValues((s) => {
      const next = { ...s, [id]: v };
      onChange?.(next);
      return next;
    });
  };
  const required = ['status', 'jur', 'surarea'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => { if (canSubmit) onSubmit(values); };

  return (
    <form className="damage-assessment-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <fieldset>
        <legend>Survey Header</legend>
        <label>Title <input value={values.title ?? ''} onChange={(e) => set('title', e.target.value)} maxLength={60} /></label>
        <label>Status <input value={values.status ?? ''} onChange={(e) => set('status', e.target.value)} maxLength={30} required /></label>
        <label>Jurisdiction <input value={values.jur ?? ''} onChange={(e) => set('jur', e.target.value)} maxLength={60} required /></label>
        <label>Survey Area <input value={values.surarea ?? ''} onChange={(e) => set('surarea', e.target.value)} maxLength={60} required /></label>
        <label>Event Date <input value={values.datetime1 ?? ''} onChange={(e) => set('datetime1', e.target.value)} maxLength={30} /></label>
        <label>Survey Date <input value={values.date ?? ''} onChange={(e) => set('date', e.target.value)} type="date" /></label>
        <label>Mission/Incident # <input value={values.misnum ?? ''} onChange={(e) => set('misnum', e.target.value)} maxLength={30} /></label>
        <label>Event Type <input value={values.event ?? ''} onChange={(e) => set('event', e.target.value)} maxLength={60} /></label>
        <label>Other Event <input value={values.other ?? ''} onChange={(e) => set('other', e.target.value)} maxLength={60} /></label>
        <label>Survey Team <input value={values.surteam ?? ''} onChange={(e) => set('surteam', e.target.value)} maxLength={60} /></label>
      </fieldset>

      <fieldset>
        <legend>Survey Report — Categories</legend>
        {FIXED_CATEGORIES.map(({ label, n }) => (
          <CategoryRow key={n} label={label} prefix={String(n)} values={values} set={set} />
        ))}
        <CategoryRow label="Other Category" prefix="13" values={values} set={set} nameField="other13" />
        <CategoryRow label="Other Category" prefix="14" values={values} set={set} nameField="other14" />
        <CategoryRow label="Other Category" prefix="15" values={values} set={set} nameField="other15" />
      </fieldset>

      <fieldset>
        <legend>Totals</legend>
        <label>Total Dollar Cost <input value={values.dollar16 ?? ''} onChange={(e) => set('dollar16', e.target.value)} maxLength={30} /></label>
        <label>Comments <textarea value={values.comments ?? ''} onChange={(e) => set('comments', e.target.value)} rows={4} maxLength={2000} /></label>
      </fieldset>

      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="damage-assessment-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
