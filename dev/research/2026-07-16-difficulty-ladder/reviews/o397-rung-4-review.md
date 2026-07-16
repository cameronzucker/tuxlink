# o397 rung 4 review — central invoke chokepoint (underspecified; judged on merits)

**Verdict: Approve-with-minors** — C:0 I:0 M:1

## Design (API fidelity, recursion safety, honest scope)
- **Chokepoint:** `invokeWithLogging<T>(command, args?)` added directly to `src/frontendErrorLog.ts`. Wraps `invoke`, on rejection reports `reportFrontendError('invoke:${command}', message, stack)` and re-throws; success passes through with no reporting. Matches acceptance criterion 1 exactly (single mechanism, command-named source, rethrow, silent success).
- **Recursion safety: sound.** `invokeWithLogging` calls the raw `@tauri-apps/api/core` `invoke`, not itself; its failure path calls `reportFrontendError`, which also uses the raw `invoke('log_frontend_error', …)`. No path routes the error-report invoke back through the wrapper, so a failing `log_frontend_error` cannot recurse. Co-locating the wrapper in the same module as `reportFrontendError` makes this self-evident.
- **API fidelity: mostly faithful, one narrowing.** Signature is `(command: string, args?: Record<string, unknown>)`. Tauri's `invoke` accepts `InvokeArgs` (broader — also array/ArrayBuffer bodies) and a third `options` param. For the migrated GPS surface (object-or-no args) this is fine, but the wrapper cannot express binary-body or options invokes. See finding.
- **Migration:** `src/location/gpsProbes.ts` fully migrated (10 call sites) — a real production module, drop-in (only the import + call name change). Its existing tests are unaffected by construction.

## Tests
`frontendErrorLog.test.ts` adds three: success passthrough asserts `log_frontend_error` NOT among invoke calls; rejection asserts rethrow AND the `log_frontend_error` payload `{source:'invoke:failing_command', message, stack}`; non-Error string reason asserts `stack: null`. These exercise the wrapper end-to-end through the real `reportFrontendError`, pinning true wire behavior (a slightly stronger integration test than mocking the reporter).

## Findings
- **Minor — args type narrows Tauri's contract.** `args?: Record<string, unknown>` drops `InvokeArgs`' array/binary-body forms and the `options` parameter. Adequate for the current migration surface but will block migrating any binary-body / options call site without a signature change. Worth a note in the follow-up recommendation.

## Scope / hygiene
No scope drift, no dead code. Placing the wrapper inside `frontendErrorLog.ts` (rather than a dedicated `ipc/` module) couples IPC concerns into the error-log module but keeps the recursion guarantee obvious — a defensible trade. Honest remaining-surface statement is a report item (not verifiable from the diff).
