/**
 * elmerModelConfig — provider presets, origin-based preset inference, and DTO types.
 *
 * Cross-language contract: `originOf` uses `new URL(endpoint).origin`, which mirrors
 * Rust's `url::Url::origin().ascii_serialization()` exactly. Keep these two in
 * lock-step (cite: Task A1 elmer_config.rs origin() test vectors):
 *
 *   https://API.OpenAI.com:443/v1/chat/completions  → https://api.openai.com      (default port omitted)
 *   http://127.0.0.1:11434/v1/chat/completions      → http://127.0.0.1:11434      (non-default port kept)
 *   https://openrouter.ai/api/v1/chat/completions   → https://openrouter.ai
 *
 * A Rust/TS mismatch silently desyncs the keyring account string — this table is
 * the contract between Task A1 (Rust) and Task G1 (TypeScript).
 *
 * DO NOT hand-roll origin extraction. Use `new URL(endpoint).origin` so that
 * default-port elision and host normalisation matches the Rust url crate exactly.
 */

// ---------------------------------------------------------------------------
// DTO types (mirror Rust serde shapes — keep in sync with elmer_config.rs)
// ---------------------------------------------------------------------------

/** Whether an API key is available in the keyring for the current endpoint. */
export type KeyStatus = 'present' | 'absent' | 'unreadable';

/** Response from the `elmer_config_read` Tauri command. */
export interface ConfigReadDto {
  agentEndpoint: string;
  agentModel: string;
  keyStatus: KeyStatus;
  /** Per-turn timeout in seconds. Valid range [30, 3600]; default 900 (15 min). */
  agentTurnTimeoutSecs: number;
  /**
   * True once the operator has completed model setup at least once. Distinguishes
   * a never-configured install (which should land on the tile picker) from one
   * running the implicit local default. Backend serializes this as `onboarded`
   * (camelCase) — keep the Rust ConfigReadDto field in lock-step.
   */
  onboarded: boolean;
}

/**
 * Keyring key-status per endpoint origin, returned by `elmer_key_status_for_origins`.
 * Statuses only — never key values. Used by the tile picker to show which providers
 * already have a stored key. The keys are origins (e.g. `https://api.openai.com`).
 */
export type KeyStatusByOrigin = Record<string, KeyStatus>;

/**
 * How to handle the stored keyring key when saving config.
 *  - keep:  leave the currently-stored key unchanged.
 *  - set:   replace the stored key with the supplied value.
 *  - clear: delete any stored key.
 */
export type SetKey =
  | { action: 'keep' }
  | { action: 'set'; value: string }
  | { action: 'clear' };

/**
 * Which API key to use when calling the agent endpoint at runtime.
 *  - useStored: read from keyring (the normal case for cloud providers).
 *  - inline:    use the supplied string (for one-shot overrides).
 *  - none:      send no key (for local endpoints that require no auth).
 */
export type KeySource =
  | { source: 'useStored' }
  | { source: 'inline'; value: string }
  | { source: 'none' };

// ---------------------------------------------------------------------------
// Provider presets
// ---------------------------------------------------------------------------

/** Pricing/commitment tier a provider falls into — drives the tile picker grouping. */
export type ProviderTier = 'free' | 'paygo' | 'local' | 'other';

/** A named provider preset shown in the model picker. */
export interface ProviderPreset {
  /** Machine-readable id: 'localOllama' | 'openai' | 'anthropic' | 'openrouter' | 'gemini' | 'groq' | 'custom'. */
  id: string;
  /** Human-readable display label. */
  label: string;
  /** Default endpoint URL for this provider. Empty for 'custom'. */
  endpoint: string;
  /** Pricing/commitment tier — groups tiles into Free / Pay-as-you-go / Local / Other. */
  tier: ProviderTier;
  /** Default model id pre-filled when this tile is selected. Empty = leave/auto-detect. */
  defaultModel?: string;
  /**
   * Hardcoded provider key-page URL opened by the "get a key" button. Present only
   * for free/paygo cloud providers. MUST be a constant (never config/endpoint-derived)
   * and MUST be on the `shell:allow-open` allowlist in capabilities/default.json.
   */
  keyPageUrl?: string;
}

