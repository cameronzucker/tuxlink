// useLocationConfig — shared grid/source state + persistence for the GpsSourcePicker,
// used by BOTH chromes (Settings → Location and the first-run wizard's Location step,
// tuxlink-9xy1). Extracting it keeps the two chromes from re-implementing the same
// config_read seed + config_set_grid / position_set_source writes.
//
// Persistence semantics (identical in both chromes):
//   - picking a GPS source → position_set_source (live arbiter switch, no restart)
//   - picking manual        → position_set_source('Manual')
//   - editing the grid      → config_set_grid (only when valid + non-empty; pins Manual)
//
// The grid write is gated on validateGrid so a mid-typing partial ("EM7") never hits
// disk or the backend; config_set_grid pins the source to Manual on the backend.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { validateGrid } from '../wizard/validators';

interface LocationConfigView {
  grid: string | null;
  /** Serde PascalCase: 'Gps' | 'Manual'. */
  position_source: string;
}

export interface UseLocationConfig {
  grid: string;
  /** Picker selection id: 'manual' | 'gpsd' | 'serial:/dev/...'. */
  selectedSource: string;
  error: string | null;
  onGridChange: (grid: string) => void;
  onSelectSource: (id: string) => void;
}

export function useLocationConfig(): UseLocationConfig {
  const [grid, setGrid] = useState('');
  // Config only persists Manual vs Gps (granular device persistence is slice 3),
  // so a restored 'Gps' source shows as the gpsd card.
  const [selectedSource, setSelectedSource] = useState('manual');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    invoke<LocationConfigView>('config_read')
      .then((c) => {
        if (!mounted) return;
        setGrid(c.grid ?? '');
        setSelectedSource(c.position_source === 'Gps' ? 'gpsd' : 'manual');
      })
      .catch(() => {
        if (mounted) setError('Could not load location settings.');
      });
    return () => {
      mounted = false;
    };
  }, []);

  const onGridChange = (g: string) => {
    setGrid(g);
    setError(null);
    // Persist only a valid, non-empty grid — avoids a disk write (and a backend
    // rejection) on every mid-typing keystroke. config_set_grid pins Manual.
    if (g && validateGrid(g) === null) {
      invoke('config_set_grid', { grid: g })
        .then(() => setSelectedSource('manual'))
        .catch(() => setError('Could not save your grid.'));
    }
  };

  const onSelectSource = (id: string) => {
    setSelectedSource(id);
    setError(null);
    invoke('position_set_source', { source: id === 'manual' ? 'Manual' : 'Gps' }).catch(() =>
      setError('Could not switch GPS source.'),
    );
  };

  return { grid, selectedSource, error, onGridChange, onSelectSource };
}
