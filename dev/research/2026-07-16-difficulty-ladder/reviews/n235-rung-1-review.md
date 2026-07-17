# n235 — rung 1 review (menu-action drift guard)

**Verdict: Approve** — C:0 I:0 M:0

## Brief conformance
Textbook. The two required edits and nothing else:
- `src/shell/AppShell.tsx` — `const` → `export const` on `ROUTINES_CLOSING_MENU_ACTIONS`; the declaration body is untouched (brief §1).
- `src/shell/AppShell.menuParity.test.tsx` — new file, explicit vitest imports (`import { describe, it, expect } from "vitest"`), honoring `globals: false` (brief §2).

## Correctness of change + test
The test **iterates** the set (`for (const action of ROUTINES_CLOSING_MENU_ACTIONS) expect(MENU_ACTION_IDS).toContain(action)`) rather than hardcoding a copy, which is exactly the drift-guard the brief demands: if a menu action is renamed in `menuModel.ts`, the stale string in the set stops matching and this assertion fires. It correctly models the membership pattern from `menuModel.test.ts`. Pins real behavior — the guard is genuine, not a tautology.

## Scope / hygiene
No scope drift; no new dependencies; imports source `MENU_ACTION_IDS` from `./chrome/menuModel` and the set from `./AppShell`. Clean.

## Notes
Effectively identical to the ns120 rung-1 submission (import ordering + describe/it wording differ only). Both are correct.
