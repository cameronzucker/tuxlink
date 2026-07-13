import { describe, it, expect } from 'vitest';
import { TOPICS, SECTIONS, getTopicBySlug } from './topics';

describe('topics registry', () => {
  // Deliberately NOT a hardcoded count. A magic number here fires on every
  // legitimate doc addition and teaches you to bump it without thinking, which is
  // how it stops catching anything. The invariant worth guarding is the one that
  // actually breaks the Help window: TOPICS is built by globbing
  // docs/user-guide/*.md and cross-referencing SECTIONS, so the two registries must
  // agree in BOTH directions. buildTopics() already throws on an ungrouped file;
  // this catches the reverse — a slug listed in SECTIONS whose markdown is gone.
  it('every grouped slug has a topic, and every topic is grouped', () => {
    const topicSlugs = new Set(TOPICS.map((t) => t.slug));
    const groupedSlugs = SECTIONS.flatMap((s) => s.topicSlugs);

    for (const slug of groupedSlugs) {
      expect(
        topicSlugs.has(slug),
        `SECTIONS lists "${slug}" but no docs/user-guide/${slug}.md was found`,
      ).toBe(true);
    }
    for (const t of TOPICS) {
      expect(
        groupedSlugs.includes(t.slug),
        `docs/user-guide/${t.slug}.md exists but is not grouped in SECTIONS`,
      ).toBe(true);
    }
    expect(TOPICS.length).toBe(groupedSlugs.length);
    // Sanity floor: the guide exists and did not silently collapse to nothing.
    expect(TOPICS.length).toBeGreaterThan(30);
  });

  it('every topic has a non-empty slug, number, displayName, body, sectionId', () => {
    for (const t of TOPICS) {
      expect(t.slug).toMatch(/^\d{2}-[a-z0-9-]+$/);
      expect(t.number).toMatch(/^\d{2}$/);
      expect(t.displayName.length).toBeGreaterThan(0);
      expect(t.body.length).toBeGreaterThan(0);
      expect([
        'quickstart',
        'winlink-fundamentals',
        'radio-integration',
        'digital-modes',
        'using-tuxlink',
        'operating-practices',
        'reference',
        'migration',
      ]).toContain(t.sectionId);
    }
  });

  it('every section references existing topic slugs', () => {
    const all = new Set(TOPICS.map((t) => t.slug));
    for (const sec of SECTIONS) {
      for (const slug of sec.topicSlugs) {
        expect(all.has(slug)).toBe(true);
      }
    }
  });

  it('every topic belongs to exactly one section', () => {
    const counts = new Map<string, number>();
    for (const sec of SECTIONS) {
      for (const slug of sec.topicSlugs) {
        counts.set(slug, (counts.get(slug) ?? 0) + 1);
      }
    }
    for (const t of TOPICS) {
      expect(counts.get(t.slug)).toBe(1);
    }
  });

  it('parses the displayName from the first # heading', () => {
    const intro = TOPICS.find((t) => t.slug === '01-what-is-tuxlink');
    expect(intro?.displayName).toBe('What is tuxlink');
  });

  it('getTopicBySlug returns the matching topic or undefined', () => {
    expect(getTopicBySlug('02-first-launch-wizard')?.displayName).toBe('First-launch wizard');
    expect(getTopicBySlug('99-no-such')).toBeUndefined();
  });
});
