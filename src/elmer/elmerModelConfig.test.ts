/**
 * elmerModelConfig.test.ts -- TDD tests for Task G1.
 *
 * Coverage targets (per task brief):
 *   - PRESETS has the four providers: localOllama, openai, openrouter, custom.
 *     Cloud presets use https; localOllama uses loopback http.
 *   - inferPreset matches by origin not exact path (R2.6):
 *     same origin, different path -> same preset id; unknown origin -> 'custom'.
 *   - originOf strips path + lowercases host: exact A1 cross-language vector table.
 *   - isLoopback classifies host only: 127.x true, localhost true, RFC1918 false,
 *     public hostname false.
 *
 * A1 cross-language contract (Rust origin() <-> JS new URL().origin):
 *   https://API.OpenAI.com:443/v1/chat/completions  -> https://api.openai.com      (default port omitted)
 *   http://127.0.0.1:11434/v1/chat/completions      -> http://127.0.0.1:11434      (non-default port kept)
 *   https://openrouter.ai/api/v1/chat/completions   -> https://openrouter.ai
 * A mismatch silently desyncs the keyring account string -- this table is the contract.
 */

import { describe, it, expect } from 'vitest';
import {
  PRESETS,
  DEFAULT_MODEL_BY_PRESET,
  nextModelForPreset,
  originOf,
  inferPreset,
  isLoopback,
} from './elmerModelConfig';

// ---------------------------------------------------------------------------
// PRESETS
// ---------------------------------------------------------------------------

