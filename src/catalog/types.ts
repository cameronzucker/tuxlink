// Catalog DTOs mirroring src-tauri/src/catalog/parser.rs::CatalogEntry.
// Grounding doc: docs/design/2026-06-02-cms-request-protocol-grounding.md.

/// A single inquiry the operator can pick.
export interface CatalogEntry {
  /// Tree group, e.g. 'WL2K_RMS', 'PROPAGATION', 'WX_BUOY'.
  category: string;
  /// The inquiry filename — sent literally in the body of the request.
  filename: string;
  /// Operator-facing description.
  description: string;
  /// Approximate response size in bytes (informational only).
  size_bytes: number;
}

/// Tree shape for the picker: entries grouped by category.
export interface CatalogTree {
  /// Map from category name to its entries (insertion-ordered).
  categories: Map<string, CatalogEntry[]>;
  /// Total entry count across all categories.
  totalCount: number;
}

/// Build a CatalogTree from a flat entry list. Preserves the source order
/// (the bundled file is already in a sensible grouping order).
export function groupByCategory(entries: CatalogEntry[]): CatalogTree {
  const categories = new Map<string, CatalogEntry[]>();
  for (const e of entries) {
    const bucket = categories.get(e.category);
    if (bucket) {
      bucket.push(e);
    } else {
      categories.set(e.category, [e]);
    }
  }
  return { categories, totalCount: entries.length };
}
