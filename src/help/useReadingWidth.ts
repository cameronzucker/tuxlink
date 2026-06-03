/**
 * Reading-column width-preset hook for the help window (tuxlink-ew3k).
 * Wikipedia-style narrow/wide toggle. Persists in localStorage.
 *
 * "Narrow" = original spec §5.3 value (720 px). Comfortable line length
 * (60–75 chars) at 18–20 px base size — kept for operators who prefer
 * tight prose columns.
 * "Wide" = 1280 px. Operator preference 2026-06-03 (tuxlink-d7a7): the
 * prior 980 px was still leaving a lot of unused horizontal real estate
 * on 1080p displays. 1280 px sits around ~120 chars at 18 px — fine for
 * technical reference docs with code blocks, tables, and short paragraphs.
 *
 * The default is "Wide" (was "Narrow") because the help docs are
 * technical reference material, not long-form prose; the wider column
 * makes better use of standard 1080p+ displays. Narrow remains one click
 * away for operators who want the tighter classic prose layout.
 *
 * The hook writes `--help-reading-max-width` on document.documentElement
 * so ReadingPane.css's .tux-help-reading-inner picks it up.
 */
import { useState, useEffect, useCallback } from 'react';

export const READING_WIDTHS = ['Narrow', 'Wide'] as const;
export type ReadingWidth = (typeof READING_WIDTHS)[number];

export const READING_WIDTH_PX: Record<ReadingWidth, string> = {
  Narrow: '720px',
  Wide: '1280px',
};

export const READING_WIDTH_STORAGE_KEY = 'tuxlink.help.readingWidth';
export const DEFAULT_READING_WIDTH: ReadingWidth = 'Wide';

function isReadingWidth(value: unknown): value is ReadingWidth {
  return typeof value === 'string' && (READING_WIDTHS as readonly string[]).includes(value);
}

function loadPersisted(): ReadingWidth {
  try {
    const raw = localStorage.getItem(READING_WIDTH_STORAGE_KEY);
    if (isReadingWidth(raw)) return raw;
  } catch {
    // localStorage may throw in private-browsing-class environments.
  }
  return DEFAULT_READING_WIDTH;
}

export function useReadingWidth() {
  const [width, setWidthState] = useState<ReadingWidth>(() => loadPersisted());

  useEffect(() => {
    document.documentElement.style.setProperty(
      '--help-reading-max-width',
      READING_WIDTH_PX[width],
    );
    try {
      localStorage.setItem(READING_WIDTH_STORAGE_KEY, width);
    } catch {
      // ignore — UI still updates, just doesn't persist
    }
  }, [width]);

  const setWidth = useCallback((w: ReadingWidth) => setWidthState(w), []);
  const toggle = useCallback(() => {
    setWidthState((cur) => (cur === 'Narrow' ? 'Wide' : 'Narrow'));
  }, []);

  return { width, setWidth, toggle };
}
