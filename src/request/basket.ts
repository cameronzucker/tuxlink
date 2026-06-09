// Request basket — the dual-rail model behind the Winlink Request Center.
//
// The basket holds requests of two rails:
//   - 'cms'      — a catalog inquiry keyed by a catalog filename.
//   - 'saildocs' — a Saildocs GRIB request.
//
// "Send all" dispatches each rail independently via `dispatchBasket`:
//   - the CMS rail collapses all cms items into ONE catalog inquiry, and
//   - the saildocs rail sends ONE GRIB request per saildocs item.
//
// Partial-failure semantics: dispatch never throws. It runs both rails with
// `Promise.allSettled` and reports per-rail / per-item ok/error so the caller
// can KEEP failed items and CLEAR only the ones that succeeded.
//
// bd issue: tuxlink-eymu (B1-B2).

import { useCallback, useMemo, useState } from 'react';
import { sendCatalogInquiry } from '../catalog/useCatalog';
import { sendGribRequest } from '../grib/useGrib';
import type { GribRequest } from '../grib/types';

export type BasketItem =
  | { id: string; label: string; rail: 'cms'; filename: string }
  | { id: string; label: string; rail: 'saildocs'; request: GribRequest };

export interface RequestBasket {
  items: BasketItem[];
  /// Append an item. Dedupes by id — adding an existing id is a no-op.
  add: (item: BasketItem) => void;
  /// Remove the item with the given id (no-op if absent).
  remove: (id: string) => void;
  /// Empty the basket.
  clear: () => void;
  /// Filenames of all cms items, in insertion order.
  cmsFilenames: string[];
  /// All saildocs items, in insertion order.
  saildocsItems: Extract<BasketItem, { rail: 'saildocs' }>[];
  /// True when the basket holds no items.
  isEmpty: boolean;
}

function cmsFilenamesOf(items: BasketItem[]): string[] {
  return items.filter((i) => i.rail === 'cms').map((i) => (i as Extract<BasketItem, { rail: 'cms' }>).filename);
}

function saildocsItemsOf(items: BasketItem[]): Extract<BasketItem, { rail: 'saildocs' }>[] {
  return items.filter((i): i is Extract<BasketItem, { rail: 'saildocs' }> => i.rail === 'saildocs');
}

export function useRequestBasket(): RequestBasket {
  const [items, setItems] = useState<BasketItem[]>([]);

  const add = useCallback((item: BasketItem) => {
    setItems((prev) => (prev.some((i) => i.id === item.id) ? prev : [...prev, item]));
  }, []);

  const remove = useCallback((id: string) => {
    setItems((prev) => prev.filter((i) => i.id !== id));
  }, []);

  const clear = useCallback(() => {
    setItems([]);
  }, []);

  const cmsFilenames = useMemo(() => cmsFilenamesOf(items), [items]);
  const saildocsItems = useMemo(() => saildocsItemsOf(items), [items]);

  return {
    items,
    add,
    remove,
    clear,
    cmsFilenames,
    saildocsItems,
    isEmpty: items.length === 0,
  };
}

export interface CmsDispatchResult {
  ok: boolean;
  mid?: string;
  error?: string;
}

export interface SaildocsDispatchResult {
  item: BasketItem;
  ok: boolean;
  mid?: string;
  error?: string;
}

export interface DispatchResult {
  /// Absent when the basket held no cms items (the rail made no call).
  cms?: CmsDispatchResult;
  /// One entry per saildocs item, in basket order.
  saildocs: SaildocsDispatchResult[];
}

function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/// Dispatch a basket across both rails independently (`Promise.allSettled`).
/// Never throws — every rejection is captured into the per-rail/per-item
/// `error` field. Empty input is a safe no-op that makes no wrapper calls.
export async function dispatchBasket(items: BasketItem[]): Promise<DispatchResult> {
  const cmsFilenames = cmsFilenamesOf(items);
  const saildocsItems = saildocsItemsOf(items);

  // CMS rail: ONE inquiry for all cms filenames; absent if there are none.
  const cmsPromise: Promise<string> | null =
    cmsFilenames.length > 0 ? sendCatalogInquiry(cmsFilenames) : null;

  // Saildocs rail: ONE request per saildocs item.
  const saildocsPromises = saildocsItems.map((item) => sendGribRequest(item.request));

  const [cmsSettled, ...saildocsSettled] = await Promise.allSettled([
    cmsPromise ?? Promise.resolve<null>(null),
    ...saildocsPromises,
  ]);

  const result: DispatchResult = { saildocs: [] };

  if (cmsPromise !== null) {
    if (cmsSettled.status === 'fulfilled') {
      result.cms = { ok: true, mid: cmsSettled.value as string };
    } else {
      result.cms = { ok: false, error: errorText(cmsSettled.reason) };
    }
  }

  result.saildocs = saildocsItems.map((item, idx) => {
    const settled = saildocsSettled[idx];
    if (settled.status === 'fulfilled') {
      return { item, ok: true, mid: settled.value as string };
    }
    return { item, ok: false, error: errorText(settled.reason) };
  });

  return result;
}
