// WindowSettings — Window section of the inline Settings panel (tuxlink-5rvp /
// #882).
//
// Loads the close-to-tray preference via config_read and persists changes via
// set_close_to_tray. Follows the self-loading section pattern used by
// MailboxSettings / AprsSettings — no external data prop, no modal.
//
// This is the change-it-later path for the one-time close-behavior prompt
// (CloseBehaviorPrompt). Unlike the prompt's resolve_close_prompt, this does NOT
// minimize/quit — it only updates the persisted preference the backend's
// CloseRequested handler reads on the next window close.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ConfigViewDto } from './useStatus';

export function WindowSettings() {
  const [closeToTray, setCloseToTray] = useState<boolean>(true);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load the live config once on mount.
  useEffect(() => {
    let mounted = true;
    invoke<ConfigViewDto>('config_read')
      .then((cfg) => {
        if (!mounted) return;
        setCloseToTray(cfg.close_to_tray);
        setLoaded(true);
      })
      .catch(() => {
        if (mounted) setError('Could not load window settings.');
      });
    return () => {
      mounted = false;
    };
  }, []);

  async function persist(value: boolean) {
    setError(null);
    try {
      await invoke('set_close_to_tray', { value });
    } catch {
      setError('Could not save window settings.');
    }
  }

  function onToggle(e: React.ChangeEvent<HTMLInputElement>) {
    const next = e.target.checked;
    setCloseToTray(next);
    void persist(next);
  }

  return (
    <div className="window-settings" data-testid="window-settings">
      {error && (
        <div className="tux-settings-error" role="alert">
          {error}
        </div>
      )}

      <div className="tux-settings-formblock">
        <fieldset className="tux-settings-group">
          <legend>Closing the window</legend>

          <label className="tux-settings-opt">
            <input
              type="checkbox"
              data-testid="close-to-tray-toggle"
              checked={loaded ? closeToTray : false}
              disabled={!loaded}
              onChange={onToggle}
            />
            <span className="tux-settings-opt-text">
              <span className="tux-settings-opt-label">
                Keep running when the window is closed
              </span>
              <span className="tux-settings-opt-help">
                Closing the window minimizes Tuxlink to the tray instead of
                quitting, so an active transfer is not interrupted. Turn this off
                to quit on close. Quit any time from File → Quit or Ctrl+Q.
              </span>
            </span>
          </label>
        </fieldset>
      </div>
    </div>
  );
}
