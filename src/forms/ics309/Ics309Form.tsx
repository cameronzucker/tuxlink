import { useState } from 'react';
import type { FormComposeProps } from '../forms';

const MAX_ENTRIES = 10;

export function Ics309Form({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const [entryCount, setEntryCount] = useState(() => {
    // Pre-populate entry count based on any existing values
    for (let i = MAX_ENTRIES; i >= 1; i--) {
      if ((initialValues[`time${i}`] ?? '').trim()) return i;
    }
    return 1;
  });

  const set = (id: string, v: string) => {
    setValues((s) => {
      const next = { ...s, [id]: v };
      onChange?.(next);
      return next;
    });
  };

  const required = ['title', 'activitydatetime1', 'opname', 'operid'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => { if (canSubmit) onSubmit(values); };

  const addEntry = () => setEntryCount((n) => Math.min(n + 1, 30));

  return (
    <form className="ics309-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <fieldset>
        <legend>ICS-309 Header</legend>
        <label>Title <input value={values.title ?? ''} onChange={(e) => set('title', e.target.value)} maxLength={60} required /></label>
        <label>Page # <input value={values.page ?? ''} onChange={(e) => set('page', e.target.value)} maxLength={10} /></label>
        <label>Task # <input value={values.task ?? ''} onChange={(e) => set('task', e.target.value)} maxLength={20} /></label>
        <label>Task Name <input value={values.taskname ?? ''} onChange={(e) => set('taskname', e.target.value)} maxLength={60} /></label>
        <label>Date/Time Prepared <input value={values.activitydatetime1 ?? ''} onChange={(e) => set('activitydatetime1', e.target.value)} maxLength={30} required /></label>
        <label>Operational Period # <input value={values.opper ?? ''} onChange={(e) => set('opper', e.target.value)} maxLength={20} /></label>
        <label>Radio Operator Name <input value={values.opname ?? ''} onChange={(e) => set('opname', e.target.value)} maxLength={60} required /></label>
        <label>Station ID <input value={values.operid ?? ''} onChange={(e) => set('operid', e.target.value)} maxLength={30} required /></label>
      </fieldset>

      <fieldset>
        <legend>Log Entries</legend>
        {Array.from({ length: entryCount }, (_, i) => {
          const n = i + 1;
          return (
            <div key={n} className="ics309-log-entry">
              <strong>Entry {n}</strong>
              <label>Time <input value={values[`time${n}`] ?? ''} onChange={(e) => set(`time${n}`, e.target.value)} maxLength={10} placeholder="HH:MMZ" /></label>
              <label>From <input value={values[`from${n}`] ?? ''} onChange={(e) => set(`from${n}`, e.target.value)} maxLength={30} /></label>
              <label>To <input value={values[`to${n}`] ?? ''} onChange={(e) => set(`to${n}`, e.target.value)} maxLength={30} /></label>
              <label>Subject <textarea value={values[`sub${n}`] ?? ''} onChange={(e) => set(`sub${n}`, e.target.value)} rows={2} maxLength={200} /></label>
            </div>
          );
        })}
        {entryCount < 30 && (
          <button type="button" onClick={addEntry}>+ Add entry</button>
        )}
      </fieldset>

      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="ics309-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
