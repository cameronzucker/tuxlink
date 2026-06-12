import { describe, it, expect } from 'vitest';
import { messageMatchesIdentity, deriveIdentityFilterOptions } from './identityFilter';
import type { IdentityListDto } from '../shell/identityTypes';

describe('messageMatchesIdentity', () => {
  it('a tagged message matches the All filter (null)', () => {
    expect(messageMatchesIdentity({ identity: 'W1ABC' }, null)).toBe(true);
  });

  it('a tagged message matches its own identity', () => {
    expect(messageMatchesIdentity({ identity: 'W1ABC' }, 'W1ABC')).toBe(true);
  });

  it('a tagged message does NOT match a different identity', () => {
    expect(messageMatchesIdentity({ identity: 'W1ABC' }, 'W7XYZ')).toBe(false);
  });

  it('an untagged message matches ONLY the All filter', () => {
    expect(messageMatchesIdentity({ identity: undefined }, 'W1ABC')).toBe(false);
    expect(messageMatchesIdentity({ identity: undefined }, null)).toBe(true);
  });

  it('a message with no identity key at all matches only All', () => {
    expect(messageMatchesIdentity({}, null)).toBe(true);
    expect(messageMatchesIdentity({}, 'W1ABC')).toBe(false);
  });
});

describe('deriveIdentityFilterOptions', () => {
  function list(over: Partial<IdentityListDto> = {}): IdentityListDto {
    return {
      full: [],
      tactical: [],
      last_selected: null,
      ...over,
    };
  }

  it('returns just All when the list is null', () => {
    expect(deriveIdentityFilterOptions(null)).toEqual([
      { value: null, label: 'All identities' },
    ]);
  });

  it('returns All when the list is empty', () => {
    expect(deriveIdentityFilterOptions(list())).toEqual([
      { value: null, label: 'All identities' },
    ]);
  });

  it('prepends All then one option per FULL callsign then per tactical label', () => {
    const options = deriveIdentityFilterOptions(
      list({
        full: [
          { callsign: 'W1ABC', label: null, has_cms_account: true, cms_registered: true, needs_auth: false },
          { callsign: 'W7XYZ', label: 'Home', has_cms_account: false, cms_registered: false, needs_auth: true },
        ],
        tactical: [
          { label: 'EOC', parent: 'W1ABC', cms_badge: 'registered' },
          { label: 'SHELTER-1', parent: 'W1ABC', cms_badge: 'unknown' },
        ],
      }),
    );
    expect(options).toEqual([
      { value: null, label: 'All identities' },
      { value: 'W1ABC', label: 'W1ABC' },
      { value: 'W7XYZ', label: 'W7XYZ' },
      { value: 'EOC', label: 'EOC' },
      { value: 'SHELTER-1', label: 'SHELTER-1' },
    ]);
  });
});
