import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const ASSET_DIR = resolve(__dirname, '../assets/aprs-symbols');

describe('vendored APRS symbol assets', () => {
  it('ships the three 64px sheets and the upstream COPYRIGHT', () => {
    for (const f of [
      'aprs-symbols-64-0.png',
      'aprs-symbols-64-1.png',
      'aprs-symbols-64-2.png',
      'COPYRIGHT.md',
    ]) {
      expect(existsSync(resolve(ASSET_DIR, f)), `missing ${f}`).toBe(true);
    }
  });

  it('NOTICE attributes hessu/aprs-symbols under CC BY-SA 2.0', () => {
    const notice = readFileSync(resolve(__dirname, '../../NOTICE'), 'utf8');
    expect(notice).toMatch(/aprs-symbols/);
    expect(notice).toMatch(/CC BY-SA 2\.0/);
  });
});
