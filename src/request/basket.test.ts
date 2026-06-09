import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { GribRequest } from '../grib/types';
import { DEFAULT_GRIB_REQUEST } from '../grib/types';

// Mock the two rail wrappers so dispatch tests can assert the exact
// underlying calls (count + args) without going through invoke().
vi.mock('../catalog/useCatalog', () => ({
  sendCatalogInquiry: vi.fn(),
}));
vi.mock('../grib/useGrib', () => ({
  sendGribRequest: vi.fn(),
}));

import { sendCatalogInquiry } from '../catalog/useCatalog';
import { sendGribRequest } from '../grib/useGrib';
import { useRequestBasket, dispatchBasket, type BasketItem } from './basket';

const grib = (subject: string): GribRequest => ({ ...DEFAULT_GRIB_REQUEST, subject });

const cmsItem = (id: string, filename: string): BasketItem => ({
  id,
  label: filename,
  rail: 'cms',
  filename,
});

const saildocsItem = (id: string, subject: string): BasketItem => ({
  id,
  label: subject,
  rail: 'saildocs',
  request: grib(subject),
});

beforeEach(() => {
  vi.mocked(sendCatalogInquiry).mockReset();
  vi.mocked(sendGribRequest).mockReset();
});

describe('useRequestBasket', () => {
  it('starts empty', () => {
    const { result } = renderHook(() => useRequestBasket());
    expect(result.current.items).toHaveLength(0);
    expect(result.current.isEmpty).toBe(true);
    expect(result.current.cmsFilenames).toEqual([]);
    expect(result.current.saildocsItems).toEqual([]);
  });

  it('adds two cms + one saildocs; items length 3, cmsFilenames in insertion order', () => {
    const { result } = renderHook(() => useRequestBasket());
    act(() => {
      result.current.add(cmsItem('a', 'WL2K_AREA.txt'));
      result.current.add(cmsItem('b', 'PROPAGATION.txt'));
      result.current.add(saildocsItem('g', 'GRIB pacific'));
    });
    expect(result.current.items).toHaveLength(3);
    expect(result.current.isEmpty).toBe(false);
    expect(result.current.cmsFilenames).toEqual(['WL2K_AREA.txt', 'PROPAGATION.txt']);
    expect(result.current.saildocsItems).toHaveLength(1);
    expect(result.current.saildocsItems[0].id).toBe('g');
  });

  it('dedupes by id — adding an existing id is a no-op', () => {
    const { result } = renderHook(() => useRequestBasket());
    act(() => {
      result.current.add(cmsItem('a', 'WL2K_AREA.txt'));
      result.current.add(cmsItem('a', 'OTHER.txt'));
    });
    expect(result.current.items).toHaveLength(1);
    expect(result.current.cmsFilenames).toEqual(['WL2K_AREA.txt']);
  });

  it('remove(id) drops the item', () => {
    const { result } = renderHook(() => useRequestBasket());
    act(() => {
      result.current.add(cmsItem('a', 'WL2K_AREA.txt'));
      result.current.add(cmsItem('b', 'PROPAGATION.txt'));
    });
    act(() => {
      result.current.remove('a');
    });
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0].id).toBe('b');
  });

  it('clear() empties the basket', () => {
    const { result } = renderHook(() => useRequestBasket());
    act(() => {
      result.current.add(cmsItem('a', 'WL2K_AREA.txt'));
      result.current.add(saildocsItem('g', 'GRIB'));
    });
    act(() => {
      result.current.clear();
    });
    expect(result.current.items).toHaveLength(0);
    expect(result.current.isEmpty).toBe(true);
  });
});

describe('dispatchBasket', () => {
  it('dual-rail: one sendCatalogInquiry with both filenames in order; one sendGribRequest per saildocs item', async () => {
    vi.mocked(sendCatalogInquiry).mockResolvedValue('CMS-MID-1');
    vi.mocked(sendGribRequest).mockResolvedValue('GRIB-MID-1');

    const items: BasketItem[] = [
      cmsItem('a', 'WL2K_AREA.txt'),
      cmsItem('b', 'PROPAGATION.txt'),
      saildocsItem('g', 'GRIB pacific'),
    ];
    const result = await dispatchBasket(items);

    expect(sendCatalogInquiry).toHaveBeenCalledTimes(1);
    expect(sendCatalogInquiry).toHaveBeenCalledWith(['WL2K_AREA.txt', 'PROPAGATION.txt']);
    expect(sendGribRequest).toHaveBeenCalledTimes(1);
    expect(sendGribRequest).toHaveBeenCalledWith(items[2].rail === 'saildocs' ? items[2].request : undefined);

    expect(result.cms?.ok).toBe(true);
    expect(result.cms?.mid).toBe('CMS-MID-1');
    expect(result.saildocs).toHaveLength(1);
    expect(result.saildocs[0].ok).toBe(true);
    expect(result.saildocs[0].mid).toBe('GRIB-MID-1');
    expect(result.saildocs[0].item.id).toBe('g');
  });

  it('cms-only: sendGribRequest is NOT called', async () => {
    vi.mocked(sendCatalogInquiry).mockResolvedValue('CMS-MID-1');
    const result = await dispatchBasket([cmsItem('a', 'WL2K_AREA.txt')]);

    expect(sendCatalogInquiry).toHaveBeenCalledTimes(1);
    expect(sendGribRequest).not.toHaveBeenCalled();
    expect(result.cms?.ok).toBe(true);
    expect(result.saildocs).toEqual([]);
  });

  it('saildocs-only: sendCatalogInquiry is NOT called and cms rail absent', async () => {
    vi.mocked(sendGribRequest).mockResolvedValue('GRIB-MID-1');
    const result = await dispatchBasket([saildocsItem('g', 'GRIB')]);

    expect(sendCatalogInquiry).not.toHaveBeenCalled();
    expect(sendGribRequest).toHaveBeenCalledTimes(1);
    expect(result.cms).toBeUndefined();
    expect(result.saildocs).toHaveLength(1);
    expect(result.saildocs[0].ok).toBe(true);
  });

  it('partial failure: cms resolves, saildocs rejects — cms ok, saildocs error captured, no throw', async () => {
    vi.mocked(sendCatalogInquiry).mockResolvedValue('CMS-MID-1');
    vi.mocked(sendGribRequest).mockRejectedValue(new Error('backend offline'));

    const result = await dispatchBasket([
      cmsItem('a', 'WL2K_AREA.txt'),
      saildocsItem('g', 'GRIB'),
    ]);

    expect(result.cms?.ok).toBe(true);
    expect(result.saildocs[0].ok).toBe(false);
    expect(result.saildocs[0].error).toContain('backend offline');
  });

  it('both rails fail: both ok:false with captured errors, no throw', async () => {
    vi.mocked(sendCatalogInquiry).mockRejectedValue(new Error('cms down'));
    vi.mocked(sendGribRequest).mockRejectedValue('saildocs string error');

    const result = await dispatchBasket([
      cmsItem('a', 'WL2K_AREA.txt'),
      saildocsItem('g', 'GRIB'),
    ]);

    expect(result.cms?.ok).toBe(false);
    expect(result.cms?.error).toContain('cms down');
    expect(result.saildocs[0].ok).toBe(false);
    expect(result.saildocs[0].error).toContain('saildocs string error');
  });

  it('empty basket: no wrapper calls, empty result', async () => {
    const result = await dispatchBasket([]);
    expect(sendCatalogInquiry).not.toHaveBeenCalled();
    expect(sendGribRequest).not.toHaveBeenCalled();
    expect(result.cms).toBeUndefined();
    expect(result.saildocs).toEqual([]);
  });
});
