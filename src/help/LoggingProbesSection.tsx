/**
 * LoggingProbesSection — Environment probes panel of the Logging window.
 *
 * Displays the current probe snapshot list with a one-line summary per probe
 * and a "Re-run probes" button. Subscribes to the backend's push events via
 * useEnvProbes so the list updates automatically when probes complete.
 *
 * tuxlink-qjgx alpha-logging plan Task 7.6 / spec §8.8.
 */
import { useEnvProbes, type ProbeSnapshot } from './useEnvProbes';

export function LoggingProbesSection() {
  const { snapshots, lastUpdated, rerun } = useEnvProbes();

  return (
    <section>
      <h2>Environment probes</h2>
      {lastUpdated && (
        <p style={{ fontSize: 11, color: 'var(--text-secondary, #888)', margin: '0 0 8px 0' }}>
          Last updated: {new Date(lastUpdated).toLocaleString()}
        </p>
      )}
      {snapshots.length === 0 ? (
        <p style={{ color: 'var(--text-secondary, #888)', fontSize: 13 }}>
          No probe results yet. Click &ldquo;Re-run probes&rdquo; below.
        </p>
      ) : (
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {snapshots.map((s) => (
            <li key={s.probe} style={{ padding: '4px 0', fontSize: 13 }}>
              <strong>{s.probe}:</strong>{' '}
              <code style={{ fontSize: 11, color: 'var(--text-secondary, #888)' }}>
                {summarize(s)}
              </code>
            </li>
          ))}
        </ul>
      )}
      <button onClick={rerun} style={{ marginTop: 8 }}>
        Re-run probes
      </button>
    </section>
  );
}

/**
 * Heuristic one-liner for each known probe kind.
 * Falls back to the first 3 result keys for unknown probes.
 */
function summarize(s: ProbeSnapshot): string {
  const relevantKeys: Record<string, string[]> = {
    keyring: ['secret_service_owner', 'gnome_keyring_daemon_systemd_active', 'dbus_session_bus_reachable'],
    audio: ['backend', 'sinks_count', 'digirig_detected'],
    serial: ['by_id_devices', 'in_dialout_group'],
    modem_process: ['varahf_running', 'ardopc_running'],
    network: ['dns_a_records_count', 'port_8772_reachable'],
    display: ['wayland_display', 'webkitgtk_version'],
  };

  const keys = relevantKeys[s.probe] ?? Object.keys(s.result).slice(0, 3);
  return keys
    .filter((k) => k in s.result)
    .map((k) => `${k}=${JSON.stringify(s.result[k])}`)
    .join(' · ');
}
