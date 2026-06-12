// FormSequenceSettings — the Settings-chrome section for WLE SeqInc serial
// counters (tuxlink-2tom / G12-C). Forms carrying the `SeqInc:` directive
// (radiograms, RRI, net logs) auto-number each send from a persisted per-form
// counter. This section lists every form that has a counter and lets the
// operator reset the next serial — e.g. restart radiogram numbering at 1 for a
// new event or year (the "Message > Template settings" affordance WLE forms
// reference).
//
// The serial itself is assigned automatically at send time; there is nothing to
// configure for normal use. This section is the reset/override surface only.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './FormSequenceSettings.css';

interface SeqCounterStatus {
  formId: string;
  nextSerial: number;
}

export function FormSequenceSettings() {
  const [counters, setCounters] = useState<SeqCounterStatus[]>([]);
  const [error, setError] = useState<string | null>(null);
  // Per-form draft value for the "next serial" input, keyed by formId.
  const [drafts, setDrafts] = useState<Record<string, string>>({});

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<SeqCounterStatus[]>('forms_sequence_status');
      setCounters(Array.isArray(list) ? list : []);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleReset = useCallback(
    async (formId: string) => {
      const raw = drafts[formId];
      const next = Number(raw);
      if (!raw || !Number.isInteger(next) || next < 1) {
        setError('Next serial must be a whole number of 1 or greater.');
        return;
      }
      try {
        await invoke('forms_sequence_reset', { formId, next });
        setDrafts((d) => {
          const { [formId]: _drop, ...rest } = d;
          return rest;
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [drafts, refresh],
  );

  return (
    <div className="form-seq-settings" data-testid="form-seq-settings">
      {error && (
        <p className="form-seq-settings__error" role="alert" data-testid="form-seq-error">
          {error}
        </p>
      )}

      {counters.length === 0 ? (
        <p className="form-seq-settings__empty" data-testid="form-seq-empty">
          No serial-numbered forms have been sent yet. Forms that auto-number
          (radiograms, RRI, net logs) will appear here after their first send.
        </p>
      ) : (
        <ul className="form-seq-settings__list">
          {counters.map((c) => (
            <li key={c.formId} className="form-seq-settings__row" data-testid={`form-seq-row-${c.formId}`}>
              <span className="form-seq-settings__id">{c.formId}</span>
              <span className="form-seq-settings__next">
                next: <strong>{c.nextSerial}</strong>
              </span>
              <label className="form-seq-settings__reset">
                <span className="form-seq-settings__reset-label">Set next to</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  className="form-seq-settings__input"
                  data-testid={`form-seq-input-${c.formId}`}
                  value={drafts[c.formId] ?? ''}
                  placeholder={String(c.nextSerial)}
                  onChange={(e) =>
                    setDrafts((d) => ({ ...d, [c.formId]: e.target.value }))
                  }
                />
              </label>
              <button
                type="button"
                className="form-seq-settings__btn"
                data-testid={`form-seq-set-${c.formId}`}
                disabled={!drafts[c.formId]}
                onClick={() => handleReset(c.formId)}
              >
                Set
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
