import { useState, useCallback, useEffect } from 'react';
import { HelpTitleBar } from './HelpTitleBar';
import { ResizeHandles } from '../shell/chrome/ResizeHandles';
import { Sidebar } from './Sidebar';
import { ReadingPane } from './ReadingPane';
import { TextSizeDropdown } from './TextSizeDropdown';
import { TOPICS, getTopicBySlug } from './topics';
import { useFontSize, stepFontSize, DEFAULT_FONT_PRESET } from './useFontSize';
import { useReadingWidth } from './useReadingWidth';
import { useHelpTheme } from './useHelpTheme';
import { useHelpSearch } from './useHelpSearch';
import './HelpView.css';

const DEFAULT_SLUG = '01-getting-started';

/**
 * HelpView — root component mounted at /help in a separate Tauri webview
 * window (label "help"). Replaces the modal HelpPanel from PR #214.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4, §5, §7, §9.
 *
 * tuxlink-ew3k polish round 1: custom titlebar (HelpTitleBar), width-preset
 * toggle (useReadingWidth), scroll reset on topic switch, parseMarkdown
 * memoization, markdown list-continuation fix, font-size CSS-scope fix.
 */
export function HelpView() {
  useHelpTheme();   // first — paint into the correct theme as early as possible
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const { preset, setPreset } = useFontSize();
  const { width: readingWidth, toggle: toggleReadingWidth } = useReadingWidth();
  const { data: hits } = useHelpSearch(searchQuery);

  const handleSelect = useCallback((slug: string) => setActiveSlug(slug), []);
  const handleNavigate = useCallback((slug: string) => {
    if (getTopicBySlug(slug)) setActiveSlug(slug);
  }, []);

  // Browser-style accelerators: Ctrl+= / Ctrl++ → up, Ctrl+- → down, Ctrl+0 → reset.
  // Skip when an input / textarea is focused so the sidebar search input
  // doesn't lose its own minus / equals keystrokes.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA';
      if (inField) return;
      if (e.key === '=' || e.key === '+') {
        e.preventDefault();
        setPreset(stepFontSize(preset, 'up'));
      } else if (e.key === '-') {
        e.preventDefault();
        setPreset(stepFontSize(preset, 'down'));
      } else if (e.key === '0') {
        e.preventDefault();
        setPreset(DEFAULT_FONT_PRESET);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preset, setPreset]);

  const activeTopic = getTopicBySlug(activeSlug) ?? TOPICS[0];

  return (
    <div className="tux-help-root" data-testid="tux-help-root">
      {/* tuxlink-ew3k: borderless GTK window has no native resize grips —
       * ResizeHandles overlays 8 invisible edge/corner zones that proxy to
       * Tauri's startResizeDragging. Position:absolute, doesn't consume a
       * grid row. */}
      <ResizeHandles />
      <HelpTitleBar />
      <header className="tux-help-header">
        <span className="tux-help-title">User Guide</span>
        <div className="tux-help-spacer" />
        <button
          type="button"
          className="tux-help-width-toggle"
          onClick={toggleReadingWidth}
          aria-label={`Switch reading width — currently ${readingWidth}`}
          title="Toggle between Narrow and Wide reading column"
        >
          <span className="lab">Width:</span>
          <span className="val">{readingWidth}</span>
        </button>
        <TextSizeDropdown value={preset} onChange={setPreset} />
      </header>
      <div className="tux-help-body">
        <Sidebar
          activeSlug={activeSlug}
          onSelect={handleSelect}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          hits={hits}
        />
        <ReadingPane topic={activeTopic} onNavigate={handleNavigate} />
      </div>
    </div>
  );
}
