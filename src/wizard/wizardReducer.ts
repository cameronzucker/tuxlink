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
    cmsVerifySubstate: 'idle',
    cmsVerifyError: null,
    cmsVerifyLog: [],
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
        step: action.skipCmsVerify ? 'complete' : 'cms_verify',
        inFlight: false,
      };

    case 'SUBMIT_OFFLINE_SUCCESS':
      return { ...state, step: 'complete', inFlight: false };

    case 'SUBMIT_FAILURE':
      return { ...state, inFlight: false };

    case 'BEGIN_CMS_VERIFY':
      if (state.cmsVerifySubstate !== 'idle') return state;
      return { ...state, cmsVerifySubstate: 'probing', cmsVerifyLog: [], skipSignaled: false };

    case 'RETRY_CMS_VERIFY':
      // Retry path: error → probing. Strict no-op from any other substate (dedup
      // invariant — a retry is only meaningful from `error`). Clears the prior
      // error + log and resets skipSignaled so the new attempt starts clean.
      // Routing [Retry] through this transition (instead of bypassing the reducer)
      // ensures React leaves `error` BEFORE the invoke, so the Retry control is
      // absent while a probe is in flight — Part 97 one-consent-one-connection.
      if (state.cmsVerifySubstate !== 'error') return state;
      return {
        ...state,
        cmsVerifySubstate: 'probing',
        cmsVerifyError: null,
        cmsVerifyLog: [],
        skipSignaled: false,
      };

    case 'CMS_VERIFY_LOG_LINE':
      if (state.skipSignaled) return state;
      return { ...state, cmsVerifyLog: [...state.cmsVerifyLog, action.line] };

    case 'CMS_VERIFY_RESULT':
      if (state.skipSignaled) return state;
      if (action.ok) {
        return { ...state, cmsVerifySubstate: 'ok' };
      }
      return {
        ...state,
        cmsVerifySubstate: 'error',
        cmsVerifyError: action.errorMessage ?? 'Unknown error',
      };

    case 'SKIP_CMS_VERIFY':
      return { ...state, step: 'complete', skipSignaled: true };

    case 'RETURN_TO_CREDENTIALS':
      return {
        ...state,
        step: 'credentials',
        password: '',
        cmsVerifySubstate: 'idle',
        cmsVerifyError: null,
        cmsVerifyLog: [],
      };

    default:
      return state;
  }
}
