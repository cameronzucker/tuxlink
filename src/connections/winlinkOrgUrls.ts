// Hardcoded module-level URL constants per spec §4.4 + R2 #9.
// MUST NOT be parameterized, interpolated from config, or read from
// runtime values. The Tauri capability scope (capabilities/default.json)
// also denies any URL outside these hosts.

export const WINLINK_ORG_PASSWORD_RESET_URL =
  'https://winlink.org/user/password-recovery';

export const WINLINK_ORG_ACCOUNT_URL =
  'https://winlink.org/user/account';

export const TUXLINK_GITHUB_ISSUE_NEW_URL =
  'https://github.com/cameronzucker/tuxlink/issues/new';
