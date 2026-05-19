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
