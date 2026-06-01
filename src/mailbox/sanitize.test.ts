import { describe, it, expect } from 'vitest';
import { sanitizeAttachmentName } from './sanitize';

describe('sanitizeAttachmentName', () => {
  it('passes through a normal filename unchanged', () => {
    expect(sanitizeAttachmentName('RMS_Express_Form_ICS213_Initial.xml')).toBe(
      'RMS_Express_Form_ICS213_Initial.xml',
    );
    expect(sanitizeAttachmentName('photo.jpg')).toBe('photo.jpg');
  });

  it('strips ASCII control characters (0x00-0x1f, 0x7f)', () => {
    expect(sanitizeAttachmentName('foo\x00bar')).toBe('foobar');
    expect(sanitizeAttachmentName('foo\r\nbar')).toBe('foobar');
    expect(sanitizeAttachmentName('foo\tbar')).toBe('foobar');
    expect(sanitizeAttachmentName('foo\x7fbar')).toBe('foobar');
    expect(sanitizeAttachmentName('foo\x01\x02\x03bar')).toBe('foobar');
  });

  it('replaces path separators with underscores (path injection defense)', () => {
    expect(sanitizeAttachmentName('../etc/passwd')).toBe('.._etc_passwd');
    expect(sanitizeAttachmentName('foo/bar.txt')).toBe('foo_bar.txt');
    expect(sanitizeAttachmentName('foo\\bar.txt')).toBe('foo_bar.txt');
  });

  it('caps display length at 255 characters', () => {
    const long = 'a'.repeat(300);
    const out = sanitizeAttachmentName(long);
    expect(out.length).toBe(255);
  });

  it('preserves non-ASCII (UTF-8) characters', () => {
    expect(sanitizeAttachmentName('résumé.pdf')).toBe('résumé.pdf');
    expect(sanitizeAttachmentName('文档.txt')).toBe('文档.txt');
  });

  it('handles empty input', () => {
    expect(sanitizeAttachmentName('')).toBe('');
  });

  it('handles input that is all stripped characters', () => {
    expect(sanitizeAttachmentName('\x00\x01\x02')).toBe('');
  });
});
