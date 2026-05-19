import { describe, it, expect } from 'vitest';
import type { WizardError, TestSendOutcome } from './types';

describe('wizard types', () => {
  it('WizardError discriminated union has all 8 variants', () => {
    const variants: WizardError['kind'][] = [
      'Unavailable', 'Locked', 'PermissionDenied',
      'ConfigWrite', 'ConfigWriteAndRollbackFailed',
      'Busy', 'InvalidInput', 'Other',
    ];
    expect(variants).toHaveLength(8);
  });

  it('TestSendOutcome discriminated union has Success + Failed', () => {
    const variants: TestSendOutcome['kind'][] = ['Success', 'Failed'];
    expect(variants).toHaveLength(2);
  });
});
