import { describe, it, expect } from 'vitest';
import {
  WINLINK_ORG_PASSWORD_RESET_URL,
  WINLINK_ORG_ACCOUNT_URL,
  TUXLINK_GITHUB_ISSUE_NEW_URL,
} from './winlinkOrgUrls';

describe('winlinkOrgUrls', () => {
  it('WINLINK_ORG_PASSWORD_RESET_URL targets winlink.org over https', () => {
    expect(WINLINK_ORG_PASSWORD_RESET_URL.startsWith('https://winlink.org/')).toBe(true);
  });
  it('WINLINK_ORG_ACCOUNT_URL targets winlink.org over https', () => {
    expect(WINLINK_ORG_ACCOUNT_URL.startsWith('https://winlink.org/')).toBe(true);
  });
  it('TUXLINK_GITHUB_ISSUE_NEW_URL targets the issue-template chooser (uhpn — not the blank /issues/new)', () => {
    expect(TUXLINK_GITHUB_ISSUE_NEW_URL.startsWith('https://github.com/cameronzucker/tuxlink/')).toBe(true);
    expect(TUXLINK_GITHUB_ISSUE_NEW_URL.endsWith('/issues/new/choose')).toBe(true);
  });
});
