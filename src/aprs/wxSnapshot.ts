// src/aprs/wxSnapshot.ts
//
// Pure header text for the weather map snapshot (ni5b). The canvas compositing +
// PNG download is the imperative shell; this builds the burned-in header so it is
// unit-testable. Honest: the grid segment is omitted when no operator grid is set.

export function composeSnapshotHeader(meta: { grid?: string; utcMs: number; stationCount: number }): string {
  const d = new Date(meta.utcMs);
  const hh = String(d.getUTCHours()).padStart(2, '0');
  const mm = String(d.getUTCMinutes()).padStart(2, '0');
  const time = `${hh}${mm}Z`;
  const parts = ['Local WX'];
  if (meta.grid) parts.push(`grid ${meta.grid}`);
  parts.push(time);
  parts.push(`${meta.stationCount} stn`);
  return parts.join(' · ');
}
