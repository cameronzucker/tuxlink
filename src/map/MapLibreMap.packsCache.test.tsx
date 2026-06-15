/**
 * B2 (tuxlink-vnk7): the module-level pack cache lets a REMOUNT construct the map
 * with installed packs already known, so the async `basemap_list_packs` fetch is
 * a no-op instead of a full post-load `setStyle` teardown/rebuild.
 *
 * Isolated in its own file because it mocks `@tauri-apps/api/core` to RESOLVE
 * packs — the main MapLibreMap.test.tsx relies on `invoke` rejecting (→ no packs)
 * for its "no setStyle on cold open" assertions, so the two mock regimes must not
 * share a file. The maplibre mock comes from the global test-setup.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import { getLastMap, resetMapLibreMock } from './testMapLibreMock';
import { MapLibreMap } from './MapLibreMap';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({ packs: [{ id: 'continent-na' }] })),
}));

describe('MapLibreMap pack cache (B2)', () => {
  it('a remount constructs WITH cached packs and does not setStyle on cold open', async () => {
    // First mount: cache starts empty → construct overview-only → after the async
    // fetch resolves, ONE setStyle composites the pack (and populates the cache).
    const first = render(<MapLibreMap />);
    await act(async () => {
      getLastMap()!.__emit('load');
      await Promise.resolve();
    });
    await waitFor(() => expect(getLastMap()!.setStyle).toHaveBeenCalled());
    first.unmount();
    resetMapLibreMock();

    // Second mount: the cache now holds the pack, so construction carries it.
    render(<MapLibreMap />);
    const map = getLastMap()!;
    const style = map.__state.options.style as { sources: Record<string, unknown> };
    expect(Object.keys(style.sources)).toContain('pack-continent-na');

    await act(async () => {
      map.__emit('load');
      await Promise.resolve();
    });
    // The async fetch resolves to the SAME packs the constructor used → no rebuild.
    expect(map.setStyle).not.toHaveBeenCalled();
  });
});
