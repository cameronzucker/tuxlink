import { describe, it, expect } from 'vitest';
import { MENU_TREE, type MenuNode } from '../shell/chrome/menuModel';
import { menuAnchorId, MENU_POINT_AT_ENTRIES } from './menuAnchors';

/** Independent walk of MENU_TREE (deliberately NOT reusing menuAnchors'
 *  internals) so a future menuModel change is caught here without the test
 *  and the implementation sharing the same blind spot. */
function countNonSeparatorItems(nodes: MenuNode[]): number {
  let count = 0;
  for (const node of nodes) {
    if (node.separator) continue;
    if (node.id) count += 1;
    if (node.submenu) count += countNonSeparatorItems(node.submenu);
  }
  return count;
}

function collectItemIds(nodes: MenuNode[]): string[] {
  const out: string[] = [];
  for (const node of nodes) {
    if (node.separator) continue;
    if (node.id) out.push(node.id);
    if (node.submenu) out.push(...collectItemIds(node.submenu));
  }
  return out;
}

describe('menuAnchorId', () => {
  it('slugifies the top-level label into menu:<slug>', () => {
    expect(menuAnchorId('Tools')).toBe('menu:tools');
    expect(menuAnchorId('Help')).toBe('menu:help');
    expect(menuAnchorId('Color scheme')).toBe('menu:color-scheme');
  });
});

describe('MENU_POINT_AT_ENTRIES', () => {
  it('has one entry per top-level menu plus one per non-separator MENU_TREE item (live count, catches future menu changes)', () => {
    const expectedItemCount = MENU_TREE.reduce(
      (sum, menu) => sum + countNonSeparatorItems(menu.items),
      0,
    );
    const expectedTotal = MENU_TREE.length + expectedItemCount;
    expect(MENU_POINT_AT_ENTRIES.length).toBe(expectedTotal);
  });

  it('every non-separator MENU_TREE item id is represented as an anchor verbatim', () => {
    const allItemIds = MENU_TREE.flatMap((m) => collectItemIds(m.items));
    const anchorIds = new Set(MENU_POINT_AT_ENTRIES.map((e) => e.anchor));
    for (const id of allItemIds) {
      expect(anchorIds.has(id)).toBe(true);
    }
  });

  it('every top-level menu is represented under menu:<slug>', () => {
    for (const menu of MENU_TREE) {
      const anchor = menuAnchorId(menu.label);
      const entry = MENU_POINT_AT_ENTRIES.find((e) => e.id === anchor);
      expect(entry).toBeDefined();
      expect(entry?.anchor).toBe(anchor);
      expect(entry?.title).toBe(menu.label);
      expect(entry?.openHint).toMatch(/menu bar/);
    }
  });

  it('ids are unique across the derived set', () => {
    const ids = MENU_POINT_AT_ENTRIES.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('no entry id contains "*"', () => {
    MENU_POINT_AT_ENTRIES.forEach((entry) => {
      expect(entry.id).not.toContain('*');
    });
  });

  it('every entry has a non-empty openHint', () => {
    MENU_POINT_AT_ENTRIES.forEach((entry) => {
      expect(entry.openHint).toBeTruthy();
      expect(entry.openHint!.length).toBeGreaterThan(0);
    });
  });

  it('separators are skipped (no anchor for a separator)', () => {
    const anchorIds = new Set(MENU_POINT_AT_ENTRIES.map((e) => e.anchor));
    expect(anchorIds.has('')).toBe(false);
    expect(anchorIds.has(undefined as unknown as string)).toBe(false);
  });

  it('a menu-item anchor carries an "In the <parent> menu." body distinct from the top-level entry', () => {
    const replayTour = MENU_POINT_AT_ENTRIES.find((e) => e.id === 'menu:help:replay_tour');
    expect(replayTour).toBeDefined();
    expect(replayTour?.title).toBe('Replay tour');
    expect(replayTour?.body).toBe('In the Help menu.');
    expect(replayTour?.openHint).toBe(
      'Open the Help menu first — this entry lives inside it.',
    );
    expect(replayTour?.fallback).toBe('center');
  });
});
