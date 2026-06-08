// useStations — imperative fetch of station lists via the `catalog_fetch_stations` command.
// Unlike useCatalog (one-shot bundled file), this is a live poll the operator triggers
// ("Get stations"); the polite TTL/coalesce/stale cache lives in Rust. bd: tuxlink-a2gd.

import { useCallback, useState } from 'react';
import { fetchStations } from './useCatalog';
import { catalogErrorMessage, type ListingMode, type StationListing } from './stationTypes';

interface UseStations {
  listings: StationListing[];
  loading: boolean;
  error: string | null;
  fetch: (modes: ListingMode[], opts?: { historyHours?: number }) => void;
}

export function useStations(): UseStations {
  const [listings, setListings] = useState<StationListing[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetch = useCallback((modes: ListingMode[], opts?: { historyHours?: number }) => {
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        setListings(await fetchStations(modes, opts));
      } catch (e) {
        setError(catalogErrorMessage(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  return { listings, loading, error, fetch };
}
