import type { ModemStatus } from '../modem/types';

export type LinkPhase =
  | 'idle'
  | 'connecting'
  | 'data-out'
  | 'data-in'
  | 'busy'
  | 'error'
  | 'closing';

export interface LinkDrawState {
  phase: LinkPhase;
  /** 0..1 intensity of the data comet (from throughput), 0 when not data phase. */
  flow: number;
  /** arc tint 0..1 from quality/snDb (1 = great link). */
  quality: number;
  /** true while an arc should be drawn at all (connecting..closing). */
  active: boolean;
}

const clamp01 = (n: number): number => (n < 0 ? 0 : n > 1 ? 1 : n);

export function linkDrawState(
  s: Pick<ModemStatus, 'state' | 'arqFlags' | 'throughputBps' | 'quality' | 'snDb'>,
): LinkDrawState {
  // Quality: prefer direct quality score, fall back to snDb heuristic, else default 0.6
  const q =
    s.quality != null
      ? s.quality / 100
      : s.snDb != null
        ? clamp01((s.snDb + 10) / 30)
        : 0.6;

  const flowOf = (): number => clamp01((s.throughputBps ?? 0) / 4000);

  let phase: LinkPhase = 'idle';
  let flow = 0;

  if (s.state === 'connecting') {
    phase = 'connecting';
  } else if (
    s.arqFlags.busy &&
    (s.state === 'connected-iss' || s.state === 'connected-irs')
  ) {
    // busy checked BEFORE data direction — arqFlags.busy overrides iss/irs split
    phase = 'busy';
  } else if (s.state === 'connected-iss') {
    phase = 'data-out';
    flow = flowOf();
  } else if (s.state === 'connected-irs') {
    phase = 'data-in';
    flow = flowOf();
  } else if (s.state === 'error') {
    phase = 'error';
  } else if (s.state === 'disconnecting') {
    phase = 'closing';
  }
  // stopped | spawning | initializing | idle → phase stays 'idle'

  const active = phase !== 'idle';

  return { phase, flow, quality: clamp01(q), active };
}
