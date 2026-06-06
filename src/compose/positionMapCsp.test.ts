import { describe, expect, it } from 'vitest';
import tauriConfig from '../../src-tauri/tauri.conf.json';

function directiveTokens(csp: string, directive: string): string[] {
  const match = csp
    .split(';')
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${directive} `));
  return match?.split(/\s+/).slice(1) ?? [];
}

describe('Position map CSP', () => {
  it('allows online OpenStreetMap raster tiles while preserving local image sources', () => {
    const csp = tauriConfig.app.security.csp;
    const imgSrc = directiveTokens(csp, 'img-src');

    expect(imgSrc).toContain("'self'");
    expect(imgSrc).toContain('data:');
    expect(imgSrc).toContain('https://tile.openstreetmap.org');
    expect(imgSrc).toContain('https://*.tile.openstreetmap.org');
  });
});
