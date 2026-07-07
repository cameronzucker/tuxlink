// StepVaraProvision.tsx — first-run wizard wrapper around the shared VARA
// provisioning flow (tuxlink-w7212).
//
// The wizard-specific concern is the self-skip: on non-x86_64 hardware (VARA
// unsupported) or when the setup engine is not bundled, onboarding must advance
// without ever showing the step. Everything else (download links, file-pick,
// install, progress) lives in the shared <VaraProvision> so the VARA panel can
// reuse the exact same flow for post-upgrade / anytime setup.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useWizard } from './wizardContext';
import { VaraProvision } from '../radio/VaraProvision';

export function StepVaraProvision() {
  const { dispatch } = useWizard();
  const [ready, setReady] = useState<boolean | null>(null);

  const advance = useCallback(
    () => dispatch({ type: 'ADVANCE_FROM_VARA_PROVISION' }),
    [dispatch],
  );

  // Self-skip on non-x86_64 hardware (VARA unsupported) or when the setup engine
  // is not bundled. Onboarding must never be blocked by this.
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
        setReady(true);
      } catch {
        if (!cancelled) advance();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [advance]);

  if (ready !== true) {
    return (
      <div className="wizard-step wizard-step-vara" data-testid="wizard-step-vara">
        <p>Checking VARA support…</p>
      </div>
    );
  }

  return (
    <div className="wizard-step wizard-step-vara" data-testid="wizard-step-vara">
      <VaraProvision variant="wizard" onComplete={advance} onSkip={advance} />
    </div>
  );
}
