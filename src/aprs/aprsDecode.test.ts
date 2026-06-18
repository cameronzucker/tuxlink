import { describe, it, expect } from 'vitest';
import { decodeAprsInfo } from './aprsDecode';

// All fixtures are real captures from the operator's Phoenix-area channel
// (2026-06-18). The decoder turns each raw info field into a readable feed
// "monitor" line — APRSIS-32 style — never raw gibberish.
describe('decodeAprsInfo', () => {
  it('decodes a timestamped weather position (symbol _) into a WX summary', () => {
    const d = decodeAprsInfo('@182019z3608.17N/11114.52W_311/004g014t097r000p000P000h12b10174.DsVP');
    expect(d.category).toBe('weather');
    // Human WX fields, present-only, conventional ham units.
    expect(d.summary).toContain('97°F');
    expect(d.summary).toContain('wind 4 mph');
    expect(d.summary).toContain('311°');
    expect(d.summary).toContain('gust 14 mph');
    expect(d.summary).toContain('hum 12%');
    expect(d.summary).toContain('1017.4 hPa');
  });

  it('decodes a digipeater position with an alphanumeric overlay + comment', () => {
    const d = decodeAprsInfo('@182021z3342.06N/11208.91W#North Phoenix WX3in1 #YAESUGANG RF & iGate  K7XYG-10  U=13.5V');
    expect(d.category).toBe('position');
    expect(d.summary).toContain('Digipeater');
    expect(d.summary).toContain('North Phoenix WX3in1');
  });

  it('decodes an igate position with an overlay table char and PHG/comment', () => {
    const d = decodeAprsInfo('!3434.19NI11228.30W&PHG33504/Prescott VHF /A=005555');
    expect(d.category).toBe('position');
    // Overlay 'I' over the alternate '&' igate symbol.
    expect(d.summary.toLowerCase()).toContain('igate');
    expect(d.summary).toContain('Prescott VHF');
  });

  it('decodes a timestamped vehicle position, stripping leading course/speed', () => {
    const d = decodeAprsInfo("/202142h3332.37N/11201.82Wk241/000Jon's Mobile Lab");
    expect(d.category).toBe('position');
    expect(d.summary).toContain("Jon's Mobile Lab");
    // The 241/000 course/speed must not leak into the readable comment.
    expect(d.summary).not.toContain('241/000');
  });

  it('decodes a position with /A= altitude in the comment', () => {
    const d = decodeAprsInfo('!3413.52N/11118.79Wk357/000/A=004993ARA Rimlink System');
    expect(d.category).toBe('position');
    expect(d.summary).toContain('ARA Rimlink System');
  });

  it('labels a Mic-E position report', () => {
    const d = decodeAprsInfo('`(0<l E>/\'"7J}MT-RTG|$A%f\'s|!w7O!|3');
    expect(d.category).toBe('mice');
    expect(d.summary.toLowerCase()).toContain('position');
    // The compressed Mic-E telemetry blob must NOT appear verbatim.
    expect(d.summary).not.toContain('|$A%');
  });

  it("labels the old-data Mic-E DTI (') as a Mic-E position too", () => {
    const d = decodeAprsInfo("''Dbl -/]=");
    expect(d.category).toBe('mice');
  });

  it('decodes a status report (>) into its text', () => {
    const d = decodeAprsInfo('>White Tanks VHF Digi, Phoenix, AZ n0rmz@arrl.net');
    expect(d.category).toBe('status');
    expect(d.summary).toContain('White Tanks VHF Digi');
    expect(d.summary).not.toMatch(/^>/);
  });

  it('decodes a telemetry data report (T#) into named-or-positional values', () => {
    const d = decodeAprsInfo('T#005,200,125,0,0,0,00000000');
    expect(d.category).toBe('telemetry');
    expect(d.summary).toContain('seq 5');
  });

  it('decodes an object report with its name and live state', () => {
    const d = decodeAprsInfo(';LEADER   *092345z4903.50N/07201.75W>Leader car');
    expect(d.category).toBe('object');
    expect(d.summary).toContain('LEADER');
  });

  it('decodes an item report with its name', () => {
    const d = decodeAprsInfo(')AID!3603.50N/11201.75WAAid station');
    expect(d.category).toBe('item');
    expect(d.summary).toContain('AID');
  });

  it('falls back to a trimmed raw string for an unrecognized frame', () => {
    const d = decodeAprsInfo('<some weird thing');
    expect(d.category).toBe('unknown');
    expect(d.summary).toBe('<some weird thing');
  });

  it('never returns an empty summary', () => {
    for (const s of ['', '   ', '!', '@', '`', '>', 'T#']) {
      expect(decodeAprsInfo(s).summary.length).toBeGreaterThan(0);
    }
  });
});
