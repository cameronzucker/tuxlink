# nu550 — rung 5 review (Station Data empty-roster bug)

**Verdict: Approve** — C:0 I:0 M:1

Symptom-only brief; the diagnosis is not graded — the delivered change is.
Worker report was not written — code judged alone.

## The delivered change
`src-tauri/capabilities/stations.json`: adds `core:event:allow-emit` to the
`stations` pop-out window capability (previously listen + unlisten only) and
updates the human description to say the grant now covers the snapshot-request
handshake.

## Correctness — right fix, right layer
I traced the handshake in `src/aprs/useEnvStations.ts`:
- The pop-out **client** calls `emit(SNAPSHOT_REQUEST)` at line 128 on mount.
- The **host** answers with `emit(SNAPSHOT_REPLY, …)` at line 97.

The `stations` window ran under a capability granting only
`core:event:allow-listen` / `allow-unlisten`. Tauri's capability layer therefore
denied the client's `emit(SNAPSHOT_REQUEST)` at runtime; the request never
reached the host, no reply seeded the roster, and the `.catch(() => {})` on the
emit swallowed the rejection silently — precisely the reported symptom (empty
until fresh RF arrives). Adding `allow-emit` is the **minimal** fix and lives at
the **correct layer**: the defect is a missing capability grant, so it belongs
in the capability JSON, not in the TS. The updated description is accurate.

This also correctly explains why unit tests stay green: the handshake tests mock
`@tauri-apps/api/event` (the file itself notes "listen() unavailable — mocked in
tests"), so the Rust/Tauri capability enforcement is never exercised in jsdom.
The runtime-only permission gate is structurally invisible to vitest.

**No test added — and that is the right call.** This defect class (capability
enforcement) cannot be reproduced under the mocked-Tauri unit harness; a
fabricated "test" would prove nothing. Declining to add one is honest, not a gap.

## Finding

**[M1] Residual: capability-grant regressions remain uncaught by CI.** Nothing
guards against a future edit dropping `allow-emit` again (the exact bug just
fixed). Not blocking and not in scope for a minimal fix, but worth a follow-up:
a schema/lint assertion that the `stations` capability retains emit, or an
integration/e2e smoke that exercises the real handshake, would convert this from
"invisible to CI" to guarded.

## Scope / hygiene
Single-file, single-permission change; no new deps; no Rust logic touched. Clean.
