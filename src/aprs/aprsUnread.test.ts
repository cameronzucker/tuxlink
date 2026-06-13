import { describe, it, expect } from 'vitest';
import { countUnread } from './aprsUnread';
import type { Thread } from './aprsTypes';

const t = (msgs: Array<{ dir: 'in' | 'out'; at: number }>): Thread => ({
  callsign: 'W7RPT-9',
  messages: msgs.map((m, i) => ({ id: `m${i}`, direction: m.dir, text: 'x', msgid: null, at: m.at })),
});

describe('countUnread', () => {
  it('counts inbound messages newer than the seen watermark', () => {
    const threads = { 'W7RPT-9': t([{ dir: 'in', at: 100 }, { dir: 'in', at: 300 }, { dir: 'out', at: 400 }]) };
    expect(countUnread(threads, 200)).toBe(1); // only the at:300 inbound
  });
  it('ignores outbound messages', () => {
    const threads = { A: t([{ dir: 'out', at: 500 }]) };
    expect(countUnread(threads, 0)).toBe(0);
  });
  it('returns 0 for an empty thread map', () => {
    expect(countUnread({}, 0)).toBe(0);
  });
});
