/// Mirrors src-tauri/src/search/types.rs — kebab-case serde tags on FilterValue,
/// snake_case SortOrder, etc. When the Rust shape changes, this file MUST be
/// updated in the same PR.

export type FilterKey =
  | 'folder'
  | 'from'
  | 'to'
  | 'date-range'
  | 'form-type'
  | 'has-form'
  | 'has-attach'
  | 'read-state'
  | 'transport';

export type ReadState = 'read' | 'unread';

export type FilterValue =
  | { kind: 'folder'; value: string }
  | { kind: 'addr'; value: string }
  | { kind: 'date-range'; value: { from: number | null; to: number | null } }
  | { kind: 'form-type'; value: string }
  | { kind: 'bool'; value: boolean }
  | { kind: 'read-state'; value: ReadState }
  | { kind: 'transport'; value: string };

export type SortOrder = 'date_desc' | 'date_asc';

export interface PageRequest {
  page_size: number;
  offset: number;
}

export interface QuerySpec {
  free_text: string | null;
  filters: Partial<Record<FilterKey, FilterValue>>;
  sort: SortOrder;
  page: PageRequest;
}

export const EMPTY_SPEC: QuerySpec = {
  free_text: null,
  filters: {},
  sort: 'date_desc',
  page: { page_size: 200, offset: 0 },
};

export interface MessageMetaDto {
  id: string;
  subject: string;
  from: string;
  to: string[];
  date: string;           // RFC3339
  unread: boolean;
  bodySize: number;
  hasAttachments: boolean;
  formTag?: string;
  folder: string;
}

export interface SearchResults {
  items: MessageMetaDto[];
  totalMatches: number;
  queryMs: number;
  effectiveSpec: QuerySpec;
}

export interface SavedSearch {
  id: string;
  name: string;
  spec: QuerySpec;
  created_at: number;
  last_used_at: number | null;
  order: number;
}

export interface RecentSearch {
  spec: QuerySpec;
  ran_at: number;
}

export interface RebuildStats {
  messagesIndexed: number;
  elapsedMs: number;
}
