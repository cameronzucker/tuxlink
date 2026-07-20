// Tests for the pure compose-route matcher (spec §4.3 / §5.4).
//
// `parseComposeRoute` decides whether a webview's path is a compose route
// ("/compose/<draftId>") — driving App.tsx's mount decision AND the F7 guard
// that keeps compose windows from listening for menu:file:new.

import { describe, it, expect } from 'vitest';
import {
  parseComposeRoute,
  parseHelpRoute,
  parseLoggingRoute,
  parseStationsRoute,
  parsePopRoute,
  isSecondaryWindow,
} from './routing';

describe('parseComposeRoute', () => {
  it('matches /compose/<draftId> and returns the draftId', () => {
    expect(parseComposeRoute('/compose/draft-2026-05-19-abc')).toBe('draft-2026-05-19-abc');
  });

  it('tolerates a trailing slash', () => {
    expect(parseComposeRoute('/compose/draft-x/')).toBe('draft-x');
  });

  it('URL-decodes the draftId segment', () => {
    expect(parseComposeRoute('/compose/draft%20one')).toBe('draft one');
  });

  it('returns null for the main route ("/")', () => {
    expect(parseComposeRoute('/')).toBeNull();
  });

  it('returns null for /compose with no id', () => {
    expect(parseComposeRoute('/compose')).toBeNull();
    expect(parseComposeRoute('/compose/')).toBeNull();
  });

  it('returns null for a nested path beyond the draftId', () => {
    // The matcher rejects extra path segments so it never mis-mounts Compose.
    expect(parseComposeRoute('/compose/draft-x/extra')).toBeNull();
  });

  it('returns null for unrelated paths', () => {
    expect(parseComposeRoute('/settings')).toBeNull();
    expect(parseComposeRoute('/composer/draft-x')).toBeNull();
  });

  it('returns null for malformed percent-encoding rather than throwing', () => {
    // A lone "%" is invalid; decodeURIComponent throws → matcher returns null.
    expect(parseComposeRoute('/compose/%E0%A4%A')).toBeNull();
  });
});

// tuxlink-0gsy / spec §4.1: the help window is single-instance with no
// parameters, so parseHelpRoute returns boolean rather than the
// optional string parseComposeRoute returns.
describe('parseHelpRoute', () => {
  it('returns true for the literal /help path', () => {
    expect(parseHelpRoute('/help')).toBe(true);
  });

  it('returns true for /help with a trailing slash', () => {
    expect(parseHelpRoute('/help/')).toBe(true);
  });

  it('returns false for the main route ("/")', () => {
    expect(parseHelpRoute('/')).toBe(false);
  });

  it('returns false for compose routes', () => {
    expect(parseHelpRoute('/compose/draft-123')).toBe(false);
  });

  it('returns false for nested paths beyond /help', () => {
    expect(parseHelpRoute('/help/something')).toBe(false);
  });

  it('returns false for paths that merely start with "help"', () => {
    expect(parseHelpRoute('/helpful')).toBe(false);
  });
});

// tuxlink-qjgx / spec §8.1: the logging window is single-instance with no
// parameters, so parseLoggingRoute returns boolean (same shape as
// parseHelpRoute).
describe('parseLoggingRoute', () => {
  it('returns true for /logging', () => {
    expect(parseLoggingRoute('/logging')).toBe(true);
  });
  it('returns true for /logging/', () => {
    expect(parseLoggingRoute('/logging/')).toBe(true);
  });
  it('returns false for /', () => {
    expect(parseLoggingRoute('/')).toBe(false);
  });
  it('returns false for /help', () => {
    expect(parseLoggingRoute('/help')).toBe(false);
  });
  it('returns false for /logging/extra', () => {
    expect(parseLoggingRoute('/logging/extra')).toBe(false);
  });
});

// tuxlink-2phz: the Station Data pop-out is single-instance, no parameters, so
// parseStationsRoute returns boolean (same shape as parseHelpRoute).
describe('parseStationsRoute', () => {
  it('returns true for /stations', () => {
    expect(parseStationsRoute('/stations')).toBe(true);
  });
  it('returns true for /stations/', () => {
    expect(parseStationsRoute('/stations/')).toBe(true);
  });
  it('returns false for /', () => {
    expect(parseStationsRoute('/')).toBe(false);
  });
  it('returns false for /help', () => {
    expect(parseStationsRoute('/help')).toBe(false);
  });
  it('returns false for /stations/extra', () => {
    expect(parseStationsRoute('/stations/extra')).toBe(false);
  });
});

// bd tuxlink-dmwte, spec §3 wire table: the three pop-out routes for
// Routines / Tac Map / APRS Chat. Same shape as parseComposeRoute, but the
// map is literal — the route segment drops the underscore irregularly
// (tacmap, aprschat), the surface id keeps it.
describe('parsePopRoute', () => {
  it('maps the three pop routes to surface ids (spec §3 table — underscore dropped in route, kept in id)', () => {
    expect(parsePopRoute('/pop/routines')).toBe('routines');
    expect(parsePopRoute('/pop/tacmap')).toBe('tac_map');
    expect(parsePopRoute('/pop/aprschat')).toBe('aprs_chat');
    expect(parsePopRoute('/pop/elmer')).toBe('elmer');           // bd tuxlink-mfssz
    expect(parsePopRoute('/pop/tacmap/')).toBe('tac_map');       // trailing slash tolerated
    expect(parsePopRoute('/pop/tac_map')).toBeNull();            // the id form is NOT a route
    expect(parsePopRoute('/pop')).toBeNull();
    expect(parsePopRoute('/')).toBeNull();
  });
});

// adrev Codex-9: pop windows must not run main-only side effects (first-paint
// emission, wizard probing). isSecondaryWindow is the shared guard predicate.
describe('isSecondaryWindow', () => {
  it('covers all five secondary kinds (adrev Codex-9: pop windows must not run main-only side effects)', () => {
    for (const p of ['/compose/d1', '/help', '/logging', '/stations', '/pop/routines', '/pop/tacmap', '/pop/aprschat', '/pop/elmer']) {
      expect(isSecondaryWindow(p)).toBe(true);
    }
    expect(isSecondaryWindow('/')).toBe(false);
  });
});
