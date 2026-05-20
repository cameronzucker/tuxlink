# Onboarding Wizard Cluster Implementation Plan (Tasks 9 + 10 + 11 + 11.5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Spec of record: [`docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md`](../specs/2026-05-18-onboarding-wizard-cluster-design.md) (commit `da605e4` on feat/v0.0.1 via PR #62).

**Goal:** Ship the v0.0.1 first-run onboarding wizard cluster — 4 React screens + 1 shared state machine + 4 Tauri commands — that captures the operator's CMS connection choice, persists credentials to the OS keyring (Linux secret-service / macOS Keychain / Windows CredentialManager) under the `(service="tuxlink-pat", account=<callsign>)` convention that the cred-handling refactor's fork-side Pat reads back, and exposes a non-blocking 4-substate test-send verification of the live CMS round-trip.

**Architecture:** Tauri 2 + React 18 + TypeScript front; Rust back. `useReducer` with effects-in-component for async dispatch. Transactional keyring-first → config-second persistence via the Rust `keyring` crate (per AMD-14 dep authorization) with snapshot-and-restore rollback. Single-flight `tokio::sync::Mutex` guards multi-window double-dispatch. Test-send substate guard + Rust-side mutex enforces Part 97 one-consent-one-transmission. Pat config rendering is **delegated to Task 3 PatProcess (see Prerequisites — `tuxlink-756` is a HARD prerequisite for this plan to function end-to-end)**.

**Tech Stack:** Rust 1.75+, Tauri 2, `keyring` crate (Rust; pin in Task 3.1), React 18, TypeScript 5, Vite, vitest, `@tauri-apps/api/core::invoke`, existing `pat_client` (Task 5 — shipped), existing config schema (Task 2 — shipped, post-AMD-1 nested shape post-AMD-11 no `winlink_password_present`), `dbus-launch` + `gnome-keyring-daemon` in CI Linux runner (per cred-handling plan Phase 9 CI integration test recipe).

---

## LDC Execution Status

> **Living Document Contract.** This table flips per-phase as subagent dispatches ship. Banner status: ⬜ Not started · 🚧 In progress · ✅ Shipped · ⚠️ Blocked.

| Phase | Title | Status | Deliverable |
|---|---|---|---|
| 0 | Pre-flight + tuxlink-756 prerequisite check | ✅ Shipped | Verify Task 3 PatProcess amendment landed; if not, STOP and surface |
| 1 | Wizard infrastructure (types + reducer + context + Tauri command skeletons + App.tsx routing) | ⬜ Not started | `wizardReducer.ts` + `types.ts` + `wizardContext.tsx` + `wizard.rs` skeleton + `get_wizard_completed` command + App.tsx routing |
| 2 | Step 1 Welcome (Task 9 / tuxlink-ko0) | ⬜ Not started | `Step1Welcome.tsx` + choice-card routing + tests |
| 3 | Step 2 Credentials + Rust keyring write (Task 10 / tuxlink-1r5) — the HEART of the cluster | ✅ Shipped | `Step2Credentials.tsx` + `validators.ts` + `wizard_persist_cms` + capability + CSP + integration test |
| 4 | Step 2 Offline Identity (Task 11.5 / tuxlink-d76) | ✅ Shipped | `Step2OfflineIdentity.tsx` + `wizard_persist_offline` + tests |
| 5 | Step 3 Test Send 4-substate (Task 11 / tuxlink-e4x) | ✅ Shipped | `Step3TestSend.tsx` + `wizard_run_test_send` (MOCKED via env var by default) + 4-substate UI + Part-97 dedup guard test + log streaming |
| 6 | CI integration tests + gnome-keyring-daemon setup | ⬜ Not started | `wizard_integration_test.rs` + `dev/scratch/cross-validate-wizard-pat.sh` + `.github/workflows/wizard-test.yml` |
| 7 | Browser smoke documentation (operator-only LIVE mode + agent-safe MOCKED mode) | ⬜ Not started | `docs/wizard-smoke-testing.md` + cross-link from `docs/live-cms-testing-policy.md` |
| 8 | bd cleanup + PR-B open against feat/v0.0.1 + final review | ⬜ Not started | PR-B opened; `tuxlink-ko0` / `tuxlink-1r5` / `tuxlink-d76` / `tuxlink-e4x` all close on PR-B merge |

---

## Prerequisites

**Every subagent executing this plan MUST read the following files before starting:**

