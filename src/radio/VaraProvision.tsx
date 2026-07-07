// VaraProvision.tsx — the shared VARA HF provisioning flow (tuxlink-w7212).
//
// Drives the vendored `wine-vara-setup` engine to install VARA HF under WINE,
// streaming its progress as a live checklist. Host-agnostic: it takes callbacks
// and does NOT self-skip or check the platform — the host decides whether to
// render it (the first-run wizard step self-skips on unsupported hardware; the
// VARA panel only shows its entry point when VARA is supported).
//
// Two consumers:
//   - StepVaraProvision (first-run onboarding)
//   - VaraRadioPanel "Set up VARA HF…" (anytime / post-upgrade reachability)
//
// VARA is proprietary and version-dependent; we cannot download it. The user
// opens the download page in their own browser and points us at the .exe.

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import './VaraProvision.css';

// EA5HVK's page (VARA's author) — the canonical source.
const ROSMODEM_URL = 'https://rosmodem.wordpress.com/';
// The Winlink site also links VARA from its Software page.
const WINLINK_URL = 'https://www.winlink.org/';

// One line of the engine's --json stream (mirrors the frozen contract).
interface EngineEvent {
  event: string;
  id?: string;
  index?: number;
  total?: number;
  state?: string;
  detail?: string;
  ok?: boolean;
  vara_version?: string;
}

// Human labels for the engine's checkpoint ids (mirror the engine's wv_label).
const CHECKPOINT_LABELS: Record<string, string> = {
  deps: 'System dependencies (WINE)',
  prefix: 'WINE prefix',
  vara: 'VARA HF installation',
  vb6: 'Visual Basic 6 runtime',
  ocx: 'OCX controls',
  verify: 'Launch + connection check',
  autostart: 'Auto-start on login',
};

type Phase = 'ready' | 'installing' | 'done' | 'error';

export interface VaraProvisionProps {
  /** Fired when the user finishes (installed OR skipped). */
  onComplete: () => void;
  /** Optional distinct skip handler; defaults to onComplete. */
  onSkip?: () => void;
  /** 'wizard' or 'panel' — only affects a little copy. */
  variant?: 'wizard' | 'panel';
}

export function VaraProvision({ onComplete, onSkip, variant = 'wizard' }: VaraProvisionProps) {
  const [phase, setPhase] = useState<Phase>('ready');
  const [checkpoints, setCheckpoints] = useState<EngineEvent[]>([]);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const unlistenRef = useRef<null | (() => void)>(null);

  const skip = onSkip ?? onComplete;

  // Tear down any live progress listener on unmount.
  useEffect(
    () => () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    },
    [],
  );

  const openDownload = useCallback((url: string) => {
    void shellOpen(url).catch(() => {
      /* opening a browser is best-effort; the URLs are also shown as text */
    });
  }, []);

  const chooseAndInstall = useCallback(async () => {
    const selected = await openDialog({
      title: 'Select the VARA HF installer you downloaded',
      filters: [{ name: 'VARA installer', extensions: ['exe'] }],
    });
    if (typeof selected !== 'string') return;

    setCheckpoints([]);
    setErrorMsg(null);
    setPhase('installing');

    const unlisten = await listen<EngineEvent>('vara_install:progress', (evt) => {
      const p = evt.payload;
      if (p.event === 'checkpoint' && p.id) {
        setCheckpoints((prev) => {
          const rest = prev.filter((c) => c.id !== p.id);
          return [...rest, p].sort((a, b) => (a.index ?? 0) - (b.index ?? 0));
        });
      }
    });
    unlistenRef.current = unlisten;

    try {
      await invoke('vara_install_start', { installerPath: selected });
      setPhase('done');
    } catch (err) {
      setErrorMsg(typeof err === 'string' ? err : String(err));
      setPhase('error');
    } finally {
      unlisten();
      if (unlistenRef.current === unlisten) unlistenRef.current = null;
    }
  }, []);

  return (
    <div className="vara-provision" data-testid="vara-provision">
      {variant === 'wizard' && <h1>Set up VARA HF (optional)</h1>}
      <p>
        VARA HF is the modem most Winlink HF gateways use. Tuxlink already includes
        ARDOP, so you can operate HF without this — but setting up VARA now, while
        you are online, means it is ready before you deploy. It cannot be installed
        in the field (it needs the internet to download).
      </p>

      {phase === 'ready' && (
        <>
          <ol className="vara-provision__steps">
            <li>
              <strong>Download VARA HF in your browser.</strong> It is free software
              from its author, but we are not allowed to bundle it, and its version
              must match what Winlink currently recommends.
              <div className="vara-provision__downloads">
                <button
                  type="button"
                  className="vara-provision__card"
                  data-testid="vara-provision-open-rosmodem"
                  onClick={() => openDownload(ROSMODEM_URL)}
                >
                  <strong>Open the VARA author page</strong>
                  <p>rosmodem.wordpress.com (EA5HVK) — the canonical source.</p>
                </button>
                <button
                  type="button"
                  className="vara-provision__card"
                  data-testid="vara-provision-open-winlink"
                  onClick={() => openDownload(WINLINK_URL)}
                >
                  <strong>Open the Winlink site</strong>
                  <p>winlink.org — VARA is linked from the Software page.</p>
                </button>
              </div>
            </li>
            <li>
              <strong>Point Tuxlink at the file you downloaded</strong> and it will
              install and configure VARA for you.
            </li>
          </ol>
          <div className="vara-provision__actions">
            <button
              type="button"
              className="vara-provision__btn-secondary"
              data-testid="vara-provision-skip"
              onClick={skip}
            >
              {variant === 'wizard' ? "Skip — I'll use ARDOP" : 'Not now'}
            </button>
            <button
              type="button"
              data-testid="vara-provision-choose"
              onClick={() => void chooseAndInstall()}
            >
              Select installer &amp; set up VARA
            </button>
          </div>
        </>
      )}

      {phase === 'installing' && (
        <div data-testid="vara-provision-installing">
          <p>Setting up VARA HF — this can take a few minutes…</p>
          <ul className="vara-provision__checklist">
            {checkpoints.map((c) => (
              <li
                key={c.id}
                className={`vara-provision__item vara-provision__item--${c.state ?? 'pending'}`}
                data-testid={`vara-provision-cp-${c.id}`}
              >
                <span className="vara-provision__item-label">
                  {CHECKPOINT_LABELS[c.id ?? ''] ?? c.id}
                </span>
                <span className="vara-provision__item-state">{c.state}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {phase === 'done' && (
        <div data-testid="vara-provision-done">
          <p className="vara-provision__ok">
            VARA HF is set up. It will start with Tuxlink when your radio is connected.
          </p>
          <div className="vara-provision__actions">
            <button type="button" data-testid="vara-provision-continue" onClick={onComplete} autoFocus>
              {variant === 'wizard' ? 'Continue' : 'Done'}
            </button>
          </div>
        </div>
      )}

      {phase === 'error' && (
        <div data-testid="vara-provision-error">
          <div role="alert" className="vara-provision__error">
            VARA setup did not finish: {errorMsg}
          </div>
          <p>You can try again, or skip and use ARDOP for now.</p>
          <div className="vara-provision__actions">
            <button
              type="button"
              className="vara-provision__btn-secondary"
              data-testid="vara-provision-skip-after-error"
              onClick={skip}
            >
              {variant === 'wizard' ? 'Skip for now' : 'Close'}
            </button>
            <button
              type="button"
              data-testid="vara-provision-retry"
              onClick={() => setPhase('ready')}
            >
              Try again
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
