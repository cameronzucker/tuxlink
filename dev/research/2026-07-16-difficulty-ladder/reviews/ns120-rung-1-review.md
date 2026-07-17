# ns120 — rung 1 review (menu-action drift guard)

**Verdict: Approve** — C:0 I:0 M:0

## Brief conformance
Complete and minimal:
- `src/shell/AppShell.tsx` — `const` → `export const` only; declaration body unchanged (brief §1).
- `src/shell/AppShell.menuParity.test.tsx` — new file, explicit vitest imports (`import { describe, it, expect } from 'vitest'`), consistent with `globals: false` (brief §2).

## Correctness of change + test
The test iterates the live set and asserts each member is in `MENU_ACTION_IDS` (`for (const action of ROUTINES_CLOSING_MENU_ACTIONS) expect(MENU_ACTION_IDS).toContain(action)`). It does not copy the set — it references the exported one — so a rename in `menuModel.ts` will surface as a real failure. This is the intended drift guard and it pins real behavior.

## Scope / hygiene
No scope drift, no new deps. `describe('ROUTINES_CLOSING_MENU_ACTIONS parity')` / `it('every action is a known menu action')` — clear naming.

## Notes
Near-identical to n235 rung-1 (differs only in import order and test-name wording). Both correct; nothing to separate them on quality.