describe('PRESETS', () => {
  it('includes the expected providers (Ollama, OpenAI, Anthropic, OpenRouter, Gemini, Groq, Custom)', () => {
    expect(PRESETS.map((p) => p.id)).toEqual([
      'localOllama',
      'openai',
      'anthropic',
      'openrouter',
      'gemini',
      'groq',
      'custom',
    ]);
  });

  it('the anthropic preset targets the native Messages API endpoint with paygo tier', () => {
    const a = PRESETS.find((p) => p.id === 'anthropic');
    // Anthropic uses its native Messages API, NOT the OpenAI-compat endpoint.
    // The Rust backend selects AnthropicProvider for api.anthropic.com.
    expect(a?.endpoint).toBe('https://api.anthropic.com/v1/messages');
    expect(a?.tier).toBe('paygo');
    expect(a?.defaultModel).toBe('claude-haiku-4-5');
    expect(a?.keyPageUrl).toMatch(/^https:\/\/console\.anthropic\.com\//);
  });

  it('maps every preset id to a default-model entry', () => {
    for (const p of PRESETS) {
      expect(DEFAULT_MODEL_BY_PRESET).toHaveProperty(p.id);
    }
  });

  it('every preset has a valid tier; free/paygo carry a https keyPageUrl', () => {
    for (const p of PRESETS) {
      expect(['free', 'paygo', 'local', 'other']).toContain(p.tier);
      if (p.tier === 'free' || p.tier === 'paygo') {
        expect(p.keyPageUrl).toMatch(/^https:\/\//);
      }
    }
  });

  it('gemini is the recommended free tier with its default model', () => {
    const g = PRESETS.find((p) => p.id === 'gemini');
    expect(g?.tier).toBe('free');
    expect(DEFAULT_MODEL_BY_PRESET.gemini).toBe('gemini-2.5-flash');
    expect(DEFAULT_MODEL_BY_PRESET.groq).toBe('llama-3.3-70b-versatile');
    expect(DEFAULT_MODEL_BY_PRESET.openai).toBe('gpt-4o-mini');
  });

  it('infers the anthropic preset from its origin', () => {
    expect(inferPreset('https://api.anthropic.com/v1/chat/completions')).toBe('anthropic');
  });

  it('the free-key cloud presets use OpenAI-compatible endpoints', () => {
    const gemini = PRESETS.find((p) => p.id === 'gemini');
    expect(gemini?.endpoint).toBe(
      'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions',
    );
    const groq = PRESETS.find((p) => p.id === 'groq');
    expect(groq?.endpoint).toMatch(/^https:\/\/api\.groq\.com\/openai\/v1\//);
  });

  it('has localOllama as the first preset', () => {
    const preset = PRESETS.find((p) => p.id === 'localOllama');
    expect(preset).toBeDefined();
    expect(preset?.label).toBeTruthy();
    expect(preset?.endpoint).toBeTruthy();
  });

  it('localOllama uses loopback http (not https)', () => {
    const preset = PRESETS.find((p) => p.id === 'localOllama');
    expect(preset?.endpoint).toMatch(/^http:\/\//);
    expect(preset?.endpoint).toMatch(/127\.0\.0\.1|localhost/);
  });

  it('openai preset uses https', () => {
    const preset = PRESETS.find((p) => p.id === 'openai');
    expect(preset).toBeDefined();
    expect(preset?.endpoint).toMatch(/^https:\/\//);
    expect(preset?.endpoint).toContain('openai.com');
  });

  it('openrouter preset uses https', () => {
    const preset = PRESETS.find((p) => p.id === 'openrouter');
    expect(preset).toBeDefined();
    expect(preset?.endpoint).toMatch(/^https:\/\//);
    expect(preset?.endpoint).toContain('openrouter.ai');
  });

  it('custom preset has empty endpoint', () => {
    const preset = PRESETS.find((p) => p.id === 'custom');
    expect(preset).toBeDefined();
    expect(preset?.endpoint).toBe('');
  });

  it('all presets have non-empty id and label', () => {
    for (const preset of PRESETS) {
      expect(preset.id).toBeTruthy();
      expect(preset.label).toBeTruthy();
    }
  });

  it('preset ids are localOllama, openai, openrouter, custom', () => {
    const ids = PRESETS.map((p) => p.id);
    expect(ids).toContain('localOllama');
    expect(ids).toContain('openai');
    expect(ids).toContain('openrouter');
    expect(ids).toContain('custom');
  });
});

// ---------------------------------------------------------------------------
// nextModelForPreset -- model repopulation on tile switch
//
// New rule (post-bug-fix): on a real provider switch to a target that HAS a
// default, always adopt the target default -- the outgoing model belongs to a
// different provider and would 404. Preserve only when re-selecting the SAME
// provider. A CROSS-provider switch to a target WITHOUT a default
// (local/custom/openrouter) returns '' to CLEAR the stale foreign model (it
// would 404 against the new endpoint); re-selecting a no-default provider
// still returns null so a detected/picked model is preserved (tuxlink-5cj61).
// ---------------------------------------------------------------------------

describe('nextModelForPreset', () => {
  const OPENAI = 'https://api.openai.com/v1/chat/completions';
  const GEMINI_ENDPOINT = PRESETS.find((p) => p.id === 'gemini')!.endpoint;

  // THE REGRESSION TEST -- local detected model must NOT survive switch to cloud.
  it('local Ollama model does not survive a switch to Gemini (the fatal 404 regression)', () => {
    expect(
      nextModelForPreset(
        'http://127.0.0.1:11434/v1/chat/completions',
        'gpt-oss:20b',
        'gemini',
      ),
    ).toBe('gemini-2.5-flash');
  });

  // Hand-edited cloud model on a provider-switch must also adopt target default.
  it('hand-edited cloud model -> target default when switching providers (cloud->cloud switch)', () => {
    expect(nextModelForPreset(OPENAI, 'gpt-4-turbo', 'anthropic')).toBe('claude-haiku-4-5');
  });

  // Re-selecting the SAME provider: null (keep whatever is there).
  it('same provider re-selected -> null (preserve current model, it may be hand-edited)', () => {
    expect(nextModelForPreset(GEMINI_ENDPOINT, 'gemini-2.5-flash', 'gemini')).toBeNull();
  });

  // THE OPERATOR-REPORTED BUG (tuxlink-5cj61): switching a cloud model to a
  // local server must CLEAR the field, not strand gemini-2.5-flash (which would
  // 404 against localhost). The empty field prompts Detect + pick.
  it('cloud model does NOT survive a switch to local Ollama -> clears to empty (tuxlink-5cj61)', () => {
    expect(nextModelForPreset(GEMINI_ENDPOINT, 'gemini-2.5-flash', 'localOllama')).toBe('');
  });

  // Cross-provider switch to any no-default target clears the stale foreign model.
  it('cross-provider switch to a no-default target (openrouter/custom/local) -> clears to empty', () => {
    expect(nextModelForPreset(OPENAI, 'gpt-4o-mini', 'openrouter')).toBe('');
    expect(nextModelForPreset(OPENAI, 'gpt-4o-mini', 'custom')).toBe('');
    expect(nextModelForPreset(OPENAI, 'gpt-4o-mini', 'localOllama')).toBe('');
  });

  // Re-selecting a NO-DEFAULT provider must NOT clear the operator's picked model:
  // the same-provider branch is checked first and returns null (preserve).
  it('re-selecting the same no-default provider -> null (preserve the detected/picked model)', () => {
    expect(
      nextModelForPreset('http://127.0.0.1:11434/v1/chat/completions', 'qwen2.5:14b', 'localOllama'),
    ).toBeNull();
  });

  // Existing coverage retained with new expected values.
  it('provider switch with outgoing-default model -> adopts target default', () => {
    expect(nextModelForPreset(OPENAI, 'gpt-4o-mini', 'anthropic')).toBe('claude-haiku-4-5');
    expect(nextModelForPreset(OPENAI, 'gpt-4o-mini', 'gemini')).toBe('gemini-2.5-flash');
  });

  it('empty current model (never set) -> adopts target default on provider switch', () => {
    expect(nextModelForPreset('', '', 'anthropic')).toBe('claude-haiku-4-5');
  });
});

// ---------------------------------------------------------------------------
// originOf -- must mirror Rust url::Url::origin().ascii_serialization() exactly.
// Keep these test vectors in lock-step with A1's Rust tests.
// ---------------------------------------------------------------------------

describe('originOf', () => {
  // A1 cross-language contract vectors (cite: Task A1 elmer_config.rs origin())
  it('A1 vector: strips default https port 443 and lowercases host', () => {
    expect(originOf('https://API.OpenAI.com:443/v1/chat/completions')).toBe(
      'https://api.openai.com',
    );
  });

  it('A1 vector: keeps non-default http port for loopback', () => {
    expect(originOf('http://127.0.0.1:11434/v1/chat/completions')).toBe(
      'http://127.0.0.1:11434',
    );
  });

  it('A1 vector: strips path from openrouter', () => {
    expect(originOf('https://openrouter.ai/api/v1/chat/completions')).toBe(
      'https://openrouter.ai',
    );
  });

  it('strips path component', () => {
    expect(originOf('https://api.openai.com/v1/chat/completions')).toBe(
      'https://api.openai.com',
    );
  });

  it('lowercases the hostname', () => {
    expect(originOf('https://API.OpenAI.COM/v1')).toBe('https://api.openai.com');
  });

  it('strips default http port 80', () => {
    expect(originOf('http://example.com:80/path')).toBe('http://example.com');
  });

  it('keeps non-default https port', () => {
    expect(originOf('https://example.com:8443/path')).toBe('https://example.com:8443');
  });
});

// ---------------------------------------------------------------------------
// inferPreset -- origin-based (R2.6): same origin, any path -> same preset id
// ---------------------------------------------------------------------------

describe('inferPreset', () => {
  it('infers openai for the canonical openai endpoint', () => {
    expect(inferPreset('https://api.openai.com/v1/chat/completions')).toBe('openai');
  });

  it('infers openai for a hand-edited path on the same origin', () => {
    // Different path, same origin -> should still match (R2.6: origin-based, not URL-based)
    expect(inferPreset('https://api.openai.com/some/other/path')).toBe('openai');
  });

  it('infers openai for the bare openai origin', () => {
    expect(inferPreset('https://api.openai.com')).toBe('openai');
  });

  it('infers openrouter for the canonical openrouter endpoint', () => {
    expect(inferPreset('https://openrouter.ai/api/v1/chat/completions')).toBe('openrouter');
  });

  it('infers openrouter for a different path on the same origin', () => {
    expect(inferPreset('https://openrouter.ai/other')).toBe('openrouter');
  });

  it('infers localOllama for the default ollama endpoint', () => {
    expect(inferPreset('http://127.0.0.1:11434/v1/chat/completions')).toBe('localOllama');
  });

  it('infers localOllama for a different path on the loopback ollama origin', () => {
    expect(inferPreset('http://127.0.0.1:11434/api/tags')).toBe('localOllama');
  });

  it('returns custom for an unknown origin', () => {
    expect(inferPreset('https://selfhosted.example.com/v1/chat/completions')).toBe('custom');
  });

  it('returns custom for an empty endpoint', () => {
    // custom preset has empty endpoint -- inferring it should produce 'custom'
    expect(inferPreset('')).toBe('custom');
  });

  it('is case-insensitive on hostname (upper-cased OpenAI)', () => {
    // Origin lowercasing means API.OpenAI.com -> api.openai.com -> matches openai
    expect(inferPreset('https://API.OpenAI.com/v1/chat/completions')).toBe('openai');
  });
});

// ---------------------------------------------------------------------------
// isLoopback -- string-only host classification; NOT a resolved-IP check
// ---------------------------------------------------------------------------

describe('isLoopback', () => {
  it('classifies 127.0.0.1 as loopback', () => {
    expect(isLoopback('http://127.0.0.1:11434/v1/chat/completions')).toBe(true);
  });

  it('classifies localhost as loopback', () => {
    expect(isLoopback('http://localhost:11434/v1')).toBe(true);
  });

  it('classifies 127.x.x.x addresses as loopback (full 127/8 range)', () => {
    expect(isLoopback('http://127.0.0.2:11434/')).toBe(true);
    expect(isLoopback('http://127.255.255.255/')).toBe(true);
  });

  it('does NOT classify RFC1918 as loopback (192.168.x)', () => {
    expect(isLoopback('http://192.168.1.5/v1')).toBe(false);
  });

  it('does NOT classify public hostname as loopback', () => {
    expect(isLoopback('https://api.openai.com/v1')).toBe(false);
  });

  it('does NOT classify 10.x (RFC1918) as loopback', () => {
    expect(isLoopback('http://10.0.0.1/v1')).toBe(false);
  });

  it('does NOT classify 172.16.x (RFC1918) as loopback', () => {
    expect(isLoopback('http://172.16.0.1/v1')).toBe(false);
  });

  it('classifies ::1 as loopback', () => {
    expect(isLoopback('http://[::1]:11434/v1')).toBe(true);
  });

  it('returns false for an empty endpoint', () => {
    expect(isLoopback('')).toBe(false);
  });
});
