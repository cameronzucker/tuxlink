import { SECTIONS, getTopicBySlug } from './topics';
import './Sidebar.css';

interface SidebarProps {
  activeSlug: string;
  onSelect: (slug: string) => void;
}

export function Sidebar({ activeSlug, onSelect }: SidebarProps) {
  return (
    <nav className="tux-help-sidebar" aria-label="Help topics">
      {SECTIONS.map((sec) => (
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
      ))}
    </nav>
  );
}
