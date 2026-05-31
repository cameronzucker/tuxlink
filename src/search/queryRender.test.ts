import { describe, it, expect } from 'vitest';
import { renderQuery } from './queryRender';
import { EMPTY_SPEC } from './types';

describe('renderQuery', () => {
  it('returns the free text when no filters', () => {
    expect(renderQuery({ ...EMPTY_SPEC, free_text: 'damage' })).toBe('damage');
  });

  it('renders chips after free text', () => {
    expect(renderQuery({
      ...EMPTY_SPEC,
      free_text: 'damage',
      filters: {
        from: { kind: 'addr', value: 'KX5DD' },
        'date-range': { kind: 'date-range', value: { from: 1_700_000_000, to: null } },
      },
    })).toBe('damage from:KX5DD date:from-1700000000');
  });

  it('renders only chips when no free text', () => {
    expect(renderQuery({
      ...EMPTY_SPEC,
      filters: { from: { kind: 'addr', value: 'KX5DD' } },
    })).toBe('from:KX5DD');
  });

  it('returns "(empty)" for a totally empty spec', () => {
    expect(renderQuery(EMPTY_SPEC)).toBe('(empty)');
  });
});
