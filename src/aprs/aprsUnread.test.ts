import { describe, it, expect } from 'vitest';
import { countUnread } from './aprsUnread';
import type { ChannelMessage } from './aprsTypes';

const feed = (msgs: Array<{ dir: 'in' | 'out'; at: number }>): ChannelMessage[] =>
  msgs.map((m, i) => ({
    id: `m${i}`,
    direction: m.dir,
    from: m.dir === 'out' ? 'me' : 'W7RPT-9',
    to: null,
    text: 'x',
    kind: 'message',
    msgid: null,
    at: m.at,
  }));

describe('countUnread', () => {
  it('counts inbound messages newer than the seen watermark', () => {
    const messages = feed([{ dir: 'in', at: 100 }, { dir: 'in', at: 300 }, { dir: 'out', at: 400 }]);
    expect(countUnread(messages, 200)).toBe(1); // only the at:300 inbound
  });
  it('ignores outbound messages', () => {
    expect(countUnread(feed([{ dir: 'out', at: 500 }]), 0)).toBe(0);
  });
  it('returns 0 for an empty feed', () => {
    expect(countUnread([], 0)).toBe(0);
  });
});
