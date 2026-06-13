// UvproControlStrip — the always-live UV-Pro device surface co-presented with the
// APRS conversation (tuxlink-ve3j). Rendered into AprsChatPanel's `controlStrip`
// slot ONLY on the UV-Pro native profile (capability gate in AppShell); a generic
// KISS TNC shows plain chat with no strip.
//
// Control + chat ride one native connection (the unified model): connecting here
// is what makes native APRS listening possible. Channel switching is the canonical
// "change the radio" operation — each memory carries its own frequency/mode.
import { useUvproControl } from './useUvproControl';
import './UvproControlStrip.css';

const STATE_LABEL: Record<string, string> = {
  disconnected: 'Disconnected',
  connecting: 'Connecting…',
  connected: 'Connected',
};

function freqLabel(rxMhz?: number, txMhz?: number): string | null {
  if (rxMhz == null) return null;
  const rx = rxMhz.toFixed(4);
  // Split (repeater) vs simplex: show the TX offset only when it differs.
  if (txMhz != null && Math.abs(txMhz - rxMhz) > 1e-6) {
    return `${rx} / ${txMhz.toFixed(4)} MHz`;
  }
  return `${rx} MHz`;
}

export function UvproControlStrip() {
  const { status, channels, busy, error, connect, disconnect, setChannel } =
    useUvproControl();
  const connected = status.state === 'connected';
  const connecting = status.state === 'connecting';
  const freq = freqLabel(status.rxMhz, status.txMhz);

  return (
    <section className="uvpro-strip" data-testid="uvpro-control-strip" aria-label="UV-Pro control">
      <header className="uvpro-strip-h">
        <span className="uvpro-strip-title">{status.deviceModel ?? 'UV-Pro'}</span>
        <span
          className={`uvpro-strip-state uvpro-strip-state-${status.state}`}
          data-testid="uvpro-state"
          data-state={status.state}
        >
          <span className="uvpro-strip-dot" />
          {STATE_LABEL[status.state] ?? status.state}
        </span>
        {connected && status.batteryPercent != null && (
          <span className="uvpro-strip-metric" data-testid="uvpro-battery" title="Battery">
            🔋 {status.batteryPercent}%
          </span>
        )}
        {connected && status.rssi != null && (
          <span className="uvpro-strip-metric" data-testid="uvpro-rssi" title="Signal (RSSI)">
            📶 {status.rssi}
          </span>
        )}
      </header>

      {connected ? (
        <div className="uvpro-strip-body" data-testid="uvpro-connected">
          <label className="uvpro-strip-channel">
            <span className="uvpro-strip-label">Channel</span>
            <select
              className="uvpro-strip-select"
              data-testid="uvpro-channel-select"
              value={status.currentChannelId ?? ''}
              disabled={busy || channels.length === 0}
              onChange={(e) => {
                const id = Number(e.target.value);
                if (!Number.isNaN(id)) void setChannel(id);
              }}
            >
              {channels.length === 0 && (
                <option value="" disabled>
                  No channels
                </option>
              )}
              {channels.map((ch) => (
                <option key={ch.channelId} value={ch.channelId}>
                  {ch.name?.trim() ? `${ch.channelId}: ${ch.name.trim()}` : `Ch ${ch.channelId}`}
                </option>
              ))}
            </select>
          </label>
          {freq && (
            <span className="uvpro-strip-freq" data-testid="uvpro-freq">
              {freq}
              {status.mode ? ` · ${status.mode}` : ''}
            </span>
          )}
          <button
            type="button"
            className="uvpro-strip-btn uvpro-strip-btn-ghost"
            data-testid="uvpro-disconnect"
            disabled={busy}
            onClick={() => void disconnect()}
          >
            Disconnect
          </button>
        </div>
      ) : (
        <div className="uvpro-strip-body" data-testid="uvpro-disconnected">
          {status.linkBusyHolder ? (
            <span className="uvpro-strip-busy" data-testid="uvpro-link-busy">
              Radio in use by {status.linkBusyHolder}
            </span>
          ) : (
            <span className="uvpro-strip-hint">
              Connect to control the radio and start native APRS.
            </span>
          )}
          <button
            type="button"
            className="uvpro-strip-btn"
            data-testid="uvpro-connect"
            disabled={busy || connecting || Boolean(status.linkBusyHolder)}
            onClick={() => void connect()}
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </button>
        </div>
      )}

      {error && (
        <p className="uvpro-strip-error" data-testid="uvpro-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
