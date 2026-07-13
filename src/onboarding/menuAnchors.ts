// Point-at-only anchors for menu chrome (tuxlink-10bkw Task 9).
//
// Elmer's point_at tool needs to be able to highlight menu chrome — the
// top-level menu buttons (File, Tools, Help, …) AND the items inside an open
// menu (Tools → Settings, Help → Replay tour, …) — not just whole panels.
// These entries are derived PROGRAMMATICALLY from MENU_TREE (the single
// source of truth for menu content, src/shell/chrome/menuModel.ts) so that a
// future menu change is automatically covered here with no manual upkeep.
//
// These are point-at-only: they exist so Elmer/MCP can point_at them, but
// they are never scheduled as first-open discretionary tips (see HINTS /
// useFirstOpenTip call sites, which all pass literal HINTS ids).

import { MENU_TREE, type MenuNode } from '../shell/chrome/menuModel';
import type { HintEntry } from './types';

function slugify(label: string): string {
  return label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/** 'menu:<label-slugified>' anchor id for a top-level menu, e.g. 'menu:tools'. */
export function menuAnchorId(menuLabel: string): string {
  return `menu:${slugify(menuLabel)}`;
}

/** Depth-first walk of a menu's items (including nested submenus), skipping
 *  separators and pure submenu-parent nodes (they carry no `id` to anchor). */
function collectItemEntries(nodes: MenuNode[], parentLabel: string, out: HintEntry[]): void {
  for (const node of nodes) {
    if (node.separator) continue;
    if (node.id) {
      const title = node.label ?? node.id;
      out.push({
        id: node.id,
        anchor: node.id,
        title,
        body: `In the ${parentLabel} menu.`,
        openHint: `Open the ${parentLabel} menu first — this entry lives inside it.`,
        fallback: 'center',
      });
    }
    if (node.submenu) {
      collectItemEntries(node.submenu, parentLabel, out);
    }
  }
}

/** One entry per top-level menu + one per non-separator menu item, derived
 *  from MENU_TREE. Item entries reuse the item's existing MenuActionId
 *  verbatim as the anchor, matching the data-tour-anchor MenuBar stamps on
 *  the rendered element while its parent menu is open. */
export const MENU_POINT_AT_ENTRIES: HintEntry[] = MENU_TREE.flatMap((menu) => {
  const anchor = menuAnchorId(menu.label);
  const topEntry: HintEntry = {
    id: anchor,
    anchor,
    title: menu.label,
    body: `The ${menu.label} menu.`,
    openHint: `The ${menu.label} menu is in the menu bar at the top of the window.`,
    fallback: 'center',
  };
  const itemEntries: HintEntry[] = [];
  collectItemEntries(menu.items, menu.label, itemEntries);
  return [topEntry, ...itemEntries];
});
