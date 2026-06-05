import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FormComposeProps } from '../forms/forms';
import './Ics309FormV2.css';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface LogRow {
  datetime: string;   // RFC 3339 UTC, e.g. "2024-05-20T10:13:00Z"
  from: string;
  to: string;
  subject: string;
  direction: 'in' | 'out';
}

type Preset = 'last-hour' | 'today' | 'op-period' | 'custom';

interface RangeState {
  preset: Preset;
  start: string;  // RFC 3339 UTC ISO string
  end: string;    // RFC 3339 UTC ISO string
}

// ── Date/time helpers ─────────────────────────────────────────────────────────

function nowIso(): string {
  return new Date().toISOString();
}

function startOfDayIso(): string {
  const now = new Date();
  return new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  ).toISOString();
}

function isoRangeForPreset(preset: Preset, custom?: { start: string; end: string }): { start: string; end: string } {
  const now = new Date();
  switch (preset) {
    case 'last-hour':
      return {
        start: new Date(now.getTime() - 60 * 60 * 1000).toISOString(),
        end: now.toISOString(),
      };
    case 'today':
    case 'op-period':
      return { start: startOfDayIso(), end: nowIso() };
    case 'custom':
      return custom ?? { start: startOfDayIso(), end: nowIso() };
  }
}

/** Convert a local `datetime-local` input string to a UTC ISO string. */
function localInputToIso(s: string): string {
  if (!s) return nowIso();
  // `datetime-local` yields "YYYY-MM-DDTHH:MM" — treat it as local time.
  return new Date(s).toISOString();
}

