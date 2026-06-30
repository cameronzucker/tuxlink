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
}

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

/** A named provider preset shown in the endpoint picker. */
export interface ProviderPreset {
  /** Machine-readable id. Values: 'localOllama' | 'openai' | 'openrouter' | 'custom'. */
  id: string;
  /** Human-readable display label. */
  label: string;
  /** Default endpoint URL for this provider. Empty for 'custom'. */
  endpoint: string;
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
    label: 'Local Ollama',
    endpoint: 'http://127.0.0.1:11434/v1/chat/completions',
  },
  {
    id: 'openai',
    label: 'OpenAI',
    endpoint: 'https://api.openai.com/v1/chat/completions',
  },
  {
    id: 'openrouter',
    label: 'OpenRouter',
    endpoint: 'https://openrouter.ai/api/v1/chat/completions',
  },
  {
    id: 'custom',
    label: 'Custom…',
    endpoint: '',
  },
];

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
