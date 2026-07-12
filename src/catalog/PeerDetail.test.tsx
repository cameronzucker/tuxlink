// PeerDetail — the peers↔L3 reconciliation's load-bearing contract:
// a peer channel/endpoint click PREFILLS the matching modem pane and NEVER dials.
//
// The dropped finder rail's connectPeerChannel/connectPeerEndpoint fired a REAL
// outbound RF dial straight from the browse list with the modem pane closed. The
// "never invokes anything" assertions below are what keep that from coming back:
// a dial would have to reach the backend through `invoke`, so an untouched invoke
// mock is direct evidence no transmit path was armed by the click.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import { PeerDetail } from './PeerDetail';
import { PEER_PREFILL_EVENT, type PeerPrefill } from '../peers/peerPrefillEvent';
import type { AggregatedPeer } from '../peers/peerModel';

/** A peer with one VARA HF channel, one packet channel carrying a relay path, an
 *  undialable 'unknown' transport, and one telnet endpoint. */
function peer(): AggregatedPeer {
  return {
    id: 'contact-1',
    callsign: 'KE7PWR',
    origin: 'operator-added',
    tier: 'good',
    grid: 'DM45bb',
    lastSeen: '2026-07-11T00:00:00Z',
    lastOk: '2026-07-11T00:00:00Z',
    channels: [
      {
        transport: 'vara-hf',
        target_callsign: 'KE7PWR',
        via: [],
        freq_hz: 14_103_000,
        bandwidth: null,
        direction: 'outgoing',
        counts: { ok: 1, fail: 0 },
        last_seen: '2026-07-11T00:00:00Z',
        last_ok: '2026-07-11T00:00:00Z',
      },
      {
        transport: 'packet',
        target_callsign: 'KE7PWR-7',
        via: ['WIDE1-1', 'WIDE2-1'],
        freq_hz: 144_390_000,
        bandwidth: null,
        direction: 'outgoing',
        counts: { ok: 0, fail: 0 },
        last_seen: '2026-07-11T00:00:00Z',
        last_ok: null,
      },
      {
        transport: 'unknown',
        target_callsign: 'KE7PWR',
        via: [],
        freq_hz: null,
        bandwidth: null,
        direction: 'unknown',
        counts: { ok: 0, fail: 0 },
        last_seen: '2026-07-11T00:00:00Z',
        last_ok: null,
      },
    ],
    endpoints: [
      {
        id: 'ep-1',
        host: '100.72.4.19',
        port: 8774,
        provenance: 'operator',
        last_seen: '2026-07-11T00:00:00Z',
        last_ok: null,
      },
    ],
  } as unknown as AggregatedPeer;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('PeerDetail — prefill, never dial', () => {
  it('an RF channel click prefills that mode with the peer target + freq — and invokes NOTHING', () => {
    const onUsePeer = vi.fn();
    render(<PeerDetail peer={peer()} onUsePeer={onUsePeer} />);

    fireEvent.click(screen.getByTestId('peer-use-vara-hf'));

    expect(onUsePeer).toHaveBeenCalledTimes(1);
    expect(onUsePeer.mock.calls[0][0]).toMatchObject({
      mode: 'vara-hf',
      target: 'KE7PWR',
      freqHz: 14_103_000,
      contactId: 'contact-1',
    });
    // The un-shippable behavior this replaces: NO backend command fired.
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('a packet channel carries its relay path (via) into the prefill', () => {
    const onUsePeer = vi.fn();
    render(<PeerDetail peer={peer()} onUsePeer={onUsePeer} />);

    fireEvent.click(screen.getByTestId('peer-use-packet'));

    expect(onUsePeer.mock.calls[0][0]).toMatchObject({
      mode: 'packet',
      target: 'KE7PWR-7',
      via: ['WIDE1-1', 'WIDE2-1'],
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('a telnet endpoint click prefills host + port, targeting the peer callsign', () => {
    const onUsePeer = vi.fn();
    render(<PeerDetail peer={peer()} onUsePeer={onUsePeer} />);

    fireEvent.click(screen.getByTestId('peer-use-telnet-ep-1'));

    expect(onUsePeer.mock.calls[0][0]).toMatchObject({
      mode: 'telnet',
      target: 'KE7PWR',
      host: '100.72.4.19',
      port: 8774,
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('an undialable transport gets no row (no modem to prefill → not rendered dead)', () => {
    render(<PeerDetail peer={peer()} onUsePeer={vi.fn()} />);
    // vara-hf + packet rows exist; the 'unknown' transport has none.
    expect(screen.getByTestId('peer-row-vara-hf')).toBeTruthy();
    expect(screen.getByTestId('peer-row-packet')).toBeTruthy();
    expect(screen.queryByTestId('peer-row-unknown')).toBeNull();
  });

  it('without onUsePeer it falls back to emitting the peer-prefill event (never a dial)', () => {
    const seen: PeerPrefill[] = [];
    const onEvent = (e: Event) => seen.push((e as CustomEvent<PeerPrefill>).detail);
    window.addEventListener(PEER_PREFILL_EVENT, onEvent);
    try {
      render(<PeerDetail peer={peer()} />);
      fireEvent.click(screen.getByTestId('peer-use-vara-hf'));
    } finally {
      window.removeEventListener(PEER_PREFILL_EVENT, onEvent);
    }

    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ mode: 'vara-hf', target: 'KE7PWR', freqHz: 14_103_000 });
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
