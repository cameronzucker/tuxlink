import { useCallback, useEffect, useMemo, useRef } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { parseMarkdown } from '../shell/markdownRender';
import type { Block, InlineText, InlineRun } from '../shell/markdownRender';
import type { HelpTopic } from './topics';
import './ReadingPane.css';

interface ReadingPaneProps {
  topic: HelpTopic;
  onNavigate: (slug: string) => void;
}

export function ReadingPane({ topic, onNavigate }: ReadingPaneProps) {
  const scrollRef = useRef<HTMLElement | null>(null);

  const handleClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      const target = event.target as HTMLElement;
      const anchor = target.closest('a');
      if (!anchor) return;
      const href = anchor.getAttribute('href') ?? '';

      // Inter-topic .md links — accept bare ("03-mailbox.md") OR a relative
      // prefix ("./03-mailbox.md", "../user-guide/03-mailbox.md").
      const mdMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z-]+)\.md$/);
      if (mdMatch) {
        event.preventDefault();
        onNavigate(mdMatch[1]);
        return;
      }
      // tuxlink-ew3k bug 5: docs/user-guide/07-settings.md ends with a link
      // to ../pitfalls/implementation-pitfalls.md (outside the bundled
      // user-guide tree). We can't render that topic in-window. Intercept
      // and no-op so the webview doesn't navigate off /help. The right
      // long-term fix lives in the docs revision (tuxlink-s8qu).
      if (/^\.{0,2}\/.*\.md$/.test(href)) {
        event.preventDefault();
        return;
      }

      // Anchor (#section) links — let the browser handle natively (scrolls).
      if (href.startsWith('#')) return;

      // External http(s) links — route to the OS browser via shell:open.
      if (/^https?:\/\//.test(href)) {
        event.preventDefault();
        void shellOpen(href);
      }
    },
    [onNavigate],
  );

  // tuxlink-ew3k bug 6: parseMarkdown ran on every render — measurable
  // sluggishness on long topics. Memoize by the topic body so the parse
  // only re-runs when the active topic (or its content) changes.
  const blocks = useMemo(() => parseMarkdown(topic.body), [topic.body]);

  // tuxlink-ew3k bug 1: when the operator switched topics, the scroll
  // position carried over. Reset to top whenever the active topic changes.
  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [topic.slug]);

  return (
    <main
      className="tux-help-reading"
      onClick={handleClick}
      ref={(el) => { scrollRef.current = el; }}
    >
      <div className="tux-help-reading-inner">
        <article className="tux-help-reading-content">
          {blocks.map((b, i) => (
            <BlockView key={i} block={b} />
          ))}
        </article>
      </div>
    </main>
  );
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case 'heading':
      if (block.level === 1) return <h1><Inline t={block.text} /></h1>;
      if (block.level === 2) return <h2><Inline t={block.text} /></h2>;
      return <h3><Inline t={block.text} /></h3>;
    case 'paragraph':
      return <p><Inline t={block.text} /></p>;
    case 'list':
      return (
        <ul>
          {block.items.map((it, i) => (
            <li key={i}><Inline t={it} /></li>
          ))}
        </ul>
      );
    case 'code':
      return <pre><code>{block.text}</code></pre>;
    case 'table':
      return (
        <table>
          <thead>
            <tr>{block.headers.map((h, i) => <th key={i}><Inline t={h} /></th>)}</tr>
          </thead>
          <tbody>
            {block.rows.map((r, i) => (
              <tr key={i}>{r.map((c, j) => <td key={j}><Inline t={c} /></td>)}</tr>
            ))}
          </tbody>
        </table>
      );
  }
}

function Inline({ t }: { t: InlineText }) {
  return (
    <>
      {t.runs.map((run, i) => <Run key={i} run={run} />)}
    </>
  );
}

function Run({ run }: { run: InlineRun }) {
  switch (run.kind) {
    case 'text':   return <>{run.text}</>;
    case 'bold':   return <strong>{run.text}</strong>;
    case 'italic': return <em>{run.text}</em>;
    case 'code':   return <code>{run.text}</code>;
    case 'link':   return <a href={run.href}>{run.text}</a>;
  }
}
