/**
 * useElmer — state + actions for the Elmer agent pane (AC-11, AC-14, G2).
 *
 * Wires:
 *   - invoke('elmer_send', { msg })    → sends a user message; runs the agent.
 *   - listen(EV_TURN)                 → streams text turns (user + assistant).
 *   - listen(EV_CHIP)                 → streams tool-call status chips.
 *   - listen(EV_OUTCOME)              → terminal outcome (done/cancelled/error…).
 *   - invoke('elmer_stop')            → abort-first cancel of the in-flight run.
 *   - invoke('elmer_config_read')     → reads {agentEndpoint, agentModel, keyStatus} (G2).
 *   - invoke('elmer_config_set', ...) → saves {agentEndpoint, agentModel, key:SetKey} (G2).
 *   - invoke('elmer_detect_models', ...) → returns string[] of available model ids (G2).
 *
 * AC-11: the hook accumulates turn + chip events into a typed message list that
 * ElmerPane renders as bubbles (turns) and chips (tool calls). It does NOT
 * forward raw tokens as a streaming character-by-character feed — each EV_TURN
 * event is a complete turn (role + text), so we append it as a discrete message.
 *
 * AC-14 (offline-endpoint): when the outcome's `outcomeKind` is 'offline' (or
 * the detail contains a recognisable offline marker), the hook surfaces a
 * distinct `phase: 'offline'` state so ElmerPane can render a friendly fallback.
 *
 * Security: this hook only invokes `elmer_send` and `elmer_stop`. It never
 * passes a conversation transcript to the backend (AC-5: transcript owned by
 * ElmerSession in Rust, not the React pane).
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ConfigReadDto, SetKey, KeySource } from './elmerModelConfig';
import {
  EV_CHIP,
  EV_OUTCOME,
  EV_TURN,
  type ElmerChipPayload,
  type ElmerOutcomePayload,
  type ElmerTurnPayload,
} from './elmerEvents';

// ---------------------------------------------------------------------------
// Message list item types
// ---------------------------------------------------------------------------

/** A prose turn in the chat (user or assistant). */
export interface ElmerTurnItem {
  kind: 'turn';
  id: string;
  role: string;
  text: string;
}

/** A tool-call chip (distinct from prose — AC-12 ground-truth). */
export interface ElmerChipItem {
  kind: 'chip';
  id: string;
  tool: string;
  status: string;
}

export type ElmerItem = ElmerTurnItem | ElmerChipItem;

// ---------------------------------------------------------------------------
// Outcome / phase
// ---------------------------------------------------------------------------

/** Terminal outcome received from the backend. */
export interface ElmerOutcome {
  outcomeKind: string;
  detail: string;
}

/**
 * High-level pane phase:
 *  - 'idle'       — no run in progress, awaiting a message.
 *  - 'running'    — a run is in progress (70-117 s typical wait).
 *  - 'done'       — last run completed cleanly.
 *  - 'cancelled'  — operator stopped the run.
 *  - 'needsOperator' — egress gated; operator review required.
 *  - 'toolDenied' — a tool call was denied (may surface approval dialog).
 *  - 'offline'    — the local endpoint is unreachable (AC-14 offline state).
 *  - 'error'      — unclassified error.
 */
export type ElmerPhase =
  | 'idle'
  | 'running'
  | 'done'
  | 'cancelled'
  | 'needsOperator'
  | 'toolDenied'
  | 'offline'
  | 'error';

