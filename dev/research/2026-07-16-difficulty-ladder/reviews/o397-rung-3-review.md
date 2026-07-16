# o397 rung 3 review — radio-panel error surfacing sweep

**Verdict: Approve-with-minors** — C:0 I:1 M:0

## Brief compliance — per-site checklist
1. Vara Send/Receive (`VaraRadioPanel.tsx` ~591): `reportFrontendError('VaraRadioPanel.sendReceive', msg, stack)` + `setActionError('Send/receive failed: …')`. ✓
2. Ardop Connect (`ArdopRadioPanel.tsx` ~855): `reportFrontendError('ArdopRadioPanel.connect', …)` + `setConnectError('Connect failed: …')`. ✓
3. Ardop Send/Receive (~888): `reportFrontendError('ArdopRadioPanel.sendReceive', …)` + `setConnectError('Send/receive failed: …')`. ✓
4. Ardop Disconnect (~918): `reportFrontendError('ArdopRadioPanel.disconnect', …)` **but NO `setConnectError`**. ✗ (see finding)
5. `persistArdop` (~551): `reportFrontendError('ArdopRadioPanel.persistArdop', …)` only. ✓
6. `config_set_rig` in `onPttMethodChange` (~592): `reportFrontendError('ArdopRadioPanel.config_set_rig', …)` only. ✓
7. bandwidth-change (~733): `reportFrontendError('ArdopRadioPanel.bandwidthChange', …)` only. ✓
- `data-testid="ardop-action-error"` added to the Ardop strip (line ~1576). ✓
- Preserved behavior: the `/session not open/i` guard on `recordAttempt` (verified in source), the gateway `failed` record in Vara/Ardop catches, `finally` resets, and the `setConnectError(null)`/`setActionError(null)` clears at action start all remain. ✓
- Stale comments rewritten at every site to stop claiming the session log covers the failure. ✓
- Only the two panels + two test files touched; `frontendErrorLog.ts` untouched; no new deps. ✓

## Rewritten pre-existing tests (verify-key requirement)
Both previously pinned deliberately-inverted "errors do NOT surface inline" behavior; both are correctly re-inverted:
- Ardop `nnjz` test: renamed and flipped from `expect(container.querySelector('.radio-panel-error')).toBeNull()` to `findByTestId('ardop-action-error')` showing `/Connect failed:/`. ✓ Correct inversion.
- Vara "does NOT record a failed attempt … (session not open)": name unchanged (its core assertion — no `failed` attempt recorded — is preserved); only the inline-error clause flipped from `queryByTestId('vara-action-error')).toBeNull()` to `findByTestId(...)` showing `/Send\/receive failed:/`. ✓ Correctly pins that the pre-air path now DOES surface inline yet still records no `failed`.

## Findings
- **Important — site 4 (disconnect) omits the error strip.** The brief's clause (b) bindingly lists "sites 1-4 additionally set the panel's existing error strip." The disconnect catch sets only `reportFrontendError`, not `setConnectError`. Low functional impact (disconnect failures are rare; the forensic-log requirement — the actual bug fix — is met) and arguably semantically defensible (a "Connect error" strip for a disconnect reads oddly), but it is an unflagged deviation from an explicit binding item. Either wire `setConnectError('Disconnect failed: …')` or report the intentional carve-out.

## Correctness / hygiene
New failure-path tests (one per panel) use per-command override (not a blanket throw) and the hoisted `mockReport` pattern exactly as prescribed — they pin real behavior (source prefix + strip text). Assertions on `expect.any(String)` for the stack are reasonable given `new Error(...)` always has a stack. Comments are accurate. No dead code.
