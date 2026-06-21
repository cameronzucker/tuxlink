// MailboxSettings — Mailbox section of the inline Settings panel (tuxlink-wl7n).
//
// Loads trash auto-purge configuration via config_read and persists changes via
// config_set_trash_auto_purge. Follows the self-loading section pattern used by
// AprsSettings and LocationSettingsPane — no external data prop, no modal.
//
// Fields:
//   - Auto-purge toggle — enable/disable scheduled Trash purge.
//   - Retention days   — number of days before a trashed message is eligible
//                        for permanent removal. Disabled when auto-purge is off.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ConfigViewDto } from './useStatus';

export function MailboxSettings() {
  const [autoPurge, setAutoPurge] = useState<boolean>(true);
  const [retentionDays, setRetentionDays] = useState<number>(30);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load the live config once on mount.
  useEffect(() => {
    let mounted = true;
    invoke<ConfigViewDto>('config_read')
      .then((cfg) => {
        if (!mounted) return;
        setAutoPurge(cfg.trash_auto_purge);
        setRetentionDays(cfg.trash_retention_days);
        setLoaded(true);
      })
      .catch(() => {
        if (mounted) setError('Could not load mailbox settings.');
      });
    return () => {
      mounted = false;
    };
  }, []);

  async function persist(enabled: boolean, days: number) {
    setError(null);
    try {
      await invoke('config_set_trash_auto_purge', {
        enabled,
        retentionDays: days,
      });
    } catch {
      setError('Could not save mailbox settings.');
    }
  }

  function onToggle(e: React.ChangeEvent<HTMLInputElement>) {
    const next = e.target.checked;
    setAutoPurge(next);
    void persist(next, retentionDays);
  }

  function onDaysChange(e: React.ChangeEvent<HTMLInputElement>) {
    const next = Number(e.target.value);
    setRetentionDays(next);
    void persist(autoPurge, next);
  }

  return (
    <div className="mailbox-settings" data-testid="mailbox-settings">
      {error && (
        <div className="tux-settings-error" role="alert">
          {error}
        </div>
      )}

      <div className="tux-settings-formblock">
        <fieldset className="tux-settings-group">
          <legend>Trash</legend>

          <label className="tux-settings-opt">
            <input
              type="checkbox"
              data-testid="auto-purge-toggle"
              checked={loaded ? autoPurge : false}
              disabled={!loaded}
              onChange={onToggle}
            />
            <span className="tux-settings-opt-text">
              <span className="tux-settings-opt-label">Automatically empty Trash</span>
              <span className="tux-settings-opt-help">
                Deleted messages are permanently removed after the retention period below. Default 30 days.
              </span>
            </span>
          </label>

          <label className="tux-settings-field">
            <span className="tux-settings-field-label">Retention days</span>
            <input
              type="number"
              data-testid="retention-days-input"
              min={1}
              max={365}
              value={loaded ? retentionDays : ''}
              disabled={!loaded || !autoPurge}
              onChange={onDaysChange}
            />
          </label>
        </fieldset>
      </div>
    </div>
  );
}
