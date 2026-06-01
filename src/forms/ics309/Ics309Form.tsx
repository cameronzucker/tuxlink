import { useState } from 'react';
import type { FormComposeProps } from '../forms';

const MAX_LOG_ROWS = 30; // WLE Form-309 caps log entries at 30
const INITIAL_VISIBLE_ROWS = 5;

export function Ics309Form({ initialValues = {}, onChange, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const [entryCount, setEntryCount] = useState(() => {
    // Codex r2 P2 #2: scan ALL 4 entry fields across the full 30-row range
    // when deriving the initial visible count from a restored draft. The
    // prior version only scanned `time1..time10`, so any saved data in
    // entries 11-30, or rows with from/to/sub populated but no time, would
    // be hidden from the operator while still being submitted on the wire.
    for (let i = MAX_LOG_ROWS; i >= 1; i--) {
      const hasAny =
        (initialValues[`time${i}`] ?? '').trim() ||
        (initialValues[`from${i}`] ?? '').trim() ||
        (initialValues[`to${i}`] ?? '').trim() ||
        (initialValues[`sub${i}`] ?? '').trim();
      if (hasAny) return i;
    }
    return INITIAL_VISIBLE_ROWS;
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

  const addEntry = () => setEntryCount((n) => Math.min(n + 1, MAX_LOG_ROWS));

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
              <label>Time #{n} <input value={values[`time${n}`] ?? ''} onChange={(e) => set(`time${n}`, e.target.value)} maxLength={10} placeholder="HH:MMZ" /></label>
              <label>From #{n} <input value={values[`from${n}`] ?? ''} onChange={(e) => set(`from${n}`, e.target.value)} maxLength={30} /></label>
              <label>To #{n} <input value={values[`to${n}`] ?? ''} onChange={(e) => set(`to${n}`, e.target.value)} maxLength={30} /></label>
              <label>Subject #{n} <textarea value={values[`sub${n}`] ?? ''} onChange={(e) => set(`sub${n}`, e.target.value)} rows={2} maxLength={200} /></label>
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
