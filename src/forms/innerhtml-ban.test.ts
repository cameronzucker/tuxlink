import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'fs';
import { join } from 'path';

function listFormFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      out.push(...listFormFiles(full));
    } else if (entry.endsWith('.tsx') || entry.endsWith('.ts')) {
      // Skip THIS test file (it mentions the string in assertion code).
      if (entry === 'innerhtml-ban.test.ts') continue;
      out.push(full);
    }
  }
  return out;
}

describe('forms module — dangerouslySetInnerHTML ban', () => {
  it('no file in src/forms/ uses dangerouslySetInnerHTML (spec §10)', () => {
    const files = listFormFiles('src/forms');
    const offenders: string[] = [];
    for (const f of files) {
      const content = readFileSync(f, 'utf-8');
      if (content.includes('dangerouslySetInnerHTML')) {
        offenders.push(f);
      }
    }
    expect(offenders).toEqual([]);
  });
});
