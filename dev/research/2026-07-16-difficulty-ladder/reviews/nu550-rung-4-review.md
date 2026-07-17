# nu550 — rung 4 review (central invoke-failure observability)

**Verdict: Approve-with-minors** — C:0 I:1 M:1

Underspecified brief; graded on the chokepoint design, API fidelity, recursion
safety, and the re-export footgun. Worker report was not written — code judged
alone.

## What was delivered
- `src/tauriInvoke.ts` — new `invokeLogged<T>(cmd, args?, options?)` that awaits
  `invoke`, and on rejection calls `reportFrontendError('invoke:'+cmd, message, stack)`
  then rethrows. Success passes the resolved value through with no reporting.
- `src/tauriInvoke.test.ts` — 7 tests.
- `src/shell/CloseBehaviorPrompt.tsx` + its test migrated to `invokeLogged` (the
  drop-in migration proof, criterion 3).

## Correctness — the chokepoint is sound
Criterion 1/2 met: one chokepoint, reports once with the command name in the
source, rethrows so caller catch-paths are unchanged, and does not report on
success. The tests pin exactly the right behaviors: report-once + propagation on
rejection; value-passthrough + no-report on success; **error-identity
preservation** (`expect(caught).toBe(error)`); Error vs string vs object
rejection shaping; options passthrough. These are real assertions against the
wrapper's contract, not smoke.

**API fidelity: good.** `args`/`options` are typed via
`Parameters<typeof invoke>[1]` / `[2]`, so the wrapper tracks `invoke`'s real
signature rather than a hand-copied one, and `invoke<T>(cmd, args, options)` is
forwarded positionally.

**Recursion safety vs reportFrontendError: safe as delivered.** I verified
`src/frontendErrorLog.ts`: `reportFrontendError` calls the *raw* `invoke('log_frontend_error', …)`
fire-and-forget with a swallowing `.catch(() => {})`. So an `invokeLogged`
rejection → `reportFrontendError` → raw `invoke` → (if that also fails) swallowed;
no loop. The migration deliberately leaves `frontendErrorLog.ts` on raw `invoke`.

## Findings

**[I1] `src/tauriInvoke.ts:213` — raw-invoke re-export is a footgun.**
`export { invoke } from '@tauri-apps/api/core';` republishes the *unlogged*
`invoke` from the very module that offers `invokeLogged`. A caller writing
`import { invoke } from '../tauriInvoke'` gets the un-instrumented path while
appearing to use the project wrapper — indistinguishable at the call site from
the logged one. It directly undercuts the "one chokepoint, not per-call-site
discipline" guarantee criterion 1 asks for, and nothing in the diff consumes it
(the migrated module imports `invokeLogged`). The comment "for type
compatibility" does not justify it — types come from `@tauri-apps/api/core`
already. Recommend deleting the re-export so the only export from this module is
the logged path.

**[M1] `src/tauriInvoke.ts` — no guard/comment against routing the log command
back through `invokeLogged`.** Safe today only because `frontendErrorLog.ts`
uses raw `invoke`. If a later sweep "migrates everything to the chokepoint"
(which criterion 4 invites via the remaining-surface recommendation), routing
`log_frontend_error` through `invokeLogged` yields unbounded recursion on a
failing log call. A one-line comment excluding `log_frontend_error`, or an
explicit early-return for that command, would harden the intended invariant.

## Scope / hygiene
No new deps, no Rust changes, migration limited to one module (as the brief
allows). Clean.
