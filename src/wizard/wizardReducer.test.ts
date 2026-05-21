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

  it('SUBMIT_CREDENTIALS_SUCCESS clears password and routes per skipTestSend flag', () => {
    const base = { ...initialWizardState(), step: 'credentials' as const, callsign: 'W4PHS', password: 'secret', inFlight: true };
    const s1 = wizardReducer(base, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipTestSend: false });
    expect(s1.password).toBe('');
    expect(s1.step).toBe('test_send');
    expect(s1.inFlight).toBe(false);
    const s2 = wizardReducer({ ...base, inFlight: true }, { type: 'SUBMIT_CREDENTIALS_SUCCESS', skipTestSend: true });
    expect(s2.step).toBe('complete');
  });

  // INVARIANT: BEGIN_TEST_SEND while sending is a no-op (Part 97 correctness)
  it('BEGIN_TEST_SEND while testSendSubstate=sending returns state unchanged', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'BEGIN_TEST_SEND' });
    expect(s2).toBe(s);
  });

  it('BEGIN_TEST_SEND from idle transitions to sending + resets log + clears skipSignaled', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'idle' as const, testSendLog: ['stale'], skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'BEGIN_TEST_SEND' });
    expect(s2.testSendSubstate).toBe('sending');
    expect(s2.testSendLog).toEqual([]);
    expect(s2.skipSignaled).toBe(false);
  });

  // FIX 1 (P0a): RETRY_TEST_SEND maps failed → sending so the retry gesture
  // goes THROUGH the reducer (React leaves `failed` before/at invoke), preserving
  // one-consent-one-transmission. BEGIN_TEST_SEND remains strictly idle→sending.
  it('RETRY_TEST_SEND from failed transitions to sending + resets log + clears skipSignaled', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'failed' as const,
      testSendError: 'connection refused', testSendLog: ['stale'], skipSignaled: true };
    const s2 = wizardReducer(s, { type: 'RETRY_TEST_SEND' });
    expect(s2.testSendSubstate).toBe('sending');
    expect(s2.testSendLog).toEqual([]);
    expect(s2.skipSignaled).toBe(false);
    expect(s2.testSendError).toBeNull();
  });

  // INVARIANT (dedup): RETRY_TEST_SEND from sending/success/idle is a strict no-op.
  it('RETRY_TEST_SEND while sending is a strict no-op (dedup invariant)', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'sending' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_TEST_SEND' });
    expect(s2).toBe(s);
  });

  it('RETRY_TEST_SEND while success is a strict no-op (dedup invariant)', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'success' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_TEST_SEND' });
    expect(s2).toBe(s);
  });

  it('RETRY_TEST_SEND while idle is a strict no-op (only failed may retry)', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'idle' as const };
    const s2 = wizardReducer(s, { type: 'RETRY_TEST_SEND' });
    expect(s2).toBe(s);
  });

  // INVARIANT: BEGIN_TEST_SEND from failed remains a strict no-op (the retry path
  // is RETRY_TEST_SEND, not BEGIN_TEST_SEND; BEGIN stays idle-only per §3.1 inv 2).
  it('BEGIN_TEST_SEND while failed returns state unchanged (idle-only guard preserved)', () => {
    const s = { ...initialWizardState(), step: 'test_send' as const, testSendSubstate: 'failed' as const, testSendError: 'err' };
    const s2 = wizardReducer(s, { type: 'BEGIN_TEST_SEND' });
    expect(s2).toBe(s);
  });

  // INVARIANT: TEST_SEND_RESULT ignored when skipSignaled
  it('TEST_SEND_RESULT after SKIP_TEST_SEND is silently ignored (skipSignaled gate)', () => {
    let s: WizardState = { ...initialWizardState(), step: 'test_send', testSendSubstate: 'sending' };
    s = wizardReducer(s, { type: 'SKIP_TEST_SEND' });
    expect(s.step).toBe('complete');
    expect(s.skipSignaled).toBe(true);
    const s2 = wizardReducer(s, { type: 'TEST_SEND_RESULT', outcome: { kind: 'Success', detail: { reply_subject: 'test' } } });
    expect(s2).toBe(s);
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
    expect(s2.testSendSubstate).toBe('idle');
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
