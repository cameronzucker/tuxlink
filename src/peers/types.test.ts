// Value-level pins for every peer TS union's exact kebab-case wire string.
// BINDING AMENDMENT (Task 22 / CLD-7): a union member with the WRONG value
// (e.g. `'observed_incoming'` instead of `'observed-incoming'`) typechecks
// fine — TS has no idea what the "correct" string is — but breaks at
// runtime the instant the backend emits the real wire string. Only a
// value-level test (not a type-level one) catches that class of bug.
//
// Source of truth for every string below: `src-tauri/src/peers/model.rs`,
// all `#[serde(rename_all = "kebab-case")]`. The Rust shape tests
// (`enum_wire_tags_are_kebab_case`, `bandwidth_wire_shape_is_pinned_exactly`)
// pin the SAME strings on the Rust side; this file is the TS-side mirror.
//
// Each `Record<Union, true>` object is a compile-time exhaustiveness check:
// TS errors if a union variant is added/removed here without a matching
// update to `types.ts` (missing/excess key). The array-equality assertion
// below each one is the value-level pin a reviewer diffs against model.rs.

import { describe, it, expect } from 'vitest';
import type {
  ChannelBandwidth,
  ChannelTransport,
  Direction,
  GridSource,
  IdentityKind,
  Origin,
  Provenance,
  RecordSource,
} from './types';

describe('peer union wire-value pins (model.rs kebab-case source of truth)', () => {
  it('IdentityKind', () => {
    const exhaustive: Record<IdentityKind, true> = {
      individual: true,
      tactical: true,
      club: true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['individual', 'tactical', 'club', 'unknown'].sort(),
    );
  });

  it('RecordSource', () => {
    const exhaustive: Record<RecordSource, true> = {
      auto: true,
      manual: true,
      'operator-pinned': true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['auto', 'manual', 'operator-pinned', 'unknown'].sort(),
    );
  });

  it('Origin', () => {
    const exhaustive: Record<Origin, true> = {
      incoming: true,
      outgoing: true,
      manual: true,
      aprs: true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['incoming', 'outgoing', 'manual', 'aprs', 'unknown'].sort(),
    );
  });

  it('GridSource', () => {
    const exhaustive: Record<GridSource, true> = {
      contact: true,
      aprs: true,
      manual: true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['contact', 'aprs', 'manual', 'unknown'].sort(),
    );
  });

  it('ChannelTransport', () => {
    const exhaustive: Record<ChannelTransport, true> = {
      packet: true,
      ardop: true,
      'vara-hf': true,
      'vara-fm': true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['packet', 'ardop', 'vara-hf', 'vara-fm', 'unknown'].sort(),
    );
  });

  it('Direction', () => {
    const exhaustive: Record<Direction, true> = {
      incoming: true,
      outgoing: true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['incoming', 'outgoing', 'unknown'].sort(),
    );
  });

  it('Provenance', () => {
    const exhaustive: Record<Provenance, true> = {
      operator: true,
      'observed-incoming': true,
      unknown: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual(
      ['operator', 'observed-incoming', 'unknown'].sort(),
    );
  });

  it('ChannelBandwidth — internally tagged on "kind", each kind value pinned', () => {
    const hz: ChannelBandwidth = { kind: 'hz', hz: 2300 };
    const wide: ChannelBandwidth = { kind: 'wide' };
    const narrow: ChannelBandwidth = { kind: 'narrow' };
    const unknown: ChannelBandwidth = { kind: 'unknown' };

    expect(hz).toEqual({ kind: 'hz', hz: 2300 });
    expect(wide).toEqual({ kind: 'wide' });
    expect(narrow).toEqual({ kind: 'narrow' });
    expect(unknown).toEqual({ kind: 'unknown' });

    // Mirrors model.rs's `bandwidth_wire_shape_is_pinned_exactly`: the exact
    // JSON the backend emits for the data-carrying variant is
    // `{"kind":"hz","hz":2300}` — this literal must serialize (via
    // `JSON.stringify` on the same shape a real invoke() payload takes) to
    // that exact string, not e.g. `{"kind":"Hz","hz":2300}`.
    expect(JSON.stringify(hz)).toBe('{"kind":"hz","hz":2300}');
    expect(JSON.stringify(wide)).toBe('{"kind":"wide"}');
    expect(JSON.stringify(narrow)).toBe('{"kind":"narrow"}');
  });
});
