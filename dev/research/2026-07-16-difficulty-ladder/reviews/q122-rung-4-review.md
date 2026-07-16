# q122 rung 4 review — make IPC-layer invoke failures observable

**Verdict: Approve-with-minors** — C:0 I:1 M:1

Underspecified brief; judged on the merits per instruction: API fidelity,
recursion safety vs `reportFrontendError`, and honest scope.

## Design (chokepoint) — sound on the core dimensions
`src/invokeWithReporting.ts:40-53` is a single wrapper:
`invokeWithReporting<T>(cmd, payload?)` awaits the real
`invoke<T>(cmd, payload)`, returns the resolved value untouched on success (no
reporting), and on rejection calls
`reportFrontendError(\`invoke:${cmd}\`, message, stack)` then `throw err`
(rethrows the ORIGINAL rejection). This satisfies acceptance criterion 1: one
chokepoint, command name in the source, rethrow so caller catch-paths are
unchanged, silent success.

- **API fidelity: good.** Signature mirrors `invoke<T>(cmd, args?)`, so the
  migration is a literal alias swap. Confirmed drop-in at
  `src/search/useSavedSearches.ts:2` (`import { invokeWithReporting as invoke }`)
  and the two existing tests in `useSavedSearches.test.tsx` still pass: they
  mock `@tauri-apps/api/core`'s `invoke`, which is exactly what the wrapper
  calls internally; the success-only tests never hit the reporting branch, so
  the `toHaveBeenCalledWith('tauri_search_save', {...})` assertion still matches
  (the wrapper forwards args verbatim). Criterion 3 met.
- **Recursion safety: correctly handled.** This was the sharp edge. The wrapper
  imports the RAW `invoke` from `@tauri-apps/api/core` (`:12`), not itself, and
  `reportFrontendError` (`frontendErrorLog.ts:18-24`) independently uses the raw
  `invoke` in a fire-and-forget `void invoke(...).catch(()=>{})` wrapped in
  `try/catch` — it never throws and never routes back through
  `invokeWithReporting`. So a failure of the `log_frontend_error` invoke itself
  cannot re-enter the wrapper: no infinite loop, and `reportFrontendError`
  throwing cannot mask the original `throw err`. The candidate got the one thing
  that could have been catastrophic right.
- **Tests: strong.** `invokeWithReporting.test.ts:31-110` proves success
  passthrough + no report, rejection reported-once-with-command-name + rethrow,
  non-Error rejection (`String(err)`), missing-stack (`undefined`), and
  once-per-failure semantics. Covers criterion 2 and then some.

## Findings

### I1 — footgun re-export of the UNWRAPPED `invoke` from the chokepoint module (`src/invokeWithReporting.ts:59-60`)
The module ends with `export { invoke } from '@tauri-apps/api/core';`, and the
JSDoc above it even tells callers to migrate via
`import { invokeWithReporting as invoke }`. The result is that the chokepoint
module now exports BOTH the wrapper and the raw, non-reporting `invoke`. A
developer who writes `import { invoke } from '../invokeWithReporting'` — the
natural thing to type, and superficially "on the chokepoint" — gets the
UNREPORTED raw invoke, silently defeating the entire goal (criterion 1's "one
chokepoint, not per-call-site discipline" and criterion 4's "keep future code on
the chokepoint"). It does not break anything shipped today, so it is an Issue
rather than Critical, but it is an actively harmful latent trap that works
against the brief's stated intent and should be deleted before merge. If future
code needs the raw invoke it should import it from `@tauri-apps/api/core`
directly, not from the module whose purpose is to prevent exactly that.

### M1 — `payload as Record<string, unknown> | undefined` narrows invoke's arg type (`src/invokeWithReporting.ts:47`)
`invoke`'s real second parameter is `InvokeArgs` (accepts `ArrayBuffer` /
`Uint8Array` byte payloads as well as record objects). Typing `payload` as
`unknown` and force-casting to a record silently accepts non-object payloads and
drops the byte-buffer arg shape. No current caller passes bytes, so it is minor,
but a plainer `payload?: InvokeArgs` (or the library's own arg type) would be
more faithful.

## Unverifiable from the diff
Criterion 4's remaining-surface measurement lives in the completion report,
which is not in the diff; cannot confirm the count or the honesty of the
migration-surface statement here. The single demonstrated migration
(`useSavedSearches`) is real and drop-in, which is what the brief required as
the floor.