1. [`CLAUDE.md`](../../../CLAUDE.md) — project ethos, git safety rails, agent moniker discipline, Part 97 live-radio rules.
2. [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — **load-bearing for Phase 5 + 7.** No automated transmission. Phase 5's `wizard_run_test_send` is `TUXLINK_TEST_SEND_MOCK=1`-gated for subagent runs; the live mode is operator-only.
3. [`docs/pitfalls/implementation-pitfalls.md`](../../pitfalls/implementation-pitfalls.md) §0 (RADIO-1), §1 (SCOPE-1).
4. [`docs/pitfalls/testing-pitfalls.md`](../../pitfalls/testing-pitfalls.md) — universal testing disciplines.
5. **Spec of record:** [`docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md`](../specs/2026-05-18-onboarding-wizard-cluster-design.md). This plan IMPLEMENTS that spec — defer to the spec for any ambiguity.
6. [`docs/superpowers/specs/2026-05-18-cred-handling-design.md`](../specs/2026-05-18-cred-handling-design.md) §2-§3 + §5 — the Pat-side keyring contract this wizard writes for.

### HARD prerequisite: `tuxlink-756` (Task 3 PatProcess amendment)

`tuxlink-756` (P1) amends `src-tauri/src/pat_process.rs` to render Pat's non-secret config (callsign, MBO, transport) at Pat-spawn time from tuxlink's config + the keyring callsign. **Without this amendment landing first, the wizard cluster ships but Pat cannot operate** — the wizard writes to tuxlink's config + the keyring, but nothing populates Pat's expected `~/.config/pat/config.json` with the non-secret fields.

**Phase 0 below verifies `tuxlink-756` has shipped.** If it hasn't, the executing agent STOPS and escalates per CLAUDE.md's "When you think you need a banned command: stop and surface the situation."

### Other prerequisites (verify shipped, not blocking-but-load-bearing)

- AMD-1 (Task 2 nested config schema) — shipped via PR #34
- AMD-2 / AMD-3 / AMD-4 / AMD-5 / AMD-10 / AMD-11 / AMD-13 / AMD-14 / AMD-15 / AMD-16 — all shipped via prior PRs
- Task 1 Tauri 2 + React + TypeScript scaffold (`tuxlink-wkz`) — shipped
- Task 2 Config struct + `write_config_atomic()` (`tuxlink-???`) — shipped per existing `src-tauri/src/config.rs`
- Task 3 PatProcess base — shipped; tuxlink-756 amends it
- Task 5 PatClient — shipped per `src-tauri/src/pat_client.rs`
- `external/tuxlink-pat` submodule pinned at PR-A merge SHA `4969aa86` (post-cred-handling refactor; Pat reads from keyring) — shipped via PR-B / `tuxlink-mib`

---

## Mandatory Per-Task Preamble

Every task below starts with this work, implicitly. Do it even though it's not repeated verbatim per task:

1. Read the 6 prerequisite files above.
2. Invoke the `superpowers:test-driven-development` skill (or equivalent guidance — failing-test-first discipline).
3. Follow TDD: write the failing test first, run to confirm it fails, implement the minimal code to pass, run to confirm green. **No implementation code before a failing test.**
4. Phase 5 + 7 ALSO require reading `docs/live-cms-testing-policy.md` — the wizard's test-send is a live-CMS surface.

## Subagent Guardrails (read first; ignore at your peril)

Inherits from `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Subagent Guardrails (no new dep additions beyond AMD-14's `keyring`, no destructive git, branch naming `bd-tuxlink-ln3/wizard-cluster-impl` for the impl branch, commit cadence one commit per task, etc.). Plus these wizard-specific:

- **NO LIVE CMS TRANSMISSION FROM SUBAGENTS.** `wizard_run_test_send` MUST be invoked with `TUXLINK_TEST_SEND_MOCK=1` set when run from any non-operator context (subagent shell, CI, automated test). The Rust command reads this env var at startup and short-circuits to a mocked outcome. Verify the env var is set before any `pnpm tauri dev` invocation. See spec §3.8 + Phase 5 + Phase 7.
- **DO NOT skip the `BEGIN_TEST_SEND` dedup guard (spec §3.1 invariant 2).** The reducer test for "BEGIN_TEST_SEND while sending is no-op" is REQUIRED, not optional. This is Part 97 correctness.
- **DO NOT relax callsign normalization** (spec §3.2 step 1). Frontend validator + Rust normalize (`trim().to_uppercase()`) BOTH run; Rust catches malicious or buggy frontend.
- **The Rust-side mutex (`tokio::sync::Mutex`) on the 3 write commands is non-optional** (spec §3.7). UI debounce alone is insufficient for multi-window.

---

## Phase 0 — Pre-flight + tuxlink-756 prerequisite check

**Files:**
- Read: `src-tauri/src/pat_process.rs` (verify Pat config rendering exists)

- [ ] **Step 1: Verify tuxlink-756 (Task 3 PatProcess amendment) landed**

```bash
bd show tuxlink-756 2>&1 | grep -E "Status|closed_at"
```

Expected: `CLOSED` with a close_reason citing a merged PR.

- [ ] **Step 2: Verify the code change is in `src-tauri/src/pat_process.rs`**

```bash
grep -nE "config\.json|render_pat_config|tuxlink-pat.*account" src-tauri/src/pat_process.rs
```

Expected: At least one match showing Pat config rendering logic exists.

- [ ] **Step 3: If either step 1 or step 2 fails, STOP and escalate.**

The wizard cluster cannot ship without the Task 3 PatProcess amendment. Surface to operator: "tuxlink-756 (Task 3 PatProcess amendment) is the HARD prerequisite for this plan. Either it hasn't landed yet OR the code change is missing from src-tauri/src/pat_process.rs. Cannot proceed."

- [ ] **Step 4: Update LDC banner: Phase 0 ⬜ → ✅**

---

## Phase 1 — Wizard infrastructure

### Task 1.1: Create `src/wizard/types.ts` (TypeScript discriminated unions for WizardError + TestSendOutcome)

**Files:**
- Create: `src/wizard/types.ts`

- [ ] **Step 1: Write the failing test (compilation-only — types module must export)**

Create `src/wizard/types.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import type { WizardError, TestSendOutcome, WizardStep, WizardState, WizardAction } from './types';

describe('wizard types', () => {
  it('WizardError discriminated union has all 8 variants', () => {
    const variants: WizardError['kind'][] = [
      'Unavailable', 'Locked', 'PermissionDenied',
      'ConfigWrite', 'ConfigWriteAndRollbackFailed',
      'Busy', 'InvalidInput', 'Other',
    ];
    expect(variants).toHaveLength(8);
  });

  it('TestSendOutcome discriminated union has Success + Failed', () => {
    const variants: TestSendOutcome['kind'][] = ['Success', 'Failed'];
    expect(variants).toHaveLength(2);
  });
});
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src/ && pnpm vitest run wizard/types.test.ts
```

Expected: "Cannot find module './types'" or similar.

- [ ] **Step 3: Implement `src/wizard/types.ts` per spec §3.1 + §3.7**

```typescript
// Mirrors src-tauri/src/wizard.rs's WizardError enum via Tauri's #[serde(tag, content)] shape.
export type WizardError =
  | { kind: 'Unavailable' }
  | { kind: 'Locked' }
  | { kind: 'PermissionDenied'; detail: { platform_hint: 'linux' | 'macos' | 'windows' } }
  | { kind: 'ConfigWrite'; detail: { detail: string } }
  | { kind: 'ConfigWriteAndRollbackFailed'; detail: { config_error: string; rollback_error: string } }
  | { kind: 'Busy' }
  | { kind: 'InvalidInput'; detail: { field: string } }
  | { kind: 'Other'; detail: { detail: string } };

// Mirrors src-tauri/src/wizard.rs's TestSendOutcome enum.
export type TestSendOutcome =
  | { kind: 'Success'; detail: { reply_subject: string | null } }
  | { kind: 'Failed'; detail: { cause: string; likely_causes_hint: string[] } };

export type WizardStep =
  | 'account'
  | 'credentials'
  | 'offline_identity'
  | 'test_send'
  | 'complete';

export interface WizardState {
  step: WizardStep;
  connectToCms: boolean | null;
  callsign: string;
  password: string;
  identifier: string;
  grid: string;
  mboAddress: string;
  testSendSubstate: 'idle' | 'sending' | 'success' | 'failed';
  testSendError: string | null;
  testSendLog: string[];
  inFlight: boolean;
  skipSignaled: boolean;
}

export type WizardAction =
  | { type: 'SET_CONNECT_TO_CMS'; payload: boolean }
  | { type: 'ADVANCE_FROM_ACCOUNT' }
  | { type: 'SET_CREDENTIALS_FIELD'; field: 'callsign' | 'password' | 'grid' | 'mboAddress'; value: string }
  | { type: 'SET_OFFLINE_FIELD'; field: 'identifier' | 'grid'; value: string }
  | { type: 'SUBMIT_BEGIN' }
  | { type: 'SUBMIT_CREDENTIALS_SUCCESS'; skipTestSend: boolean }
  | { type: 'SUBMIT_OFFLINE_SUCCESS' }
  | { type: 'SUBMIT_FAILURE'; error: WizardError }
  | { type: 'BEGIN_TEST_SEND' }
  | { type: 'TEST_SEND_LOG_LINE'; line: string }
  | { type: 'TEST_SEND_RESULT'; outcome: TestSendOutcome }
  | { type: 'SKIP_TEST_SEND' }
  | { type: 'RETURN_TO_CREDENTIALS' };
```

- [ ] **Step 4: Run to confirm pass**

```bash
cd src/ && pnpm vitest run wizard/types.test.ts
```

Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/wizard/types.ts src/wizard/types.test.ts
git commit -m "$(cat <<'EOF'
feat(wizard): types for WizardError + TestSendOutcome + WizardState + WizardAction

Mirrors src-tauri/src/wizard.rs's Rust enum shape (via Tauri #[serde(tag, content)]).
Discriminated unions enable React-side error-class pattern-matching per
wizard-cluster spec §3.1 + §3.7. 13 WizardAction variants cover every reducer
transition including RETURN_TO_CREDENTIALS (Codex R5 wrong-password recovery).

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 1.2: Implement `wizardReducer.ts` with all 13 actions + 4 invariants

**Files:**
- Create: `src/wizard/wizardReducer.ts`
- Create: `src/wizard/wizardReducer.test.ts`

- [ ] **Step 1: Write the failing tests for the reducer (full coverage)**

Create `src/wizard/wizardReducer.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { wizardReducer, initialWizardState } from './wizardReducer';
import type { WizardState, WizardAction } from './types';

describe('wizardReducer', () => {
  it('initial state has step=account, all fields cleared', () => {
    const s = initialWizardState();
    expect(s.step).toBe('account');
    expect(s.connectToCms).toBeNull();
    expect(s.callsign).toBe('');
    expect(s.password).toBe('');
    expect(s.inFlight).toBe(false);
    expect(s.skipSignaled).toBe(false);
  });

  it('SET_CONNECT_TO_CMS sets connectToCms but does NOT advance step', () => {
    const s = wizardReducer(initialWizardState(), { type: 'SET_CONNECT_TO_CMS', payload: true });
    expect(s.connectToCms).toBe(true);
    expect(s.step).toBe('account');  // step transition is separate via ADVANCE_FROM_ACCOUNT
  });

  it('ADVANCE_FROM_ACCOUNT routes to credentials when connectToCms=true', () => {
    let s = wizardReducer(initialWizardState(), { type: 'SET_CONNECT_TO_CMS', payload: true });
    s = wizardReducer(s, { type: 'ADVANCE_FROM_ACCOUNT' });
    expect(s.step).toBe('credentials');
  });

  it('ADVANCE_FROM_ACCOUNT routes to offline_identity when connectToCms=false', () => {
    let s = wizardReducer(initialWizardState(), { type: 'SET_CONNECT_TO_CMS', payload: false });
    s = wizardReducer(s, { type: 'ADVANCE_FROM_ACCOUNT' });
    expect(s.step).toBe('offline_identity');
  });

  it('ADVANCE_FROM_ACCOUNT is no-op when connectToCms is null', () => {
    const s = wizardReducer(initialWizardState(), { type: 'ADVANCE_FROM_ACCOUNT' });
    expect(s.step).toBe('account');
  });

  it('SUBMIT_CREDENTIALS_SUCCESS clears password and routes per skipTestSend flag', () => {
    let s = { ...initialWizardState(), step: 'credentials' as const, callsign: 'W4PHS', password: 'secret', inFlight: true };
    let s2 = wizardReducer(s, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipTestSend: false });
    expect(s2.password).toBe('');
    expect(s2.step).toBe('test_send');
    expect(s2.inFlight).toBe(false);
    s = { ...s, inFlight: true };
    s2 = wizardReducer(s, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipTestSend: true });
    expect(s2.step).toBe('complete');
  });

  // INVARIANT: BEGIN_TEST_SEND while sending is a no-op (Part 97 correctness)
  it('BEGIN_TEST_SEND while testSendSubstate=sending returns state unchanged', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'BEGIN_TEST_SEND' });
    expect(s2).toBe(s);  // strict equality — must return same reference, not just shallow-equal
  });

  it('BEGIN_TEST_SEND from idle transitions to sending + resets log + clears skipSignaled', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'idle' as const, testSendLog: ['stale'], skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'BEGIN_TEST_SEND' });
    expect(s2.testSendSubstate).toBe('sending');
    expect(s2.testSendLog).toEqual([]);
    expect(s2.skipSignaled).toBe(false);
  });

  // INVARIANT: TEST_SEND_RESULT ignored when skipSignaled
  it('TEST_SEND_RESULT after SKIP_TEST_SEND is silently ignored (skipSignaled gate)', () => {
    let s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    s = wizardReducer(s, { type: 'SKIP_TEST_SEND' });
    expect(s.step).toBe('complete');
    expect(s.skipSignaled).toBe(true);
    const s2 = wizardReducer(s, { type: 'TEST_SEND_RESULT', outcome: { kind: 'Success', detail: { reply_subject: 'test' } } });
    expect(s2).toBe(s);  // unchanged
  });

  it('TEST_SEND_RESULT Success transitions sending → success', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'TEST_SEND_RESULT', outcome: { kind: 'Success', detail: { reply_subject: 'auto-reply' } } });
    expect(s2.testSendSubstate).toBe('success');
  });

  it('TEST_SEND_RESULT Failed populates testSendError + transitions sending → failed', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'TEST_SEND_RESULT', outcome: { kind: 'Failed', detail: { cause: 'wrong password', likely_causes_hint: [] } } });
    expect(s2.testSendSubstate).toBe('failed');
    expect(s2.testSendError).toBe('wrong password');
  });

  it('RETURN_TO_CREDENTIALS from failed substate clears password but preserves callsign/grid/MBO', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'failed' as const,
      callsign: 'W4PHS', password: '', grid: 'EM75', mboAddress: 'W4PHS@winlink.org' };
    const s2 = wizardReducer(s, { type: 'RETURN_TO_CREDENTIALS' });
    expect(s2.step).toBe('credentials');
    expect(s2.password).toBe('');
    expect(s2.callsign).toBe('W4PHS');
    expect(s2.grid).toBe('EM75');
    expect(s2.mboAddress).toBe('W4PHS@winlink.org');
    expect(s2.testSendSubstate).toBe('idle');  // reset for fresh attempt
  });

  it('TEST_SEND_LOG_LINE appends to testSendLog', () => {
    const s = { ...initialWizardState(), testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'TEST_SEND_LOG_LINE', line: 'Connecting via CMS-SSL...' });
    expect(s2.testSendLog).toEqual(['Connecting via CMS-SSL...']);
  });

  it('TEST_SEND_LOG_LINE ignored when skipSignaled', () => {
    const s = { ...initialWizardState(), testSendSubstate: 'sending' as const, skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'TEST_SEND_LOG_LINE', line: 'stale' });
    expect(s2).toBe(s);
  });
});
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src/ && pnpm vitest run wizard/wizardReducer.test.ts
```

Expected: import errors (reducer module not yet created).

- [ ] **Step 3: Implement `src/wizard/wizardReducer.ts`**

```typescript
import type { WizardState, WizardAction } from './types';

