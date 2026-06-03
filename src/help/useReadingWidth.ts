/**
 * Reading-column width-preset hook for the help window (tuxlink-ew3k).
 * Wikipedia-style narrow/wide toggle. Persists in localStorage.
 *
 * "Narrow" = original spec §5.3 default (720 px). Comfortable line length
 * (60–75 chars) at 18–20 px base size.
 * "Wide" = 980 px. Makes better use of big screens; line length sits
 * around 90 chars at 18 px which is still reasonable for short reference
 * docs and technical content with code blocks.
 *
 * The hook writes `--help-reading-max-width` on document.documentElement
 * so ReadingPane.css's .tux-help-reading-inner picks it up.
 */
import { useState, useEffect, useCallback } from 'react';

export const READING_WIDTHS = ['Narrow', 'Wide'] as const;
export type ReadingWidth = (typeof READING_WIDTHS)[number];

export const READING_WIDTH_PX: Record<ReadingWidth, string> = {
  Narrow: '720px',
  Wide: '980px',
};

export const READING_WIDTH_STORAGE_KEY = 'tuxlink.help.readingWidth';
export const DEFAULT_READING_WIDTH: ReadingWidth = 'Narrow';

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
