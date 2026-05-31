import { describe, it, expect } from 'vitest';

// Vite-native filesystem scan: eager raw-import every .ts/.tsx under
// src/forms/ at build time. No @types/node dep required. The build
// embeds each file's text content; we grep for the ban substring.
//
// `eager: true` loads modules synchronously at module-evaluation time,
// matching what vitest does for the rest of the suite.
const FORM_FILES = import.meta.glob('/src/forms/**/*.{ts,tsx}', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

describe('forms module — dangerouslySetInnerHTML ban', () => {
  it('no file in src/forms/ uses dangerouslySetInnerHTML (spec §10)', () => {
    const offenders: string[] = [];
    for (const [path, content] of Object.entries(FORM_FILES)) {
      // Skip THIS test file (it mentions the string in assertion code).
      if (path.endsWith('/innerhtml-ban.test.ts')) continue;
      if (content.includes('dangerouslySetInnerHTML')) {
        offenders.push(path);
      }
    }
    expect(offenders).toEqual([]);
  });
});
