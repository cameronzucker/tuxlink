/**
 * HelpPanel — inline (in-webview) Documentation viewer (tuxlink-35g0).
 *
 * Opened from Help → Documentation. Two-pane layout: topic list on the
 * left, rendered markdown on the right. The bundled topic markdown lives
 * under docs/user-guide/ and is imported at build time via
 * `import.meta.glob` (the same TEST-1-safe pattern used by the rest of the
 * app — no `node:fs`, no `__dirname`).
 *
 * Links inside the markdown go to either:
 *   - Another bundled topic (relative .md paths) — handled in-panel; the
 *     panel swaps the active topic without leaving the modal.
 *   - An external URL — opens in the operator's default browser via
 *     `@tauri-apps/plugin-shell::open`.
 *
 * NOT a separate OS window — inline overlay per feedback_inline_ui_no_window_clutter.
 */

import { useEffect, useMemo, useState } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { parseMarkdown, type Block, type InlineText, type InlineRun } from './markdownRender';
import './HelpPanel.css';

// Bundle every user-guide markdown file as a raw string at build time. The
// glob is relative to THIS file, hence the `../../docs/user-guide/` path.
// The `?raw` query is the TEST-1-safe pattern (no node:fs needed).
const RAW_TOPICS = import.meta.glob('../../docs/user-guide/*.md', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

interface Topic {
  /** The filename stem (e.g. "01-getting-started") — used as the URL-safe
   *  identifier in cross-topic links and the current-topic state. */
  id: string;
  /** The H1 lifted from the markdown — shown in the topic list and as the
   *  panel's active-topic title. */
  title: string;
  /** Raw markdown source. */
  source: string;
}

/** Build the bundled topic list from the raw glob output. Topics are
 *  sorted by filename (the `NN-` numeric prefix carries the canonical
 *  order). Each topic's title is the first H1 in the markdown — if no H1
 *  exists, the filename stem is used. */
const TOPICS: Topic[] = Object.entries(RAW_TOPICS)
  .map(([path, source]) => {
    const stem = path.replace(/^.*\//, '').replace(/\.md$/, '');
    const m = /^#\s+(.+)$/m.exec(source);
    const title = m ? m[1] : stem;
    return { id: stem, title, source };
  })
  .sort((a, b) => a.id.localeCompare(b.id));

const FALLBACK_TOPIC: Topic | undefined = TOPICS[0];

export interface HelpPanelProps {
  open: boolean;
  onClose: () => void;
}

function renderInline(text: InlineText, key: string, onTopicLink: (topicId: string) => void): React.ReactNode {
  return text.runs.map((run: InlineRun, i: number) => {
    const k = `${key}-${i}`;
    switch (run.kind) {
      case 'text':
        return <span key={k}>{run.text}</span>;
      case 'bold':
        return <strong key={k}>{run.text}</strong>;
      case 'italic':
        return <em key={k}>{run.text}</em>;
      case 'code':
        return <code key={k}>{run.text}</code>;
      case 'link': {
        const isExternal = /^https?:/.test(run.href) || run.href.startsWith('mailto:');
        const isTopic = /^\d{2}-[a-z0-9-]+\.md$/.test(run.href);
        if (isExternal) {
          return (
            <a
              key={k}
              href={run.href}
              onClick={(e) => {
                e.preventDefault();
                void shellOpen(run.href).catch(() => { /* no-op */ });
              }}
            >
              {run.text}
            </a>
          );
        }
        if (isTopic) {
          const target = run.href.replace(/\.md$/, '');
          return (
            <a
              key={k}
              href={`#${target}`}
              onClick={(e) => {
                e.preventDefault();
                onTopicLink(target);
              }}
            >
              {run.text}
            </a>
          );
        }
        // Relative path outside the topic set (e.g. ../pitfalls/...) — open
        // in the browser pointed at the repo so the operator can read it.
        const repoBlob = `https://github.com/cameronzucker/tuxlink/blob/main/docs/${run.href.replace(/^\.\.\//, '')}`;
        return (
          <a
            key={k}
            href={repoBlob}
            onClick={(e) => {
              e.preventDefault();
              void shellOpen(repoBlob).catch(() => { /* no-op */ });
            }}
          >
            {run.text}
          </a>
        );
      }
    }
  });
}

function renderBlock(block: Block, key: string, onTopicLink: (topicId: string) => void): React.ReactNode {
  switch (block.kind) {
    case 'heading': {
      if (block.level === 1) return <h2 key={key} className="tux-help-h1">{renderInline(block.text, key, onTopicLink)}</h2>;
      if (block.level === 2) return <h3 key={key} className="tux-help-h2">{renderInline(block.text, key, onTopicLink)}</h3>;
      return <h4 key={key} className="tux-help-h3">{renderInline(block.text, key, onTopicLink)}</h4>;
    }
    case 'paragraph':
      return <p key={key} className="tux-help-p">{renderInline(block.text, key, onTopicLink)}</p>;
    case 'list':
      return (
        <ul key={key} className="tux-help-list">
          {block.items.map((item, i) => (
            <li key={`${key}-${i}`}>{renderInline(item, `${key}-${i}`, onTopicLink)}</li>
          ))}
        </ul>
      );
    case 'code':
      return (
        <pre key={key} className="tux-help-pre">
          <code>{block.text}</code>
        </pre>
      );
    case 'table':
      return (
        <table key={key} className="tux-help-table">
          <thead>
            <tr>{block.headers.map((h, i) => (
              <th key={`${key}-h-${i}`}>{renderInline(h, `${key}-h-${i}`, onTopicLink)}</th>
            ))}</tr>
          </thead>
          <tbody>
            {block.rows.map((row, ri) => (
              <tr key={`${key}-r-${ri}`}>
                {row.map((cell, ci) => (
                  <td key={`${key}-r-${ri}-c-${ci}`}>{renderInline(cell, `${key}-r-${ri}-c-${ci}`, onTopicLink)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
  }
}

export function HelpPanel({ open, onClose }: HelpPanelProps) {
  const [activeId, setActiveId] = useState<string>(FALLBACK_TOPIC?.id ?? '');

  // Esc closes (matches the rest of the chrome's overlays).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Reset to the first topic each time the panel opens. The operator's last
  // selection is intentionally not persisted — Help → Documentation should
  // land them on Getting started by default.
  useEffect(() => {
    if (open && FALLBACK_TOPIC) setActiveId(FALLBACK_TOPIC.id);
  }, [open]);

  const active = TOPICS.find((t) => t.id === activeId) ?? FALLBACK_TOPIC;
  const blocks = useMemo<Block[]>(
    () => (active ? parseMarkdown(active.source) : []),
    [active],
  );

  if (!open) return null;

  if (!active) {
    // No topics bundled (build-time misconfiguration). Render an inert
    // message rather than throw — the panel still closes cleanly.
    return (
      <div className="tux-help-backdrop" data-testid="help-backdrop" onClick={onClose}>
        <div className="tux-help-panel" onClick={(e) => e.stopPropagation()} data-testid="help-panel">
          <div className="tux-help-empty">No documentation topics bundled.</div>
        </div>
      </div>
    );
  }

  return (
    <div className="tux-help-backdrop" data-testid="help-backdrop" onClick={onClose}>
      <div
        className="tux-help-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Documentation"
        data-testid="help-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-help-header">
          <h2 className="tux-help-title">Documentation</h2>
          <button
            type="button"
            className="tux-help-close"
            data-testid="help-close"
            aria-label="Close documentation"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        <div className="tux-help-body">
          <nav className="tux-help-toc" aria-label="Documentation topics">
            <ul className="tux-help-toc-list">
              {TOPICS.map((t) => (
                <li key={t.id}>
                  <button
                    type="button"
                    className={`tux-help-toc-item${t.id === active.id ? ' active' : ''}`}
                    data-testid={`help-topic-${t.id}`}
                    onClick={() => setActiveId(t.id)}
                  >
                    {t.title}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
          <article className="tux-help-content" data-testid="help-content">
            {blocks.map((b, i) => renderBlock(b, `b-${i}`, setActiveId))}
          </article>
        </div>
      </div>
    </div>
  );
}
