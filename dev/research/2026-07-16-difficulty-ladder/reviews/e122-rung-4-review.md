# e122 rung 4 review — central invoke chokepoint (underspecified; code-only, report never written)

**Verdict: Approve-with-minors** — C:0 I:0 M:2

Session crashed before a worker report existed, so acceptance criterion 4 (honest remaining-surface statement) is unverifiable; judged on the code alone.

## Design (API fidelity, recursion safety, honest scope)
- **Chokepoint:** new dedicated module `src/ipc/invoke.ts` exporting `safeInvoke<T>(command, args?)` — wraps `invoke`, on rejection reports `reportFrontendError('invoke:${command}', message, stack)` and re-throws; success passes through silently. Matches acceptance criterion 1. The dedicated `ipc/` module is a cleaner separation of concerns than folding the wrapper into `frontendErrorLog.ts`.
- **Recursion safety: sound.** `safeInvoke` calls raw core `invoke`, not itself; the failure path calls `reportFrontendError` (raw invoke). No self-recursion; a failing `log_frontend_error` cannot loop.
- **API fidelity:** `safeInvoke<T = unknown>(command, args?: unknown)` casts `args as InvokeArgs | undefined`, so it accepts the full `InvokeArgs` surface (closer to Tauri than the o397 candidate's `Record<string,unknown>`). Still drops the third `options` param, but that is rarely used.
- **Migration:** `src/mailbox/mailboxCommands.ts` fully migrated (4 call sites) — a real production module, drop-in.

## Tests
`src/ipc/invoke.test.ts` is thorough (success passthrough / rejection reports+rethrows / non-Error reason / no-stack / multi-await) and mocks BOTH core `invoke` and `reportFrontendError`, asserting the reporter is called exactly once with the precise `('invoke:cmd', message, stack)` args — well-isolated, pins real behavior.

## Findings
- **Minor — unsolicited `invokeTyped` alias.** `invokeTyped<T>` is a thin passthrough to `safeInvoke<T>` (the test even titles it "is an alias for safeInvoke with type parameter"). Two public names for one behavior is needless surface / mild scope creep; `safeInvoke` already takes a type parameter.
- **Minor — existing test assertion edited.** `mailboxCommands.test.ts` `emptyTrash` assertion changed from `toHaveBeenCalledWith('trash_empty')` to `('trash_empty', undefined)`, because `safeInvoke` always forwards a 2nd arg. Semantically identical to Tauri and correctly updated, but it means the migration was not purely additive at the test layer — acceptable, worth noting.

## Scope / hygiene
No dead code. Module header docstring accurately describes the chokepoint and migration path. The `boom_command` "multiple await patterns" test slightly overlaps the basic rejection test — trivial.
