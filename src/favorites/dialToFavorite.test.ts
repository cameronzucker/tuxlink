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

  it('carries peer_id through when present ([R5-7]); absent for an ordinary dial', () => {
    const withPeer = dialToNewFavorite({ mode: 'vara-hf', gateway: 'N0DAJ', peer_id: 'p1' });
    expect(withPeer.peer_id).toBe('p1');

    const withoutPeer = dialToNewFavorite({ mode: 'vara-hf', gateway: 'N0DAJ' });
    expect(withoutPeer.peer_id).toBeUndefined();
  });
});

describe('favoriteKey', () => {
  it('keys by mode + case-folded gateway (same call in two modes is two units)', () => {
    expect(favoriteKey({ mode: 'vara-hf', gateway: 'n0daj' })).toBe('vara-hf|N0DAJ');
    expect(favoriteKey({ mode: 'ardop-hf', gateway: 'N0DAJ' })).toBe('ardop-hf|N0DAJ');
    expect(favoriteKey({ mode: 'vara-hf', gateway: ' N0DAJ ' })).toBe('vara-hf|N0DAJ');
  });
});
