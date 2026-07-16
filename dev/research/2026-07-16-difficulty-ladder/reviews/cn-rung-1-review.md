# cn rung 1 review — menu-action drift guard

**Verdict: Approve** — C:0 I:0 M:0

## Brief compliance
- `ROUTINES_CLOSING_MENU_ACTIONS` changed `const` → `export const`, nothing else about the declaration touched (`src/shell/AppShell.tsx:190`). ✓
- New file `src/shell/AppShell.menuParity.test.tsx` created; iterates the set with `for (const action of ROUTINES_CLOSING_MENU_ACTIONS) expect(MENU_ACTION_IDS).toContain(action)`. ✓ It iterates the live set rather than hardcoding a copy — a genuine drift guard. ✓
- Explicit vitest imports (`import { describe, it, expect } from 'vitest'`), consistent with `globals: false`. ✓
- Only the two allowed files touched; no new deps. ✓

## Correctness
The test is a real guard: renaming any action in `menuModel.ts` without updating the set fires `toContain`. Modeled correctly on `menuModel.test.ts`'s membership idiom. Green run is acceptable evidence per the brief.

## Scope / hygiene
No drift, no dead code, no stale comments. Minimal and correct.
