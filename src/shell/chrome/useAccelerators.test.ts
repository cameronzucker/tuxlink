import { describe, it, expect } from 'vitest';
import { matchAccelerator } from './useAccelerators';

describe('matchAccelerator', () => {
  it('matches Ctrl+N → file:new', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBe('menu:file:new');
  });

  it('treats Meta as Ctrl (CmdOrCtrl)', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: false, metaKey: true, shiftKey: false }))
      .toBe('menu:file:new');
  });

  it('distinguishes Ctrl+R from Ctrl+Shift+R', () => {
    expect(matchAccelerator({ key: 'r', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBe('menu:message:reply');
    expect(matchAccelerator({ key: 'R', ctrlKey: true, metaKey: false, shiftKey: true }))
      .toBe('menu:message:reply_all');
  });

  it('matches F5 with no modifier and Ctrl+Shift+O → connect', () => {
    expect(matchAccelerator({ key: 'F5', ctrlKey: false, metaKey: false, shiftKey: false }))
      .toBe('menu:session:connect');
    expect(matchAccelerator({ key: 'o', ctrlKey: true, metaKey: false, shiftKey: true }))
      .toBe('menu:session:connect');
  });

  it('returns null for unbound combos', () => {
    expect(matchAccelerator({ key: 'z', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBeNull();
    expect(matchAccelerator({ key: 'n', ctrlKey: false, metaKey: false, shiftKey: false }))
      .toBeNull();
  });
});
