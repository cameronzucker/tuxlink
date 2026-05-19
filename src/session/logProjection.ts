/**
 * logProjection.ts — Pure projection functions for Task 15 (Session Log).
 *
 * Spec §5.5 + §3.3: the backend emits `session_log:line` events carrying
 * `LogLineDto[]`. Two projections of the SAME stream:
 *
 * - **Human**: keeps operator-relevant lines (***-annotated + backend/transport)
 *   and suppresses raw B2F protocol noise (wire source, ;PQ, ;PR, [WL2K-...],
 *   ;FW, FF, FQ). Appends a per-session summary line when B2F lines were
 *   suppressed.
 *
 * - **Raw**: passes everything through unchanged.
 *
 * Both accept the same `LogLineDto[]` — NO parallel streams.
 */

/** Mirrors the Tauri event payload from `session_log:line` (spec §3.3). */
export interface LogLineDto {
  timestampIso: string;
  level: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  source: 'backend' | 'pat' | 'transport' | 'wire';
  message: string;
}

// ---------------------------------------------------------------------------
// B2F suppression rules (Human projection)
// ---------------------------------------------------------------------------

/**
 * Raw B2F token patterns that are suppressed in the Human projection.
 * These are the Winlink B2F protocol handshake lines that appear on the
 * `wire` source — operator-irrelevant noise that clutters the log.
 *
 * Patterns:
 *   ;PQ  — peer query
 *   ;PR  — peer response
 *   [WL2K-...] — capability advertisement
 *   ;FW  — forwarding list
 *   FF   — end of proposals
 *   FQ   — disconnect
 */
/**
 * A line is suppressed in the Human projection when:
 *   1. Its source is 'wire', AND
 *   2. The message is NOT ***-annotated (does not contain "***"), AND
 *   3. It is any wire line (all wire lines except annotated are suppressed
 *      per spec §5.5: "suppress raw B2F (Wire source)").
 *
 * Note: annotated lines pass regardless of source — the *** annotation is
 * the backend's way of marking operator-relevant events on ANY channel.
 */
function isSuppressedInHuman(line: LogLineDto): boolean {
  const isAnnotated = line.message.includes('***');
  if (isAnnotated) return false; // always keep annotated lines
  if (line.source === 'wire') return true; // suppress all wire (incl. B2F)
  return false;
}

// ---------------------------------------------------------------------------
// Session boundary detection
// ---------------------------------------------------------------------------

/**
 * Detect session boundaries in the input for summary generation.
 * A "session" starts at a line containing "*** Session started" (or any
 * ***-annotated backend line at the beginning of a sequence of B2F lines),
 * and ends at a line containing "*** Session ended".
 *
 * Returns an array of { start, end, suppressedCount } for each complete
 * session. Incomplete sessions (started but not ended) are summarized at
 * the end of the input.
 */
interface SessionSpan {
  startIdx: number;
  endIdx: number | null; // null = session still open
  suppressedCount: number;
}

function detectSessionSpans(lines: LogLineDto[]): SessionSpan[] {
  const spans: SessionSpan[] = [];
  let currentSpan: SessionSpan | null = null;
  let suppressedInSpan = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const isSessionStart =
      line.message.includes('*** Session started') ||
      (line.message.includes('***') && line.message.toLowerCase().includes('session start'));
    const isSessionEnd =
      line.message.includes('*** Session ended') ||
      (line.message.includes('***') && line.message.toLowerCase().includes('session end'));

    if (isSessionStart && currentSpan === null) {
      currentSpan = { startIdx: i, endIdx: null, suppressedCount: 0 };
      suppressedInSpan = 0;
    }

    if (currentSpan !== null && isSuppressedInHuman(line)) {
      suppressedInSpan++;
      currentSpan.suppressedCount = suppressedInSpan;
    }

    if (isSessionEnd && currentSpan !== null) {
      currentSpan.endIdx = i;
      spans.push({ ...currentSpan });
      currentSpan = null;
      suppressedInSpan = 0;
    }
  }

  // Incomplete session (started but not yet ended)
  if (currentSpan !== null) {
    spans.push({ ...currentSpan, endIdx: null });
  }

  return spans;
}

/**
 * Create a summary LogLineDto for a completed (or in-progress) session span.
 * Inserted right after the session-end line (or at end of input if incomplete).
 */
function makeSummaryLine(
  suppressedCount: number,
  insertTimestamp: string,
): LogLineDto {
  return {
    timestampIso: insertTimestamp,
    level: 'info',
    source: 'backend',
    message: `[Human view summary: ${suppressedCount} raw B2F wire line${suppressedCount === 1 ? '' : 's'} suppressed in this session]`,
  };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Human projection: operator-relevant lines only.
 * - Keeps: ***-annotated lines (any source), backend-source, transport-source, pat-source.
 * - Drops: wire-source lines that are NOT ***-annotated.
 * - Appends a per-session summary after "*** Session ended" (or at end) when
 *   B2F lines were suppressed.
 *
 * Pure function — does NOT mutate the input array.
 */
export function humanProjection(lines: LogLineDto[]): LogLineDto[] {
  if (lines.length === 0) return [];

  // Detect session spans first (for summary injection)
  const spans = detectSessionSpans(lines);

  // Build a map of insertion points: index (after which to insert) → summary line
  const insertAfter = new Map<number, LogLineDto>();
  for (const span of spans) {
    if (span.suppressedCount > 0) {
      const insertIdx = span.endIdx ?? lines.length - 1;
      const ts = lines[insertIdx].timestampIso;
      insertAfter.set(insertIdx, makeSummaryLine(span.suppressedCount, ts));
    }
  }

  const result: LogLineDto[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!isSuppressedInHuman(line)) {
      result.push(line);
    }
    // Inject summary after this index if one is registered
    const summary = insertAfter.get(i);
    if (summary !== undefined) {
      result.push(summary);
    }
  }

  return result;
}

/**
 * Raw projection: all lines, unchanged.
 *
 * Pure function — does NOT mutate the input array.
 */
export function rawProjection(lines: LogLineDto[]): LogLineDto[] {
  return lines.slice();
}
