// favorites-fixture — realistic Favorites domain data for tests + dev (Task B5).
//
// Real-looking amateur-radio content: callsigns, Maidenhead grids, HF freqs,
// bands, and a telnet gateway with a transport discriminator. Used by the
// component tests and available for dev/storybook-style mounting.

import type { ConnectionAttempt, Favorite } from './types';

const T0 = '2026-06-01T00:00:00-07:00';

/** Starred favorites across RF + telnet modes. */
export const FIXTURE_FAVORITES: Favorite[] = [
  {
    id: 'fav-w7dxg',
    mode: 'ardop-hf',
    gateway: 'W7DXG',
    freq: '14105.0',
    band: '20m',
    grid: 'CN87',
    note: 'Pacific NW ARDOP gateway',
    starred: true,
    last_attempt_at: '2026-06-07T21:42:00-07:00',
    created_at: T0,
    updated_at: T0,
  },
  {
    id: 'fav-kh6rs',
    mode: 'ardop-hf',
    gateway: 'KH6RS',
    freq: '7102.0',
    band: '40m',
    grid: 'BL11',
    note: 'Maui RMS',
    starred: true,
    last_attempt_at: '2026-06-04T18:30:00-10:00',
    created_at: T0,
    updated_at: T0,
  },
  {
    id: 'fav-cms-ssl',
    mode: 'telnet',
    gateway: 'cms.winlink.org',
    transport: 'CmsSsl',
    note: 'Winlink CMS over TLS',
    starred: true,
    created_at: T0,
    updated_at: T0,
  },
];

/** Server-sorted non-starred recents (mode-scoped at the call site). */
export const FIXTURE_RECENTS: Favorite[] = [
  {
    id: 'rec-w6drz',
    mode: 'ardop-hf',
    gateway: 'W6DRZ',
    freq: '10145.5',
    band: '30m',
    grid: 'CM97',
    starred: false,
    last_attempt_at: '2026-06-06T08:15:00-07:00',
    created_at: T0,
    updated_at: T0,
  },
  {
    id: 'rec-n7nix',
    mode: 'ardop-hf',
    gateway: 'N7NIX',
    freq: '3585.0',
    band: '80m',
    grid: 'CN85',
    starred: false,
    last_attempt_at: '2026-06-05T23:50:00-07:00',
    created_at: T0,
    updated_at: T0,
  },
];

/** A small connection log spanning reached + failed outcomes across units. */
export const FIXTURE_ATTEMPTS: ConnectionAttempt[] = [
  { unit_id: 'fav-w7dxg', ts_local: '2026-06-07T21:42:00-07:00', freq: '14105.0', outcome: 'reached' },
  { unit_id: 'fav-w7dxg', ts_local: '2026-06-07T19:10:00-07:00', freq: '14105.0', outcome: 'failed' },
  { unit_id: 'fav-w7dxg', ts_local: '2026-06-06T06:05:00-07:00', freq: '14105.0', outcome: 'reached' },
  { unit_id: 'fav-kh6rs', ts_local: '2026-06-04T18:30:00-10:00', freq: '7102.0', outcome: 'failed' },
  { unit_id: 'rec-w6drz', ts_local: '2026-06-06T08:15:00-07:00', freq: '10145.5', outcome: 'reached' },
];
