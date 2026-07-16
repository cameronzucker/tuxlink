# Rung 3 brief — radio-panel error surfacing sweep (vehicle: bd tuxlink-46hof)

You are implementing ONE multi-site bug fix in the tuxlink repository. Your
working directory is the repository root of a dedicated checkout; work only
there.

## Repo context (all you need; do not explore beyond it)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`).
- User actions in the radio panels call the Rust backend via Tauri `invoke`.
  When an invoke fails AT THE IPC BOUNDARY (argument deserialization,
  unregistered command, unmanaged state), the promise rejects with NO backend
  log line — so catch blocks that assume "the backend surfaced it in the
  session log" are wrong for that entire failure class, and
  `console.debug` goes nowhere in WebKitGTK production builds.
- The project's frontend error channel is
  `src/frontendErrorLog.ts`:
  `export function reportFrontendError(source: string, message: string, stack?: string): void`
  — fire-and-forget; lands in the backend tracing pipeline (the forensic
  `.jsonl` logs AND the in-app Logging window). Existing call sites format
  like: `reportFrontendError('window.error', msg, e instanceof Error ? e.stack : undefined)`.
- Both panels already have an operator-visible inline error strip:
  - `src/radio/modes/VaraRadioPanel.tsx`: state `actionError` (line ~152),
    rendered at line ~1031 as
    `<p className="radio-panel-error" role="alert" data-testid="vara-action-error">`.
  - `src/radio/modes/ArdopRadioPanel.tsx`: state `connectError` (line ~403),
    rendered at line ~1537 as `<p className="radio-panel-error" role="alert">`
    (note: currently NO data-testid).

## The bug (bd tuxlink-46hof) — seven swallow sites

These catches swallow user-initiated-action failures invisibly:

1. `VaraRadioPanel.tsx` ~591-607 — Send/Receive catch: `console.debug` only
   (the comment claims the session log covers it; false for IPC-layer
   rejections).
2. `ArdopRadioPanel.tsx` ~833-841 — Connect catch: `console.debug` only.
3. `ArdopRadioPanel.tsx` ~862-879 — Send/Receive catch: `console.debug` only.
4. `ArdopRadioPanel.tsx` ~887-892 — Disconnect catch: `console.debug` only.
5. `ArdopRadioPanel.tsx` ~553-555 — `persistArdop` config write:
   `.catch(() => {})`, fully silent.
6. `ArdopRadioPanel.tsx` ~585-587 — `config_set_rig` write inside
   `onPttMethodChange`: `.catch(() => {})`, fully silent.
7. `ArdopRadioPanel.tsx` ~713-722 — bandwidth-change persist: silent
   `catch {}`.

## The fix (binding at each site)

At EVERY site above:

- (a) call `reportFrontendError(source, message, stack)` with a
  site-specific source string of the form `'VaraRadioPanel.sendReceive'`,
  `'ArdopRadioPanel.connect'`, `'ArdopRadioPanel.persistArdop'`, etc., the
  error message, and the stack when the error is an `Error`.
- (b) surface operator-visibly: sites 1-4 additionally set the panel's
  existing error strip (`setActionError` in Vara, `setConnectError` in
  Ardop) with a short action-prefixed message (e.g.
  `'Send/receive failed: <msg>'`). Sites 5-7 (config persists) call
  `reportFrontendError` only — a background settings write must not raise a
  modal-grade strip; the forensic log entry is the requirement.
- (c) update each site's now-stale comment to state the new behavior (these
  comments currently assert the session log covers the failure — after this
  change they must not claim that).
- Preserve every other behavior in those catch/finally blocks EXACTLY:
  the `recordAttempt(...)` bookkeeping, the `/session not open/i` guard, the
  `finally` state resets, and the deliberate `setActionError(null)` /
  `setConnectError(null)` clears at action START all stay.
- Add `data-testid="ardop-action-error"` to the Ardop error strip so it is
  testable like Vara's.

## Tests to add (binding)

In the existing test files (`src/radio/modes/VaraRadioPanel.test.tsx`,
`src/radio/modes/ArdopRadioPanel.test.tsx`), add one failure-path test per
panel:

- Vara: make the send/receive command (`modem_vara_b2f_exchange`) reject,
  click Send/Receive, assert `reportFrontendError` was called with a
  `VaraRadioPanel.`-prefixed source AND the `vara-action-error` strip shows
  the failure.
- Ardop: make the connect command (`modem_ardop_connect`) reject, click
  Connect, assert `reportFrontendError` was called AND the
  `ardop-action-error` strip shows the failure.

Follow the files' established mock idioms: the module-scope
`vi.mock('@tauri-apps/api/core', ...)` with per-command dispatch is already
there — override ONLY the specific command to throw (a blanket
always-throwing mock will also throw on unrelated teardown invokes and
corrupt later tests). Mock `reportFrontendError` via the project's
established hoisted pattern:

```ts
const { mockReport } = vi.hoisted(() => ({ mockReport: vi.fn() }));
vi.mock('../../frontendErrorLog', () => ({
  reportFrontendError: mockReport,
  installGlobalErrorForwarding: vi.fn(),
}));
```

## Constraints (binding)

- Touch ONLY the two panel files and their two test files. No new
  dependencies. Do NOT modify `frontendErrorLog.ts`.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Gates (run these exact commands from the repo root; capture real output)

- `pnpm vitest run src/radio/modes/VaraRadioPanel.test.tsx src/radio/modes/ArdopRadioPanel.test.tsx`
- `pnpm typecheck`

## Completion report (your final message)

1. Files touched (paths) and a per-site checklist (sites 1-7: done/skipped).
2. Test names added.
3. Verbatim final output lines of both gate commands.
4. Any deviation from the brief, with the reason (deviating without
   reporting is a defect).
