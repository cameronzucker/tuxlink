import { useCallback, useEffect, useMemo, useRef } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { renderMarkdown } from '../shell/markdownRender';
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

      // Case 1: same-topic anchor (#section-id) — let native scroll fire.
      if (href.startsWith('#')) {
        // Native scroll into view via the browser; nothing to do.
        return;
      }

      // Case 2: cross-topic with anchor (e.g. 02-connections.md#vara-hf).
      // Must be tested before the bare .md case so the anchor is captured.
      const mdWithAnchorMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z0-9-]+)\.md(#[\w-]+)$/);
      if (mdWithAnchorMatch) {
        event.preventDefault();
        const slug = mdWithAnchorMatch[1];
        const anchorId = mdWithAnchorMatch[2];
        onNavigate(slug);
        // Schedule scroll-to-anchor after the next render completes.
        requestAnimationFrame(() => {
          const el = document.querySelector(anchorId);
          if (el) el.scrollIntoView({ behavior: 'auto', block: 'start' });
        });
        return;
      }

      // Case 3: cross-topic without anchor (existing behavior).
      // Accept bare ("03-mailbox.md") OR a relative prefix.
      const mdMatch = href.match(/^(?:.*\/)?(\d{2}-[a-z0-9-]+)\.md$/);
      if (mdMatch) {
        event.preventDefault();
        onNavigate(mdMatch[1]);
        return;
      }

      // Case 4: out-of-bundle .md (e.g. ../pitfalls/implementation-pitfalls.md).
      // tuxlink-ew3k bug 5: the build-time linter is the primary gate; this
      // no-op is belt-and-suspenders so the webview doesn't navigate off /help.
      if (/^\.{0,2}\/.*\.md$/.test(href)) {
        event.preventDefault();
        return;
      }

      // Case 5: external http(s) — route to the OS browser via shell:open.
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