/**
 * Ordered list of provider presets.
 *
 * Cloud presets use https. The local Ollama preset uses loopback http so
 * `isLoopback` correctly hides the key field for it (no API key required
 * for local Ollama). The 'custom' preset has an empty endpoint; the user
 * fills it in manually.
 */
export const PRESETS: ProviderPreset[] = [
  {
    id: 'localOllama',
    label: 'On this computer (Ollama)',
    endpoint: 'http://127.0.0.1:11434/v1/chat/completions',
    tier: 'local',
    // No defaultModel: local model is whatever the operator has pulled (auto-detect).
  },
  {
    id: 'openai',
    label: 'OpenAI',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tier: 'paygo',
    defaultModel: 'gpt-4o-mini',
    keyPageUrl: 'https://platform.openai.com/api-keys',
  },
  {
    // OpenAI-compatibility endpoint (base https://api.anthropic.com/v1/ + chat/completions),
    // bearer auth. Pay-as-you-go (needs a billing card). Default haiku is cheap + a strong
    // tool-driver; sonnet is the quality step-up.
    id: 'anthropic',
    label: 'Anthropic — Claude',
    endpoint: 'https://api.anthropic.com/v1/chat/completions',
    tier: 'paygo',
    defaultModel: 'claude-haiku-4-5',
    keyPageUrl: 'https://console.anthropic.com/settings/keys',
  },
  {
    id: 'openrouter',
    label: 'OpenRouter / custom endpoint',
    endpoint: 'https://openrouter.ai/api/v1/chat/completions',
    tier: 'other',
    // No keyPageUrl: OpenRouter's key flow is less beginner-friendly; it lives in "Other".
  },
  {
    // Free-key cloud option: Google AI Studio issues a free API key (no billing
    // card) and exposes an OpenAI-compatible endpoint. Capable models like
    // gemini-2.5-flash. Lowest-friction cloud path for the non-developer audience.
    id: 'gemini',
    label: 'Google Gemini',
    endpoint: 'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions',
    tier: 'free',
    defaultModel: 'gemini-2.5-flash',
    keyPageUrl: 'https://aistudio.google.com/apikey',
  },
  {
    // Free-key cloud option: Groq issues a free API key, fast inference, OpenAI-
    // compatible. Models like llama-3.3-70b-versatile.
    id: 'groq',
    label: 'Groq',
    endpoint: 'https://api.groq.com/openai/v1/chat/completions',
    tier: 'free',
    defaultModel: 'llama-3.3-70b-versatile',
    keyPageUrl: 'https://console.groq.com/keys',
  },
  {
    id: 'custom',
    label: 'Custom…',
    endpoint: '',
    tier: 'other',
  },
];

/**
 * Default model id pre-filled when a provider tile is selected. Single source of
 * truth, keyed by preset id (mirrors each preset's `defaultModel`; kept as a flat
 * map for the `nextModelForPreset` helper + tile-selection logic). Empty string
 * means "leave the model field as-is / rely on detect" (local + custom + other).
 */
export const DEFAULT_MODEL_BY_PRESET: Record<string, string> = {
  localOllama: '',
  openai: 'gpt-4o-mini',
  anthropic: 'claude-haiku-4-5',
  openrouter: '',
  gemini: 'gemini-2.5-flash',
  groq: 'llama-3.3-70b-versatile',
  custom: '',
};

