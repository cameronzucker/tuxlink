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
 * provider's origin stays with that provider.
 *
 * Storage posture (adrev round: both reviewers):
 * - Persisted in localStorage beside the app's other UI prefs, with a
 *   SESSION-SCOPED in-memory layer on top: a failed `setItem` (quota,
 *   disabled storage) degrades to session-only memory instead of silently
 *   dropping the one copy the switch is about to overwrite. Reads merge, the
 *   memory layer wins.
 * - Drafts must never hold credentials — the API key lives in the OS keyring,
 *   addressed per endpoint-origin. An endpoint is stashed ONLY when it parses
 *   as a URL carrying no userinfo and no credential-shaped query parameter
 *   (`?key=`, `?token=`, …, the Gemini-style key-in-URL pattern); anything
 *   else is not remembered at all — a refused stash loses convenience, a
 *   persisted secret loses the keyring-only guarantee.
 * - On read, an entry whose endpoint does not infer back to its own bucket
 *   (foreign/corrupt write) is ignored rather than restored.
 */
import { inferPreset } from './elmerModelConfig';

export const PROVIDER_DRAFTS_STORAGE_KEY = 'tuxlink.elmer.providerDrafts';

export interface ProviderDraft {
  endpoint: string;
  model: string;
}

type DraftMap = Record<string, ProviderDraft>;

/** Session-scoped layer over localStorage: written on every stash, so a
 *  failed persistent write still leaves the draft recoverable for the rest
 *  of this session (adrev: "remembered" must not be a lie under quota
 *  failure). */
const memoryLayer: DraftMap = {};

/** Query-parameter names that smell like credentials. Case-insensitive,
 *  matched against each param NAME. Deliberately broad: a false positive
 *  costs one skipped convenience stash; a false negative writes a secret to
 *  cleartext localStorage. */
const CREDENTIAL_PARAM_RE = /^(key|api[-_]?key|token|access[-_]?token|auth|authorization|secret|password|pass|sig|signature|sas)$/i;

/** True when `endpoint` is a parseable URL that carries no credential
 *  material we can detect: no userinfo, no credential-shaped query param.
 *  Unparseable strings are NOT safe — we cannot inspect what we cannot
 *  parse, and a half-typed value is a low-value draft anyway. */
export function isStashableEndpoint(endpoint: string): boolean {
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    return false;
  }
  if (url.username !== '' || url.password !== '') return false;
  for (const name of url.searchParams.keys()) {
    if (CREDENTIAL_PARAM_RE.test(name)) return false;
  }
  return true;
}

function loadPersisted(): DraftMap {
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

function merged(): DraftMap {
  return { ...loadPersisted(), ...memoryLayer };
}

/** Remember `endpoint`+`model` under the provider bucket the endpoint's
 *  origin belongs to. No-ops: an empty endpoint (nothing worth remembering —
 *  and a blank must never clobber a real draft) and a non-stashable one
 *  (unparseable, or carrying credential material that must not reach
 *  localStorage). */
export function stashProviderDraft(endpoint: string, model: string): void {
  if (endpoint === '' || !isStashableEndpoint(endpoint)) return;
  const bucket = inferPreset(endpoint);
  memoryLayer[bucket] = { endpoint, model };
  const map = { ...loadPersisted(), [bucket]: { endpoint, model } };
  try {
    localStorage.setItem(PROVIDER_DRAFTS_STORAGE_KEY, JSON.stringify(map));
  } catch {
    /* persistent write failed — the memoryLayer copy above still serves
       this session (documented degradation, not silent loss) */
  }
}

/** The remembered draft for a provider bucket, or null. A stored entry whose
 *  endpoint does not infer back to `presetId` (foreign or corrupt write) is
 *  ignored — restoring it would silently point a known provider's form at an
 *  unexpected host. */
export function providerDraft(presetId: string): ProviderDraft | null {
  const draft = merged()[presetId];
  if (!draft) return null;
  if (inferPreset(draft.endpoint) !== presetId) return null;
  return draft;
}

/** Test seam. */
export function clearProviderDrafts(): void {
  for (const k of Object.keys(memoryLayer)) delete memoryLayer[k];
  try {
    localStorage.removeItem(PROVIDER_DRAFTS_STORAGE_KEY);
  } catch {
    /* no-op */
  }
}
