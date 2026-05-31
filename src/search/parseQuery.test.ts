import { describe, it, expect } from 'vitest';
import { parseQuery, deparseQuery } from './parseQuery';
import { EMPTY_SPEC } from './types';

describe('parseQuery', () => {
  it('returns EMPTY_SPEC for an empty string', () => {
    expect(parseQuery('')).toEqual(EMPTY_SPEC);
  });

  it('bare tokens become free_text', () => {
    expect(parseQuery('damage report').free_text).toBe('damage report');
    expect(parseQuery('damage report').filters).toEqual({});
  });

  it('from:ADDR becomes a filter and is stripped from free_text', () => {
    const s = parseQuery('from:KX5DD damage');
    expect(s.free_text).toBe('damage');
    expect(s.filters.from).toEqual({ kind: 'addr', value: 'KX5DD' });
  });

  it('multiple operators all parse', () => {
    const s = parseQuery('from:KX5DD to:N7CPZ form:ICS-213 is:unread damage');
    expect(s.free_text).toBe('damage');
    expect(s.filters.from).toEqual({ kind: 'addr', value: 'KX5DD' });
    expect(s.filters.to).toEqual({ kind: 'addr', value: 'N7CPZ' });
    expect(s.filters['form-type']).toEqual({ kind: 'form-type', value: 'ICS-213' });
    expect(s.filters['read-state']).toEqual({ kind: 'read-state', value: 'unread' });
  });

  it('has:attach maps to bool true on has-attach', () => {
    const s = parseQuery('has:attach storm');
    expect(s.filters['has-attach']).toEqual({ kind: 'bool', value: true });
    expect(s.free_text).toBe('storm');
  });

  it('date:7d builds a 7-day range starting from "from"', () => {
    const s = parseQuery('date:7d');
    expect(s.filters['date-range']?.kind).toBe('date-range');
    const v = s.filters['date-range']?.kind === 'date-range' ? s.filters['date-range'].value : null;
    expect(v?.from).toBeDefined();
    expect(v?.to).toBeNull();
  });

  it('unknown operators fall through to free_text', () => {
    const s = parseQuery('xyz:foo damage');
    expect(s.free_text).toBe('xyz:foo damage');
    expect(s.filters).toEqual({});
  });

  it('a token with trailing colon is free_text', () => {
    expect(parseQuery('from: damage').free_text).toBe('from: damage');
  });

  it('case-insensitive on operator key', () => {
    expect(parseQuery('FROM:foo').filters.from).toEqual({ kind: 'addr', value: 'foo' });
  });
});

describe('deparseQuery', () => {
  it('empty spec → empty string', () => {
    expect(deparseQuery(EMPTY_SPEC)).toBe('');
  });

  it('free_text only', () => {
    expect(deparseQuery({ ...EMPTY_SPEC, free_text: 'damage' })).toBe('damage');
  });

  it('round-trips from:ADDR damage', () => {
    const s = parseQuery('from:KX5DD damage');
    expect(deparseQuery(s)).toBe('damage from:KX5DD');
  });

  it('round-trips has:attach', () => {
    const s = parseQuery('has:attach');
    expect(deparseQuery(s)).toBe('has:attach');
  });

  it('round-trips is:unread', () => {
    const s = parseQuery('is:unread');
    expect(deparseQuery(s)).toBe('is:unread');
  });
});
