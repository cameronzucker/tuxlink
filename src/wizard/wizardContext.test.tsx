import { describe, it, expect } from 'vitest';
import type { ReactNode } from 'react';
import { renderHook } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';

describe('wizardContext', () => {
  it('useWizard outside WizardProvider throws', () => {
    expect(() => renderHook(() => useWizard())).toThrow();
  });

  it('useWizard inside WizardProvider returns {state, dispatch}', () => {
    const wrapper = ({ children }: { children: ReactNode }) => <WizardProvider>{children}</WizardProvider>;
    const { result } = renderHook(() => useWizard(), { wrapper });
    expect(result.current.state.step).toBe('account');
    expect(typeof result.current.dispatch).toBe('function');
  });
});
