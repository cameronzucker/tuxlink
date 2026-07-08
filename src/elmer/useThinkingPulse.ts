import { useEffect, useState } from 'react';
import { RADIO_VERBS } from './radioVerbs';

export interface ThinkingPulse {
  /** Current ham-radio verb phrase (rotates ~every 3s while active). */
  verb: string;
  /** Seconds since this active window began. */
  elapsedSecs: number;
}

function randomVerb(exclude?: string): string {
  const pool = exclude ? RADIO_VERBS.filter((v) => v !== exclude) : RADIO_VERBS;
  return pool[Math.floor(Math.random() * pool.length)];
}

/**
 * Verb + elapsed ticker for the in-flight indicator. Runs a 1s interval only
 * while `active`; resets elapsed and picks a fresh verb each time it goes
 * active. Extracted from the old ThinkingIndicator so the presentational card
 * stays free of timers (and both are independently testable).
 */
export function useThinkingPulse(active: boolean): ThinkingPulse {
  const [verb, setVerb] = useState<string>(() => RADIO_VERBS[0]);
  const [elapsedSecs, setElapsedSecs] = useState(0);

  useEffect(() => {
    if (!active) return undefined;
    setElapsedSecs(0);
    let lastVerb = randomVerb();
    setVerb(lastVerb);
    let ticks = 0;
    const id = setInterval(() => {
      ticks += 1;
      setElapsedSecs((s) => s + 1);
      if (ticks % 3 === 0) {
        lastVerb = randomVerb(lastVerb);
        setVerb(lastVerb);
      }
    }, 1000);
    return () => clearInterval(id);
  }, [active]);

  return { verb, elapsedSecs };
}
