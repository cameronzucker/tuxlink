import { describe, it, expect } from 'vitest';
import { wizardReducer, initialWizardState } from './wizardReducer';
import type { WizardState } from './types';

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
    expect(s.step).toBe('account');
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

  it('SUBMIT_CREDENTIALS_SUCCESS clears password and routes per skipCmsVerify flag', () => {
    const base = { ...initialWizardState(), step: 'credentials' as const, callsign: 'W4PHS', password: 'secret', inFlight: true };
    const s1 = wizardReducer(base, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipCmsVerify: false });
    expect(s1.password).toBe('');
    expect(s1.step).toBe('cms_verify');
    expect(s1.inFlight).toBe(false);
    // skip-verify now lands on the Location step (not straight to complete) — tuxlink-9xy1.
    const s2 = wizardReducer({ ...base, inFlight: true }, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipCmsVerify: true });
    expect(s2.step).toBe('location');
  });

  // tuxlink-9xy1: every identity path threads through the Location step before complete.
  it('SUBMIT_OFFLINE_SUCCESS routes to the Location step (not straight to complete)', () => {
    const base = { ...initialWizardState(), step: 'offline_identity' as const, inFlight: true };
    const s = wizardReducer(base, { type: 'SUBMIT_OFFLINE_SUCCESS' });
    expect(s.step).toBe('location');
    expect(s.inFlight).toBe(false);
  });

  it('ADVANCE_FROM_LOCATION routes location → complete', () => {
    const base = { ...initialWizardState(), step: 'location' as const };
    const s = wizardReducer(base, { type: 'ADVANCE_FROM_LOCATION' });
    expect(s.step).toBe('complete');
  });

  // INVARIANT: BEGIN_CMS_VERIFY while probing is a no-op (dedup correctness)
  it('BEGIN_CMS_VERIFY while cmsVerifySubstate=probing returns state unchanged', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'BEGIN_CMS_VERIFY' });
    expect(s2).toBe(s);
  });

  it('BEGIN_CMS_VERIFY from idle transitions to probing + resets log + clears skipSignaled', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'idle' as const, cmsVerifyLog: ['stale'], skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'BEGIN_CMS_VERIFY' });
    expect(s2.cmsVerifySubstate).toBe('probing');
    expect(s2.cmsVerifyLog).toEqual([]);
    expect(s2.skipSignaled).toBe(false);
  });

  // RETRY_CMS_VERIFY maps error → probing so the retry gesture goes THROUGH the
  // reducer (React leaves `error` before/at invoke), preserving the dedup invariant.
  // BEGIN_CMS_VERIFY remains strictly idle→probing.
  it('RETRY_CMS_VERIFY from error transitions to probing + resets log + clears skipSignaled', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'error' as const,
      cmsVerifyError: 'connection refused', cmsVerifyLog: ['stale'], skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'RETRY_CMS_VERIFY' });
    expect(s2.cmsVerifySubstate).toBe('probing');
    expect(s2.cmsVerifyLog).toEqual([]);
    expect(s2.skipSignaled).toBe(false);
    expect(s2.cmsVerifyError).toBeNull();
  });

  // INVARIANT (dedup): RETRY_CMS_VERIFY from probing/ok/idle is a strict no-op.
  it('RETRY_CMS_VERIFY while probing is a strict no-op (dedup invariant)', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_CMS_VERIFY' });
    expect(s2).toBe(s);
  });

  it('RETRY_CMS_VERIFY while ok is a strict no-op (dedup invariant)', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'ok' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_CMS_VERIFY' });
    expect(s2).toBe(s);
  });

  it('RETRY_CMS_VERIFY while idle is a strict no-op (only error may retry)', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'idle' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_CMS_VERIFY' });
    expect(s2).toBe(s);
  });

  // INVARIANT: BEGIN_CMS_VERIFY from error remains a strict no-op (the retry path
  // is RETRY_CMS_VERIFY, not BEGIN_CMS_VERIFY; BEGIN stays idle-only).
  it('BEGIN_CMS_VERIFY while error returns state unchanged (idle-only guard preserved)', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'error' as const, cmsVerifyError: 'err' };
    const s2 = wizardReducer(s, { type: 'BEGIN_CMS_VERIFY' });
    expect(s2).toBe(s);
  });

  // INVARIANT: CMS_VERIFY_RESULT ignored when skipSignaled
  it('CMS_VERIFY_RESULT after SKIP_CMS_VERIFY is silently ignored (skipSignaled gate)', () => {
    let s: WizardState = { ...initialWizardState(), step: 'cms_verify', cmsVerifySubstate: 'probing' };
    s = wizardReducer(s, { type: 'SKIP_CMS_VERIFY' });
    // SKIP now lands on the Location step (not complete) but still sets skipSignaled
    // so a late result from the abandoned probe is ignored (tuxlink-9xy1).
    expect(s.step).toBe('location');
    expect(s.skipSignaled).toBe(true);
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_RESULT', ok: true });
    expect(s2).toBe(s);
  });

  it('CMS_VERIFY_RESULT ok=true transitions probing → ok', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_RESULT', ok: true });
    expect(s2.cmsVerifySubstate).toBe('ok');
  });

  it('CMS_VERIFY_RESULT ok=false populates cmsVerifyError + transitions probing → error', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_RESULT', ok: false, errorMessage: 'wrong password' });
    expect(s2.cmsVerifySubstate).toBe('error');
    expect(s2.cmsVerifyError).toBe('wrong password');
  });

  it('CMS_VERIFY_RESULT ok=false with no errorMessage uses fallback string', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_RESULT', ok: false });
    expect(s2.cmsVerifySubstate).toBe('error');
    expect(s2.cmsVerifyError).toBeTruthy();
  });

  it('RETURN_TO_CREDENTIALS from error substate clears password but preserves callsign/grid/MBO', () => {
    const s = { ...initialWizardState(), step: 'cms_verify' as const, cmsVerifySubstate: 'error' as const,
      callsign: 'W4PHS', password: '', grid: 'EM75', mboAddress: 'W4PHS@winlink.org' };
    const s2 = wizardReducer(s, { type: 'RETURN_TO_CREDENTIALS' });
    expect(s2.step).toBe('credentials');
    expect(s2.password).toBe('');
    expect(s2.callsign).toBe('W4PHS');
    expect(s2.grid).toBe('EM75');
    expect(s2.mboAddress).toBe('W4PHS@winlink.org');
    expect(s2.cmsVerifySubstate).toBe('idle');
  });

  it('CMS_VERIFY_LOG_LINE appends to cmsVerifyLog', () => {
    const s = { ...initialWizardState(), cmsVerifySubstate: 'probing' as const };
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_LOG_LINE', line: 'Connecting via CMS-SSL...' });
    expect(s2.cmsVerifyLog).toEqual(['Connecting via CMS-SSL...']);
  });

  it('CMS_VERIFY_LOG_LINE ignored when skipSignaled', () => {
    const s = { ...initialWizardState(), cmsVerifySubstate: 'probing' as const, skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'CMS_VERIFY_LOG_LINE', line: 'stale' });
    expect(s2).toBe(s);
  });
});
