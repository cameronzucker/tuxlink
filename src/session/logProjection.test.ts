/**
 * Tests for logProjection.ts — the prime unit-test target for Task 15.
 * Pure function; spec §6 Task 15 + spec §5.5.
 *
 * Rules under test:
 * - Human projection keeps lines that are:
 *   (a) `***`-annotated (message contains "***")
 *   (b) source === 'backend' or source === 'transport'
 *   Suppresses: source === 'wire', or message starts/contains raw B2F tokens
 *   (;PQ, ;PR, [WL2K-...], ;FW, FF, FQ)
 * - Raw projection keeps everything.
 * - Both read the same LogLineDto[] input (no dual streams).
 * - Empty input → empty output.
 * - Summary line derived per session (Human only).
 * - LogLineDto level/source enums round-trip through the projection.
 */

import { describe, it, expect } from 'vitest';
import {
  humanProjection,
  rawProjection,
  type LogLineDto,
} from './logProjection';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const annotated: LogLineDto = {
  seq: 1,
  timestampIso: '2026-05-19T12:00:00Z',
  level: 'info',
  source: 'wire',
  message: '*** Session started — CMS connected',
};

const backendInfo: LogLineDto = {
  seq: 2,
  timestampIso: '2026-05-19T12:00:01Z',
  level: 'info',
  source: 'backend',
  message: 'Pat process started (pid 12345)',
};

const transportDebug: LogLineDto = {
  seq: 3,
  timestampIso: '2026-05-19T12:00:02Z',
  level: 'debug',
  source: 'transport',
  message: 'Connected to cms-ssl.winlink.org:8772',
};

const wirePQ: LogLineDto = {
  seq: 4,
  timestampIso: '2026-05-19T12:00:03Z',
  level: 'debug',
  source: 'wire',
  message: ';PQ: WL2K AUTH REQUIRED',
};

const wirePR: LogLineDto = {
  seq: 5,
  timestampIso: '2026-05-19T12:00:04Z',
  level: 'debug',
  source: 'wire',
  message: ';PR: some challenge response',
};

const wireWL2K: LogLineDto = {
  seq: 6,
  timestampIso: '2026-05-19T12:00:05Z',
  level: 'debug',
  source: 'wire',
  message: '[WL2K-5.1.3.8-B2FWIHJM$]',
};

const wireFW: LogLineDto = {
  seq: 7,
  timestampIso: '2026-05-19T12:00:06Z',
  level: 'debug',
  source: 'wire',
  message: ';FW: F6KGL KC9MHZ',
};

const wireFF: LogLineDto = {
  seq: 8,
  timestampIso: '2026-05-19T12:00:07Z',
  level: 'debug',
  source: 'wire',
  message: 'FF',
};

const wireFQ: LogLineDto = {
  seq: 9,
  timestampIso: '2026-05-19T12:00:08Z',
  level: 'debug',
  source: 'wire',
  message: 'FQ',
};

const wirePlain: LogLineDto = {
  seq: 10,
  timestampIso: '2026-05-19T12:00:09Z',
  level: 'debug',
  source: 'wire',
  message: 'Some plain wire line',
};

const backendWarn: LogLineDto = {
  seq: 11,
  timestampIso: '2026-05-19T12:00:10Z',
  level: 'warn',
  source: 'backend',
  message: 'Retrying connection (attempt 2)',
};

const allLines: LogLineDto[] = [
  annotated,
  backendInfo,
  transportDebug,
  wirePQ,
  wirePR,
  wireWL2K,
  wireFW,
  wireFF,
  wireFQ,
  wirePlain,
  backendWarn,
];

// ---------------------------------------------------------------------------
// Test 1: Human projection keeps *** annotated + Backend/Transport lines,
//         drops Wire / ;PQ / ;PR / FF / FQ / ;FW / [WL2K-...]
// ---------------------------------------------------------------------------

