// ADR 0027 parity-manifest enforcement, frontend half (tuxlink-ybf9f).
//
// Every `invoke('<command>')` literal in production frontend code must name
// a command classified in docs/parity/parity-manifest.json. This is the
// check that would have caught the 2026-07-21 favorites gap the day the
// designer picker started consuming `favorites_read`: a UI surface cannot
// quietly grow a dependency on an unclassified backend capability. The
// backend half (registration completeness, mapping liveness, authority
// defense, tool budget) lives in src-tauri/src/parity_check.rs.

import { describe, it, expect } from 'vitest';

import manifest from '../docs/parity/parity-manifest.json';

// Raw sources of every production module (tests excluded: they mock
// commands and may reference deliberately-fake names).
const SOURCES = import.meta.glob(
  ['./**/*.ts', './**/*.tsx', '!./**/*.test.ts', '!./**/*.test.tsx', '!./test-setup.ts'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const INVOKE_RE = /\binvoke\s*(?:<[^>()]*>)?\s*\(\s*'([a-z0-9_]+)'/g;

describe('parity manifest — frontend consumers (ADR 0027)', () => {
  it('every invoked command is classified', () => {
    const classified = new Set(Object.keys(manifest.commands));
    const violations: string[] = [];
    for (const [file, src] of Object.entries(SOURCES)) {
      for (const match of src.matchAll(INVOKE_RE)) {
        const cmd = match[1];
        if (!classified.has(cmd)) {
          violations.push(`${file}: invoke('${cmd}')`);
        }
      }
    }
    expect(
      violations,
      `unclassified command consumption — add each to docs/parity/parity-manifest.json ` +
        `(class it honestly; a capability needs an agent path or a bd id, ADR 0027):\n` +
        violations.join('\n'),
    ).toEqual([]);
  });

  it('the manifest itself is structurally sound', () => {
    // Belt-and-braces mirror of the Rust-side class check, so a manifest
    // typo fails BOTH suites rather than silently passing one.
    const CLASSES = new Set(['chrome', 'presentation', 'operator-authority', 'capability']);
    for (const [name, entry] of Object.entries(manifest.commands)) {
      const e = entry as Record<string, string>;
      expect(CLASSES.has(e.class), `${name}: unknown class ${e.class}`).toBe(true);
      const paths = ['mcp', 'mcp-field', 'finding', 'pending'].filter((k) => k in e).length;
      if (e.class === 'capability') {
        expect(paths, `${name}: capability needs exactly one agent path`).toBe(1);
      } else {
        expect(paths, `${name}: terminal class must not carry an agent path`).toBe(0);
      }
    }
  });
});
