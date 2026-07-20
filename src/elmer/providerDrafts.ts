/**
 * providerDrafts — per-provider memory for the Elmer model form
 * (tuxlink-inasr).
 *
 * The config is a SINGLE endpoint/model slot, so before this module existed a
 * provider switch destroyed the outgoing provider's values twice over:
 * `handlePresetChange` replaced a hand-typed endpoint with the incoming
 * preset's default, and re-selecting Custom actively cleared the endpoint to
 * `''` (the stick-on-Custom controlled-select fix). The operator lost the DGX
 * Spark endpoint to exactly this round-trip.
 *
 * This module remembers the last endpoint+model PER PROVIDER BUCKET, keyed by
 * `inferPreset(endpoint)` — so a hand-typed endpoint whose origin matches no
 * preset lands in the 'custom' bucket, and an edited path on a known
 * provider's origin stays with that provider. Persisted in localStorage
 * beside the app's other UI prefs: these are endpoint URLs and model names,
 * never keys — the API key lives in the OS keyring, addressed per
 * endpoint-origin, and already survives switches.
 *
 * Same try/catch posture as `colorScheme.ts`: storage failures degrade to
 * session-only memory, never throw.
 */
import { inferPreset } from './elmerModelConfig';

export const PROVIDER_DRAFTS_STORAGE_KEY = 'tuxlink.elmer.providerDrafts';

export interface ProviderDraft {
  endpoint: string;
  model: string;
}

type DraftMap = Record<string, ProviderDraft>;

function loadMap(): DraftMap {
  try {
    const raw = localStorage.getItem(PROVIDER_DRAFTS_STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
    const out: DraftMap = {};
    for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
      if (
        typeof v === 'object' &&
        v !== null &&
        typeof (v as ProviderDraft).endpoint === 'string' &&
        typeof (v as ProviderDraft).model === 'string'
      ) {
        out[k] = { endpoint: (v as ProviderDraft).endpoint, model: (v as ProviderDraft).model };
      }
    }
    return out;
  } catch {
    return {};
  }
}

function saveMap(map: DraftMap): void {
  try {
    localStorage.setItem(PROVIDER_DRAFTS_STORAGE_KEY, JSON.stringify(map));
  } catch {
    /* storage unavailable — memory degrades to session-only */
  }
}

/** Remember `endpoint`+`model` under the provider bucket the endpoint's
 *  origin belongs to. An empty endpoint is a no-op — there is nothing worth
 *  remembering, and stashing it would clobber a real draft with a blank. */
export function stashProviderDraft(endpoint: string, model: string): void {
  if (endpoint === '') return;
  const map = loadMap();
  map[inferPreset(endpoint)] = { endpoint, model };
  saveMap(map);
}

/** The remembered draft for a provider bucket, or null. */
export function providerDraft(presetId: string): ProviderDraft | null {
  return loadMap()[presetId] ?? null;
}

/** Test seam. */
export function clearProviderDrafts(): void {
  try {
    localStorage.removeItem(PROVIDER_DRAFTS_STORAGE_KEY);
  } catch {
    /* no-op */
  }
}
