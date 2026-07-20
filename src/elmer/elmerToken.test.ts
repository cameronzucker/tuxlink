// elmerToken validator tests (bd tuxlink-mfssz). The guard is the seed gate
// for both hosting windows — a malformed token must fall back to an empty
// conversation, never crash a renderer or half-adopt a scrollback.
import { describe, expect, it } from 'vitest';
import { isElmerTokenState } from './elmerToken';

const turn = { kind: 'turn', id: 'a', role: 'user', text: 'hi' };
const chip = { kind: 'chip', id: 'b', tool: 'aprs_send', status: 'calling' };
const attribution = { kind: 'attribution', id: 'c', model: 'qwen3.5' };
const error = { kind: 'error', id: 'd', outcomeKind: 'error', detail: 'boom' };

describe('isElmerTokenState', () => {
  it('accepts a token with every item kind', () => {
    expect(isElmerTokenState({ items: [turn, chip, attribution, error] })).toBe(true);
  });

  it('accepts an empty conversation and optional fields', () => {
    expect(isElmerTokenState({ items: [] })).toBe(true);
    expect(isElmerTokenState({ items: [turn], running: true })).toBe(true);
    expect(isElmerTokenState({ items: [], context: null })).toBe(true);
    expect(
      isElmerTokenState({ items: [], context: { promptTokens: 10, numCtx: 4096 } }),
    ).toBe(true);
    expect(
      isElmerTokenState({ items: [], context: { promptTokens: 10, numCtx: null } }),
    ).toBe(true);
  });

  it('rejects non-objects, missing items, and malformed containers', () => {
    expect(isElmerTokenState(null)).toBe(false);
    expect(isElmerTokenState(undefined)).toBe(false);
    expect(isElmerTokenState('items')).toBe(false);
    expect(isElmerTokenState({})).toBe(false);
    expect(isElmerTokenState({ items: 'not-an-array' })).toBe(false);
  });

  it('rejects the token whole when ANY item is malformed (no partial scrollback)', () => {
    expect(isElmerTokenState({ items: [turn, { kind: 'turn', id: 'x' }] })).toBe(false);
    expect(isElmerTokenState({ items: [{ kind: 'mystery', id: 'x' }] })).toBe(false);
    expect(isElmerTokenState({ items: [{ ...chip, id: 42 }] })).toBe(false);
  });

  it('rejects malformed running/context fields', () => {
    expect(isElmerTokenState({ items: [], running: 'yes' })).toBe(false);
    expect(isElmerTokenState({ items: [], context: { promptTokens: 'many', numCtx: null } })).toBe(
      false,
    );
    expect(isElmerTokenState({ items: [], context: { promptTokens: 1, numCtx: 'big' } })).toBe(
      false,
    );
  });
});