/** Convert a UTC ISO string to a `datetime-local` input value (local time, no tz). */
function isoToLocalInput(iso: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  // datetime-local format: "YYYY-MM-DDTHH:MM"
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

// ── CSV export helper ─────────────────────────────────────────────────────────

function downloadCsv(rows: LogRow[], rangeStart: string, rangeEnd: string): void {
  const header = 'Datetime (UTC),Dir,From,To,Subject\r\n';
  const body = rows
    .map((r) =>
      [r.datetime, r.direction, r.from, r.to, r.subject]
        .map((cell) => `"${cell.replace(/"/g, '""')}"`)
        .join(',')
    )
    .join('\r\n');
  const csv = header + body;
  const blob = new Blob([csv], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `ics309-${rangeStart.slice(0, 10)}-to-${rangeEnd.slice(0, 10)}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

// ── Wire-format builder ───────────────────────────────────────────────────────

/** Map up-to-30 LogRows to the flat Form-309_Initial field IDs.
 *
 * Self-review verified: these IDs match the template in
 * src-tauri/src/forms/templates/ics309.rs — time1/from1/to1/sub1 .. time30/..
 * Template also requires header fields: title, activitydatetime1, opname, operid,
 * opper; those are not captured by this form (aggregation-only) and left empty.
 */
function rowsToWireFields(
  rows: LogRow[],
  rangeStart: string,
  rangeEnd: string,
): Record<string, string> {
  const fields: Record<string, string> = {
    title: 'ICS-309 Communications Log',
    activitydatetime1: new Date().toISOString(),
    // opname and operid are identity fields the operator fills in their client
    // profile — not available here. Leave empty per template convention.
    opname: '',
    operid: '',
    opper: `${rangeStart.slice(0, 16)}Z – ${rangeEnd.slice(0, 16)}Z`,
  };

  const MAX_ROWS = 30;
  const slice = rows.slice(0, MAX_ROWS);
  slice.forEach((row, i) => {
    const n = i + 1;
    // time field: HH:MM UTC extracted from the ISO datetime
    const dt = new Date(row.datetime);
    const hh = String(dt.getUTCHours()).padStart(2, '0');
    const mm = String(dt.getUTCMinutes()).padStart(2, '0');
    fields[`time${n}`] = `${hh}:${mm}Z`;
    fields[`from${n}`] = row.from;
    fields[`to${n}`] = row.to;
    fields[`sub${n}`] = `[${row.direction.toUpperCase()}] ${row.subject}`;
  });

  return fields;
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * ICS-309 Comms Log compose form — native messages_meta aggregation.
 *
 * Wire-format contract:
 *   onSubmit emits flat {title, activitydatetime1, opname, operid, opper,
 *   time1, from1, to1, sub1, …, time30, from30, to30, sub30} — the field IDs
 *   that Form-309_Initial's template expects (verified against
 *   src-tauri/src/forms/templates/ics309.rs).
 *
 * Draft contract:
 *   onChange emits {preset, rangeStart, rangeEnd, rows: JSON string} (UI shape)
 *   so autosave stores operator-editable state. On mount, initialValues?.preset,
 *   .rangeStart, .rangeEnd, and .rows rehydrate the inputs.
 *
 * onChange pattern: fired inside input event handlers, NOT from a useEffect dep
 *   array (ICS-213 convention; avoids infinite loop — Compose.tsx's inline arrow
 *   creates a new reference on every render).
 *
 * FormDraftLibrary integration — INTENTIONALLY OMITTED:
 *   ICS-309 has no operator-authored template fields. Every value displayed in
 *   this form (rows, range times) is either derived from the live message store
 *   (`messages_meta_query_for_log`) or is a session-time value (start/end RFC
 *   3339 strings). A slot saved from a prior session would contain yesterday's
 *   date range, which is actively misleading — the operator would need to
 *   change the range immediately anyway. There are no free-text fields (e.g. a
 *   "net name" header) to persist. If a "header preset" field (opname, operid,
 *   net name) is added to this form in a future task, revisit FormDraftLibrary
 *   integration at that time. (bd tuxlink-hnkn P2 Task 4 decision, 2026-06-04)
 */
export function Ics309FormV2({
  initialValues,
  onChange,
  onSubmit,
  onCancel,
}: FormComposeProps) {
  // Rehydrate preset from draft or default to 'today'.
  const initPreset = (initialValues?.preset as Preset | undefined) ?? 'today';
  const initStart  = initialValues?.rangeStart ?? isoRangeForPreset(initPreset).start;
  const initEnd    = initialValues?.rangeEnd   ?? isoRangeForPreset(initPreset).end;

  // Rehydrate rows from draft JSON if present.
  function parseInitRows(): LogRow[] {
    if (!initialValues?.rows) return [];
    try { return JSON.parse(initialValues.rows) as LogRow[]; }
    catch { return []; }
  }

  const [range, setRange] = useState<RangeState>({
    preset: initPreset,
    start: initStart,
    end: initEnd,
  });
  const [rows, setRows] = useState<LogRow[]>(parseInitRows);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pdfLoading, setPdfLoading] = useState(false);
  const [pdfError, setPdfError] = useState<string | null>(null);

  /** Fire onChange with UI-shape payload (not wire format). */
  function notifyChange(newRange: RangeState, newRows: LogRow[]) {
    onChange?.({
      preset: newRange.preset,
      rangeStart: newRange.start,
      rangeEnd: newRange.end,
      rows: JSON.stringify(newRows),
    });
  }

  /** Pick a preset and immediately query. */
  function selectPreset(p: Preset) {
    if (p === 'custom') {
      // Custom: keep current range values, just flip the preset label.
      const next = { ...range, preset: p };
      setRange(next);
      notifyChange(next, rows);
      return;
    }
    const { start, end } = isoRangeForPreset(p);
    const next: RangeState = { preset: p, start, end };
    setRange(next);
    notifyChange(next, rows);
    runQuery(next);
  }

  /** Run the IPC query and update rows state. */
  async function runQuery(r: RangeState) {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<LogRow[]>('messages_meta_query_for_log', {
        startRfc3339: r.start,
        endRfc3339: r.end,
      });
      setRows(result);
      notifyChange(r, result);
    } catch (e) {
      setError(String(e));
      setRows([]);
    } finally {
      setLoading(false);
    }
  }

  /** Download the current preview rows as CSV. */
  function handleCsvDownload() {
    downloadCsv(rows, range.start, range.end);
  }

  /** Request a PDF render from the backend and trigger download. */
  async function handlePdfDownload() {
    setPdfLoading(true);
    setPdfError(null);
    try {
      const pdfBytes = await invoke<number[]>('render_ics309_pdf', {
        req: {
          rows,
          rangeStart: range.start,
          rangeEnd: range.end,
          stationCallsign: null,
        },
      });
      const blob = new Blob([new Uint8Array(pdfBytes)], { type: 'application/pdf' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `ics309-${range.start.slice(0, 10)}.pdf`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setPdfError(String(e));
    } finally {
      setPdfLoading(false);
    }
  }

  /** Send the form — transforms UI state to wire-format field IDs. */
  function handleSend() {
    const wireFields = rowsToWireFields(rows, range.start, range.end);
    onSubmit(wireFields);
  }

  const PRESETS: { id: Preset; label: string }[] = [
    { id: 'last-hour', label: 'Last hour' },
    { id: 'today',     label: 'Today' },
    { id: 'op-period', label: 'Op period' },
    { id: 'custom',    label: 'Custom' },
  ];

  return (
    <div className="ics309-form-v2" data-testid="ics309-form-v2">
      {/* ── Header ─────────────────────────────────────────────────────── */}
      <div className="ics309-form-v2__header">
        <h2>ICS-309 Comms Log</h2>
        <p className="ics309-form-v2__subtitle">
          Aggregates sent + received messages from your mailbox over a time range.
        </p>
      </div>

      {/* ── Time-range presets ─────────────────────────────────────────── */}
      <div className="ics309-form-v2__presets" role="group" aria-label="Time range preset">
        {PRESETS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            className={`ics309-form-v2__preset-btn${range.preset === id ? ' active' : ''}`}
            aria-pressed={range.preset === id}
            onClick={() => selectPreset(id)}
            data-testid={`preset-${id}`}
          >
            {label}
          </button>
        ))}
      </div>

      {/* ── Custom range pickers (visible only when custom is selected) ── */}
      {range.preset === 'custom' && (
        <div className="ics309-form-v2__custom-range">
          <label htmlFor="ics309-range-start">From</label>
          <input
            id="ics309-range-start"
            type="datetime-local"
            value={isoToLocalInput(range.start)}
            onChange={(e) => {
              const next = { ...range, start: localInputToIso(e.target.value) };
              setRange(next);
              notifyChange(next, rows);
            }}
          />
          <label htmlFor="ics309-range-end">To</label>
          <input
            id="ics309-range-end"
            type="datetime-local"
            value={isoToLocalInput(range.end)}
            onChange={(e) => {
              const next = { ...range, end: localInputToIso(e.target.value) };
              setRange(next);
              notifyChange(next, rows);
            }}
          />
          <button
            type="button"
            className="ics309-form-v2__query-btn"
            onClick={() => runQuery(range)}
            data-testid="query-btn"
          >
            Query
          </button>
        </div>
      )}

      {/* ── Preview table ──────────────────────────────────────────────── */}
      <div className="ics309-form-v2__preview">
        {loading && (
          <p className="ics309-form-v2__loading" role="status">
            Loading…
          </p>
        )}
        {error && (
          <p className="ics309-form-v2__error" role="alert">
            {error}
          </p>
        )}
        {!loading && !error && rows.length === 0 && (
          <p className="ics309-form-v2__empty" data-testid="preview-empty">
            Pick a time range to preview messages.
          </p>
        )}
        {!loading && rows.length > 0 && (
          <table
            className="ics309-form-v2__table"
            aria-label="Comms log preview"
            data-testid="preview-table"
          >
            <thead>
              <tr>
                <th>Datetime (UTC)</th>
                <th>Dir</th>
                <th>From</th>
                <th>To</th>
                <th>Subject</th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, 30).map((row, i) => (
                <tr key={i} className={`dir-${row.direction}`}>
                  <td>{row.datetime}</td>
                  <td>{row.direction}</td>
                  <td>{row.from}</td>
                  <td>{row.to}</td>
                  <td>{row.subject}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {rows.length > 30 && (
          <p className="ics309-form-v2__overflow-note" role="note">
            Showing first 30 of {rows.length} messages. The form supports up to 30 log entries.
          </p>
        )}
      </div>

      {/* ── PDF error ──────────────────────────────────────────────────── */}
      {pdfError && (
        <p className="ics309-form-v2__error" role="alert">
          PDF export failed: {pdfError}
        </p>
      )}

      {/* ── Actions ────────────────────────────────────────────────────── */}
      <div className="ics309-form-v2__actions">
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          onClick={handleCsvDownload}
          disabled={rows.length === 0}
          data-testid="csv-download-btn"
        >
          Download CSV
        </button>
        <button
          type="button"
          onClick={handlePdfDownload}
          disabled={rows.length === 0 || pdfLoading}
          data-testid="pdf-download-btn"
        >
          {pdfLoading ? 'Generating PDF…' : 'Download PDF'}
        </button>
        <button
          type="button"
          className="primary"
          onClick={handleSend}
          disabled={rows.length === 0}
          data-testid="send-btn"
        >
          Send
        </button>
      </div>
    </div>
  );
}
