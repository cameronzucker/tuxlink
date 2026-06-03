import { Fragment } from 'react';
import { SECTIONS, getTopicBySlug } from './topics';
import type { DocsHit } from './useHelpSearch';
import './Sidebar.css';

/**
 * Render an FTS5 snippet without `dangerouslySetInnerHTML`. The snippet
 * contains exactly the markers we passed to FTS5's `snippet()` call
 * (`<mark>` and `</mark>`); split on them and emit a React `<mark>` element
 * for the highlighted runs. Any other angle-bracket text from the source
 * markdown is rendered as plain text by React rather than parsed as HTML —
 * so a future doc containing literal HTML cannot trigger XSS.
 */
function renderSnippet(snippet: string): React.ReactNode {
  const parts = snippet.split(/(<mark>|<\/mark>)/g);
  let inMark = false;
  return parts.map((p, i) => {
    if (p === '<mark>') { inMark = true; return null; }
    if (p === '</mark>') { inMark = false; return null; }
    if (p === '') return null;
    return inMark ? <mark key={i}>{p}</mark> : <Fragment key={i}>{p}</Fragment>;
  });
}

interface SidebarProps {
  activeSlug: string;
  onSelect: (slug: string) => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  hits: DocsHit[] | undefined;
}

export function Sidebar({
  activeSlug,
  onSelect,
  searchQuery,
  onSearchChange,
  hits,
}: SidebarProps) {
  const showHits = searchQuery.trim().length > 0;

  return (
    <nav className="tux-help-sidebar" aria-label="Help topics">
      <div className="tux-help-sb-search">
        <input
          type="search"
          className="tux-help-sb-search-input"
          placeholder="Search topics…"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          aria-label="Search topics"
        />
        {searchQuery.length > 0 && (
          <button
            type="button"
            className="tux-help-sb-search-clear"
            aria-label="Clear search"
            onClick={() => onSearchChange('')}
          >
            ×
          </button>
        )}
      </div>

      {showHits ? (
        <div className="tux-help-sb-hits">
          {!hits ? (
            <div className="tux-help-sb-status">Searching…</div>
          ) : hits.length === 0 ? (
            <div className="tux-help-sb-status">No matches.</div>
          ) : (
            hits.map((hit) => {
              const isActive = hit.slug === activeSlug;
              return (
                <a
                  key={hit.slug}
                  role="link"
                  aria-current={isActive ? 'page' : undefined}
                  className={`tux-help-sb-hit${isActive ? ' active' : ''}`}
                  href={`#${hit.slug}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onSelect(hit.slug);
                  }}
                >
                  <div className="tux-help-sb-hit-title">{hit.title}</div>
                  <div className="tux-help-sb-hit-snippet">
                    {renderSnippet(hit.snippet)}
                  </div>
                </a>
              );
            })
          )}
        </div>
      ) : (
        SECTIONS.map((sec) => (
          <div key={sec.id} className="tux-help-sb-section">
            <div className="tux-help-sb-section-title">{sec.displayName}</div>
            {sec.topicSlugs.map((slug) => {
              const t = getTopicBySlug(slug);
              if (!t) return null;
              const isActive = slug === activeSlug;
              return (
                <a
                  key={slug}
                  role="link"
                  aria-current={isActive ? 'page' : undefined}
                  className={`tux-help-sb-item${isActive ? ' active' : ''}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onSelect(slug);
                  }}
                  href={`#${slug}`}
                  tabIndex={0}
                >
                  <span className="tux-help-sb-num">{t.number}</span>
                  <span className="tux-help-sb-name">{t.displayName}</span>
                </a>
              );
            })}
          </div>
        ))
      )}
    </nav>
  );
}
