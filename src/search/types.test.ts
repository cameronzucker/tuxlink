import { describe, it, expect } from 'vitest';
import type {
  QuerySpec, FilterKey as _FilterKey, FilterValue as _FilterValue,
  ReadState as _ReadState, SortOrder as _SortOrder, PageRequest as _PageRequest,
  SavedSearch, RecentSearch as _RecentSearch, SearchResults as _SearchResults,
  MessageMetaDto as _MessageMetaDto, RebuildStats as _RebuildStats,
} from './types';

describe('search/types', () => {
  it('compose a QuerySpec literal that matches the Rust serde shape', () => {
    const spec: QuerySpec = {
      free_text: 'damage',
      filters: {
        from: { kind: 'addr', value: 'KX5DD' },
        'form-type': { kind: 'form-type', value: 'ICS-213' },
        'date-range': { kind: 'date-range', value: { from: 1_700_000_000, to: null } },
      },
      sort: 'date_desc',
      page: { page_size: 200, offset: 0 },
    };
    expect(spec.free_text).toBe('damage');
    expect(spec.filters.from?.kind).toBe('addr');
  });

  it('SavedSearch has required fields', () => {
    const s: SavedSearch = {
      id: 'uuid',
      name: 'Storm Net',
      spec: { free_text: null, filters: {}, sort: 'date_desc', page: { page_size: 200, offset: 0 } },
      created_at: 1,
      last_used_at: null,
      order: 0,
    };
    expect(s.name).toBe('Storm Net');
  });
});
