/**
 * LoggingExportSection — Overview / Export panel of the Logging window.
 *
 * Displays current disk usage, retained window, event rate, and last export
 * metadata from logging_status. Provides three actions:
 *   • Export logs… — Save As dialog → logging_export Tauri command
 *   • Open log directory — logging_open_directory Tauri command
 *   • Clear history… — confirmation + logging_clear_history Tauri command
 *
 * tuxlink-qjgx alpha-logging plan Task 7.4 / spec §8.2.
 */
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { useLoggingStatus } from './useLoggingStatus';

export function LoggingExportSection() {
  const { data: status, refetch } = useLoggingStatus();
  const [busy, setBusy] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const onExport = async () => {
    setBusy('exporting');
    setFeedback(null);
    const ts = new Date().toISOString().replace(/[:.]/g, '-');
    const attempt = status?.last_export?.correlation_id || `boot-${status?.boot_id_short || 'unknown'}`;
    const defaultName = `tuxlink-logs-${ts}-${attempt}.tar.zst`;
    const filePath = await saveDialog({
      defaultPath: defaultName,
      filters: [{ name: 'Tuxlink Log Archive', extensions: ['tar.zst'] }],
    });
    if (!filePath) {
      setBusy(null);
      setFeedback('Export canceled.');
      return;
    }
    try {
      const result = await invoke<{ archive_size_bytes: number; correlation_id: string | null }>(
        'logging_export',
        { outputPath: filePath },
      );
      setFeedback(`Saved ${formatBytes(result.archive_size_bytes)} to ${filePath}`);
      await refetch();
    } catch (e) {
      setFeedback(`Export failed: ${e}`);
    } finally {
      setBusy(null);
    }
  };

  const onOpenDir = async () => {
    try {
      await invoke('logging_open_directory');
    } catch (e) {
      setFeedback(`Open failed: ${e}`);
    }
  };

  const onClear = async () => {
    if (!confirm('Clear all retained logs? This cannot be undone.')) return;
    try {
      await invoke('logging_clear_history');
      setFeedback('History cleared.');
      await refetch();
    } catch (e) {
      setFeedback(`Clear failed: ${e}`);
    }
  };

  return (
    <section>
      <h2>Export</h2>
      {status && (
        <table style={{ width: '100%', fontSize: 13, lineHeight: 1.7 }}>
          <tbody>
            <tr>
              <td style={{ width: 160, color: 'var(--text-secondary, #888)' }}>Disk usage</td>
              <td>{formatBytes(status.disk_usage_bytes)} / {formatBytes(status.disk_cap_bytes)}</td>
            </tr>
            <tr>
              <td style={{ color: 'var(--text-secondary, #888)' }}>Retained window</td>
              <td>{formatDuration(status.retained_window_seconds)}</td>
            </tr>
            <tr>
              <td style={{ color: 'var(--text-secondary, #888)' }}>Event rate (24h)</td>
              <td>~{status.event_rate_per_hour}/hour</td>
            </tr>
            <tr>
              <td style={{ color: 'var(--text-secondary, #888)' }}>Last export</td>
              <td>
                {status.last_export
                  ? `${status.last_export.at} · ${formatBytes(status.last_export.size_bytes)}`
                  : '(none)'}
              </td>
            </tr>
          </tbody>
        </table>
      )}
      {status?.degraded && (
        <p style={{
          marginTop: 8,
          padding: '8px 12px',
          background: '#3a1a1a',
          border: '1px solid #6a2a2a',
          color: '#e89a9a',
          fontSize: 13,
          borderRadius: 2,
        }}>
          ⚠ Logging degraded: {status.degraded}
        </p>
      )}
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button onClick={onExport} disabled={!!busy}>
          {busy === 'exporting' ? 'Exporting…' : 'Export logs…'}
        </button>
        <button onClick={onOpenDir} disabled={!!busy}>Open log directory</button>
        <button
          onClick={onClear}
          disabled={!!busy}
          style={{ marginLeft: 'auto', color: '#c97370' }}
        >
          Clear history…
        </button>
      </div>
      {feedback && (
        <p style={{ marginTop: 8, fontSize: 12, color: '#7aa2f7' }} role="status">
          {feedback}
        </p>
      )}
    </section>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let n = bytes / 1024;
  for (const u of units) {
    if (n < 1024) return `${n.toFixed(1)} ${u}`;
    n /= 1024;
  }
  return `${n.toFixed(1)} PB`;
}

function formatDuration(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  return `${d}d ${h}h`;
}
