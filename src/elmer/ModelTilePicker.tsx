/**
 * ModelTilePicker — Elmer's canonical model surface (Task 8a, F1-consume / F4).
 *
 * A cloud-first, price-tiered tile picker that replaces the dense `<select>`
 * form as the primary way an operator chooses a model. Tiles are grouped by
 * the `tier` field on each `ProviderPreset` into four sections:
 *   Free · no credit card   (gemini, groq)
 *   Pay-as-you-go · needs a card (openai, anthropic)
 *   On this computer        (localOllama)
 *   Other                   (openrouter, custom)
 *
 * Each tile is a real <button role="radio"> inside a per-tier role="radiogroup"
 * so the whole picker is keyboard-reachable (Tab to the group, arrows/Enter to
 * pick) — NOT a wall of clickable <div>s.
 *
 * State model: the picker owns a WORKING endpoint + model, seeded from
 * initialEndpoint/initialModel (the operator's SAVED config), so the
 * pre-selected tile shows the saved model rather than that tile's default.
 * Selecting a different cloud/local tile sets the working endpoint to the
 * preset's endpoint and uses `nextModelForPreset` (the single source of truth
 * shared with ModelForm.handlePresetChange) to decide the model — null = keep.
 *
 * The "Other" tier (openrouter + custom) renders the EXPORTED `ModelForm`
 * VERBATIM as its editor — the picker does not re-implement the custom-endpoint
 * flow (no second model-selection path). The guided "get a free key" flow and
 * the open-browser button are Task 9 — NOT built here.
 *
 * Security: `keyStatusByOrigin` is a PROP (statuses only — never key values).
 * The picker renders no key value anywhere. There is NO renderer-side fetch/XHR
 * to any provider endpoint (SSRF-1) — all egress goes through the onSave/onDetect
 * callbacks the parent wires to the vetted Tauri commands.
 */

import { useState } from 'react';
import {
  PRESETS,
  inferPreset,
  originOf,
  nextModelForPreset,
  type ProviderTier,
  type KeyStatusByOrigin,
  type KeyStatus,
  type SetKey,
  type KeySource,
} from './elmerModelConfig';
import { ModelForm } from './ElmerPane';
import { GetKeyCard } from './GetKeyCard';
import type { DetectState } from './useElmer';
import './ElmerPane.css';

export interface ModelTilePickerProps {
  onSave: (args: {
    agentEndpoint: string;
    agentModel: string;
    key: SetKey;
    agentTurnTimeoutSecs: number;
  }) => Promise<void>;
  onDetect: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  detectState: DetectState;
  /** Per-origin keyring status (statuses only, never values). Drives the per-tile key-saved badge. */
  keyStatusByOrigin: KeyStatusByOrigin;
  initialEndpoint: string;
  initialModel: string;
  initialKeyStatus: KeyStatus;
  initialTurnTimeoutSecs: number;
  /**
   * T10: When set, pre-selects the first tile in the given tier on mount.
   * Used by the rate-limit recovery "Switch provider" button to land the
   * operator on the paygo tier (Anthropic/OpenAI) directly.
   */
  focusTier?: ProviderTier;
}

/** Tier section headers + subtitles, in display order. */
const TIER_ORDER: { tier: ProviderTier; header: string; subtitle: string }[] = [
  { tier: 'free', header: 'Free · no credit card', subtitle: '' },
  { tier: 'paygo', header: 'Pay-as-you-go · needs a card', subtitle: '~pennies/turn' },
  { tier: 'local', header: 'Local · offline · no key', subtitle: '' },
  { tier: 'other', header: 'Other', subtitle: '' },
];

/**
 * Per-tile copy: description blurb and optional price badge shown inside the card.
 * The "Get a free key →" / "Get a key →" affordance on cloud tiles opens the key
 * page inline with the existing GetKeyCard flow when that tile is selected.
 */
