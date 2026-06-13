import { describe, it, expect } from 'vitest';
import { humanSize, airtimeEstimate, isImageFilename } from './attachmentFormat';

describe('humanSize', () => {
  it('formats bytes/KB/MB', () => {
    expect(humanSize(512)).toBe('512 B');
    expect(humanSize(2048)).toBe('2.0 KB');
    expect(humanSize(2 * 1024 * 1024)).toBe('2.0 MB');
  });
});

describe('airtimeEstimate', () => {
  it('reports a worst-case (slow packet) duration string', () => {
    // ~90 B/s floor -> 10KB ~ 110s -> minutes.
    expect(airtimeEstimate(10 * 1024)).toMatch(/min|sec/);
  });
  it('uses seconds for small payloads', () => {
    expect(airtimeEstimate(900)).toMatch(/sec/);
  });
});

describe('isImageFilename', () => {
  it('detects image extensions case-insensitively incl. heic', () => {
    expect(isImageFilename('IMG_0001.HEIC')).toBe(true);
    expect(isImageFilename('map.png')).toBe(true);
    expect(isImageFilename('brief.pdf')).toBe(false);
    expect(isImageFilename('noext')).toBe(false);
  });
});
