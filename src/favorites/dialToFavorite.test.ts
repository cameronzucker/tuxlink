import { describe, it, expect } from 'vitest';
import { dialToNewFavorite, favoriteKey } from './dialToFavorite';
import type { FavoriteDial } from './types';

describe('dialToNewFavorite (tuxlink-5016)', () => {
  it('maps a dial into a NEW favorite with placeholder id/timestamps + starred:false', () => {
    const dial: FavoriteDial = { mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' };
    const fav = dialToNewFavorite(dial);
    expect(fav).toMatchObject({
      id: '',
      mode: 'vara-hf',
      gateway: 'N0DAJ',
      freq: '7.103',
      grid: 'DM34oa',
      starred: false,
      created_at: '',
      updated_at: '',
    });
    // The backend mints id + stamps timestamps + forces starred — these are
    // placeholders the wire shape requires, not authoritative values.
  });

  it('carries the telnet transport discriminator + band when present', () => {
    const fav = dialToNewFavorite({ mode: 'telnet', gateway: 'cms', transport: 'CmsSsl', band: 'internet' });
    expect(fav.transport).toBe('CmsSsl');
    expect(fav.band).toBe('internet');
  });

  it('carries contact_id through when present ([R5-7]); absent for an ordinary dial', () => {
    const withPeer = dialToNewFavorite({ mode: 'vara-hf', gateway: 'N0DAJ', contact_id: 'p1' });
    expect(withPeer.contact_id).toBe('p1');

    const withoutPeer = dialToNewFavorite({ mode: 'vara-hf', gateway: 'N0DAJ' });
    expect(withoutPeer.contact_id).toBeUndefined();
  });
});

describe('favoriteKey (tuxlink-ixasg: per-CHANNEL identity)', () => {
  it('keys by mode + case-folded gateway + freq — same call, same mode, two freqs = two units', () => {
    expect(favoriteKey({ mode: 'vara-hf', gateway: 'n0daj', freq: '7.103' })).toBe('vara-hf|N0DAJ|7.103');
    expect(favoriteKey({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '14.109' })).toBe('vara-hf|N0DAJ|14.109');
    expect(favoriteKey({ mode: 'ardop-hf', gateway: 'N0DAJ', freq: '7.103' })).toBe('ardop-hf|N0DAJ|7.103');
    expect(favoriteKey({ mode: 'vara-hf', gateway: ' N0DAJ ', freq: '7.103' })).toBe('vara-hf|N0DAJ|7.103');
  });

  it('canonicalizes the freq numerically so formatting drift cannot split a star', () => {
    expect(favoriteKey({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.1030' })).toBe(
      favoriteKey({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103' }),
    );
  });

  it('freq-less dials fall back to transport, then empty', () => {
    expect(favoriteKey({ mode: 'telnet', gateway: 'N0DAJ', transport: 'CmsSsl' })).toBe('telnet|N0DAJ|CmsSsl');
    expect(favoriteKey({ mode: 'telnet', gateway: 'N0DAJ' })).toBe('telnet|N0DAJ|');
  });

  it('passes an unparseable freq through raw rather than keying on NaN', () => {
    expect(favoriteKey({ mode: 'vara-hf', gateway: 'N0DAJ', freq: 'scan' })).toBe('vara-hf|N0DAJ|scan');
  });
});
