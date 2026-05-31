/// Inline filter-operator parser, Gmail-style. Operators recognized:
///   from:ADDR        → filters.from = addr
///   to:ADDR          → filters.to = addr
///   form:ID          → filters.form-type = form-type
///   folder:NAME      → filters.folder
///   transport:ID     → filters.transport
///   has:attach       → filters.has-attach = true (alias: has:attachment, has:yes)
///   has-form:yes|no  → filters.has-form
///   is:unread|read   → filters.read-state (also: read-state:unread)
///   date:Nd          → filters.date-range = last N days (also date:today)
///
/// Unrecognized `key:value` tokens fall through to free_text as-is.
/// Bare tokens become free_text.

import { EMPTY_SPEC, type FilterKey, type FilterValue, type QuerySpec, type ReadState } from './types';

const KEY_ALIASES: Record<string, FilterKey> = {
  from: 'from',
  to: 'to',
  folder: 'folder',
  form: 'form-type',
  'form-type': 'form-type',
  transport: 'transport',
  has: 'has-attach',
  'has-form': 'has-form',
  'has-attach': 'has-attach',
  is: 'read-state',
  read: 'read-state',
  'read-state': 'read-state',
  date: 'date-range',
};

export function parseQuery(input: string): QuerySpec {
  const tokens = input.trim().split(/\s+/).filter(Boolean);
  const filters: Partial<Record<FilterKey, FilterValue>> = {};
  const freeText: string[] = [];

  for (const tok of tokens) {
    const colon = tok.indexOf(':');
    if (colon <= 0 || colon === tok.length - 1) {
      freeText.push(tok);
      continue;
    }

    const rawKey = tok.slice(0, colon).toLowerCase();
    const rawVal = tok.slice(colon + 1);
    const key = KEY_ALIASES[rawKey];

    if (!key) {
      freeText.push(tok);
      continue;
    }

    const fv = buildFilterValue(key, rawKey, rawVal);
    if (fv) filters[key] = fv;
    else freeText.push(tok);
  }

  return {
    ...EMPTY_SPEC,
    free_text: freeText.length ? freeText.join(' ') : null,
    filters,
  };
}

function buildFilterValue(key: FilterKey, rawKey: string, rawVal: string): FilterValue | null {
  switch (key) {
    case 'from':
    case 'to':
      return { kind: 'addr', value: rawVal };
    case 'folder':
      return { kind: 'folder', value: rawVal.toLowerCase() };
    case 'form-type':
      return { kind: 'form-type', value: rawVal };
    case 'transport':
      return { kind: 'transport', value: rawVal.toLowerCase() };
    case 'has-attach': {
      if (rawKey === 'has') {
        const v = rawVal.toLowerCase();
        // `has:attach` / `has:attachment` / `has:yes`
        return v === 'attach' || v === 'attachment' || v === 'yes' || v === 'true'
          ? { kind: 'bool', value: true }
          : null;
      }
      return { kind: 'bool', value: parseBool(rawVal) };
    }
    case 'has-form':
      return { kind: 'bool', value: parseBool(rawVal) };
    case 'read-state': {
      const v = rawVal.toLowerCase();
      if (v === 'read' || v === 'unread') return { kind: 'read-state', value: v as ReadState };
      return null;
    }
    case 'date-range': {
      const range = parseDateRange(rawVal);
      return range ? { kind: 'date-range', value: range } : null;
    }
  }
}

function parseBool(s: string): boolean {
  const v = s.toLowerCase();
  return v === 'true' || v === 'yes' || v === '1';
}

function parseDateRange(s: string): { from: number | null; to: number | null } | null {
  const now = Math.floor(Date.now() / 1000);
  const daySec = 86_400;
  const m = s.match(/^(\d+)d$/);
  if (m) return { from: now - parseInt(m[1], 10) * daySec, to: null };
  if (s.toLowerCase() === 'today') return { from: now - daySec, to: null };
  return null;
}

/// Reverse: render a QuerySpec back into a typeable string. Used when an
/// operator-typed query is committed (e.g. unsaving a saved search) and we
/// need to repopulate the input with the equivalent operator string. Best-
/// effort — date ranges round-trip as `date:Nd` only if they parse cleanly;
/// otherwise the spec.filters survive but the deparsed string drops them.
export function deparseQuery(spec: QuerySpec): string {
  const parts: string[] = [];
  if (spec.free_text) parts.push(spec.free_text);
  for (const [key, val] of Object.entries(spec.filters)) {
    if (!val) continue;
    switch (val.kind) {
      case 'addr':
        parts.push(`${key}:${val.value}`);
        break;
      case 'folder':
        parts.push(`folder:${val.value}`);
        break;
      case 'form-type':
        parts.push(`form:${val.value}`);
        break;
      case 'transport':
        parts.push(`transport:${val.value}`);
        break;
      case 'bool':
        if (key === 'has-attach' && val.value) parts.push('has:attach');
        else parts.push(`${key}:${val.value ? 'yes' : 'no'}`);
        break;
      case 'read-state':
        parts.push(`is:${val.value}`);
        break;
      case 'date-range': {
        const { from, to } = val.value;
        if (from != null && to == null) {
          const days = Math.round((Math.floor(Date.now() / 1000) - from) / 86_400);
          if (days > 0) parts.push(`date:${days}d`);
        }
        break;
      }
    }
  }
  return parts.join(' ');
}
