/**
 * DetectedModelCombobox — a filter-as-you-type, height-capped scrollable picker for
 * a detected model list (tuxlink-qhe8n).
 *
 * Replaces the native `<select>` whose option popup ran off the bottom of the
 * screen under WebKitGTK when a provider returned a long list (OpenRouter ~300
 * models), leaving lower models unreachable. The listbox constrains its own height
 * and scrolls (`.elmer-combobox-list`), so it never exceeds the viewport, and the
 * filter input makes a large list navigable by typing.
 */
import { useMemo, useState, useCallback, type KeyboardEvent } from 'react';

export interface DetectedModelComboboxProps {
  /** The detected model ids (may be hundreds for a cloud aggregator). */
  models: string[];
  /** The currently-selected model id (marked in the list). */
  value: string;
  /** Called with the chosen model id on click / Enter. */
  onSelect: (model: string) => void;
  /** Stable test id: the listbox gets `testId`, the filter input `${testId}-filter`,
   *  the empty state `${testId}-empty`. Preserves the callers' existing hooks. */
  testId: string;
  /** Optional id for the filter input (label association). */
  id?: string;
}

export function DetectedModelCombobox({
  models,
  value,
  onSelect,
  testId,
  id,
}: DetectedModelComboboxProps) {
  const [query, setQuery] = useState('');
  // -1 = nothing highlighted; Enter then falls back to the first match.
  const [activeIndex, setActiveIndex] = useState(-1);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? models.filter((m) => m.toLowerCase().includes(q)) : models;
  }, [models, query]);

  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const idx = activeIndex >= 0 ? activeIndex : 0;
        const model = filtered[idx];
        if (model) onSelect(model);
      } else if (e.key === 'Escape') {
        setQuery('');
        setActiveIndex(-1);
      }
    },
    [filtered, activeIndex, onSelect],
  );

  const listId = `${testId}-listbox`;

  return (
    <div className="elmer-combobox">
      <input
        id={id}
        className="elmer-form-input elmer-combobox-filter"
        data-testid={`${testId}-filter`}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        autoComplete="off"
        placeholder={value ? value : 'Filter models…'}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActiveIndex(-1);
        }}
        onKeyDown={onKeyDown}
      />
      <ul
        className="elmer-combobox-list"
        data-testid={testId}
        id={listId}
        role="listbox"
      >
        {filtered.length === 0 ? (
          <li className="elmer-combobox-empty" data-testid={`${testId}-empty`}>
            No models match “{query}”.
          </li>
        ) : (
          filtered.map((m, i) => (
            <li
              key={m}
              role="option"
              aria-selected={m === value}
              className={
                'elmer-combobox-option' +
                (i === activeIndex ? ' is-active' : '') +
                (m === value ? ' is-selected' : '')
              }
              onMouseEnter={() => setActiveIndex(i)}
              onClick={() => onSelect(m)}
            >
              {m}
            </li>
          ))
        )}
      </ul>
    </div>
  );
}