export function initialWizardState(): WizardState {
  return {
    step: 'account',
    connectToCms: null,
    callsign: '',
    password: '',
    identifier: '',
    grid: '',
    mboAddress: '',
    testSendSubstate: 'idle',
    testSendError: null,
    testSendLog: [],
    inFlight: false,
    skipSignaled: false,
  };
}

export function wizardReducer(state: WizardState, action: WizardAction): WizardState {
  switch (action.type) {
    case 'SET_CONNECT_TO_CMS':
      return { ...state, connectToCms: action.payload };

    case 'ADVANCE_FROM_ACCOUNT':
      if (state.connectToCms === null) return state;  // invariant: no advance without decision
      return { ...state, step: state.connectToCms ? 'credentials' : 'offline_identity' };

    case 'SET_CREDENTIALS_FIELD':
      return { ...state, [action.field]: action.value };

    case 'SET_OFFLINE_FIELD':
      return { ...state, [action.field]: action.value };

    case 'SUBMIT_BEGIN':
      return { ...state, inFlight: true };

    case 'SUBMIT_CREDENTIALS_SUCCESS':
      return {
        ...state,
        password: '',  // INVARIANT: clear password from JS memory
        step: action.skipTestSend ? 'complete' : 'test_send',
        inFlight: false,
      };

    case 'SUBMIT_OFFLINE_SUCCESS':
      return { ...state, step: 'complete', inFlight: false };

    case 'SUBMIT_FAILURE':
      // Per-error UX handled in component; reducer just clears inFlight + records the error if useful
      return { ...state, inFlight: false };

    case 'BEGIN_TEST_SEND':
      // INVARIANT 2: dedup guard — no-op if not idle (Part 97 correctness)
      if (state.testSendSubstate !== 'idle') return state;
      return { ...state, testSendSubstate: 'sending', testSendLog: [], skipSignaled: false };

    case 'TEST_SEND_LOG_LINE':
      // INVARIANT 3: skipSignaled gate
      if (state.skipSignaled) return state;
      return { ...state, testSendLog: [...state.testSendLog, action.line] };

    case 'TEST_SEND_RESULT':
      if (state.skipSignaled) return state;
      if (action.outcome.kind === 'Success') {
        return { ...state, testSendSubstate: 'success' };
      }
      return {
        ...state,
        testSendSubstate: 'failed',
        testSendError: action.outcome.detail.cause,
      };

    case 'SKIP_TEST_SEND':
      return { ...state, step: 'complete', skipSignaled: true };

    case 'RETURN_TO_CREDENTIALS':
      return {
        ...state,
        step: 'credentials',
        password: '',
        testSendSubstate: 'idle',
        testSendError: null,
        testSendLog: [],
      };

    default:
      return state;
  }
}
```

- [ ] **Step 4: Run to confirm pass**

```bash
cd src/ && pnpm vitest run wizard/wizardReducer.test.ts
```

Expected: 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/wizard/wizardReducer.ts src/wizard/wizardReducer.test.ts
git commit -m "$(cat <<'EOF'
feat(wizard): reducer with all 13 actions + 4 invariants

WizardState reducer per spec §3.1. Invariants enforced:
1. password cleared on SUBMIT_CREDENTIALS_SUCCESS
2. BEGIN_TEST_SEND while sending is no-op (Part 97 correctness)
3. SKIP_TEST_SEND sets skipSignaled; subsequent log lines + result ignored
4. ADVANCE_FROM_ACCOUNT routes per connectToCms (no-op if null)

14 vitest cases cover happy paths + edge cases + invariants.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 1.3: Implement `wizardContext.tsx` (React context for {state, dispatch})

**Files:**
- Create: `src/wizard/wizardContext.tsx`
- Create: `src/wizard/wizardContext.test.tsx`

- [ ] **Step 1: Write failing test**

```typescript
import { describe, it, expect } from 'vitest';
import { render, renderHook } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';

