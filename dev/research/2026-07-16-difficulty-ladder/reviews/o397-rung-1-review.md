# o397 rung 1 review — menu-action drift guard

**Verdict: Approve** — C:0 I:0 M:0

## Brief compliance
- `export const ROUTINES_CLOSING_MENU_ACTIONS` — one-word export change only (`src/shell/AppShell.tsx:190`). ✓
- New `src/shell/AppShell.menuParity.test.tsx` iterates the set: `for (const actionId of ROUTINES_CLOSING_MENU_ACTIONS) expect(MENU_ACTION_IDS).toContain(actionId)`. ✓ Iterates the live set, no hardcoded copy. ✓
- Explicit vitest imports; `globals: false` honored. ✓
- Only the two allowed files touched; no new deps. ✓

## Correctness
Functionally identical to the cn candidate; a real drift guard modeled on `menuModel.test.ts`. Test name is more descriptive ("every member of ROUTINES_CLOSING_MENU_ACTIONS is contained in MENU_ACTION_IDS").

## Scope / hygiene
Clean, minimal, no stale artifacts.
