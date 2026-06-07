import { describe, it, expect } from 'vitest';
import { TOPICS, SECTIONS, getTopicBySlug } from './topics';

describe('topics registry', () => {
  it('exposes thirty-three topics', () => {
    expect(TOPICS).toHaveLength(33);
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