describe('humanProjection', () => {
  it('keeps ***-annotated lines regardless of source', () => {
    const out = humanProjection([annotated]);
    // annotated is source='wire' but has ***, so it must survive
    expect(out).toContain(annotated);
  });

  it('keeps backend-source lines', () => {
    const out = humanProjection([backendInfo]);
    expect(out).toContain(backendInfo);
  });

  it('keeps transport-source lines', () => {
    const out = humanProjection([transportDebug]);
    expect(out).toContain(transportDebug);
  });

  it('drops wire-source ;PQ lines', () => {
    const out = humanProjection([wirePQ]);
    expect(out).not.toContain(wirePQ);
  });

  it('drops wire-source ;PR lines', () => {
    const out = humanProjection([wirePR]);
    expect(out).not.toContain(wirePR);
  });

  it('drops wire-source [WL2K-...] lines', () => {
    const out = humanProjection([wireWL2K]);
    expect(out).not.toContain(wireWL2K);
  });

  it('drops wire-source ;FW lines', () => {
    const out = humanProjection([wireFW]);
    expect(out).not.toContain(wireFW);
  });

  it('drops wire-source FF lines', () => {
    const out = humanProjection([wireFF]);
    expect(out).not.toContain(wireFF);
  });

  it('drops wire-source FQ lines', () => {
    const out = humanProjection([wireFQ]);
    expect(out).not.toContain(wireFQ);
  });

  it('drops plain wire lines (source=wire, no ***)', () => {
    const out = humanProjection([wirePlain]);
    expect(out).not.toContain(wirePlain);
  });

  it('keeps backend warn lines', () => {
    const out = humanProjection([backendWarn]);
    expect(out).toContain(backendWarn);
  });

  it('keeps only the right lines from a mixed array', () => {
    const out = humanProjection(allLines);
    // Must include
    expect(out).toContain(annotated);
    expect(out).toContain(backendInfo);
    expect(out).toContain(transportDebug);
    expect(out).toContain(backendWarn);
    // Must exclude all wire-only (non-annotated) lines
    expect(out).not.toContain(wirePQ);
    expect(out).not.toContain(wirePR);
    expect(out).not.toContain(wireWL2K);
    expect(out).not.toContain(wireFW);
    expect(out).not.toContain(wireFF);
    expect(out).not.toContain(wireFQ);
    expect(out).not.toContain(wirePlain);
  });
});

// ---------------------------------------------------------------------------
// Test 2: Raw projection keeps everything
// ---------------------------------------------------------------------------

describe('rawProjection', () => {
  it('returns all lines unchanged', () => {
    const out = rawProjection(allLines);
    expect(out).toHaveLength(allLines.length);
    for (const line of allLines) {
      expect(out).toContain(line);
    }
  });

  it('preserves wire B2F lines that Human drops', () => {
    const out = rawProjection([wirePQ, wirePR, wireWL2K, wireFW, wireFF, wireFQ]);
    expect(out).toHaveLength(6);
  });
});

// ---------------------------------------------------------------------------
// Test 3: Both read the same input array (no dual stream)
// ---------------------------------------------------------------------------

describe('projection purity — same input', () => {
  it('humanProjection and rawProjection do not mutate the input array', () => {
    const input = [...allLines];
    const inputSnapshot = [...input];
    humanProjection(input);
    rawProjection(input);
    expect(input).toEqual(inputSnapshot);
  });

  it('humanProjection and rawProjection both accept the same LogLineDto[]', () => {
    const input: LogLineDto[] = [backendInfo, transportDebug];
    const human = humanProjection(input);
    const raw = rawProjection(input);
    // raw >= human (human is a strict subset)
    expect(raw.length).toBeGreaterThanOrEqual(human.length);
    // every human line also appears in raw
    for (const line of human) {
      expect(raw).toContain(line);
    }
  });
});

// ---------------------------------------------------------------------------
// Test 4: Empty input → empty output
// ---------------------------------------------------------------------------

