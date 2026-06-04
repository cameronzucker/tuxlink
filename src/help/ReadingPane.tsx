import { useCallback, useEffect, useMemo, useRef } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { renderMarkdown } from '../shell/markdownRenderV2';
import { sanitizeHtml } from '../shell/sanitizeHtml';
import { useMermaidRender } from './useMermaidRender';
import { addCopyButtons } from './copyButton';
import type { HelpTopic } from './topics';
import './ReadingPane.css';

interface ReadingPaneProps {
  topic: HelpTopic;
  onNavigate: (slug: string) => void;
}

export function ReadingPane({ topic, onNavigate }: ReadingPaneProps) {
  const scrollRef = useRef<HTMLElement | null>(null);
  const contentRef = useRef<HTMLElement | null>(null);

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
  // only re-runs when the active topic (or its content) changes. The
  // V2 pipeline emits HTML; we sanitize it via DOMPurify before injection
  // through dangerouslySetInnerHTML.
  const html = useMemo(
    () => sanitizeHtml(renderMarkdown(topic.body)),
    [topic.body],
  );

  useMermaidRender(contentRef);

  // Decorate <pre> blocks with copy buttons after the HTML lands. Re-runs
  // when the topic body changes so a newly-rendered topic gets its buttons.
  useEffect(() => {
    if (contentRef.current) {
      addCopyButtons(contentRef.current);
    }
  }, [html]);

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
        <article
          className="tux-help-reading-content"
          ref={(el) => { contentRef.current = el; }}
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </div>
    </main>
  );
}
