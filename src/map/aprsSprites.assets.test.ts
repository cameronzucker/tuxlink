import { describe, it, expect } from 'vitest';
import sheet0 from '../assets/aprs-symbols/aprs-symbols-64-0.png';
import sheet1 from '../assets/aprs-symbols/aprs-symbols-64-1.png';
import sheet2 from '../assets/aprs-symbols/aprs-symbols-64-2.png';
import copyright from '../assets/aprs-symbols/COPYRIGHT.md?raw';
import notice from '../../NOTICE?raw';

// Asserts the vendored assets resolve through Vite (the path the app bake uses)
// and that attribution is present — the licensing half of tuxlink-90xb.
describe('vendored APRS symbol assets', () => {
  it('resolves the three 64px sheets and the upstream COPYRIGHT', () => {
    for (const url of [sheet0, sheet1, sheet2]) expect(typeof url).toBe('string');
    expect(copyright).toMatch(/APRS/i);
  });

  it('NOTICE attributes hessu/aprs-symbols under CC BY-SA 2.0', () => {
    expect(notice).toMatch(/aprs-symbols/);
    expect(notice).toMatch(/CC BY-SA 2\.0/);
  });
});