describe('empty input', () => {
  it('humanProjection returns [] for empty input', () => {
    expect(humanProjection([])).toEqual([]);
  });

  it('rawProjection returns [] for empty input', () => {
    expect(rawProjection([])).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Test 5: Summary line derived per session (Human projection)
// ---------------------------------------------------------------------------

describe('session summary', () => {
  it('Human projection produces a summary line for a session that had B2F traffic', () => {
    // Lines that represent a complete session: annotated open + B2F + close
    const sessionLines: LogLineDto[] = [
      { seq: 100, timestampIso: '2026-05-19T12:00:00Z', level: 'info', source: 'backend', message: '*** Session started' },
      wirePQ,
      wirePR,
      wireWL2K,
      wireFW,
      wireFF,
      { seq: 101, timestampIso: '2026-05-19T12:01:00Z', level: 'info', source: 'backend', message: '*** Session ended' },
    ];
    const out = humanProjection(sessionLines);
    // The summary must be present (one line with the session summary info)
    const hasSummary = out.some(l => l.source === 'backend' && l.message.includes('summary'));
    expect(hasSummary).toBe(true);
  });

  it('Human projection summary contains the session B2F line count', () => {
    const sessionLines: LogLineDto[] = [
      { seq: 200, timestampIso: '2026-05-19T12:00:00Z', level: 'info', source: 'backend', message: '*** Session started' },
      wirePQ,
      wirePR,
      wireWL2K,
      { seq: 201, timestampIso: '2026-05-19T12:01:00Z', level: 'info', source: 'backend', message: '*** Session ended' },
    ];
    const out = humanProjection(sessionLines);
    const summaryLine = out.find(l => l.message.includes('summary'));
    expect(summaryLine).toBeDefined();
    // Summary mentions the number of suppressed wire lines (3 in this case)
    expect(summaryLine!.message).toMatch(/3/);
  });
});

// ---------------------------------------------------------------------------
// Test 6: LogLineDto level/source enums round-trip
// ---------------------------------------------------------------------------

describe('LogLineDto type validity', () => {
  it('all valid level values are accepted', () => {
    const levels: LogLineDto['level'][] = ['trace', 'debug', 'info', 'warn', 'error'];
    levels.forEach((level, idx) => {
      const line: LogLineDto = {
        seq: idx + 1,
        timestampIso: '2026-05-19T00:00:00Z',
        level,
        source: 'backend',
        message: `test ${level}`,
      };
      // rawProjection must pass it through unchanged
      const out = rawProjection([line]);
      expect(out[0].level).toBe(level);
    });
  });

  it('all valid source values are accepted', () => {
    const sources: LogLineDto['source'][] = ['backend', 'transport', 'wire'];
    sources.forEach((source, idx) => {
      const line: LogLineDto = {
        seq: idx + 1,
        timestampIso: '2026-05-19T00:00:00Z',
        level: 'info',
        source,
        message: `test ${source}`,
      };
      const out = rawProjection([line]);
      expect(out[0].source).toBe(source);
    });
  });
});

// ---------------------------------------------------------------------------
// Task 13: Packet/AX.25 session-log projection contract
// ---------------------------------------------------------------------------

describe('logProjection — packet/AX.25 lines', () => {
  const inbound: LogLineDto = {
    seq: 100, timestampIso: '2026-05-22T14:28:31Z', level: 'info', source: 'transport',
    message: '◀ Inbound packet call from W7AUX-10 — 1 message received',
  };
  const outbound: LogLineDto = {
    seq: 101, timestampIso: '2026-05-22T13:48:10Z', level: 'info', source: 'transport',
    message: '▶ Connected W7AUX-10 via W7RPT-1 — 2 sent, 0 received',
  };
  const linkEvt: LogLineDto = {
    seq: 102, timestampIso: '2026-05-22T13:47:55Z', level: 'info', source: 'transport',
    message: 'Link established (SABM → UA), path N7CPZ-7 → W7RPT-1 → W7AUX-10',
  };
  const ax25Raw: LogLineDto = {
    seq: 103, timestampIso: '2026-05-22T13:47:55Z', level: 'debug', source: 'wire',
    message: 'KISS C0 00 ... AX.25 SABM C/R=1 N(S)=0 N(R)=0',
  };

  it('Human keeps shaped transport lines (◀ inbound, ▶ outbound, SABM→UA)', () => {
    const out = humanProjection([inbound, outbound, linkEvt]);
    const text = out.map((l) => l.message).join('\n');
    expect(text).toContain('◀ Inbound packet call from W7AUX-10');
    expect(text).toContain('▶ Connected W7AUX-10 via W7RPT-1');
    expect(text).toContain('Link established (SABM → UA)');
  });

  it('Human suppresses raw AX.25/KISS wire frame dumps', () => {
    const out = humanProjection([linkEvt, ax25Raw]);
    const text = out.map((l) => l.message).join('\n');
    expect(text).not.toContain('AX.25 SABM C/R=1');
  });

  it('Raw reveals the AX.25/KISS frame detail', () => {
    const out = rawProjection([linkEvt, ax25Raw]);
    const text = out.map((l) => l.message).join('\n');
    expect(text).toContain('AX.25 SABM C/R=1');
  });
});
