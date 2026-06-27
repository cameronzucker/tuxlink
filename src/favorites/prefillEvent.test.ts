import { describe, it, expect, vi } from 'vitest';
import { emitGatewayPrefill, listenGatewayPrefill } from './prefillEvent';
import type { FavoriteDial } from './types';

const dial = (mode: FavoriteDial['mode']): FavoriteDial => ({ mode, gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });

describe('prefillEvent', () => {
  it('delivers a live emit to an already-subscribed listener of the same mode', () => {
    const cb = vi.fn();
    const off = listenGatewayPrefill('vara-hf', cb);
    emitGatewayPrefill(dial('vara-hf'));
    // tuxlink-8fkkk Task B: the callback now receives (dial, candidates); a bare
    // emit passes no candidates.
    expect(cb).toHaveBeenCalledWith(dial('vara-hf'), undefined);
    off();
  });

  it('retains the prefill so a listener that subscribes AFTER the emit still gets it (arm-on-demand)', () => {
    // This is the tuxlink-s0r1 case: Use → opens a modem panel that was not
    // mounted yet, so the live event fires before the panel subscribes.
    emitGatewayPrefill(dial('ardop-hf'));
    const cb = vi.fn();
    const off = listenGatewayPrefill('ardop-hf', cb);
    expect(cb).toHaveBeenCalledWith(dial('ardop-hf'), undefined);
    off();
  });

  it('carries an ordered candidate list through a live emit (tuxlink-8fkkk Task B)', () => {
    const cb = vi.fn();
    const off = listenGatewayPrefill('ardop-hf', cb);
    const candidates: FavoriteDial[] = [
      { mode: 'ardop-hf', gateway: 'W7DG', freq: '7.103' },
      { mode: 'ardop-hf', gateway: 'W7DG', freq: '14.105' },
    ];
    emitGatewayPrefill(dial('ardop-hf'), candidates);
    expect(cb).toHaveBeenCalledWith(dial('ardop-hf'), candidates);
    off();
  });

  it('does not deliver a retained prefill to a listener of a different mode', () => {
    emitGatewayPrefill(dial('packet'));
    const cb = vi.fn();
    const off = listenGatewayPrefill('vara-hf', cb);
    expect(cb).not.toHaveBeenCalled();
    off();
  });

  it('consumes the retained prefill once — a second same-mode listener does not re-fire', () => {
    emitGatewayPrefill(dial('vara-fm'));
    const first = vi.fn();
    const off1 = listenGatewayPrefill('vara-fm', first);
    expect(first).toHaveBeenCalledTimes(1);
    const second = vi.fn();
    const off2 = listenGatewayPrefill('vara-fm', second);
    expect(second).not.toHaveBeenCalled();
    off1();
    off2();
  });
});
