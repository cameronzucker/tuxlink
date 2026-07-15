import { describe, expect, it } from 'vitest';
import * as api from './routinesApi';

const BINDINGS = Object.keys(api).filter((k) => typeof (api as any)[k] === 'function');

// Vite-native filesystem scan (per docs/pitfalls/implementation-pitfalls.md
// TEST-1): eager raw-import every non-test .ts/.tsx under src/, excluding
// routinesApi.ts itself. A Node `fs`-based scan here passes vitest but fails
// `tsc --noEmit` (no @types/node in this frontend's tsconfig) — shadow CI.
const SOURCE_FILES = import.meta.glob(
  ['/src/**/*.{ts,tsx}', '!/src/**/*.test.*', '!/src/routines/routinesApi.ts'],
  { eager: true, query: '?raw', import: 'default' },
) as Record<string, string>;

describe('routinesApi command coverage', () => {
  it('every routines API binding has at least one non-test call site (ADR 0022 coverage invariant)', () => {
    const corpus = Object.values(SOURCE_FILES).join('\n');
    const orphans = BINDINGS.filter((b) => !new RegExp(`\\b${b}\\b`).test(corpus));
    expect(orphans).toEqual([]);
  });
});
