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

// TestSendOutcome removed (Task 5.4 / tuxlink-9phd): the Pat-based test-send is
// replaced by verify_cms_connection, which returns Result<(), WizardError>. The
// FE state machine uses CmsVerifySubstate + cmsVerifyError directly.

export type WizardStep =
  | 'account'
  | 'credentials'
  | 'offline_identity'
  | 'cms_verify'
  | 'complete';

export interface WizardState {
  step: WizardStep;
  connectToCms: boolean | null;
  callsign: string;
  password: string;
  identifier: string;
  grid: string;
  mboAddress: string;
  cmsVerifySubstate: 'idle' | 'probing' | 'ok' | 'error';
  cmsVerifyError: string | null;
  cmsVerifyLog: string[];
  inFlight: boolean;
  skipSignaled: boolean;
}

export type WizardAction =
  | { type: 'SET_CONNECT_TO_CMS'; payload: boolean }
  | { type: 'ADVANCE_FROM_ACCOUNT' }
  | { type: 'SET_CREDENTIALS_FIELD'; field: 'callsign' | 'password' | 'grid' | 'mboAddress'; value: string }
  | { type: 'SET_OFFLINE_FIELD'; field: 'identifier' | 'grid'; value: string }
  | { type: 'SUBMIT_BEGIN' }
  | { type: 'SUBMIT_CREDENTIALS_SUCCESS'; skipCmsVerify: boolean }
  | { type: 'SUBMIT_OFFLINE_SUCCESS' }
  | { type: 'SUBMIT_FAILURE'; error: WizardError }
  | { type: 'BEGIN_CMS_VERIFY' }
  // RETRY_CMS_VERIFY: error → probing. Distinct from BEGIN_CMS_VERIFY (which stays
  // strictly idle → probing per spec §3.1 invariant 2). Routing the [Retry] gesture
  // through the reducer makes React leave `error` and enter `probing` at the moment
  // of invoke, so the Retry control (rendered only in `error`) is gone while a probe
  // is in flight — preserving one-consent-one-connection invariant.
  | { type: 'RETRY_CMS_VERIFY' }
  | { type: 'CMS_VERIFY_LOG_LINE'; line: string }
  // CMS_VERIFY_RESULT: the verify_cms_connection command returned.
  // ok=true: connection verified; ok=false: error with message.
  | { type: 'CMS_VERIFY_RESULT'; ok: boolean; errorMessage?: string }
  | { type: 'SKIP_CMS_VERIFY' }
  | { type: 'RETURN_TO_CREDENTIALS' };
