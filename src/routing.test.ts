// Tests for the pure compose-route matcher (spec §4.3 / §5.4).
//
// `parseComposeRoute` decides whether a webview's path is a compose route
// ("/compose/<draftId>") — driving App.tsx's mount decision AND the F7 guard
// that keeps compose windows from listening for menu:file:new.

import { describe, it, expect } from 'vitest';
import { parseComposeRoute } from './routing';

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
