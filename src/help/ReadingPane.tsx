import { useCallback } from 'react';
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
  const handleClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      const target = event.target as HTMLElement;
      const anchor = target.closest('a');
      if (!anchor) return;
      const href = anchor.getAttribute('href') ?? '';

      // Inter-topic .md links — match digits-name-.md and navigate in-window.
      const mdMatch = href.match(/^(\d{2}-[a-z-]+)\.md$/);
      if (mdMatch) {
        event.preventDefault();
        onNavigate(mdMatch[1]);
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

  const blocks = parseMarkdown(topic.body);

  return (
    <main className="tux-help-reading" onClick={handleClick}>
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
