# Rung 1 brief — menu-action drift guard (vehicle: bd tuxlink-y6195 item 3)

You are implementing ONE small, fully-specified task in the tuxlink repository.
Your working directory is the repository root of a dedicated checkout; work
only there.

## Repo context (all you need; do not explore beyond it)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`).
- `src/shell/AppShell.tsx` declares a module-private set at line ~190:

```ts
const ROUTINES_CLOSING_MENU_ACTIONS = new Set<string>([
  'menu:message:new',
  'menu:message:reply',
  'menu:message:reply_all',
  'menu:message:forward',
  'menu:message:archive',
  'menu:message:delete',
  'menu:message:print',
]);
```

- `src/shell/chrome/menuModel.ts` exports the canonical menu-action manifest
  at line ~160: `export const MENU_ACTION_IDS: MenuActionId[] = ...`.
- Nothing guards these two against drift: a menu action could be renamed in
  `menuModel.ts` and the stale string in `ROUTINES_CLOSING_MENU_ACTIONS`
  would silently stop matching.

## The task

1. Export `ROUTINES_CLOSING_MENU_ACTIONS` from `AppShell.tsx` (change `const`
   to `export const`; nothing else about the declaration changes).
2. Create `src/shell/AppShell.menuParity.test.tsx` with a drift-guard test:
   every member of `ROUTINES_CLOSING_MENU_ACTIONS` must be contained in
   `MENU_ACTION_IDS`. Model it on the membership assertions in
   `src/shell/chrome/menuModel.test.ts` (which does
   `expect(MENU_ACTION_IDS).toContain(a.id)`). Use explicit vitest imports
   (`import { describe, it, expect } from 'vitest'`) — this project sets
   `globals: false`.
3. TDD evidence: after writing the test, demonstrate it is a real guard by
   showing what it would catch (e.g. temporarily noting which assertion fires
   for a hypothetical stale id is NOT required — a plain green run is fine
   here, but the test must iterate the set, not hardcode a copy of it).

## Constraints (binding)

- Touch ONLY `src/shell/AppShell.tsx` (the one-word export change) and the
  new test file. No new dependencies.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Gates (run these exact commands from the repo root; capture real output)

- `pnpm vitest run src/shell/AppShell.menuParity.test.tsx`
- `pnpm typecheck`

## Completion report (your final message)

1. Files touched (paths).
2. Test names added.
3. Verbatim final output lines of both gate commands.
4. Any deviation from the brief, with the reason (deviating without reporting
   is a defect).