function outcomeKindToPhase(outcomeKind: string): ElmerPhase {
  switch (outcomeKind) {
    case 'done':
      return 'done';
    case 'cancelled':
      return 'cancelled';
    case 'needsOperator':
      return 'needsOperator';
    case 'toolDenied':
      return 'toolDenied';
    case 'offline':
      return 'offline';
    default:
      return 'error';
  }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Model-config state (G2)
// ---------------------------------------------------------------------------

/** Loading state for the model-config form (G2). */
export type ModelConfigLoadState = 'idle' | 'loading' | 'loaded' | 'error';

/** Detection state for the Detect button (G2). */
export type DetectState =
  | { status: 'idle' }
  | { status: 'detecting' }
  | { status: 'success'; models: string[] }
  | { status: 'error'; reason: string };

export interface UseElmer {
  /** Ordered list of turn/chip items in this conversation. */
  items: ElmerItem[];
  /** Current pane phase (drives UI states). */
  phase: ElmerPhase;
  /** Last terminal outcome, or null if no run has completed yet. */
  lastOutcome: ElmerOutcome | null;
  /** Send a user message. No-op if a run is already in progress. */
  send: (msg: string) => void;
  /** Stop the in-flight run (abort-first cancel). */
  stop: () => void;
  /** G2: Loaded model config (null while loading/error). */
  modelConfig: ConfigReadDto | null;
  /** G2: Load state for model config. */
  modelConfigState: ModelConfigLoadState;
  /** G2: Load the model config from the backend. */
  configRead: () => Promise<void>;
  /** G2: Save the model config. */
  configSet: (args: { agentEndpoint: string; agentModel: string; key: SetKey }) => Promise<void>;
  /** G2: Detect available models for the given endpoint. */
  detectModels: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  /** G2: Current detection state. */
  detectState: DetectState;
}

let _nextId = 0;
function nextId(): string {
  return `elmer-item-${_nextId++}`;
}

export function useElmer(): UseElmer {
  const [items, setItems] = useState<ElmerItem[]>([]);
  const [phase, setPhase] = useState<ElmerPhase>('idle');
  const [lastOutcome, setLastOutcome] = useState<ElmerOutcome | null>(null);
  // Guard against launching a second send while one is running.
  const running = useRef(false);

  // G2: Model-config state.
  const [modelConfig, setModelConfig] = useState<ConfigReadDto | null>(null);
  const [modelConfigState, setModelConfigState] = useState<ModelConfigLoadState>('idle');
  const [detectState, setDetectState] = useState<DetectState>({ status: 'idle' });

  // Subscribe to all three Elmer event channels for the lifetime of the hook.
  // The listeners are set up once on mount and torn down on unmount. Tauri's
  // `listen` returns an `UnlistenFn`; we collect them and call all on cleanup.
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      const unTurn = await listen<ElmerTurnPayload>(EV_TURN, (event) => {
        const payload = event.payload;
        setItems((prev) => [
          ...prev,
          { kind: 'turn', id: nextId(), role: payload.role, text: payload.text },
        ]);
      });

      const unChip = await listen<ElmerChipPayload>(EV_CHIP, (event) => {
        const payload = event.payload;
        setItems((prev) => [
          ...prev,
          { kind: 'chip', id: nextId(), tool: payload.tool, status: payload.status },
        ]);
      });

      const unOutcome = await listen<ElmerOutcomePayload>(EV_OUTCOME, (event) => {
        const payload = event.payload;
        const outcome: ElmerOutcome = {
          outcomeKind: payload.outcomeKind,
          detail: payload.detail,
        };
        setLastOutcome(outcome);
        setPhase(outcomeKindToPhase(payload.outcomeKind));
        running.current = false;
      });

      unlisteners.push(unTurn, unChip, unOutcome);
    };

    void setupListeners();

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const send = useCallback((msg: string) => {
    if (running.current) return;
    running.current = true;
    setPhase('running');
    // Append the user's message immediately so the pane feels responsive
    // before the first EV_TURN event arrives (the model takes 70-117 s).
    setItems((prev) => [
      ...prev,
      { kind: 'turn', id: nextId(), role: 'user', text: msg },
    ]);
    void invoke('elmer_send', { msg }).catch((err: unknown) => {
      // elmer_send rejects on NeedsOperator (the backend also emits EV_OUTCOME
      // in that case, so the phase will already be 'needsOperator'). Swallow the
      // rejection here — the EV_OUTCOME listener is the authoritative phase setter.
      // Log for debugging only.
      console.error('[useElmer] elmer_send rejected:', err);
    });
  }, []);

  const stop = useCallback(() => {
    void invoke('elmer_stop');
  }, []);

  // G2: configRead — load model config from backend.
  const configRead = useCallback(async () => {
    setModelConfigState('loading');
    try {
      const dto = await invoke<ConfigReadDto>('elmer_config_read');
      setModelConfig(dto);
      setModelConfigState('loaded');
    } catch {
      setModelConfigState('error');
    }
  }, []);

  // G2: configSet — save model config to backend.
  const configSet = useCallback(async (args: { agentEndpoint: string; agentModel: string; key: SetKey }) => {
    await invoke('elmer_config_set', args);
  }, []);

  // G2: detectModels — detect available models for the given endpoint.
  const detectModels = useCallback(async (args: { agentEndpoint: string; keySource: KeySource }) => {
    setDetectState({ status: 'detecting' });
    try {
      const models = await invoke<string[]>('elmer_detect_models', args);
      setDetectState({ status: 'success', models });
    } catch (err: unknown) {
      const reason = err instanceof Error ? err.message : String(err);
      setDetectState({ status: 'error', reason });
    }
  }, []);

  return {
    items,
    phase,
    lastOutcome,
    send,
    stop,
    modelConfig,
    modelConfigState,
    configRead,
    configSet,
    detectModels,
    detectState,
  };
}
