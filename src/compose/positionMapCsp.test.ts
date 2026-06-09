import { describe, expect, it } from 'vitest';
import tauriConfig from '../../src-tauri/tauri.conf.json';

function directiveTokens(csp: string, directive: string): string[] {
  const match = csp
    .split(';')
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${directive} `));
  return match?.split(/\s+/).slice(1) ?? [];
}

describe('Position map CSP — offline-first, never public OSM', () => {
  const csp = tauriConfig.app.security.csp;
  const imgSrc = directiveTokens(csp, 'img-src');
  const connectSrc = directiveTokens(csp, 'connect-src');

  it('forbids any OpenStreetMap tile host in img-src and connect-src', () => {
    for (const tok of [...imgSrc, ...connectSrc]) expect(tok).not.toContain('openstreetmap');
  });

  it('preserves the load-bearing retain-list', () => {
    // data: = position-form dropdown SVGs; 'self' = bundled world-map asset
    expect(imgSrc).toEqual(expect.arrayContaining(["'self'", 'data:']));
    // 'self' + the local WLE forms server (127.0.0.1) must survive the revert
    expect(connectSrc).toEqual(expect.arrayContaining(["'self'", 'http://127.0.0.1:*']));
  });
});
