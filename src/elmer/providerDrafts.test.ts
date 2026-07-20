/** providerDrafts — per-provider endpoint/model memory (tuxlink-inasr). */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  stashProviderDraft,
  providerDraft,
  clearProviderDrafts,
  isStashableEndpoint,
  PROVIDER_DRAFTS_STORAGE_KEY,
} from './providerDrafts';
import { PRESETS } from './elmerModelConfig';

const SPARK = 'https://inference.twin-bramble.ts.net/v1/chat/completions';

beforeEach(() => {
  clearProviderDrafts(); // clears BOTH layers (memory + localStorage)
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('providerDrafts', () => {
  it('buckets a hand-typed endpoint under custom and round-trips it', () => {
    stashProviderDraft(SPARK, 'qwen3-coder-next');
    expect(providerDraft('custom')).toEqual({ endpoint: SPARK, model: 'qwen3-coder-next' });
  });

  it('buckets a known-provider endpoint (edited path included) under that provider', () => {
    const openai = PRESETS.find((p) => p.id === 'openai')!;
    stashProviderDraft(openai.endpoint, 'gpt-4o');
    stashProviderDraft(new URL('/v2/other', openai.endpoint).toString(), 'gpt-4o-mini');
    // Same origin → same bucket; last write wins, custom untouched.
    expect(providerDraft('openai')?.model).toBe('gpt-4o-mini');
    expect(providerDraft('custom')).toBeNull();
  });

  it('ignores an empty endpoint — a blank must never clobber a real draft', () => {
    stashProviderDraft(SPARK, 'qwen3-coder-next');
    stashProviderDraft('', 'whatever');
    expect(providerDraft('custom')).toEqual({ endpoint: SPARK, model: 'qwen3-coder-next' });
  });

  // ── adrev round: credential material must never reach localStorage ──

  it('refuses to stash credential-bearing or unparseable endpoints', () => {
    expect(isStashableEndpoint(SPARK)).toBe(true);
    // Gemini-style key-in-URL, userinfo, and token variants — all refused.
    for (const bad of [
      'https://example.invalid/v1/chat/completions?key=SECRET',
      'https://example.invalid/v1?api_key=SECRET',
      'https://example.invalid/v1?access-token=SECRET',
      'https://user:pw@example.invalid/v1/chat/completions',
      'not a url at all',
    ]) {
      expect(isStashableEndpoint(bad)).toBe(false);
      stashProviderDraft(bad, 'm');
    }
    expect(providerDraft('custom')).toBeNull();
    expect(localStorage.getItem(PROVIDER_DRAFTS_STORAGE_KEY)).toBeNull();
    // A benign non-credential query param is still stashable.
    expect(isStashableEndpoint('https://example.invalid/v1?version=2')).toBe(true);
  });

  // ── adrev round: quota failure degrades to session memory, not loss ──

  it('serves the draft from session memory when localStorage writes fail', () => {
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('QuotaExceededError');
    });
    stashProviderDraft(SPARK, 'qwen3-coder-next');
    spy.mockRestore();
    expect(localStorage.getItem(PROVIDER_DRAFTS_STORAGE_KEY)).toBeNull();
    // The promise "your values are remembered" holds for this session.
    expect(providerDraft('custom')).toEqual({ endpoint: SPARK, model: 'qwen3-coder-next' });
  });

  // ── adrev round (5.5): foreign/corrupt buckets are not restored ──

  it('ignores a stored entry whose endpoint does not infer back to its bucket', () => {
    localStorage.setItem(
      PROVIDER_DRAFTS_STORAGE_KEY,
      JSON.stringify({ openai: { endpoint: SPARK, model: 'planted' } }),
    );
    expect(providerDraft('openai')).toBeNull();
  });

  it('persists across a reload (localStorage) and survives garbage entries', () => {
    stashProviderDraft(SPARK, 'm');
    // Simulate a fresh session: memory layer gone, persisted copy remains.
    const persisted = localStorage.getItem(PROVIDER_DRAFTS_STORAGE_KEY)!;
    clearProviderDrafts();
    localStorage.setItem(
      PROVIDER_DRAFTS_STORAGE_KEY,
      persisted.replace('}}', '}, "junk": 42}'),
    );
    expect(providerDraft('custom')).toEqual({ endpoint: SPARK, model: 'm' });
    expect(providerDraft('junk')).toBeNull();
    localStorage.setItem(PROVIDER_DRAFTS_STORAGE_KEY, 'not json');
    expect(providerDraft('custom')).toBeNull(); // degrades to empty, never throws
  });

  it('clearProviderDrafts empties both layers', () => {
    stashProviderDraft(SPARK, 'm');
    clearProviderDrafts();
    expect(providerDraft('custom')).toBeNull();
  });
});