describe('wizardContext', () => {
  it('useWizard outside WizardProvider throws', () => {
    expect(() => renderHook(() => useWizard())).toThrow();
  });

  it('useWizard inside WizardProvider returns {state, dispatch}', () => {
    const wrapper = ({ children }: { children: React.ReactNode }) => <WizardProvider>{children}</WizardProvider>;
    const { result } = renderHook(() => useWizard(), { wrapper });
    expect(result.current.state.step).toBe('account');
    expect(typeof result.current.dispatch).toBe('function');
  });
});
```

- [ ] **Step 2: Run to confirm failure** — module not found.

- [ ] **Step 3: Implement `src/wizard/wizardContext.tsx`**

```typescript
import { createContext, useContext, useReducer, Dispatch, ReactNode } from 'react';
import { wizardReducer, initialWizardState } from './wizardReducer';
import type { WizardState, WizardAction } from './types';

interface WizardContextValue {
  state: WizardState;
  dispatch: Dispatch<WizardAction>;
}

const WizardContext = createContext<WizardContextValue | null>(null);

export function WizardProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(wizardReducer, undefined, initialWizardState);
  return <WizardContext.Provider value={{ state, dispatch }}>{children}</WizardContext.Provider>;
}

export function useWizard(): WizardContextValue {
  const ctx = useContext(WizardContext);
  if (!ctx) throw new Error('useWizard must be used inside <WizardProvider>');
  return ctx;
}
```

- [ ] **Step 4: Run** — 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/wizard/wizardContext.tsx src/wizard/wizardContext.test.tsx
git commit -m "feat(wizard): React context for {state, dispatch}

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.4: Create `src-tauri/src/wizard.rs` skeleton + register commands + `keyring::use_native_store(true)?` at app init

**Files:**
- Create: `src-tauri/src/wizard.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod wizard;`, call `keyring::use_native_store(true)?` once at app init, register 4 commands in `tauri::generate_handler![...]`)
- Modify: `src-tauri/Cargo.toml` (add `keyring = "X.Y"` per AMD-14 — pin per current cred-handling spec §3.4's keyring-core API surface)

- [ ] **Step 1: Add the `keyring` crate dependency**

```bash
cd src-tauri/
cargo add keyring  # let cargo pick the latest 2.x or 3.x compatible with keyring-core API
```

Verify in `Cargo.toml`: `keyring = "^N.M"` line present.

- [ ] **Step 2: Implement `src-tauri/src/wizard.rs` skeleton**

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "detail")]
pub enum WizardError {
    Unavailable,
    Locked,
    PermissionDenied { platform_hint: String },
    ConfigWrite { detail: String },
    ConfigWriteAndRollbackFailed { config_error: String, rollback_error: String },
    Busy,
    InvalidInput { field: String },
    Other { detail: String },
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "detail")]
pub enum TestSendOutcome {
    Success { reply_subject: Option<String> },
    Failed { cause: String, likely_causes_hint: Vec<String> },
}

/// Single-flight mutex for the 3 write commands (per spec §3.7 multi-window guard).
pub struct WizardMutex(pub Arc<Mutex<()>>);

/// Map a Rust keyring crate error to WizardError. Detailed mapping per spec §3.5.
pub(crate) fn map_keyring_error(err: keyring::Error) -> WizardError {
    use keyring::Error::*;
    match err {
        NoEntry => WizardError::Unavailable,  // entry not found; treated as backend-side issue here
        NoStorageAccess(_) => WizardError::Unavailable,
        PlatformFailure(e) => WizardError::Other { detail: format!("{e}") },
        BadEncoding(_) => WizardError::InvalidInput { field: "password".into() },
        _ => WizardError::Other { detail: format!("{err:?}") },
        // PermissionDenied / Locked detection is platform-specific; refine per integration test
    }
}

/// Read-only: returns whether the wizard has completed (config.json exists + wizard_completed=true).
#[tauri::command]
pub async fn get_wizard_completed() -> Result<bool, WizardError> {
    match crate::config::read_config().ok() {
        Some(cfg) => Ok(cfg.wizard_completed),
        None => Ok(false),
    }
}

/// Writes credentials path config + keyring entry transactionally. See spec §3.2.
#[tauri::command]
pub async fn wizard_persist_cms(
    state: tauri::State<'_, WizardMutex>,
    raw_callsign: String,
    password: String,
    grid: String,
    mbo_address: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    // Full implementation in Task 3.1
    todo!("Task 3.1 fleshes out the wizard_persist_cms transactional flow")
}

#[tauri::command]
pub async fn wizard_persist_offline(
    state: tauri::State<'_, WizardMutex>,
    identifier: String,
    grid: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    // Full implementation in Task 4.1
    todo!("Task 4.1 fleshes out the wizard_persist_offline flow")
}

#[tauri::command]
pub async fn wizard_run_test_send(
    state: tauri::State<'_, WizardMutex>,
) -> Result<TestSendOutcome, WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    // Full implementation in Task 5.1; MOCKED via TUXLINK_TEST_SEND_MOCK env var
    todo!("Task 5.1 fleshes out the wizard_run_test_send flow")
}
```

