import { useState, useCallback } from 'react';
import { Sidebar } from './Sidebar';
import { ReadingPane } from './ReadingPane';
import { TOPICS, getTopicBySlug } from './topics';
import './HelpView.css';

const DEFAULT_SLUG = '01-getting-started';

/**
 * HelpView — root component mounted at /help in a separate Tauri webview
 * window (label "help"). Replaces the modal HelpPanel from PR #214.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4, §5.
 *
 * Task 3 of the implementation plan adds the Variant A layout (sidebar +
 * reading pane). Text-size dropdown lands in Task 4; theme inheritance
 * in Task 5; search in Tasks 6-7.
 */
export function HelpView() {
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);

  const handleSelect = useCallback((slug: string) => {
    setActiveSlug(slug);
  }, []);

  const handleNavigate = useCallback((slug: string) => {
    if (getTopicBySlug(slug)) setActiveSlug(slug);
  }, []);

  const activeTopic = getTopicBySlug(activeSlug) ?? TOPICS[0];

  return (
    <div className="tux-help-root" data-testid="tux-help-root">
      <header className="tux-help-header">
        <span className="tux-help-title">User Guide</span>
        <div className="tux-help-spacer" />
        {/* Text-size dropdown lands in Task 4. */}
      </header>
      <div className="tux-help-body">
        <Sidebar activeSlug={activeSlug} onSelect={handleSelect} />
        <ReadingPane topic={activeTopic} onNavigate={handleNavigate} />
      </div>
    </div>
  );
}
