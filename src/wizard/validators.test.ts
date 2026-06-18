// validators.test.ts — per wizard-cluster plan Phase 3 Task 3.1
// Tests: callsign (loose AMD-3 + non-ASCII rejection per spec §5.9),
//        password (≥6 chars per Express convention),
//        grid (4/6-char Maidenhead, optional),
//        normalizeGrid.
import { describe, it, expect } from 'vitest';
import {
  validateCallsign,
  validatePassword,
  validateGrid,
  normalizeGrid,
  validateAccountPassword,
  validateAmateurCallsign,
  validateRecoveryEmail,
} from './validators';

describe('validators', () => {
  describe('validateCallsign', () => {
    it('accepts standard callsigns', () => {
      expect(validateCallsign('W4PHS')).toBeNull();   // null = no error
      expect(validateCallsign('K0SWE-7')).toBeNull();
      expect(validateCallsign('VK2/W4PHS/P')).toBeNull();
    });
    it('accepts tactical strings (AMD-3 loose validator)', () => {
      expect(validateCallsign('EOC-1')).toBeNull();
      expect(validateCallsign('BAOFENG-FM-01')).toBeNull();
    });
    it('rejects empty', () => {
      expect(validateCallsign('')).toMatch(/non-empty/i);
    });
    it('rejects internal whitespace', () => {
      expect(validateCallsign('W 4PHS')).toMatch(/whitespace/i);
    });
    it('rejects >32 chars', () => {
      expect(validateCallsign('A'.repeat(33))).toMatch(/32/);
    });
    it('rejects non-ASCII (Cyrillic А homoglyph)', () => {
      // U+0410 CYRILLIC CAPITAL LETTER A — visually identical to Latin A
      expect(validateCallsign('W4PHSА')).toMatch(/ASCII/i);
      // U+200D ZERO WIDTH JOINER
      expect(validateCallsign('W4PHS‍')).toMatch(/ASCII/i);
    });
    it('rejects ASCII control characters', () => {
      expect(validateCallsign('W4PHS\x00')).toMatch(/ASCII/i);
      expect(validateCallsign('W4PHS\x1f')).toMatch(/ASCII/i);
      expect(validateCallsign('W4PHS\x7f')).toMatch(/ASCII/i);
    });
  });

  describe('validatePassword', () => {
    it('rejects empty', () => {
      expect(validatePassword('')).toMatch(/required/i);
    });
    it('rejects < 6 chars', () => {
      expect(validatePassword('12345')).toMatch(/6/);
    });
    it('accepts 6+ chars', () => {
      expect(validatePassword('secret')).toBeNull();
      expect(validatePassword('a-very-long-passphrase-with-symbols!@#')).toBeNull();
    });
    it('accepts exactly 6 chars', () => {
      expect(validatePassword('123456')).toBeNull();
    });
  });

  describe('validateGrid', () => {
    it('accepts 4-char Maidenhead', () => {
      expect(validateGrid('EM75')).toBeNull();
      expect(validateGrid('FN20')).toBeNull();
    });
    it('accepts 4-char Maidenhead lowercase', () => {
      expect(validateGrid('em75')).toBeNull();
    });
    it('accepts 6-char Maidenhead', () => {
      expect(validateGrid('EM75xx')).toBeNull();
      expect(validateGrid('FN20ab')).toBeNull();
    });
    it('rejects malformed (first pair out of range: X > R)', () => {
      expect(validateGrid('XY99')).toMatch(/Maidenhead/);
    });
    it('rejects too many chars', () => {
      expect(validateGrid('em75abcde')).toMatch(/Maidenhead/);
    });
    it('rejects 5 chars (not 4 or 6)', () => {
      expect(validateGrid('EM75x')).toMatch(/Maidenhead/);
    });
    it('accepts empty (optional field)', () => {
      expect(validateGrid('')).toBeNull();
    });
  });

  // tuxlink-vfb3 sub-project 1 — account-creation validators.

  describe('validateAccountPassword (6–12, create only)', () => {
    it('rejects empty', () => {
      expect(validateAccountPassword('')).toMatch(/required/i);
    });
    it('rejects < 6 chars', () => {
      expect(validateAccountPassword('12345')).toMatch(/6 to 12/);
    });
    it('rejects > 12 chars (the live API upper bound)', () => {
      expect(validateAccountPassword('1234567890123')).toMatch(/6 to 12/);
    });
    it('accepts the 6 and 12 boundaries', () => {
      expect(validateAccountPassword('123456')).toBeNull();
      expect(validateAccountPassword('123456789012')).toBeNull();
    });
  });

  describe('validateAmateurCallsign (strict, create only)', () => {
    it('accepts standard US forms (case-insensitive, SSID stripped)', () => {
      expect(validateAmateurCallsign('KK7ABC')).toBeNull();
      expect(validateAmateurCallsign('kk7abc')).toBeNull();
      expect(validateAmateurCallsign('W1AW')).toBeNull();
      expect(validateAmateurCallsign('AA7BC')).toBeNull();
      expect(validateAmateurCallsign('KH6ABC')).toBeNull();
      expect(validateAmateurCallsign('KK7ABC-7')).toBeNull(); // SSID stripped to base
    });
    it('accepts digit-led international prefixes', () => {
      expect(validateAmateurCallsign('2E0AAA')).toBeNull();
      expect(validateAmateurCallsign('9A1AA')).toBeNull();
    });
    it('rejects tactical / word labels (the dangerous direction)', () => {
      for (const raw of ['RELAY1', 'EOC1', 'TEST123', 'ARES', 'EOC']) {
        expect(validateAmateurCallsign(raw)).toMatch(/callsign/i);
      }
    });
    it('rejects empty', () => {
      expect(validateAmateurCallsign('')).toMatch(/required/i);
    });
  });

  describe('validateRecoveryEmail (mandatory)', () => {
    it('rejects empty / whitespace', () => {
      expect(validateRecoveryEmail('')).toMatch(/required/i);
      expect(validateRecoveryEmail('   ')).toMatch(/required/i);
    });
    it('rejects malformed addresses', () => {
      expect(validateRecoveryEmail('notanemail')).toMatch(/valid email/i);
      expect(validateRecoveryEmail('no@domain')).toMatch(/valid email/i);
      expect(validateRecoveryEmail('a b@c.com')).toMatch(/valid email/i);
    });
    it('accepts a normal address', () => {
      expect(validateRecoveryEmail('kk7abc.ops@gmail.com')).toBeNull();
    });
  });

  describe('normalizeGrid', () => {
    it('uppercases the first 2 chars + lowercases the last 2 for 4-char', () => {
      expect(normalizeGrid('em75')).toBe('EM75');
    });
    it('uppercases first 2 + keeps digits + lowercases last 2 for 6-char', () => {
      expect(normalizeGrid('em75XX')).toBe('EM75xx');
      expect(normalizeGrid('EM75xx')).toBe('EM75xx');
    });
    it('passthrough for already-normalized input', () => {
      expect(normalizeGrid('EM75')).toBe('EM75');
    });
  });
});
