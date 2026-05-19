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
      if (state.connectToCms === null) return state;
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
        password: '',
        step: action.skipTestSend ? 'complete' : 'test_send',
        inFlight: false,
      };

    case 'SUBMIT_OFFLINE_SUCCESS':
      return { ...state, step: 'complete', inFlight: false };

    case 'SUBMIT_FAILURE':
      return { ...state, inFlight: false };

    case 'BEGIN_TEST_SEND':
      if (state.testSendSubstate !== 'idle') return state;
      return { ...state, testSendSubstate: 'sending', testSendLog: [], skipSignaled: false };

    case 'TEST_SEND_LOG_LINE':
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
