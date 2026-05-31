import type { FilterKey, FilterValue, QuerySpec } from './types';

const KEY_ORDER: FilterKey[] = [
  'folder', 'from', 'to', 'date-range', 'form-type',
  'has-form', 'has-attach', 'read-state', 'transport',
];

export function renderQuery(spec: QuerySpec): string {
  const parts: string[] = [];
  if (spec.free_text && spec.free_text.trim()) parts.push(spec.free_text.trim());
  for (const key of KEY_ORDER) {
    const v = spec.filters[key];
    if (!v) continue;
    parts.push(renderChip(key, v));
  }
  return parts.length === 0 ? '(empty)' : parts.join(' ');
}

function renderChip(key: FilterKey, v: FilterValue): string {
  switch (v.kind) {
    case 'addr':       return `${key}:${v.value}`;
    case 'folder':     return `folder:${v.value}`;
    case 'form-type':  return `form:${v.value}`;
    case 'transport':  return `transport:${v.value}`;
    case 'bool':       return `${key}:${v.value}`;
    case 'read-state': return `read-state:${v.value}`;
    case 'date-range': {
      const f = v.value.from != null ? `from-${v.value.from}` : '';
      const t = v.value.to != null ? `to-${v.value.to}` : '';
      return `date:${[f, t].filter(Boolean).join('..')}`;
    }
  }
}
