/**
 * Help topic registry. Bundles docs/user-guide/*.md at build time via
 * import.meta.glob (TEST-1-safe pattern — no node:fs) and exposes a typed
 * read-only registry to the rest of the help/* components.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.3.
 */

export type HelpSectionId = 'getting-started' | 'using' | 'config' | 'reference';

export interface HelpTopic {
  slug: string;         // "01-getting-started"
  number: string;       // "01"
  displayName: string;  // parsed from the first # heading
  body: string;         // raw markdown
  sectionId: HelpSectionId;
}

export interface HelpSection {
  id: HelpSectionId;
  displayName: string;
  topicSlugs: readonly string[];
}

// Section grouping is hand-authored — filename ordering is stable but the
// Getting-started / Using / Config / Reference grouping is editorial.
export const SECTIONS: readonly HelpSection[] = [
  {
    id: 'getting-started',
    displayName: 'Getting started',
    topicSlugs: ['01-getting-started', '02-connections'],
  },
  {
    id: 'using',
    displayName: 'Using Tuxlink',
    topicSlugs: ['03-mailbox', '04-composing', '05-forms', '06-search'],
  },
  {
    id: 'config',
    displayName: 'Configuration',
    topicSlugs: ['07-settings', '08-color-schemes', '09-keyboard'],
  },
  {
    id: 'reference',
    displayName: 'Reference',
    topicSlugs: ['10-troubleshooting'],
  },
];

// Build a slug → sectionId map once.
const SLUG_TO_SECTION: Record<string, HelpSectionId> = {};
for (const sec of SECTIONS) {
  for (const slug of sec.topicSlugs) {
    SLUG_TO_SECTION[slug] = sec.id;
  }
}

// Bundle all markdown files at build time. Vite's import.meta.glob with
// { eager: true, query: '?raw' } returns { '/path/01.md': 'raw content', ... }.
const RAW_TOPICS = import.meta.glob('/docs/user-guide/*.md', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

// Parse the first `# heading` from a markdown body. Returns the heading text
// (without `#` or leading/trailing whitespace) or the slug as a fallback.
function parseDisplayName(body: string, slug: string): string {
  const match = body.match(/^#\s+(.+)$/m);
  if (match) return match[1].trim();
  return slug;
}

function buildTopics(): readonly HelpTopic[] {
  const out: HelpTopic[] = [];
  for (const [path, body] of Object.entries(RAW_TOPICS)) {
    const filename = path.split('/').pop()!.replace(/\.md$/, '');  // "01-getting-started"
    const numberMatch = filename.match(/^(\d{2})-/);
    if (!numberMatch) continue;  // filename does not match the convention
    const slug = filename;
    const sectionId = SLUG_TO_SECTION[slug];
    if (!sectionId) {
      throw new Error(
        `topics.ts: markdown file ${slug} is not grouped in SECTIONS. ` +
        `Add it to a section or rename the file.`,
      );
    }
    out.push({
      slug,
      number: numberMatch[1],
      displayName: parseDisplayName(body, slug),
      body,
      sectionId,
    });
  }
  out.sort((a, b) => a.slug.localeCompare(b.slug));
  return out;
}

export const TOPICS: readonly HelpTopic[] = buildTopics();

export function getTopicBySlug(slug: string): HelpTopic | undefined {
  return TOPICS.find((t) => t.slug === slug);
}
