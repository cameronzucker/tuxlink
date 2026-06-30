/**
 * elmerModelConfig.test.ts — TDD tests for Task G1.
 *
 * Coverage targets (per task brief):
 *   - PRESETS has the four providers: localOllama, openai, openrouter, custom.
 *     Cloud presets use https; localOllama uses loopback http.
 *   - inferPreset matches by origin not exact path (R2.6):
 *     same origin, different path → same preset id; unknown origin → 'custom'.
 *   - originOf strips path + lowercases host: exact A1 cross-language vector table.
 *   - isLoopback classifies host only: 127.x true, localhost true, RFC1918 false,
 *     public hostname false.
 *
 * A1 cross-language contract (Rust origin() ↔ JS new URL().origin):
 *   https://API.OpenAI.com:443/v1/chat/completions  → https://api.openai.com      (default port omitted)
 *   http://127.0.0.1:11434/v1/chat/completions      → http://127.0.0.1:11434      (non-default port kept)
 *   https://openrouter.ai/api/v1/chat/completions   → https://openrouter.ai
 * A mismatch silently desyncs the keyring account string — this table is the contract.
 */

import { describe, it, expect } from 'vitest';
import {
  PRESETS,
  originOf,
  inferPreset,
  isLoopback,
} from './elmerModelConfig';

// ---------------------------------------------------------------------------
// PRESETS
// ---------------------------------------------------------------------------

describe('PRESETS', () => {
  it('includes the expected providers (Ollama, OpenAI, OpenRouter, Gemini, Groq, Custom)', () => {
    expect(PRESETS.map((p) => p.id)).toEqual([
      'localOllama',
      'openai',
      'openrouter',
      'gemini',
      'groq',
      'custom',
    ]);
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
// originOf — must mirror Rust url::Url::origin().ascii_serialization() exactly.
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
// inferPreset — origin-based (R2.6): same origin, any path → same preset id
// ---------------------------------------------------------------------------

describe('inferPreset', () => {
  it('infers openai for the canonical openai endpoint', () => {
    expect(inferPreset('https://api.openai.com/v1/chat/completions')).toBe('openai');
  });

  it('infers openai for a hand-edited path on the same origin', () => {
    // Different path, same origin → should still match (R2.6: origin-based, not URL-based)
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
    // custom preset has empty endpoint — inferring it should produce 'custom'
    expect(inferPreset('')).toBe('custom');
  });

  it('is case-insensitive on hostname (upper-cased OpenAI)', () => {
    // Origin lowercasing means API.OpenAI.com → api.openai.com → matches openai
    expect(inferPreset('https://API.OpenAI.com/v1/chat/completions')).toBe('openai');
  });
});

// ---------------------------------------------------------------------------
// isLoopback — string-only host classification; NOT a resolved-IP check
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