- [ ] **Step 3: Modify `src-tauri/src/lib.rs` to wire up the module, init keyring, and register commands**

In `src-tauri/src/lib.rs`, add near the top with other module declarations:

```rust
pub mod wizard;
```

In the Tauri app builder (`run()` function or equivalent), BEFORE `.invoke_handler`:

```rust
// Per cred-handling spec §3.4 + AMD-14 — required keyring-core init at app startup.
keyring::set_default_credential_builder(keyring::default_credential_builder());
```

(The exact API depends on the pinned `keyring` crate version — adjust based on what `cargo add` resolved to. The spec mandates the equivalent of `use_native_store(true)?`; current keyring-core API is `set_default_credential_builder`. Confirm via `cargo doc --open -p keyring` after install.)

Also:

```rust
.manage(wizard::WizardMutex(std::sync::Arc::new(tokio::sync::Mutex::new(()))))
.invoke_handler(tauri::generate_handler![
    /* existing commands */,
    wizard::get_wizard_completed,
    wizard::wizard_persist_cms,
    wizard::wizard_persist_offline,
    wizard::wizard_run_test_send,
])
```

- [ ] **Step 4: Verify build succeeds**

```bash
cd src-tauri/
cargo build
```

Expected: compiles. The `todo!()` placeholders are fine for now (they'll only fire at runtime if the command is invoked — which won't happen until Phases 3/4/5 wire up the components).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/wizard.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
feat(wizard): Tauri command skeleton + WizardMutex + keyring crate

Adds wizard module with WizardError + TestSendOutcome enum definitions
(matching src/wizard/types.ts serde shape), the 4-command surface, and
the single-flight tokio::sync::Mutex per spec §3.7. Bodies are todo!()
placeholders fleshed out in Phases 3/4/5.

Registers AMD-14-authorized keyring crate dependency and calls the
keyring-core initialization at app startup per cred-handling spec §3.4.

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Task 1.5: Wire `App.tsx` routing — wizard-vs-shell based on `get_wizard_completed`

**Files:**
- Modify: `src/App.tsx` (read wizard_completed on mount; conditional mount of `<Wizard>` vs main shell)
- Create: `src/wizard/Wizard.tsx` (parent component; mounts the current-step child per `state.step`; just dispatches for now since the children don't exist yet)

- [ ] **Step 1: Write failing test for App routing**

```typescript
// src/App.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

describe('<App>', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders wizard placeholder when wizard_completed=false', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(false);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });

  it('renders main shell placeholder when wizard_completed=true', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('main-shell-root')).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run to confirm failure** — `wizard-root` not found.

- [ ] **Step 3: Implement `src/wizard/Wizard.tsx` (parent)**

```typescript
import { WizardProvider, useWizard } from './wizardContext';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root">
      <p>Wizard step: {state.step}</p>
      {/* Child components (Step1Welcome, etc.) mount here in Phases 2-5 */}
    </div>
  );
}

export function Wizard() {
  return (
    <WizardProvider>
      <WizardInner />
    </WizardProvider>
  );
}
```

- [ ] **Step 4: Modify `src/App.tsx`**

```typescript
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Wizard } from './wizard/Wizard';

function MainShell() {
  return <div data-testid="main-shell-root">Main shell — Tasks 12+ will render here</div>;
}

export default function App() {
  const [wizardCompleted, setWizardCompleted] = useState<boolean | null>(null);
  useEffect(() => {
    invoke<boolean>('get_wizard_completed').then(setWizardCompleted).catch(() => setWizardCompleted(false));
  }, []);
  if (wizardCompleted === null) return <div>Loading…</div>;
  return wizardCompleted ? <MainShell /> : <Wizard />;
}
```

- [ ] **Step 5: Run to confirm pass** — 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/App.test.tsx src/wizard/Wizard.tsx
git commit -m "feat(wizard): App routing — get_wizard_completed gates Wizard vs MainShell mount

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.6: Phase 1 completion — verify build + flip LDC

- [ ] **Step 1: Full build**

```bash
cd src-tauri && cargo build && cd ..
pnpm vitest run wizard/
```

Expected: all tests pass; Rust compiles cleanly.

- [ ] **Step 2: Update LDC banner: Phase 1 ⬜ → ✅** (commit the plan-doc edit).

---

## Phase 2 — Step 1 Welcome (Task 9 / `tuxlink-ko0`)

### Task 2.1: Implement `Step1Welcome.tsx` with choice cards + tests

**Files:**
- Create: `src/wizard/Step1Welcome.tsx`
- Create: `src/wizard/Step1Welcome.test.tsx`
- Modify: `src/wizard/Wizard.tsx` (mount `<Step1Welcome>` when `state.step === 'account'`)

- [ ] **Step 1: Write failing tests** (component renders 2 choice cards, click dispatches reducer actions, advancing routes correctly)

```typescript
// src/wizard/Step1Welcome.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';

function StepWithProbe() {
  const { state } = useWizard();
  return (
    <>
      <Step1Welcome />
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-cms">{String(state.connectToCms)}</div>
    </>
  );
}

describe('<Step1Welcome>', () => {
  it('renders the canonical question + both choice cards', () => {
    render(<WizardProvider><Step1Welcome /></WizardProvider>);
    expect(screen.getByText(/Will this installation connect to the Winlink CMS/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Yes, connect to the Winlink CMS/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /No, this is an offline/i })).toBeInTheDocument();
  });

  it('CMS card click → SET_CONNECT_TO_CMS(true) + ADVANCE → step=credentials', () => {
    render(<WizardProvider><StepWithProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /Yes, connect/i }));
    expect(screen.getByTestId('probe-cms')).toHaveTextContent('true');
    expect(screen.getByTestId('probe-step')).toHaveTextContent('credentials');
  });

  it('Offline card click → SET_CONNECT_TO_CMS(false) + ADVANCE → step=offline_identity', () => {
    render(<WizardProvider><StepWithProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /No, this is an offline/i }));
    expect(screen.getByTestId('probe-cms')).toHaveTextContent('false');
    expect(screen.getByTestId('probe-step')).toHaveTextContent('offline_identity');
  });
});
```

- [ ] **Step 2: Run to confirm failure**.

- [ ] **Step 3: Implement `Step1Welcome.tsx`**

```typescript
import { useWizard } from './wizardContext';