const TILE_COPY: Record<string, { badge?: string; blurb: string }> = {
  gemini: {
    // No badge — RECOMMENDED label removed per operator decision
    blurb:
      'Capable cloud model. Free key from Google AI Studio, no billing card. ~2 minutes to set up. Model: gemini-2.5-flash.',
  },
  groq: {
    badge: 'Free',
    blurb: 'Very fast answers. Free key, no billing card. Model: llama-3.3-70b-versatile.',
  },
  anthropic: {
    badge: 'Paid',
    blurb:
      'Strongest tool-driver. Paste a Claude API key (console.anthropic.com). Default: claude-haiku-4-5 (cheap); switch to Sonnet for harder jobs.',
  },
  openai: {
    badge: 'Paid',
    blurb: 'Paste an OpenAI API key (platform.openai.com). Default: gpt-4o-mini.',
  },
  localOllama: {
    blurb:
      'Fully private, works with no internet. Needs a capable PC — a Raspberry Pi is too small to run one usefully.',
  },
  openrouter: {
    blurb: 'Any OpenAI-compatible endpoint — paste a URL, key, and model name.',
  },
  custom: {
    blurb: 'Any OpenAI-compatible endpoint — paste a URL, key, and model name.',
  },
};

// ---------------------------------------------------------------------------
// T10: Tier framing copy — honest per-tier context shown below the tile editor.
// Free tier: training-on-data note + what-gets-sent note.
// Local tier: private/offline constructive reframe.
// ---------------------------------------------------------------------------

interface TierFramingProps {
  tier: ProviderTier | undefined;
}

function TierFramingCopy({ tier }: TierFramingProps) {
  if (tier === 'free') {
    return (
      <p className="elmer-tier-framing" data-testid="elmer-tier-framing-free">
        Free tiers may train on submitted content. Messages you send to Elmer —
        including any station callsign, location, or message content — are transmitted
        to the provider. Review the provider's data policy before sending sensitive
        operational data.
      </p>
    );
  }
  if (tier === 'local') {
    return (
      <p className="elmer-tier-framing" data-testid="elmer-tier-framing-local">
        Runs privately on this computer — no data leaves the local network. Model
        capability is weaker than cloud options but nothing is sent off-device.
      </p>
    );
  }
  // paygo and other tiers: no additional framing copy (cloud paid tiers have
  // contractual data handling; users self-selected a bespoke endpoint).
  return null;
}

