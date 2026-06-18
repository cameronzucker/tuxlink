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
  // In-app Winlink account creation (tuxlink-vfb3 sub-project 1). Reached from
  // `credentials` via the "Create a Winlink account" affordance; on success it joins
  // the existing cms_verify → location → complete tail. NOT an up-front fork.
  | 'account_create'
  | 'offline_identity'
  | 'cms_verify'
  // Location / GPS-source setup (tuxlink-9xy1). Every identity path threads through
  // here before `complete`, so GPS setup assistance is part of first-run onboarding —
  // the wizard-chrome counterpart to Settings → Location. Grid + source persist via
  // config_set_grid / position_set_source (the same commands the Settings panel uses).
  | 'location'
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
  // GO_TO_ACCOUNT_CREATE: the "Create a Winlink account" affordance on the credentials
  // step (tuxlink-vfb3). Only meaningful from `credentials`.
  | { type: 'GO_TO_ACCOUNT_CREATE' }
  // ACCOUNT_CREATE_SUCCESS: the CMS account was created + identity persisted; clear the
  // password and join the verify tail (mirrors the non-skip SUBMIT_CREDENTIALS_SUCCESS).
  | { type: 'ACCOUNT_CREATE_SUCCESS' }
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
  | { type: 'RETURN_TO_CREDENTIALS' }
  // ADVANCE_FROM_LOCATION: the Location step's "Continue" — grid + source are
  // already persisted (via config_set_grid / position_set_source) by the time this
  // fires, so it only advances `location → complete`. Location is non-blocking:
  // Continue is always available (grid is optional everywhere in onboarding).
  | { type: 'ADVANCE_FROM_LOCATION' };
