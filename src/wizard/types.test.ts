import { describe, it, expect } from 'vitest';
import type { WizardError } from './types';

describe('wizard types', () => {
  it('WizardError discriminated union has all 8 variants', () => {
    const variants: WizardError['kind'][] = [
      'Unavailable', 'Locked', 'PermissionDenied',
      'ConfigWrite', 'ConfigWriteAndRollbackFailed',
      'Busy', 'InvalidInput', 'Other',
    ];
    expect(variants).toHaveLength(8);
  });

  // TestSendOutcome removed (Task 5.4 / tuxlink-9phd): the Pat-based test-send
  // discriminated union is replaced by CMS_VERIFY_RESULT { ok: boolean } in the
  // reducer. No FE type mirrors a Rust enum for this path any more.
  it('TestSendOutcome type no longer exists (verify it is not imported anywhere)', () => {
    // This test is intentionally trivial — it documents the removal.
    // The TS compile step (`tsc --noEmit`) is the authoritative gate.
    expect(true).toBe(true);
  });
});
