// LocationSettings — the Settings-chrome wrapper around GpsSourcePicker
// (tuxlink-9xy1 slice 1). Loads the operator's current grid + position source
// from config and persists changes:
//   - picking a GPS source  → position_set_source (live arbiter switch, no restart)
//   - picking manual         → position_set_source('Manual')
//   - editing the grid       → config_set_grid (validated; pins Manual)
//
// Rendered inside the inline Settings overlay so GPS setup help is reachable
// where operators already go for GPS/privacy — the surface that was missing.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { GpsSourcePicker } from './GpsSourcePicker';
import { validateGrid } from '../wizard/validators';

interface LocationConfigView {
  grid: string | null;
  /** Serde PascalCase: 'Gps' | 'Manual'. */
  position_source: string;
}

export function LocationSettings() {
  const [grid, setGrid] = useState('');
  // Picker selection id: 'manual' | 'gpsd' | 'serial:/dev/...'. Config only
  // persists Manual vs Gps (granular device persistence is slice 3), so a
  // restored 'Gps' source shows as the gpsd card.
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

  const handleGridChange = (g: string) => {
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

  const handleSelectSource = (id: string) => {
    setSelectedSource(id);
    setError(null);
    invoke('position_set_source', { source: id === 'manual' ? 'Manual' : 'Gps' }).catch(() =>
      setError('Could not switch GPS source.'),
    );
  };

  return (
    <div className="location-settings" data-testid="location-settings">
      {error && (
        <p className="location-settings__error" role="alert" data-testid="location-settings-error">
          {error}
        </p>
      )}
      <GpsSourcePicker
        grid={grid}
        onGridChange={handleGridChange}
        selectedSource={selectedSource}
        onSelectSource={handleSelectSource}
      />
    </div>
  );
}
