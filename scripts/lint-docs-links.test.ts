import { describe, it, expect } from 'vitest';
import { lintMarkdownLinks } from './lint-docs-links';

describe('lintMarkdownLinks', () => {
  it('accepts a bare .md ref to an existing topic', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [bar](02-bar.md).',
        'docs/user-guide/02-bar.md': '# bar',
      },
    });
    expect(result.errors).toEqual([]);
  });

  it('rejects a bare .md ref to a missing topic', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [bar](02-bar.md).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        file: 'docs/user-guide/01-foo.md',
        href: '02-bar.md',
        reason: 'target topic does not exist',
      }),
    );
  });

  it('rejects an out-of-bundle ../ link', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [pitfalls](../pitfalls/x.md).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '../pitfalls/x.md',
        reason: 'links outside the user-guide bundle are not allowed',
      }),
    );
  });

  it('accepts an existing in-topic anchor', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': '# foo\n\n## bar\n\nSee [bar](#bar).',
      },
    });
    expect(result.errors).toEqual([]);
  });

  it('rejects a missing in-topic anchor', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': '# foo\n\nSee [bar](#bar).',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '#bar',
        reason: 'anchor target does not exist in this topic',
      }),
    );
  });

  it('rejects a cross-topic anchor that does not exist', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [x](02-bar.md#nonexistent).',
        'docs/user-guide/02-bar.md': '# bar',
      },
    });
    expect(result.errors).toContainEqual(
      expect.objectContaining({
        href: '02-bar.md#nonexistent',
        reason: 'anchor target does not exist in cross-topic file',
      }),
    );
  });

  it('accepts http(s) URLs without checking them', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/01-foo.md': 'See [winlink](https://winlink.org).',
      },
    });
    expect(result.errors).toEqual([]);
  });

  it('accepts a bare .md ref to a topic with digits in the slug', () => {
    const result = lintMarkdownLinks({
      files: {
        'docs/user-guide/06-the-b2f-protocol.md': '# B2F',
        'docs/user-guide/14-packet-on-ax25.md': '# Packet',
        'docs/user-guide/01-foo.md': 'See [B2F](06-the-b2f-protocol.md) and [Packet](14-packet-on-ax25.md).',
      },
    });
    expect(result.errors).toEqual([]);
  });
});
