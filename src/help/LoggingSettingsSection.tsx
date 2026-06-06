/**
 * LoggingSettingsSection — Settings panel of the Logging window.
 *
 * Two sub-controls:
 *   1. Detailed mode — Off / On / Bounded-for-N-hours radio group.
 *      Invokes logging_set_detailed_mode on change.
 *   2. Retention — Days + MB/GB size-cap inputs with Apply button.
 *      Invokes logging_set_retention on apply.
 *
 * Reads current state from useLoggingStatus; syncs local inputs when status
 * first loads. Validates before invoking (hours 1–720, days 1–365,
 * cap 50 MB – 10 GB). Shows inline feedback line on success or error.
 *
 * tuxlink-qjgx alpha-logging plan Task 7.5 / spec §8.2.
 */
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLoggingStatus } from './useLoggingStatus';

export function LoggingSettingsSection() {
  const { data: status, refetch } = useLoggingStatus();
  const [boundedHours, setBoundedHours] = useState<string>('4');
  const [retentionDays, setRetentionDays] = useState<string>('14');
  const [retentionAmount, setRetentionAmount] = useState<string>('500');
  const [retentionUnit, setRetentionUnit] = useState<'MB' | 'GB'>('MB');
  const [feedback, setFeedback] = useState<string | null>(null);

  // Sync local inputs from status when data first arrives.
  useEffect(() => {
    if (!status) return;
    setRetentionDays(String(status.retention_days));
    // Display cap in MB; if ≥1024 and divisible, show as GB.
    const mbCap = status.retention_mb_cap;
    if (mbCap >= 1024 && mbCap % 1024 === 0) {
      setRetentionAmount(String(mbCap / 1024));
      setRetentionUnit('GB');
    } else {
      setRetentionAmount(String(mbCap));
      setRetentionUnit('MB');
    }
  }, [status?.retention_days, status?.retention_mb_cap]); // eslint-disable-line react-hooks/exhaustive-deps

  const setMode = async (mode: 'off' | 'on' | 'bounded') => {
    if (mode === 'bounded') {
      const h = parseInt(boundedHours, 10);
      if (Number.isNaN(h) || h < 1 || h > 720) {
        setFeedback(`Bounded hours must be 1–720 (got "${boundedHours}").`);
        return;
      }
      try {
        await invoke('logging_set_detailed_mode', { mode: 'bounded', boundedHours: h });
        setFeedback(`Detailed mode set: Bounded ${h}h`);
        await refetch();
      } catch (e) {
        setFeedback(`Set failed: ${e}`);
      }
    } else {
      try {
        await invoke('logging_set_detailed_mode', { mode });
        setFeedback(`Detailed mode set: ${mode}`);
        await refetch();
      } catch (e) {
        setFeedback(`Set failed: ${e}`);
      }
    }
  };

  const applyRetention = async () => {
    const days = parseInt(retentionDays, 10);
    const amt = parseInt(retentionAmount, 10);
    if (Number.isNaN(days) || days < 1 || days > 365) {
      setFeedback(`Days must be 1–365 (got "${retentionDays}").`);
      return;
    }
    if (Number.isNaN(amt) || amt <= 0) {
      setFeedback(`Size must be positive (got "${retentionAmount}").`);
      return;
    }
    const mbCap = retentionUnit === 'GB' ? amt * 1024 : amt;
    if (mbCap < 50 || mbCap > 10240) {
      setFeedback(`Size cap must be 50 MB – 10 GB (got ${mbCap} MB).`);
      return;
    }
    try {
      await invoke('logging_set_retention', { days, mbCap });
      setFeedback(`Retention set: ${days}d / ${mbCap} MB`);
      await refetch();
    } catch (e) {
      setFeedback(`Set failed: ${e}`);
    }
  };

  return (
    <section>
      <h2>Settings</h2>

      {/* Detailed mode */}
      <div>
        <p style={{ fontSize: 12, color: 'var(--text-secondary, #888)', margin: '0 0 6px 0' }}>
          Detailed mode
        </p>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 6 }}>
            <input
              type="radio"
              name="detailed_mode"
              aria-label="Detailed mode off"
              checked={status?.detailed_mode === 'off' || !status}
              onChange={() => setMode('off')}
            />
            Off
          </label>
          <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 6 }}>
            <input
              type="radio"
              name="detailed_mode"
              aria-label="Detailed mode on"
              checked={status?.detailed_mode === 'on'}
              onChange={() => setMode('on')}
            />
            On (until disabled)
          </label>
          <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 6 }}>
            <input
              type="radio"
              name="detailed_mode"
              aria-label="Detailed mode bounded"
              checked={status?.detailed_mode === 'bounded'}
              onChange={() => setMode('bounded')}
            />
            Bounded for
            <input
              value={boundedHours}
              onChange={(e) => setBoundedHours(e.target.value)}
              aria-label="Bounded hours"
              style={{ width: 50, marginLeft: 4, marginRight: 4 }}
            />
            hours
          </label>
        </div>
        {status?.detailed_mode === 'bounded' && status.bounded_remaining_seconds != null && (
          <p style={{ fontSize: 12, color: 'var(--text-secondary, #888)', margin: '4px 0 0 0' }}>
            {Math.floor(status.bounded_remaining_seconds / 3600)}h{' '}
            {Math.floor((status.bounded_remaining_seconds % 3600) / 60)}m remaining
          </p>
        )}
      </div>

      {/* Retention */}
      <div style={{ marginTop: 14 }}>
        <p style={{ fontSize: 12, color: 'var(--text-secondary, #888)', margin: '0 0 6px 0' }}>
          Retention
        </p>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
          <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 4 }}>
            Days:
            <input
              value={retentionDays}
              onChange={(e) => setRetentionDays(e.target.value)}
              aria-label="Retention days"
              style={{ width: 60, marginLeft: 4 }}
            />
          </label>
          <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 4 }}>
            Size:
            <input
              value={retentionAmount}
              onChange={(e) => setRetentionAmount(e.target.value)}
              aria-label="Retention size"
              style={{ width: 80, marginLeft: 4 }}
            />
            <select
              value={retentionUnit}
              onChange={(e) => setRetentionUnit(e.target.value as 'MB' | 'GB')}
              aria-label="Retention unit"
              style={{ marginLeft: 2 }}
            >
              <option value="MB">MB</option>
              <option value="GB">GB</option>
            </select>
          </label>
          <button onClick={applyRetention}>Apply</button>
        </div>
      </div>

      {feedback && (
        <p style={{ marginTop: 8, fontSize: 12, color: '#7aa2f7' }} role="status">
          {feedback}
        </p>
      )}
    </section>
  );
}
