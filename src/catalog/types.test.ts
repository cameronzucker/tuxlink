import { describe, it, expect } from 'vitest';
import { groupByCategory, type CatalogEntry } from './types';

function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}

describe('groupByCategory', () => {
  it('returns an empty tree for an empty list', () => {
    const tree = groupByCategory([]);
    expect(tree.totalCount).toBe(0);
    expect(tree.categories.size).toBe(0);
  });

  it('groups entries under their category', () => {
    const tree = groupByCategory([
      entry('WL2K_RMS', 'PUB_PACKET'),
      entry('WL2K_RMS', 'PUB_VARA'),
      entry('PROPAGATION', 'PROP_WWV'),
    ]);
    expect(tree.totalCount).toBe(3);
    expect(tree.categories.get('WL2K_RMS')?.map((e) => e.filename)).toEqual(['PUB_PACKET', 'PUB_VARA']);
    expect(tree.categories.get('PROPAGATION')?.map((e) => e.filename)).toEqual(['PROP_WWV']);
  });

  it('preserves the source ordering of categories', () => {
    const tree = groupByCategory([
      entry('CAT_A', 'X'),
      entry('CAT_B', 'Y'),
      entry('CAT_A', 'Z'),
    ]);
    expect(Array.from(tree.categories.keys())).toEqual(['CAT_A', 'CAT_B']);
  });

  it('preserves entry order within a category', () => {
    const tree = groupByCategory([
      entry('CAT', 'first'),
      entry('CAT', 'second'),
      entry('CAT', 'third'),
    ]);
    expect(tree.categories.get('CAT')?.map((e) => e.filename)).toEqual(['first', 'second', 'third']);
  });
});
