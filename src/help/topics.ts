/**
 * Help topic registry. Bundles docs/user-guide/*.md at build time via
 * import.meta.glob (TEST-1-safe pattern — no node:fs) and exposes a typed
 * read-only registry to the rest of the help/* components.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.3.
 */

export type HelpSectionId =
  | 'quickstart'
  | 'winlink-fundamentals'
  | 'radio-integration'
  | 'digital-modes'
  | 'using-tuxlink'
  | 'operating-practices'
  | 'reference'
  | 'migration';

export interface HelpTopic {
  slug: string;         // "01-what-is-tuxlink"
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
// 8-section IA grouping is editorial (spec §3.1).
export const SECTIONS: readonly HelpSection[] = [
  {
    id: 'quickstart',
    displayName: 'Quickstart',
    topicSlugs: ['01-what-is-tuxlink', '02-first-launch-wizard', '03-sending-your-first'],
  },
  {
    // Migration sits SECOND (operator request, tuxlink-5ceg): arriving from
    // Winlink Express / Pat is the most frequently-asked first question, so it
    // belongs near the top of the reading order, not buried at the bottom.
    id: 'migration',
    displayName: 'Migration',
    topicSlugs: ['32-from-express-or-pat'],
  },
  {
    id: 'winlink-fundamentals',
    displayName: 'Winlink fundamentals',
    topicSlugs: [
      '04-the-winlink-ecosystem',
      '05-cms-and-rms',
      '06-the-b2f-protocol',
      '07-mailbox-model',
      '33-operating-modes',
      '08-picking-a-transport',
    ],
  },
  {
    id: 'radio-integration',
    displayName: 'Radio integration',
    topicSlugs: [
      '09-ptt-overview',
      '10-digirig',
      '11-signalink-and-others',
      '12-cat-and-rigctld',
      '13-radio-specific-notes',
    ],
  },
  {
    id: 'digital-modes',
    displayName: 'Digital modes',
    topicSlugs: [
      '14-packet-on-ax25',
      '15-ardop-deep-dive',
      '16-vara-hf-deep-dive',
      '17-choosing-the-right-mode',
    ],
  },
  {
    id: 'using-tuxlink',
    displayName: 'Using tuxlink',
    topicSlugs: [
      '18-the-mailbox',
      '19-composing',
      '34-contacts-and-groups',
      '20-html-forms',
      '21-search',
      '22-user-folders',
      '23-catalog-requests',
      '35-agent-mcp',
    ],
  },
  {
    id: 'operating-practices',
    displayName: 'Operating practices',
    topicSlugs: [
      '24-emcomm-and-ics',
      '25-net-check-ins',
      '26-position-and-privacy',
    ],
  },
  {
    id: 'reference',
    displayName: 'Reference',
    topicSlugs: [
      '27-settings',
      '28-keyboard',
      '29-troubleshooting',
      '30-glossary',
      '31-credits',
    ],
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
