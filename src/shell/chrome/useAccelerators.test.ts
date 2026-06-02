import { describe, it, expect } from 'vitest';
import { matchAccelerator, isTextInputFocused } from './useAccelerators';

describe('matchAccelerator', () => {
  it('matches Ctrl+N → message:new', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBe('menu:message:new');
  });

  it('treats Meta as Ctrl (CmdOrCtrl)', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: false, metaKey: true, shiftKey: false }))
      .toBe('menu:message:new');
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

  // tuxlink-ca5x: Archive shortcut is plain `A` and must be suppressed when a
  // text input is focused so typing the letter in the search bar / compose body
  // doesn't archive the open message.
  it('matches plain A → message:archive when no text input is focused', () => {
    expect(matchAccelerator({ key: 'a', ctrlKey: false, metaKey: false, shiftKey: false }, false))
      .toBe('menu:message:archive');
    expect(matchAccelerator({ key: 'A', ctrlKey: false, metaKey: false, shiftKey: false }, false))
      .toBe('menu:message:archive');
  });

  it('suppresses A when a text input is focused', () => {
    expect(matchAccelerator({ key: 'a', ctrlKey: false, metaKey: false, shiftKey: false }, true))
      .toBeNull();
  });

  it('does NOT suppress modifier-bound combos when a text input is focused', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: true, metaKey: false, shiftKey: false }, true))
      .toBe('menu:message:new');
  });
});

describe('isTextInputFocused', () => {
  it('returns false for null target', () => {
    expect(isTextInputFocused(null)).toBe(false);
  });

  it('returns true for TEXTAREA', () => {
    const ta = document.createElement('textarea');
    expect(isTextInputFocused(ta)).toBe(true);
  });

  it('returns true for INPUT type=text (default)', () => {
    const input = document.createElement('input');
    expect(isTextInputFocused(input)).toBe(true);
  });

  it('returns true for INPUT type=search/email/url/password/number/tel', () => {
    for (const t of ['search', 'email', 'url', 'password', 'number', 'tel']) {
      const input = document.createElement('input');
      input.type = t;
      expect(isTextInputFocused(input)).toBe(true);
    }
  });

  it('returns false for INPUT type=checkbox/radio/button/submit/range', () => {
    for (const t of ['checkbox', 'radio', 'button', 'submit', 'range']) {
      const input = document.createElement('input');
      input.type = t;
      expect(isTextInputFocused(input)).toBe(false);
    }
  });

  it('returns true for contenteditable elements', () => {
    const div = document.createElement('div');
    div.setAttribute('contenteditable', 'true');
    expect(isTextInputFocused(div)).toBe(true);
  });

  it('returns false for plain divs and buttons', () => {
    expect(isTextInputFocused(document.createElement('div'))).toBe(false);
    expect(isTextInputFocused(document.createElement('button'))).toBe(false);
  });
});
