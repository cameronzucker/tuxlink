import { createContext, useContext, useReducer } from 'react';
import type { Dispatch, ReactNode } from 'react';
import { wizardReducer, initialWizardState } from './wizardReducer';
import type { WizardState, WizardAction } from './types';

interface WizardContextValue {
  state: WizardState;
  dispatch: Dispatch<WizardAction>;
}

const WizardContext = createContext<WizardContextValue | null>(null);

interface WizardProviderProps {
  children: ReactNode;
  /** Optional initial state override — for testing substates directly. */
  initialStateOverride?: Partial<WizardState>;
}

export function WizardProvider({ children, initialStateOverride }: WizardProviderProps) {
  const baseState = initialWizardState();
  const mergedState: WizardState = initialStateOverride
    ? { ...baseState, ...initialStateOverride }
    : baseState;
  const [state, dispatch] = useReducer(wizardReducer, mergedState);
  return <WizardContext.Provider value={{ state, dispatch }}>{children}</WizardContext.Provider>;
}

export function useWizard(): WizardContextValue {
  const ctx = useContext(WizardContext);
  if (!ctx) throw new Error('useWizard must be used inside <WizardProvider>');
  return ctx;
}
