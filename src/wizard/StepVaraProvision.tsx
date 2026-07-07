// StepVaraProvision.tsx — wizard VARA HF provisioning step (tuxlink-w7212).
//
// The onboarding counterpart to setting up VARA by hand. VARA HF is a proprietary
// 32-bit VB6 Windows app that runs under WINE; getting it working is a fragile
// multi-step dance. This step drives the vendored `wine-vara-setup` engine
// (resources/wine-vara-setup, MIT) to do it, streaming its progress as a live
// checklist. See the engine's docs/tuxlink-integration.md for the JSONL contract.
//
// Why it lives at the END of onboarding, online, at prep time: provisioning is
// internet-bound (apt fetches WINE, winetricks downloads the VB6 runtime) so it
// CANNOT run in a field deployment with no signal. Doing it here, while the
// operator is set up and online, is the whole point.
//
// Posture: this only automates the one-time INSTALL. At runtime Tuxlink still
// treats VARA as a third-party external process it connects to on 8300/8301 — it
// does not manage VARA's lifecycle. ARDOP is the always-there HF floor, so this
// step is fully skippable, and it self-skips where VARA can't run (non-x86_64) or
// isn't bundled.
//
// We cannot download VARA for the user: it is proprietary, version-dependent
// (VARA rejects version mismatches), has no stable direct-download URL, and its
// distribution pages block automated fetches. So the user opens the download page
// in their own browser, then points us at the .exe they downloaded.

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { useWizard } from './wizardContext';

// EA5HVK's page (VARA's author) — verified reachable; the canonical source.
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

type Phase = 'checking' | 'ready' | 'installing' | 'done' | 'error';

export function StepVaraProvision() {
  const { dispatch } = useWizard();
  const [phase, setPhase] = useState<Phase>('checking');
  const [checkpoints, setCheckpoints] = useState<EngineEvent[]>([]);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const unlistenRef = useRef<null | (() => void)>(null);

  const advance = useCallback(
    () => dispatch({ type: 'ADVANCE_FROM_VARA_PROVISION' }),
    [dispatch],
  );

  // Self-skip on non-x86_64 hardware (VARA unsupported) or when the setup engine
  // is not bundled (e.g. a dev build). Onboarding must never be blocked by this.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [platform, engineOk] = await Promise.all([
          invoke<{ varaSupported: boolean }>('platform_info'),
          invoke<boolean>('vara_engine_available'),
        ]);
        if (cancelled) return;
        if (!platform.varaSupported || !engineOk) {
          advance();
          return;
        }
        setPhase('ready');
      } catch {
        if (!cancelled) advance();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [advance]);

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
    // Cancelled, or a multi-selection (we requested single): do nothing.
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

  if (phase === 'checking') {
    return (
      <div className="wizard-step wizard-step-vara" data-testid="wizard-step-vara">
        <p>Checking VARA support…</p>
      </div>
    );
  }

  return (
    <div className="wizard-step wizard-step-vara" data-testid="wizard-step-vara">
      <h1>Set up VARA HF (optional)</h1>
      <p>
        VARA HF is the modem most Winlink HF gateways use. Tuxlink already includes
        ARDOP, so you can operate HF without this — but setting up VARA now, while
        you are online, means it is ready before you deploy. It cannot be installed
        in the field (it needs the internet to download).
      </p>

      {phase === 'ready' && (
        <>
          <ol className="wizard-vara__steps">
            <li>
              <strong>Download VARA HF in your browser.</strong> It is free software
              from its author, but we are not allowed to bundle it, and its version
              must match what Winlink currently recommends.
              <div className="wizard-choice-cards wizard-vara__downloads">
                <button
                  type="button"
                  className="wizard-choice-card"
                  data-testid="wizard-vara-open-rosmodem"
                  onClick={() => openDownload(ROSMODEM_URL)}
                >
                  <strong>Open the VARA author page</strong>
                  <p>rosmodem.wordpress.com (EA5HVK) — the canonical source.</p>
                </button>
                <button
                  type="button"
                  className="wizard-choice-card"
                  data-testid="wizard-vara-open-winlink"
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
          <div className="wizard-submit-row">
            <button
              type="button"
              className="wizard-btn-secondary"
              data-testid="wizard-vara-skip"
              onClick={advance}
            >
              Skip — I&apos;ll use ARDOP
            </button>
            <button
              type="button"
              data-testid="wizard-vara-choose"
              onClick={() => void chooseAndInstall()}
            >
              Select installer &amp; set up VARA
            </button>
          </div>
        </>
      )}

      {phase === 'installing' && (
        <div data-testid="wizard-vara-installing">
          <p>Setting up VARA HF — this can take a few minutes…</p>
          <ul className="wizard-vara__checklist">
            {checkpoints.map((c) => (
              <li
                key={c.id}
                className={`wizard-vara__item wizard-vara__item--${c.state ?? 'pending'}`}
                data-testid={`wizard-vara-cp-${c.id}`}
              >
                <span className="wizard-vara__item-label">
                  {CHECKPOINT_LABELS[c.id ?? ''] ?? c.id}
                </span>
                <span className="wizard-vara__item-state">{c.state}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {phase === 'done' && (
        <div data-testid="wizard-vara-done">
          <p className="wizard-vara__ok">VARA HF is set up. It will start with Tuxlink when your radio is connected.</p>
          <div className="wizard-submit-row">
            <button type="button" data-testid="wizard-vara-continue" onClick={advance} autoFocus>
              Continue
            </button>
          </div>
        </div>
      )}

      {phase === 'error' && (
        <div data-testid="wizard-vara-error">
          <div role="alert" className="wizard-error-banner">
            VARA setup did not finish: {errorMsg}
          </div>
          <p>
            You can try again, or skip and use ARDOP for now — you can set up VARA
            later from the VARA panel.
          </p>
          <div className="wizard-submit-row">
            <button
              type="button"
              className="wizard-btn-secondary"
              data-testid="wizard-vara-skip-after-error"
              onClick={advance}
            >
              Skip for now
            </button>
            <button
              type="button"
              data-testid="wizard-vara-retry"
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