/**
 * Decide the model to set when the operator switches to `targetPresetId`.
 *
 * Returns the target preset's default model ONLY when the current model is
 * "untouched" — i.e. it still equals the OUTGOING preset's default (or is empty /
 * never set). Returns `null` to PRESERVE the current model when the operator has
 * hand-edited it, or when the target has no default (local / custom / other).
 *
 * Shared single source of truth for both `handlePresetChange` (the dense form) and
 * the tile picker, so the two cannot drift. Pure function — no React, unit-tested.
 */
export function nextModelForPreset(
  currentEndpoint: string,
  currentModel: string,
  targetPresetId: string,
): string | null {
  const targetDefault = DEFAULT_MODEL_BY_PRESET[targetPresetId] ?? '';
  if (!targetDefault) return null; // target has no default — leave the model field as-is
  const outgoingDefault = DEFAULT_MODEL_BY_PRESET[inferPreset(currentEndpoint)] ?? '';
  const untouched = currentModel === '' || currentModel === outgoingDefault;
  return untouched ? targetDefault : null;
}

// ---------------------------------------------------------------------------
// originOf — mirrors Rust url::Url::origin().ascii_serialization()
// ---------------------------------------------------------------------------

/**
 * Returns the scheme://host[:port] origin of `endpoint`.
 *
 * Uses `new URL(endpoint).origin` — DO NOT hand-roll this. The Web URL API
 * matches the Rust `url` crate's behaviour precisely:
 *   - default ports (http:80, https:443) are omitted.
 *   - the host is lowercased.
 *   - path, query, and fragment are stripped.
 *
 * This must stay in lock-step with Task A1's Rust `origin()` helper.
 * If the two diverge, the keyring account string desyncs silently.
 *
 * Returns an empty string for an unparseable endpoint (matches the Rust
 * fallback of returning an opaque origin string, which keyring logic treats
 * as a key-less endpoint).
 */
export function originOf(endpoint: string): string {
  try {
    return new URL(endpoint).origin;
  } catch {
    return '';
  }
}

// ---------------------------------------------------------------------------
// inferPreset — origin-based matching (R2.6)
// ---------------------------------------------------------------------------

/**
 * Returns the preset id whose origin matches `endpoint`'s origin.
 *
 * Matching is origin-based, not URL-exact: a hand-edited path on the same
 * origin still infers the same preset (R2.6). Returns 'custom' for any
 * endpoint whose origin doesn't match a known preset, and for empty / invalid
 * endpoints.
 */
export function inferPreset(endpoint: string): string {
  const origin = originOf(endpoint);
  if (!origin) return 'custom';

  for (const preset of PRESETS) {
    if (!preset.endpoint) continue; // 'custom' has empty endpoint — skip
    const presetOrigin = originOf(preset.endpoint);
    if (presetOrigin && origin === presetOrigin) {
      return preset.id;
    }
  }

  return 'custom';
}

// ---------------------------------------------------------------------------
// isLoopback — string-only host classification
// ---------------------------------------------------------------------------

/**
 * Returns true if the host in `endpoint` is a loopback address.
 *
 * Classification is STRING-ONLY — this is NOT a resolved-IP check.
 * Loopback hosts are:
 *   - 127.0.0.0/8 (any 127.x.x.x address, checked as a string prefix)
 *   - ::1 (IPv6 loopback literal)
 *   - the literal string "localhost"
 *
 * RFC1918 private addresses (10.x, 172.16-31.x, 192.168.x) are NOT loopback.
 * This matches Rust AgentEndpoint::is_loopback's host-classification logic.
 *
 * Used by G2 to decide whether to hide the key field (local Ollama needs no key).
 */
export function isLoopback(endpoint: string): boolean {
  if (!endpoint) return false;

  let host: string;
  try {
    host = new URL(endpoint).hostname;
  } catch {
    return false;
  }

  if (host === 'localhost') return true;
  // URL.hostname returns '[::1]' (with brackets) for IPv6 literals.
  if (host === '::1' || host === '[::1]') return true;

  // 127.0.0.0/8: any address whose first octet is 127
  if (/^127\./.test(host)) return true;

  return false;
}
