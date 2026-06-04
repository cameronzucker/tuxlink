import { describe, it, expect, beforeEach } from 'vitest';
import { loadMermaid, resetMermaidLoaderForTesting } from './mermaidLoader';

describe('mermaid loader', () => {
  beforeEach(() => resetMermaidLoaderForTesting());

  it('returns the same promise on multiple calls (no double-load)', () => {
    const p1 = loadMermaid();
    const p2 = loadMermaid();
    expect(p1).toBe(p2);
  });

  it('resolves to an object with a `render` method', async () => {
    const m = await loadMermaid();
    expect(typeof m.render).toBe('function');
  });
});
