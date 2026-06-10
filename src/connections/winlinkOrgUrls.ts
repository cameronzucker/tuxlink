// Hardcoded module-level URL constants per spec §4.4 + R2 #9.
// MUST NOT be parameterized, interpolated from config, or read from
// runtime values. The Tauri capability scope (capabilities/default.json)
// also denies any URL outside these hosts.

export const WINLINK_ORG_PASSWORD_RESET_URL =
  'https://winlink.org/user/password-recovery';

export const WINLINK_ORG_ACCOUNT_URL =
  'https://winlink.org/user/account';

// uhpn: the template chooser, not the bare `/issues/new`. The bare path opens
// GitHub's blank issue editor and bypasses the repo's bug_report.yml (which
// `blank_issues_enabled: false` is meant to enforce); `/choose` reliably shows
// the Bug report form. Still a static constant — no parameterization.
export const TUXLINK_GITHUB_ISSUE_NEW_URL =
  'https://github.com/cameronzucker/tuxlink/issues/new/choose';
