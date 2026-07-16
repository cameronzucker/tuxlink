# q122 rung 1 review — menu-action drift guard

**Verdict: Approve** — C:0 I:0 M:0

## Scope vs brief
Brief asked for exactly two touches: (1) flip `const` → `export const` on
`ROUTINES_CLOSING_MENU_ACTIONS` in `AppShell.tsx` with nothing else about the
declaration changing, and (2) a new `AppShell.menuParity.test.tsx` that iterates
the set and asserts each member is contained in `MENU_ACTION_IDS`, modeled on
`menuModel.test.ts`'s `toContain` membership style, explicit vitest imports
(`globals: false`).

The diff does precisely this and nothing else.

## Correctness
- `src/shell/AppShell.tsx:190` — the change is the single-word `export`
  addition; the seven-element set body is untouched. Matches the binding
  constraint "nothing else about the declaration changes."
- `src/shell/AppShell.menuParity.test.tsx:1-17` — explicit
  `import { describe, it, expect } from 'vitest'`; imports the set from
  `./AppShell` and `MENU_ACTION_IDS` from `./chrome/menuModel`. The test
  **iterates** the set (`for (const action of ROUTINES_CLOSING_MENU_ACTIONS)`)
  and asserts `expect(MENU_ACTION_IDS).toContain(action)` — it does not hardcode
  a copy, satisfying the brief's explicit "must iterate the set, not hardcode a
  copy of it" clause.
- Guard is real: I confirmed all seven ids
  (`new/reply/reply_all/forward/archive/delete/print`) are present in the live
  `MENU_TREE` (`menuModel.ts:29-44`), so `MENU_ACTION_IDS`
  (`menuModel.ts:160`) contains them and the test goes green today. If any
  member were renamed in `menuModel.ts`, the stale string would fail
  `toContain` — exactly the drift the brief wanted caught.
- Type note: `MENU_ACTION_IDS` is `MenuActionId[]` and the iterated value is
  `string`; `expect(...).toContain(string)` typechecks fine (vitest's
  `toContain` accepts `unknown`). No typecheck risk.

## Hygiene
Minimal, well-scoped, no new deps, no `git`/commit actions. Test lives in the
correct sibling location. Nothing to flag.

## Findings
None.
