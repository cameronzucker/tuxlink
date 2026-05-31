import './ChipStrip.css';
import type { FilterKey, FilterValue, QuerySpec } from './types';

export interface ChipStripProps {
  spec: QuerySpec;
  onSpecChange: (spec: QuerySpec) => void;
  metaText: string | null;
}

const ALL_KEYS: FilterKey[] = [
  'folder', 'from', 'to', 'date-range', 'form-type',
  'has-form', 'has-attach', 'read-state', 'transport',
];

function chipLabel(key: FilterKey, v: FilterValue): string {
  switch (v.kind) {
    case 'addr':       return `${key.toUpperCase()}:${v.value}`;
    case 'folder':     return `FOLDER:${v.value}`;
    case 'form-type':  return `FORM-TYPE:${v.value}`;
    case 'transport':  return `TRANSPORT:${v.value}`;
    case 'bool':       return `${key.toUpperCase()}`;
    case 'read-state': return `READ-STATE:${v.value}`;
    case 'date-range': {
      const f = v.value.from != null ? new Date(v.value.from * 1000).toISOString().slice(0, 10) : '*';
      const t = v.value.to   != null ? new Date(v.value.to   * 1000).toISOString().slice(0, 10) : '*';
      return `DATE:${f}..${t}`;
    }
  }
}

export function ChipStrip({ spec, onSpecChange, metaText }: ChipStripProps) {
  const activeKeys = Object.keys(spec.filters) as FilterKey[];
  const inactiveKeys = ALL_KEYS.filter((k) => !activeKeys.includes(k));
  const isEmpty = activeKeys.length === 0 && !(spec.free_text && spec.free_text.trim());

  const removeChip = (key: FilterKey) => {
    const filters = { ...spec.filters };
    delete filters[key];
    onSpecChange({ ...spec, filters });
  };

  return (
    <div className="chip-strip" data-testid="chip-strip">
      {isEmpty && <span className="empty-prefix" data-testid="chipstrip-empty">No active filter — click + to add</span>}
      {!isEmpty && <span className="label-prefix">Filters:</span>}
      {activeKeys.map((k) => {
        const v = spec.filters[k]!;
        return (
          <span className="chip active" key={`active-${k}`} data-testid={`chip-active-${k}`}>
            {chipLabel(k, v)}
            <button
              type="button"
              className="x"
              data-testid={`chip-x-${k}`}
              aria-label={`Remove ${k} filter`}
              onClick={() => removeChip(k)}
            >×</button>
          </span>
        );
      })}
      {inactiveKeys.map((k) => (
        <span className="chip inactive" key={`ghost-${k}`} data-testid={`chip-ghost-${k}`}>
          + {k.toUpperCase()}
        </span>
      ))}
      <span className="meta" data-testid="chipstrip-meta">{metaText ?? ''}</span>
    </div>
  );
}
