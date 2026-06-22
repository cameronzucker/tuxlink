import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
// ModemLinkSection loads device lists via invoke; mock it so jsdom doesn't crash.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
import { AprsConnectStrip } from './AprsConnectStrip';
import type { AprsConnectStripProps } from './AprsConnectStrip';

function renderStrip(over: Partial<AprsConnectStripProps> = {}) {
  const onConnect = over.onConnect ?? vi.fn().mockResolvedValue(undefined);
  const onDisconnect = over.onDisconnect ?? vi.fn().mockResolvedValue(undefined);
  const onLinkChange = over.onLinkChange ?? vi.fn();
  const props: AprsConnectStripProps = {
    listening: false,
    linkKind: 'Tcp',
    radioLabel: '127.0.0.1:8001',
    allowUvproNative: true,
    onConnect,
    onDisconnect,
    onLinkChange,
    ...over,
  };
  return { ...render(<AprsConnectStrip {...props} />), onConnect, onDisconnect, onLinkChange };
}

describe('AprsConnectStrip', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the compact always-visible strip with radio label + state', () => {
    renderStrip();
    expect(screen.getByTestId('aprs-connect-strip')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-connect-strip')).toHaveTextContent('127.0.0.1:8001');
    expect(screen.getByTestId('aprs-connect-state')).toHaveTextContent(/not listening/i);
  });

  it('shows "no link" when no link is configured', () => {
    renderStrip({ linkKind: null, radioLabel: null });
    expect(screen.getByTestId('aprs-connect-strip')).toHaveTextContent(/no link/i);
  });

  it('shows the listening state when listening is true', () => {
    renderStrip({ listening: true });
    expect(screen.getByTestId('aprs-connect-state')).toHaveTextContent(/^listening$/i);
  });

  it('renders Connect when not listening, Disconnect when listening', () => {
    const { rerender } = renderStrip();
    expect(screen.getByTestId('aprs-connect-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('aprs-disconnect-btn')).not.toBeInTheDocument();
    rerender(
      <AprsConnectStrip
        listening={true}
        linkKind="Tcp"
        radioLabel="127.0.0.1:8001"
        onConnect={vi.fn()}
        onDisconnect={vi.fn()}
        onLinkChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId('aprs-disconnect-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('aprs-connect-btn')).not.toBeInTheDocument();
  });

  it('calls onConnect and shows connecting… while the promise is in flight', async () => {
    let resolve!: () => void;
    const onConnect = vi.fn(() => new Promise<void>((r) => (resolve = r)));
    renderStrip({ onConnect });
    fireEvent.click(screen.getByTestId('aprs-connect-btn'));
    expect(onConnect).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(screen.getByTestId('aprs-connect-state')).toHaveTextContent(/connecting/i),
    );
    resolve();
    await waitFor(() =>
      expect(screen.getByTestId('aprs-connect-state')).not.toHaveTextContent(/connecting/i),
    );
  });

  it('surfaces a connect error inline and does NOT flip to listening', async () => {
    const onConnect = vi.fn().mockRejectedValue(new Error('backend offline'));
    renderStrip({ onConnect });
    fireEvent.click(screen.getByTestId('aprs-connect-btn'));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(/backend offline/i);
    // Still showing Connect (listening prop never flipped — backend is truth).
    expect(screen.getByTestId('aprs-connect-btn')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-connect-state')).toHaveTextContent(/not listening/i);
  });

  it('calls onDisconnect when listening and Disconnect is clicked', async () => {
    const onDisconnect = vi.fn().mockResolvedValue(undefined);
    renderStrip({ listening: true, onDisconnect });
    fireEvent.click(screen.getByTestId('aprs-disconnect-btn'));
    await waitFor(() => expect(onDisconnect).toHaveBeenCalledTimes(1));
  });

  it('auto-expands the setup picker when no link is configured', () => {
    renderStrip({ linkKind: null, radioLabel: null });
    expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
  });

  it('hides the setup picker by default when a link IS configured, toggled by the caret', () => {
    renderStrip({ linkKind: 'Tcp', radioLabel: '127.0.0.1:8001' });
    expect(screen.queryByTestId('modem-link-section')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('aprs-connect-setup-toggle'));
    expect(screen.getByTestId('modem-link-section')).toBeInTheDocument();
  });

  it('locks link edits while listening: the setup toggle is disabled and the picker is hidden', () => {
    // Codex adrev P1: changing the transport/radio under a live listener would
    // leave the engine on the old link / orphan a UV-Pro session. Disconnect first.
    renderStrip({ listening: true, linkKind: 'UvproNative', radioLabel: 'UV-Pro AA:BB' });
    expect(screen.getByTestId('aprs-connect-setup-toggle')).toBeDisabled();
    fireEvent.click(screen.getByTestId('aprs-connect-setup-toggle'));
    expect(screen.queryByTestId('modem-link-section')).not.toBeInTheDocument();
  });

  it('does not persist an incomplete (null-MAC) link on a bare BT switch (tuxlink-614x)', () => {
    const onLinkChange = vi.fn();
    renderStrip({ linkKind: null, radioLabel: null, onLinkChange });
    // No paired radio yet: switching to BT reveals the picker locally but must NOT
    // persist a null-MAC link. The old behavior emitted Bluetooth+null → the backend
    // rejected it and the rollback snapped the segment back, so BT was unselectable
    // (tuxlink-614x). The real persist happens when a radio is chosen in the picker.
    fireEvent.click(screen.getByTestId('modem-seg-bt'));
    expect(screen.getByTestId('modem-seg-bt')).toHaveAttribute('aria-pressed', 'true');
    expect(onLinkChange).not.toHaveBeenCalled();
  });

  it('seeds the picker so a UV-Pro segment tap preserves the saved MAC (does not blank it)', () => {
    // tuxlink-hoi1 B2: the strip mounted ModemLinkSection with NO address props,
    // so opening setup on a configured UV-Pro and tapping the segment emitted
    // btMac: null — corrupting the live link state (which a later persist makes
    // permanent via B1). The persisted MAC must flow through to the picker.
    const onLinkChange = vi.fn();
    renderStrip({
      linkKind: 'UvproNative',
      radioLabel: 'UV-Pro AA:BB:CC:DD:EE:FF',
      btMac: 'AA:BB:CC:DD:EE:FF',
      onLinkChange,
    });
    // A configured link hides the picker; open it via the ⚙ caret.
    fireEvent.click(screen.getByTestId('aprs-connect-setup-toggle'));
    // Tap the (already-active) UV-Pro segment — this fires emit().
    fireEvent.click(screen.getByTestId('modem-seg-uvpro'));
    expect(onLinkChange).toHaveBeenCalledWith(
      expect.objectContaining({ linkKind: 'UvproNative', btMac: 'AA:BB:CC:DD:EE:FF' }),
    );
  });

  // tuxlink-28o0: a connect started OUTSIDE the strip (e.g. the status-bar
  // control) must show "Connecting…" here too — not a stale "Connect" — until the
  // backend `listening` event lands. `externalConnecting` drives the shared
  // in-flight state even though this strip's own button was never clicked.
  it('shows Connecting… when a connect is in flight from another surface (externalConnecting)', () => {
    renderStrip({ listening: false, externalConnecting: true });
    expect(screen.getByTestId('aprs-connect-state')).toHaveTextContent(/connecting/i);
    expect(screen.getByTestId('aprs-connect-state')).toHaveAttribute('data-state', 'connecting');
    const btn = screen.getByTestId('aprs-connect-btn');
    expect(btn).toHaveTextContent(/connecting/i);
    expect(btn).toBeDisabled();
  });

  it('does not start a second connect while an external connect is in flight', () => {
    const onConnect = vi.fn().mockResolvedValue(undefined);
    renderStrip({ listening: false, externalConnecting: true, onConnect });
    fireEvent.click(screen.getByTestId('aprs-connect-btn'));
    expect(onConnect).not.toHaveBeenCalled();
  });
});
