/** providerDrafts — per-provider endpoint/model memory (tuxlink-inasr). */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  stashProviderDraft,
  providerDraft,
  clearProviderDrafts,
  PROVIDER_DRAFTS_STORAGE_KEY,
} from './providerDrafts';
import { PRESETS } from './elmerModelConfig';

const SPARK = 'https://inference.twin-bramble.ts.net/v1/chat/completions';

beforeEach(() => {
  localStorage.clear();
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

  it('persists across a reload (localStorage) and survives garbage entries', () => {
    stashProviderDraft(SPARK, 'm');
    const raw = localStorage.getItem(PROVIDER_DRAFTS_STORAGE_KEY)!;
    // Simulate a partially-corrupt store from a future/foreign writer.
    localStorage.setItem(
      PROVIDER_DRAFTS_STORAGE_KEY,
      raw.replace('}}', '}, "junk": 42}'),
    );
    expect(providerDraft('custom')).toEqual({ endpoint: SPARK, model: 'm' });
    expect(providerDraft('junk')).toBeNull();
    localStorage.setItem(PROVIDER_DRAFTS_STORAGE_KEY, 'not json');
    expect(providerDraft('custom')).toBeNull(); // degrades to empty, never throws
  });

  it('clearProviderDrafts empties the store', () => {
    stashProviderDraft(SPARK, 'm');
    clearProviderDrafts();
    expect(providerDraft('custom')).toBeNull();
  });
});
