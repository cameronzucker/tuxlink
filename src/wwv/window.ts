// Pure scheduling logic for the off-air WWV capture window (Task 16, wwv
// offair spec). WWV's space-weather voice bulletin airs at :18 past the
// hour; WWVH's at :45. The backend's `wwv_offair_refresh` captures
// immediately when invoked (arecord -d 70 now) — it has no notion of the
// broadcast schedule — so the frontend must decide WHEN to fire that call.
// This module is the pure decision function; useWwvOffair.ts wraps it with
// the actual setTimeout.

// WWV space-weather voice is at :18; WWVH at :45. Start capture 5 s early to
// catch the whole ~45 s announcement; run ~70 s (backend dwell).
const WWV_START_S = 18 * 60 - 5; // 1075 s into the hour
const WWVH_START_S = 45 * 60 - 5; // 2695 s
const CAPTURE_SPAN_S = 70;
const HOUR_S = 3600;

export interface NextCapture {
  delayMs: number; // 0 if a window is active right now
  atUnixMs: number; // when capture should fire
  label: string; // "WWV :18" | "WWVH :45"
}

/** When to fire the off-air capture, given `nowMs` (unix ms). If we're already
 * inside a window's capture span, fire now (delayMs 0); otherwise schedule to
 * the nearest upcoming window start (this hour or next). */
export function nextCapture(nowMs: number): NextCapture {
  const nowS = Math.floor(nowMs / 1000);
  const intoHour = ((nowS % HOUR_S) + HOUR_S) % HOUR_S;
  const hourStart = nowS - intoHour;
  const windows: Array<[number, string]> = [
    [WWV_START_S, 'WWV :18'],
    [WWVH_START_S, 'WWVH :45'],
  ];
  // Active now?
  for (const [start, label] of windows) {
    if (intoHour >= start && intoHour < start + CAPTURE_SPAN_S) {
      return { delayMs: 0, atUnixMs: nowMs, label };
    }
  }
  // Next upcoming start (this hour or next).
  let best: { start: number; label: string } | null = null;
  for (const [start, label] of windows) {
    const abs = intoHour < start ? hourStart + start : hourStart + HOUR_S + start;
    if (best === null || abs < best.start) best = { start: abs, label };
  }
  const chosen = best as { start: number; label: string };
  return { delayMs: (chosen.start - nowS) * 1000, atUnixMs: chosen.start * 1000, label: chosen.label };
}
