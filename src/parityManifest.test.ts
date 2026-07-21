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
// Command registries and constants (Codex adrev 2026-07-21 P2): production
// code routes many invokes through maps (`CMD.get: 'routines_get'`) and
// consts (`const CMD_SUB = 'ft8_waterfall_subscribe'`) — in files that
// import the Tauri invoke, any assigned snake_case string literal is
// treated as a candidate command name. Residue this still cannot see:
// fully dynamic command strings (none known in production).
const ASSIGNED_LITERAL_RE = /[:=]\s*'([a-z0-9_]+_[a-z0-9_]+)'/g;
const IMPORTS_INVOKE_RE = /@tauri-apps\/api\/core/;
// Assigned snake_case literals in invoke-importing files that are NOT
// commands — reviewed by hand; additions need the same review.
const NOT_COMMANDS = new Set<string>([
  // Journal event kinds, auth-diagnostic reasons, config field names, and
  // UI action ids that live in invoke-importing files (2026-07-21 review).
  'branch_taken',
  'call_child',
  'callsign_rejected',
  'cat_command',
  'client_rejected',
  'cmd_port',
  'dock_back',
  'end_reached',
  'inbound_proposals_offered',
  'move_to_inbox',
  'new_routine',
  'open_model',
  'password_rejected',
  'run_finished',
  'run_started',
  'serial_rts',
  'session_dropped_after_auth',
  'state_changed',
  'step_err',
  'step_intent',
  'step_ok',
  'step_skipped',
  'unset_variable',
]);

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
      if (IMPORTS_INVOKE_RE.test(src)) {
        for (const match of src.matchAll(ASSIGNED_LITERAL_RE)) {
          const cmd = match[1];
          if (!classified.has(cmd) && !NOT_COMMANDS.has(cmd)) {
            violations.push(`${file}: assigned literal '${cmd}' (command registry?)`);
          }
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