export function Step1Welcome() {
  const { dispatch } = useWizard();
  function choose(connectToCms: boolean) {
    dispatch({ type: 'SET_CONNECT_TO_CMS', payload: connectToCms });
    dispatch({ type: 'ADVANCE_FROM_ACCOUNT' });
  }
  return (
    <div className="wizard-step wizard-step-account">
      <h1>Will this installation connect to the Winlink CMS?</h1>
      <p>Your choice determines whether tuxlink uses internet-backed CMS authentication (most operators) or runs offline (radio-only / drills / lab work).</p>
      <div className="wizard-choice-cards">
        <button type="button" onClick={() => choose(true)} autoFocus>
          <strong>Yes, connect to the Winlink CMS</strong>
          <p>Default. Uses the internet-backed CMS for authentication. You'll enter your callsign and CMS password next.</p>
        </button>
        <button type="button" onClick={() => choose(false)}>
          <strong>No, this is an offline / radio-only deployment</strong>
          <p>For Winlink Hybrid Network operators, ARES drills, EOC tabletops, lab work. No CMS connection attempts.</p>
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Update `Wizard.tsx` to mount Step1Welcome when step is account**

```typescript
import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {state.step === 'credentials' && <p>Step 2 credentials — Phase 3</p>}
      {state.step === 'offline_identity' && <p>Step 2 offline — Phase 4</p>}
      {state.step === 'test_send' && <p>Step 3 test-send — Phase 5</p>}
      {state.step === 'complete' && <p>Wizard complete — shell mounts</p>}
    </div>
  );
}
// (WizardProvider wrap as in Task 1.5)
```

- [ ] **Step 5: Run to confirm pass + commit**.

```bash
git add src/wizard/Step1Welcome.tsx src/wizard/Step1Welcome.test.tsx src/wizard/Wizard.tsx
git commit -m "feat(wizard): Step 1 Welcome — connection-type routing (closes tuxlink-ko0 partial)

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Update LDC: Phase 2 → ✅**.

---

## Phase 3 — Step 2 Credentials + Rust keyring write (Task 10 / `tuxlink-1r5`) — HEART of the cluster

### Task 3.1: Implement `validators.ts` (callsign, password, grid; non-ASCII rejection per spec §5.9)

**Files:**
- Create: `src/wizard/validators.ts`
- Create: `src/wizard/validators.test.ts`

- [ ] **Step 1: Write tests covering loose validator + non-ASCII rejection + edge cases**

```typescript
// src/wizard/validators.test.ts
import { describe, it, expect } from 'vitest';
import { validateCallsign, validatePassword, validateGrid, normalizeGrid } from './validators';

describe('validators', () => {
  describe('validateCallsign', () => {
    it('accepts standard callsigns', () => {
      expect(validateCallsign('W4PHS')).toBeNull();  // null = no error
      expect(validateCallsign('K0SWE-7')).toBeNull();
      expect(validateCallsign('VK2/W4PHS/P')).toBeNull();
    });
    it('accepts tactical strings (AMD-3 loose validator)', () => {
      expect(validateCallsign('EOC-1')).toBeNull();
      expect(validateCallsign('BAOFENG-FM-01')).toBeNull();
    });
    it('rejects empty', () => {
      expect(validateCallsign('')).toMatch(/non-empty/i);
    });
    it('rejects internal whitespace', () => {
      expect(validateCallsign('W 4PHS')).toMatch(/whitespace/i);
    });
    it('rejects >32 chars', () => {
      expect(validateCallsign('A'.repeat(33))).toMatch(/32/);
    });
    it('rejects non-ASCII (Cyrillic А homoglyph)', () => {
      expect(validateCallsign('W4PHSА')).toMatch(/ASCII/i);  // Cyrillic A!
      expect(validateCallsign('W4PHS' + '‍')).toMatch(/ASCII/i);  // zero-width joiner
    });
  });

  describe('validatePassword', () => {
    it('rejects empty', () => {
      expect(validatePassword('')).toMatch(/required/i);
    });
    it('rejects < 6 chars', () => {
      expect(validatePassword('12345')).toMatch(/6/);
    });
    it('accepts 6+ chars', () => {
      expect(validatePassword('secret')).toBeNull();
      expect(validatePassword('a-very-long-passphrase-with-symbols!@#')).toBeNull();
    });
  });

  describe('validateGrid', () => {
    it('accepts 4-char Maidenhead', () => {
      expect(validateGrid('EM75')).toBeNull();
    });
    it('accepts 6-char Maidenhead', () => {
      expect(validateGrid('EM75xx')).toBeNull();
    });
    it('rejects malformed', () => {
      expect(validateGrid('XY99')).toMatch(/Maidenhead/);  // X and Y out of range
      expect(validateGrid('em75abcde')).toMatch(/Maidenhead/);
    });
    it('accepts empty (optional field)', () => {
      expect(validateGrid('')).toBeNull();
    });
  });

  describe('normalizeGrid', () => {
    it('uppercases the first 2 chars + lowercases the last 2', () => {
      expect(normalizeGrid('em75XX')).toBe('EM75xx');
    });
  });
});
```

- [ ] **Step 2: Run to confirm failure**.

- [ ] **Step 3: Implement `validators.ts`**

```typescript
export function validateCallsign(input: string): string | null {
  if (!input) return 'Callsign is required (non-empty).';
  if (/\s/.test(input)) return 'Callsign must contain no internal whitespace.';
  if (input.length > 32) return 'Callsign must be ≤32 characters.';
  // Non-ASCII rejection — defense against homoglyph + zero-width characters per spec §5.9
  // eslint-disable-next-line no-control-regex
  if (!/^[\x20-\x7E]+$/.test(input)) return 'Callsign must contain only ASCII letters, digits, and `/`.';
  return null;
}

export function validatePassword(input: string): string | null {
  if (!input) return 'Password is required.';
  if (input.length < 6) return 'Password must be ≥6 characters (per Express convention).';
  return null;
}

export function validateGrid(input: string): string | null {
  if (!input) return null;  // optional
  const re4 = /^[A-Ra-r]{2}[0-9]{2}$/;
  const re6 = /^[A-Ra-r]{2}[0-9]{2}[A-Xa-x]{2}$/;
  if (!re4.test(input) && !re6.test(input)) {
    return 'Grid must be a 4- or 6-character Maidenhead locator (e.g. EM75 or EM75xx).';
  }
  return null;
}

export function normalizeGrid(input: string): string {
  if (input.length === 4) return input.slice(0, 2).toUpperCase() + input.slice(2);
  return input.slice(0, 2).toUpperCase() + input.slice(2, 4) + input.slice(4).toLowerCase();
}
```

- [ ] **Step 4: Run + commit**.

### Task 3.2: Flesh out `wizard_persist_cms` in `src-tauri/src/wizard.rs` per spec §3.2

**Files:**
- Modify: `src-tauri/src/wizard.rs` (replace `todo!()` body)
- Create: `src-tauri/tests/wizard_persist_cms_test.rs`

- [ ] **Step 1: Write unit tests for each error path**

```rust
// src-tauri/tests/wizard_persist_cms_test.rs — uses a MOCK keyring (feature-gate keyring's mock-keyring or build a thin trait wrapper)
// NOTE: integration tests with real keyring run in Phase 6
```

Stub the unit tests at this granularity: happy path (valid input → ok), invalid callsign → `InvalidInput`, Busy on second concurrent invocation, snapshot-and-restore on config-write failure.

(Full code omitted here for brevity; implementing agent writes per the spec §3.5 error table — 8 variants + the `Busy` race + snapshot-and-restore.)

- [ ] **Step 2: Flesh out `wizard_persist_cms` body**

Per spec §3.2:
1. `let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;`
2. `let callsign = raw_callsign.trim().to_uppercase();`
3. Validate normalized callsign (`is_ascii` + `validate_identity` from Task 2's config crate).
4. Build the new `Config` struct from inputs (`crate::config::Config` per AMD-1 nested shape).
5. `let entry = keyring::Entry::new("tuxlink-pat", &callsign).map_err(map_keyring_error)?;`
6. `let prior = entry.get_password().ok();` (snapshot for rollback)
7. `entry.set_password(&password).map_err(map_keyring_error)?;`
8. `if let Err(e) = crate::config::write_config_atomic(&new_config) {`
   `   let rollback = match prior { Some(p) => entry.set_password(&p), None => entry.delete_credential() };`
   `   match rollback { Ok(_) => return Err(WizardError::ConfigWrite { detail: format!("{e}") }), Err(re) => return Err(WizardError::ConfigWriteAndRollbackFailed { config_error: format!("{e}"), rollback_error: format!("{re}") }) }`
   `}`
9. `Ok(())`

- [ ] **Step 3: Run tests + commit**.

### Task 3.3: Implement `Step2Credentials.tsx` (UI + submit handler with invoke + reducer dispatches)

**Files:**
- Create: `src/wizard/Step2Credentials.tsx`
- Create: `src/wizard/Step2Credentials.test.tsx`
- Modify: `src/wizard/Wizard.tsx` (mount when step=credentials)

- [ ] **Step 1: Write tests** (form renders, validation blocks submit, valid submit calls `invoke('wizard_persist_cms')` + dispatches SUBMIT_BEGIN then SUBMIT_CREDENTIALS_SUCCESS on success; ErrUnavailable shows the §3.5 message; Save-and-skip routes to complete).

(Full test code follows the patterns from Task 1.2's wizardReducer.test.ts + Task 2.1's Step1Welcome.test.tsx — implementing agent writes per the spec §3.3 Step 2-CMS description.)

- [ ] **Step 2: Implement `Step2Credentials.tsx`** — form with callsign / password / grid / MBO inputs, Continue + Save-and-skip buttons, per-error UX (spec §3.5), header Register link via `tauri-plugin-shell::open` (system browser per spec §3.7), submit handler calls `invoke('wizard_persist_cms', {rawCallsign, password, grid, mboAddress})`.

- [ ] **Step 3: Wire into Wizard.tsx; run + commit**.

### Task 3.4: Create Tauri capability declaration

**Files:**
- Create: `src-tauri/capabilities/wizard.json`
- Modify: `src-tauri/tauri.conf.json` (declare capability + CSP per spec §3.7)

- [ ] **Step 1: Author `src-tauri/capabilities/wizard.json`**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "wizard",
  "description": "First-run wizard cluster commands (Tasks 9 + 10 + 11 + 11.5)",
  "windows": ["main"],
  "permissions": [
    "wizard:allow-get-wizard-completed",
    "wizard:allow-wizard-persist-cms",
    "wizard:allow-wizard-persist-offline",
    "wizard:allow-wizard-run-test-send"
  ]
}
```

- [ ] **Step 2: Modify `tauri.conf.json` to register the capability + CSP per spec §3.7**

Add to `app.security`:

```json
"csp": "default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:;"
```

Add `wizard` to the capabilities list.

- [ ] **Step 3: Add `tauri-plugin-shell` for the Register link**

```bash
cd src-tauri/ && cargo add tauri-plugin-shell
# also add to tauri.conf.json plugins; allowlist winlink.org URLs in shell.open scope
```

- [ ] **Step 4: Build + verify** (`cargo build` succeeds; tauri.conf.json validates).

- [ ] **Step 5: Commit**.

### Task 3.5: Phase 3 completion — flip LDC

- [ ] **Step 1: Update LDC banner: Phase 3 → ✅**.

---

## Phase 4 — Step 2 Offline Identity (Task 11.5 / `tuxlink-d76`)

### Task 4.1: Flesh out `wizard_persist_offline` in `src-tauri/src/wizard.rs`

Mirrors Task 3.2 but single-write (config.json only). Hardcodes `connect.connect_to_cms = false`, `identity.callsign = null`, `pat_mbo_address = null`, and includes the `identity.identifier` + `identity.grid` fields from inputs (null if empty).

### Task 4.2: Implement `Step2OfflineIdentity.tsx`

Simple form with 2 optional fields + 1 submit button. Pattern follows Task 3.3 but lighter. Footer copy per spec §3.3 + design doc §5.4.

### Task 4.3: Wire + test + commit + flip LDC Phase 4 → ✅

---

## Phase 5 — Step 3 Test Send (Task 11 / `tuxlink-e4x`) — 4-substate + Part-97 guard

### Task 5.1: Flesh out `wizard_run_test_send` in `src-tauri/src/wizard.rs`

**Files:**
- Modify: `src-tauri/src/wizard.rs` (replace todo body)
- Create: `src-tauri/tests/wizard_run_test_send_test.rs`

- [ ] **Step 1: Implement mock-mode gating** — at the top of `wizard_run_test_send`, check `std::env::var("TUXLINK_TEST_SEND_MOCK").is_ok()`. If set, return a mocked outcome alternating Success / Failed; do NOT invoke `pat_client.send()`. Mocked branch covers the agent-safe browser smoke per spec §3.8.

- [ ] **Step 2: Real-mode implementation** — when env var NOT set:
  - Read tuxlink config (callsign, MBO address).
  - Invoke `pat_client.send(SERVICE@winlink.org, /test/<callsign>-YYYYMMDD-HHMMSS, body, RFC3339 date)`.
  - Poll Pat's inbox (`pat_client.list(Inbox)`) every 2 seconds for up to 30 seconds.
  - For each poll, emit `wizard:test_send:log` Tauri event with a human-shaped projection line.
  - On reply detected: return `TestSendOutcome::Success { reply_subject: Some(reply.subject) }`.
  - On timeout: return `TestSendOutcome::Failed { cause: "no autoresponder within 30s", likely_causes_hint: vec!["captive portal / network login page", "CMS busy", "outbound port 8773 blocked"] }`.
  - On `pat_client` error: return `TestSendOutcome::Failed { cause: format!("{err}"), likely_causes_hint: vec!["pat sidecar crashed", "tuxlink-side I/O error"] }`.

- [ ] **Step 3: Tests** (unit-level; mock pat_client; cover mock-mode success/fail alternation, real-mode happy path, timeout, pat-error).

### Task 5.2: Implement `Step3TestSend.tsx` with 4 substates

**Files:**
- Create: `src/wizard/Step3TestSend.tsx`
- Create: `src/wizard/Step3TestSend.test.tsx`

- [ ] **Step 1: Tests for each substate's UI + controls** — verify `[Send test]` button is UNCONDITIONALLY ABSENT (not just disabled) when substate ≠ idle (Part 97 correctness per spec §3.1 invariant 2 + §5.8). Verify Skip + Edit credentials + Retry + Go to inbox + Open Settings buttons per spec §3.4 table.

- [ ] **Step 2: Implementation** — subscribe to `wizard:test_send:log` Tauri event with `listen()` from `@tauri-apps/api/event`; dispatch `TEST_SEND_LOG_LINE`. Render substate-specific UI per spec §3.4 + §5.12 (captive-portal in failed-substate likely-causes).

- [ ] **Step 3: Wire + test + commit**.

### Task 5.3: Phase 5 completion — flip LDC Phase 5 → ✅

---

## Phase 6 — CI integration tests + gnome-keyring-daemon setup

### Task 6.1: Author `wizard_integration_test.rs` (CI-only via `--ignored`)

**Files:**
- Create: `src-tauri/tests/wizard_integration_test.rs`

Per spec §3.8. Tests run with `cargo test --test wizard_integration_test --ignored`. Uses `gnome-keyring-daemon + dbus-launch` started by the CI workflow (Task 6.2). Cases:
1. Wizard write via `keyring::Entry::new` at `(service="tuxlink-pat", account="W4PHS")` → read back via same crate → password matches.
2. `wizard_persist_cms` happy path → assert keyring entry exists at the expected `(service, account)` + tuxlink config.json shape.
3. Snapshot-and-restore: pre-write a keyring entry, simulate config-write failure (e.g., point write path at `/dev/full`), assert the prior keyring entry is restored.

### Task 6.2: Create `.github/workflows/wizard-test.yml`

Per cred-handling plan Phase 9 CI recipe — copy verbatim adapted for the wizard tests. `dbus-launch` + `gnome-keyring-daemon --components=secrets` + integration `cargo test --test wizard_integration_test --ignored` on push + PR.

### Task 6.3: Author `dev/scratch/cross-validate-wizard-pat.sh`

Per spec §3.9. 5-line shell script: writes via a tiny Rust helper (or `secret-tool store` directly), reads via `secret-tool get service tuxlink-pat account W4PHS`, asserts equality. Developer-local sanity check for the cross-language keyring contract.

### Task 6.4: Phase 6 completion — flip LDC Phase 6 → ✅

---

## Phase 7 — Browser smoke documentation (operator-only LIVE + agent-safe MOCKED)

### Task 7.1: Create `docs/wizard-smoke-testing.md`

Author per spec §3.8. Two sections: agent-safe MOCKED mode (default; `TUXLINK_TEST_SEND_MOCK=1`) + operator-only LIVE mode (with RADIO-1 consent gate verbatim from `docs/live-cms-testing-policy.md`).

### Task 7.2: Cross-link from `docs/live-cms-testing-policy.md`

Add a paragraph: "The first-run wizard's Step 3 test-send is a live-CMS surface subject to this policy. The wizard's `wizard_run_test_send` command honors the `TUXLINK_TEST_SEND_MOCK=1` env var which short-circuits the live transmission. Subagents MUST set this env var; operators MAY unset it to exercise the real CMS round-trip under explicit RADIO-1 consent."

### Task 7.3: Phase 7 completion — flip LDC Phase 7 → ✅

---

## Phase 8 — bd cleanup + PR-B + final review

### Task 8.1: Verify full integration

- [ ] `cd src-tauri && cargo test` — all unit tests pass.
- [ ] `cd src-tauri && cargo test --test wizard_integration_test --ignored` (after starting `dbus-launch + gnome-keyring-daemon`) — integration tests pass.
- [ ] `pnpm vitest run` — all frontend tests pass.
- [ ] `cargo build --release` — release build passes (the Pat sidecar build kicks in here).
- [ ] Manual browser smoke (MOCKED): `TUXLINK_TEST_SEND_MOCK=1 pnpm tauri dev`; walk all 4 paths.

### Task 8.2: Open PR-B against feat/v0.0.1

PR title: `[<MONIKER>] feat(wizard): onboarding wizard cluster (Tasks 9 + 10 + 11 + 11.5) — closes tuxlink-ln3 + ko0 + 1r5 + d76 + e4x`
PR body: summary + per-phase deliverables + test plan checklist + RADIO-1 + Codex post-impl review note.

### Task 8.3: Run parent-level Codex review on the impl commits

Per memory `feedback_codex_post_subagent_review`. Codex round on the impl PR commits to catch self-review bias. Note Codex sandbox-write workaround per CLAUDE.md Codex section + AMD-cyy PR-63 follow-up.

### Task 8.4: On Codex clean + operator review pass → merge PR-B

bd close `tuxlink-ko0` + `tuxlink-1r5` + `tuxlink-d76` + `tuxlink-e4x` + `tuxlink-ln3` (last one closes via deliverable on this PR-B merge).

### Task 8.5: Phase 8 completion — flip LDC all phases → ✅; dispose worktree per ADR 0009

---

## Plan-Review Disposition (post-cycle)

_(To be filled in after the 4-round plan-review-cycle per `superpowers:plan-review-cycle`.)_

Planned rounds:
- R1 — Claude subagent, execution-friction lens
- R2 — Claude subagent, contract-verification lens (does each task's tests cover the spec's invariants?)
- R3 — Claude subagent, coverage lens (any spec requirement not implemented by a task?)
- R4 — Codex CLI, cross-provider lens (with stdout-fallback per CLAUDE.md Codex section)

Findings consolidated in a plan revision commit.

---

## References

- **Spec of record:** `docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md` (commit `da605e4`)
- **Cred-handling spec:** `docs/superpowers/specs/2026-05-18-cred-handling-design.md` (Pat-side keyring contract)
- **Cred-handling plan:** `docs/plans/2026-05-18-cred-handling-plan.md` (CI gnome-keyring-daemon recipe template for Phase 6)
- **Hard prerequisite:** `tuxlink-756` (Task 3 PatProcess amendment) — must land first
- **Base plan:** `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Tasks 9, 10, 11, 11.5 + AMDs 1-16
- **ADR 0011** (Pat fork rationale)
- **CLAUDE.md** §"Live radio network operations" + §"Codex CLI" (with sandbox-write workaround per PR-63)
