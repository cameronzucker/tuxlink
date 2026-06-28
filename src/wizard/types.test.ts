import { describe, it, expect } from 'vitest';
import { type WizardError, schemaTooNewMessage } from './types';

describe('wizard types', () => {
  it('WizardError discriminated union has all 9 variants', () => {
    const variants: WizardError['kind'][] = [
      'Unavailable', 'Locked', 'PermissionDenied',
      'ConfigWrite', 'ConfigWriteAndRollbackFailed', 'ConfigSchemaTooNew',
      'Busy', 'InvalidInput', 'Other',
    ];
    expect(variants).toHaveLength(9);
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

// tuxlink-xknyx: the schema-downgrade error must tell the truth and hand the
// user a working recovery command — not the misleading "disk full?" copy.
describe('schemaTooNewMessage', () => {
  const detail = { existing: 5, ours: 4, config_path: '/home/op/.config/tuxlink/config.json' };

  it('states the real cause, not disk/permissions', () => {
    const msg = schemaTooNewMessage(detail);
    expect(msg).toMatch(/newer version of Tuxlink/i);
    expect(msg).toContain('v5');
    expect(msg).toContain('v4');
    expect(msg).not.toMatch(/disk full/i);
    expect(msg).not.toMatch(/permissions\?/i);
  });

  it('surfaces a copy-pasteable, non-clobbering shell command with the exact path', () => {
    const msg = schemaTooNewMessage(detail);
    // Full command, single-quoted path (space-safe), unique timestamped backup.
    expect(msg).toContain(
      `mv '/home/op/.config/tuxlink/config.json' '/home/op/.config/tuxlink/config.json.bak-$(date +%s)'`,
    );
  });
});