export function ModelTilePicker({
  onSave,
  onDetect,
  detectState,
  // Default to {} so a production mount that hasn't resolved key-status yet (or a
  // backend that returned nullish) renders no badges instead of crashing on a
  // per-origin read.
  keyStatusByOrigin = {},
  initialEndpoint,
  initialModel,
  initialKeyStatus,
  initialTurnTimeoutSecs,
  focusTier,
}: ModelTilePickerProps) {
  // Working endpoint + model, seeded from the SAVED config (not a tile default)
  // so the pre-selected tile shows the operator's model.
  // T10: when focusTier is set, initialise to the first preset in that tier
  // so the picker lands on it immediately (used by rate-limit "Switch provider").
  const [endpoint, setEndpoint] = useState(() => {
    if (focusTier) {
      const firstInTier = PRESETS.find((p) => p.tier === focusTier);
      if (firstInTier) return firstInTier.endpoint;
    }
    return initialEndpoint;
  });
  const [model, setModel] = useState(() => {
    if (focusTier) {
      const firstInTier = PRESETS.find((p) => p.tier === focusTier);
      if (firstInTier?.defaultModel) return firstInTier.defaultModel;
    }
    return initialModel;
  });

  // Which preset id is currently selected, derived from the working endpoint's
  // origin so a hand-edited path on a known origin still maps to its tile.
  const selectedId = inferPreset(endpoint);
  const selectedPreset = PRESETS.find((p) => p.id === selectedId);

  function handleTileSelect(presetId: string) {
    const preset = PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    // Decide the next model BEFORE moving the endpoint (nextModelForPreset reads
    // the OUTGOING endpoint to detect an untouched-vs-hand-edited model).
    const nextModel = nextModelForPreset(endpoint, model, presetId);
    setEndpoint(preset.endpoint);
    if (nextModel !== null) setModel(nextModel);
  }

  return (
    <div className="elmer-tile-picker" data-testid="elmer-tile-picker">
      {/* Intro paragraph — briefly explains what Elmer is and prompts model selection. */}
      <p className="elmer-tile-intro">
        Elmer is your on-station assistant — it can read your inbox, check the map, and draft
        messages. It&rsquo;s optional, and you choose what powers it.{' '}
        <strong>Pick a brain to get started.</strong>
      </p>

      {TIER_ORDER.map(({ tier, header, subtitle }) => {
        const tilesInTier = PRESETS.filter((p) => p.tier === tier);
        if (tilesInTier.length === 0) return null;
        return (
          <section className="elmer-tier" key={tier} data-tier={tier}>
            <h3 className="elmer-tier-header">
              {header}
              {subtitle && (
                <span className="elmer-tier-header-sub"> · {subtitle}</span>
              )}
            </h3>
            <div className="elmer-tier-tiles" role="radiogroup" aria-label={header}>
              {tilesInTier.map((preset) => {
                const isSelected = preset.id === selectedId;
                const origin = originOf(preset.endpoint);
                const keySaved = origin !== '' && keyStatusByOrigin[origin] === 'present';
                const tileCopy = TILE_COPY[preset.id];
                return (
                  <button
                    key={preset.id}
                    type="button"
                    role="radio"
                    aria-checked={isSelected}
                    className={`elmer-tile${isSelected ? ' elmer-tile--selected' : ''}${preset.id === 'localOllama' ? ' elmer-tile--local' : ''}`}
                    data-testid={`elmer-tile-${preset.id}`}
                    onClick={() => handleTileSelect(preset.id)}
                  >
                    {/* Top row: label + badges */}
                    <span className="elmer-tile-top-row">
                      <span className="elmer-tile-label">{preset.label}</span>
                      <span className="elmer-tile-badges">
                        {tileCopy?.badge && (
                          <span
                            className={`elmer-tile-badge elmer-tile-badge--tier elmer-tile-badge--${tileCopy.badge.toLowerCase()}`}
                          >
                            {tileCopy.badge}
                          </span>
                        )}
                        {keySaved && (
                          <span
                            className="elmer-tile-badge elmer-tile-badge--keysaved"
                            data-testid={`elmer-tile-keysaved-${preset.id}`}
                          >
                            ✓ key saved
                          </span>
                        )}
                      </span>
                    </span>
                    {/* Description blurb */}
                    {tileCopy?.blurb && (
                      <span className="elmer-tile-blurb">{tileCopy.blurb}</span>
                    )}
                    {/* "Get a key →" affordance for cloud tiles */}
                    {preset.keyPageUrl && (
                      <span className="elmer-tile-get-key">
                        {preset.tier === 'free' ? 'Get a free key →' : 'Get a key →'}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </section>
        );
      })}

      {/* Editor for the selected tile.
          - Cloud tiles with a keyPageUrl get the guided GetKeyCard flow (Task 9).
          - All other tiles (local Ollama + Other tier: openrouter, custom) render
            ModelForm verbatim so detect + model-select + endpoint editing + the
            loopback-keyless path are all available. The local tier previously
            rendered a bare summary (model text input + Save only) which dropped
            the Detect-models button; that branch is removed.
          T10: TierFramingCopy renders honest per-tier context below the editor. */}
      <div className="elmer-tile-editor" data-testid="elmer-tile-editor">
        {selectedPreset?.keyPageUrl ? (
          <>
            <GetKeyCard
              key={selectedPreset.id}
              preset={selectedPreset}
              onSave={onSave}
              agentModel={model}
              agentTurnTimeoutSecs={initialTurnTimeoutSecs}
              keyStatus={(() => {
                // Thread the per-origin key status to GetKeyCard so it can show
                // the "Key saved" affordance and skip forced re-entry in settings path.
                const origin = originOf(selectedPreset.endpoint);
                return origin !== '' ? (keyStatusByOrigin[origin] ?? 'absent') : 'absent';
              })()}
            />
            <TierFramingCopy tier={selectedPreset?.tier} />
          </>
        ) : (
          <>
            <ModelForm
              onSave={onSave}
              onDetect={onDetect}
              detectState={detectState}
              initialEndpoint={endpoint}
              initialModel={model}
              initialKeyStatus={initialKeyStatus}
              initialTurnTimeoutSecs={initialTurnTimeoutSecs}
            />
            <TierFramingCopy tier={selectedPreset?.tier} />
          </>
        )}
      </div>
    </div>
  );
}
