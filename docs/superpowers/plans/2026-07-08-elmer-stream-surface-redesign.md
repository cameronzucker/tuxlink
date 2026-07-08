# Elmer Streaming-Surface Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Elmer pane's transient streaming bubble + standalone thinking indicator with one `StreamingStatusCard` that fixes the render glitches (tuxlink-h5azu), bounds the stream box and stops the scroll-lock (tuxlink-06v9s), and collapses to a live token counter by default (tuxlink-d5zns).

**Architecture:** Three small new frontend units — `useStreamAutoFollow` (pin-to-bottom scroll hook), `useThinkingPulse` (verb + elapsed ticker), and `StreamingStatusCard` (presentational) — plus a `radioVerbs` module. `ElmerPane.tsx` deletes the old `StreamingBubble` and `ThinkingIndicator` and renders the single card off one `isInFlight` predicate. Pure frontend render/layout change; the Rust backend, `useElmer` hook state shape, and the `elmerEvents` contract are untouched.

**Tech Stack:** React 18 + TypeScript, Vitest + @testing-library/react (jsdom), Tauri (WebKitGTK) at runtime.

## Global Constraints

- **No backend / hook / event-contract changes.** Do NOT edit `src-tauri/**`, `src/elmer/useElmer.ts`, or `src/elmer/elmerEvents.ts`. This is frontend render/layout only.
- **Token counter is an estimate:** `Math.round((streamingAnswer.length + streamingReasoning.length) / 4)`, rendered as `~N tok` with the leading `~`. Show it only when `> 0`.
- **Bounded stream box:** expanded body `max-height` ≈ 210px (~10 lines), `overflow-y: auto`.
- **Collapsed by default; expand is sticky for the session** (component-level `useState`, no persistence to disk).
- **Verb:** while awaiting / reasoning, cycle the ham-radio verbs (preserved feature); once answer tokens stream (`isResponding`), show a stable `responding`.
- **Scroll rule:** auto-follow fires only when the container is pinned to the bottom (within 24px). Applies independently to the transcript and the expanded stream body.
- **TDD:** write the failing test first for each new unit. **Commit after each task** with the trailer `Agent: basin-juniper-fjord` and `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **Worktree git ops:** the persistent shell cwd must be the worktree (`worktrees/tuxlink-h5azu`) BEFORE any `git` write, or the main-checkout hook denies it. `cd` in its own call first.
- **Local gates:** `pnpm exec vitest run src/elmer/<file>`; `pnpm typecheck`; `pnpm lint:css`. CI runs the full eslint + vitest.

---

## File Structure

- **Create** `src/elmer/radioVerbs.ts` — the `RADIO_VERBS` pool (moved out of ElmerPane so the pulse hook and pane share it without a cycle).
- **Create** `src/elmer/useStreamAutoFollow.ts` — pin-to-bottom scroll hook for any scroll container.
- **Create** `src/elmer/useStreamAutoFollow.test.ts` — hook unit tests.
- **Create** `src/elmer/useThinkingPulse.ts` — verb-cycling + elapsed-seconds ticker, gated on an `active` flag.
- **Create** `src/elmer/useThinkingPulse.test.ts` — hook unit tests (fake timers).
- **Create** `src/elmer/StreamingStatusCard.tsx` — the presentational card (collapsed row + bounded expandable body).
- **Create** `src/elmer/StreamingStatusCard.test.tsx` — component tests.
- **Modify** `src/elmer/ElmerPane.tsx` — delete `StreamingBubble` + `ThinkingIndicator`, import + render the card, wire transcript auto-follow, compute derived values, sticky expand.
- **Modify** `src/elmer/ElmerPane.css` — card / bounded-body / jump-to-live / cursor styles; remove obsolete `.elmer-streaming-*` and `.elmer-thinking*` rules.
- **Modify** `src/elmer/ElmerPane.test.tsx` — migrate the thinking-indicator + phase-2b streaming test blocks to the new surface; add integration regression tests.

---

## Task 0: Worktree setup

**Files:** none (environment only).

- [ ] **Step 1: Make the worktree the persistent cwd**

Run: `cd /home/administrator/Code/tuxlink/worktrees/tuxlink-h5azu && pwd`
Expected: prints the worktree path.

- [ ] **Step 2: Install deps (fresh worktree has no node_modules)**

Run: `pnpm install --prefer-offline`
Expected: completes; `node_modules/` now present.

- [ ] **Step 3: Baseline the existing Elmer suite (green before changes)**

Run: `pnpm exec vitest run src/elmer/ElmerPane.test.tsx`
Expected: PASS (this is the pre-change baseline).

---

## Task 1: `radioVerbs` module (extract, no behavior change)

**Files:**
- Create: `src/elmer/radioVerbs.ts`
- Modify: `src/elmer/ElmerPane.tsx` (remove the inline `RADIO_VERBS`, import from the new module, keep a re-export for back-compat)
- Modify: `src/elmer/ElmerPane.test.tsx:20` (import `RADIO_VERBS` from `./radioVerbs`)

**Interfaces:**
- Produces: `export const RADIO_VERBS: readonly string[]` from `./radioVerbs`.

- [ ] **Step 1: Create the module**

`src/elmer/radioVerbs.ts`:
```ts
/** Ham-radio verb phrases cycled while Elmer is awaiting/reasoning (pre-answer). */
export const RADIO_VERBS: readonly string[] = [
  'tuning the bands',
  'listening on frequency',
  'working the pileup',
  'spinning the VFO',
  'chasing DX',
  'checking propagation',
  'reading the waterfall',
  'copying your signal',
  'pulling it out of the noise',
  'netting in',
  'keying up',
  'warming up the tubes',
  'checking the SWR',
  'rolling the dial',
  'squelching the static',
  'working simplex',
  'consulting the band plan',
  'peaking the signal',
  'calling CQ',
  'logging the contact',
];
```

- [ ] **Step 2: Replace the inline constant in ElmerPane.tsx**

In `src/elmer/ElmerPane.tsx`, delete the `RADIO_VERBS` block (currently the `export const RADIO_VERBS = [ ... ];` under the `ThinkingIndicator constants` header) and add, near the other `./` imports at the top of the file:
```ts
import { RADIO_VERBS } from './radioVerbs';
```
Then, to preserve the existing public export, add just below the imports:
```ts
export { RADIO_VERBS } from './radioVerbs';
```

- [ ] **Step 3: Point the test import at the new module**

In `src/elmer/ElmerPane.test.tsx` line 20, change:
```ts
import { ElmerPane, ModelForm, RADIO_VERBS } from './ElmerPane';
```
to:
```ts
import { ElmerPane, ModelForm } from './ElmerPane';
import { RADIO_VERBS } from './radioVerbs';
```

- [ ] **Step 4: Typecheck + run the suite**

Run: `pnpm typecheck && pnpm exec vitest run src/elmer/ElmerPane.test.tsx`
Expected: PASS (pure extraction, no behavior change).

- [ ] **Step 5: Commit**

```bash
git add src/elmer/radioVerbs.ts src/elmer/ElmerPane.tsx src/elmer/ElmerPane.test.tsx
git commit -F - <<'EOF'
refactor(elmer): extract RADIO_VERBS into radioVerbs module

Prep for the streaming-surface redesign so the pulse hook and the pane
share the verb pool without a circular import. No behavior change.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 2: `useStreamAutoFollow` hook

**Files:**
- Create: `src/elmer/useStreamAutoFollow.ts`
- Test: `src/elmer/useStreamAutoFollow.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export interface StreamAutoFollow {
    onScroll: () => void;
    atBottom: boolean;
    followIfPinned: () => void;
    jumpToLive: () => void;
  }
  export function useStreamAutoFollow(ref: RefObject<HTMLElement | null>): StreamAutoFollow
  ```
- Consumed by: Task 3 (`StreamingStatusCard` body) and Task 5 (`ElmerPane` transcript).

- [ ] **Step 1: Write the failing test**

`src/elmer/useStreamAutoFollow.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRef } from 'react';
import { useStreamAutoFollow } from './useStreamAutoFollow';

// jsdom does no layout, so scroll geometry must be defined by hand.
function makeScrollEl(scrollHeight: number, clientHeight: number, scrollTop: number): HTMLElement {
  const el = document.createElement('div');
  Object.defineProperty(el, 'scrollHeight', { value: scrollHeight, configurable: true });
  Object.defineProperty(el, 'clientHeight', { value: clientHeight, configurable: true });
  let top = scrollTop;
  Object.defineProperty(el, 'scrollTop', {
    get: () => top,
    set: (v: number) => { top = v; },
    configurable: true,
  });
  return el;
}

function useHarness(el: HTMLElement) {
  const ref = useRef<HTMLElement | null>(el);
  return useStreamAutoFollow(ref);
}

describe('useStreamAutoFollow', () => {
  it('starts pinned to the bottom', () => {
    const el = makeScrollEl(1000, 200, 800); // 1000-800-200 = 0 <= 24
    const { result } = renderHook(() => useHarness(el));
    expect(result.current.atBottom).toBe(true);
  });

  it('releases the pin once the user scrolls up', () => {
    const el = makeScrollEl(1000, 200, 800);
    const { result } = renderHook(() => useHarness(el));
    el.scrollTop = 100; // 1000-100-200 = 700 > 24
    act(() => { result.current.onScroll(); });
    expect(result.current.atBottom).toBe(false);
  });

  it('followIfPinned scrolls to bottom only when pinned', () => {
    const el = makeScrollEl(1000, 200, 100); // scrolled up
    const { result } = renderHook(() => useHarness(el));
    act(() => { result.current.onScroll(); }); // register scrolled-up state
    expect(result.current.atBottom).toBe(false);
    act(() => { result.current.followIfPinned(); });
    expect(el.scrollTop).toBe(100); // NOT moved — pin released

    el.scrollTop = 800;
    act(() => { result.current.onScroll(); }); // back at bottom
    el.scrollTop = 700; // pretend new content arrived above the fold
    act(() => { result.current.followIfPinned(); });
    expect(el.scrollTop).toBe(1000); // snapped to scrollHeight
  });

  it('jumpToLive snaps to bottom and re-pins', () => {
    const el = makeScrollEl(1000, 200, 100);
    const { result } = renderHook(() => useHarness(el));
    act(() => { result.current.onScroll(); });
    expect(result.current.atBottom).toBe(false);
    act(() => { result.current.jumpToLive(); });
    expect(el.scrollTop).toBe(1000);
    expect(result.current.atBottom).toBe(true);
  });
});
```

- [ ] **Step 2: Run it — expect failure**

Run: `pnpm exec vitest run src/elmer/useStreamAutoFollow.test.ts`
Expected: FAIL — `useStreamAutoFollow` is not defined / module not found.

- [ ] **Step 3: Implement the hook**

`src/elmer/useStreamAutoFollow.ts`:
```ts
import { useCallback, useRef, useState } from 'react';
import type { RefObject } from 'react';

/** A container counts as "at the bottom" within this many pixels of the end. */
const BOTTOM_THRESHOLD_PX = 24;

export interface StreamAutoFollow {
  /** Wire to the scroll container's `onScroll`. Updates the pinned state. */
  onScroll: () => void;
  /** True while the user is pinned to (near) the bottom. */
  atBottom: boolean;
  /** Scroll to the bottom ONLY if currently pinned (call after content grows). */
  followIfPinned: () => void;
  /** Force-scroll to the bottom and re-pin (the "Jump to live" action). */
  jumpToLive: () => void;
}

/**
 * Pin-to-bottom auto-follow for a scroll container. Auto-scroll only happens
 * when the user is already at the bottom; scrolling up releases the pin so the
 * viewport is never yanked away mid-read (tuxlink-06v9s).
 */
export function useStreamAutoFollow(ref: RefObject<HTMLElement | null>): StreamAutoFollow {
  const [atBottom, setAtBottom] = useState(true);
  // Mirror in a ref so followIfPinned reads the latest value without being
  // re-created on every pin change (it is called from a content-change effect).
  const atBottomRef = useRef(true);

  const computeAtBottom = useCallback((): boolean => {
    const el = ref.current;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_THRESHOLD_PX;
  }, [ref]);

  const onScroll = useCallback(() => {
    const now = computeAtBottom();
    atBottomRef.current = now;
    setAtBottom(now);
  }, [computeAtBottom]);

  const scrollToBottom = useCallback(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [ref]);

  const followIfPinned = useCallback(() => {
    if (atBottomRef.current) scrollToBottom();
  }, [scrollToBottom]);

  const jumpToLive = useCallback(() => {
    scrollToBottom();
    atBottomRef.current = true;
    setAtBottom(true);
  }, [scrollToBottom]);

  return { onScroll, atBottom, followIfPinned, jumpToLive };
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `pnpm exec vitest run src/elmer/useStreamAutoFollow.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/elmer/useStreamAutoFollow.ts src/elmer/useStreamAutoFollow.test.ts
git commit -F - <<'EOF'
feat(elmer): useStreamAutoFollow pin-to-bottom scroll hook (06v9s)

Auto-scroll fires only when the container is pinned to the bottom; scrolling
up releases the pin so the viewport is never yanked mid-read. Shared by the
transcript and the expandable stream box.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 3: `useThinkingPulse` hook

**Files:**
- Create: `src/elmer/useThinkingPulse.ts`
- Test: `src/elmer/useThinkingPulse.test.ts`

**Interfaces:**
- Consumes: `RADIO_VERBS` from `./radioVerbs` (Task 1).
- Produces:
  ```ts
  export interface ThinkingPulse { verb: string; elapsedSecs: number; }
  export function useThinkingPulse(active: boolean): ThinkingPulse
  ```

- [ ] **Step 1: Write the failing test**

`src/elmer/useThinkingPulse.test.ts`:
```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useThinkingPulse } from './useThinkingPulse';
import { RADIO_VERBS } from './radioVerbs';

describe('useThinkingPulse', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('is inert while inactive (elapsed stays 0)', () => {
    const { result } = renderHook(() => useThinkingPulse(false));
    act(() => { vi.advanceTimersByTime(5000); });
    expect(result.current.elapsedSecs).toBe(0);
  });

  it('starts from a RADIO_VERBS phrase and ticks elapsed once per second', () => {
    const { result } = renderHook(() => useThinkingPulse(true));
    expect(RADIO_VERBS).toContain(result.current.verb);
    act(() => { vi.advanceTimersByTime(3000); });
    expect(result.current.elapsedSecs).toBe(3);
  });

  it('rotates to a different RADIO_VERBS phrase after ~3s', () => {
    const { result } = renderHook(() => useThinkingPulse(true));
    const before = result.current.verb;
    act(() => { vi.advanceTimersByTime(3000); });
    const after = result.current.verb;
    expect(RADIO_VERBS).toContain(after);
    expect(after).not.toBe(before);
  });

  it('resets elapsed to 0 when re-activated', () => {
    const { result, rerender } = renderHook(({ active }) => useThinkingPulse(active), {
      initialProps: { active: true },
    });
    act(() => { vi.advanceTimersByTime(4000); });
    expect(result.current.elapsedSecs).toBe(4);
    rerender({ active: false });
    rerender({ active: true });
    expect(result.current.elapsedSecs).toBe(0);
  });
});
```

- [ ] **Step 2: Run it — expect failure**

Run: `pnpm exec vitest run src/elmer/useThinkingPulse.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the hook**

`src/elmer/useThinkingPulse.ts`:
```ts
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
```

- [ ] **Step 4: Run tests — expect pass**

Run: `pnpm exec vitest run src/elmer/useThinkingPulse.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/elmer/useThinkingPulse.ts src/elmer/useThinkingPulse.test.ts
git commit -F - <<'EOF'
feat(elmer): useThinkingPulse verb + elapsed ticker

Extracts the ham-radio verb rotation and elapsed timer from ThinkingIndicator
into a gated hook, so the new streaming card stays presentational.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 4: `StreamingStatusCard` component

**Files:**
- Create: `src/elmer/StreamingStatusCard.tsx`
- Test: `src/elmer/StreamingStatusCard.test.tsx`

**Interfaces:**
- Consumes: `useStreamAutoFollow` (Task 2).
- Produces:
  ```ts
  export interface StreamingStatusCardProps {
    verb: string;
    isResponding: boolean;
    answer: string;
    reasoning: string;
    tokensEstimate: number;
    elapsedSecs: number;
    expanded: boolean;
    onToggleExpand: () => void;
  }
  export function StreamingStatusCard(props: StreamingStatusCardProps): JSX.Element
  ```
- testids: `elmer-stream-card`, `elmer-stream-card-toggle`, `elmer-stream-verb`, `elmer-stream-elapsed`, `elmer-stream-tokens`, `elmer-stream-body`, `elmer-stream-reasoning`, `elmer-stream-cursor`, `elmer-stream-jump-live`.

- [ ] **Step 1: Write the failing test**

`src/elmer/StreamingStatusCard.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StreamingStatusCard, type StreamingStatusCardProps } from './StreamingStatusCard';

function props(overrides: Partial<StreamingStatusCardProps> = {}): StreamingStatusCardProps {
  return {
    verb: 'chasing DX',
    isResponding: false,
    answer: '',
    reasoning: '',
    tokensEstimate: 0,
    elapsedSecs: 3,
    expanded: false,
    onToggleExpand: () => {},
    ...overrides,
  };
}

describe('StreamingStatusCard', () => {
  it('collapsed by default: shows verb + elapsed, no body', () => {
    render(<StreamingStatusCard {...props()} />);
    expect(screen.getByTestId('elmer-stream-card')).toBeTruthy();
    expect(screen.getByTestId('elmer-stream-verb').textContent).toContain('chasing DX');
    expect(screen.getByTestId('elmer-stream-elapsed').textContent).toContain('3s');
    expect(screen.queryByTestId('elmer-stream-body')).toBeNull();
  });

  it('shows no token counter when the estimate is 0, and shows ~N tok when > 0', () => {
    const { rerender } = render(<StreamingStatusCard {...props({ tokensEstimate: 0 })} />);
    expect(screen.queryByTestId('elmer-stream-tokens')).toBeNull();
    rerender(<StreamingStatusCard {...props({ tokensEstimate: 1240 })} />);
    expect(screen.getByTestId('elmer-stream-tokens').textContent).toBe('~1,240 tok');
  });

  it('shows "responding" (not a radio verb) once isResponding is true', () => {
    render(<StreamingStatusCard {...props({ isResponding: true, answer: 'Hi' })} />);
    expect(screen.getByTestId('elmer-stream-verb').textContent).toContain('responding');
    expect(screen.getByTestId('elmer-stream-verb').textContent).not.toContain('chasing DX');
  });

  it('toggle button invokes onToggleExpand', () => {
    const onToggleExpand = vi.fn();
    render(<StreamingStatusCard {...props({ onToggleExpand })} />);
    fireEvent.click(screen.getByTestId('elmer-stream-card-toggle'));
    expect(onToggleExpand).toHaveBeenCalledOnce();
  });

  it('expanded: renders the bounded body with reasoning, answer, and cursor', () => {
    render(<StreamingStatusCard {...props({ expanded: true, isResponding: true, reasoning: 'weighing options', answer: 'The answer' })} />);
    const body = screen.getByTestId('elmer-stream-body');
    expect(body).toBeTruthy();
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('weighing options');
    expect(body.textContent).toContain('The answer');
    expect(screen.getByTestId('elmer-stream-cursor')).toBeTruthy();
  });

  it('h5azu-a: reasoning stays visible when the answer starts (no auto-collapse to a cursor)', () => {
    // A long reasoning trace plus a 1-char first answer token, expanded.
    render(<StreamingStatusCard {...props({ expanded: true, isResponding: true, reasoning: 'a very long thinking trace the operator was reading', answer: 'X' })} />);
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('very long thinking trace');
    expect(screen.getByTestId('elmer-stream-body').textContent).toContain('X');
  });
});
```

- [ ] **Step 2: Run it — expect failure**

Run: `pnpm exec vitest run src/elmer/StreamingStatusCard.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the component**

`src/elmer/StreamingStatusCard.tsx`:
```tsx
import { useEffect, useRef } from 'react';
import { useStreamAutoFollow } from './useStreamAutoFollow';

export interface StreamingStatusCardProps {
  /** Current radio verb (used while not yet responding). */
  verb: string;
  /** True once answer tokens are streaming (shows a stable "responding"). */
  isResponding: boolean;
  /** Live answer buffer (plain text). */
  answer: string;
  /** Live reasoning buffer (plain text). */
  reasoning: string;
  /** Estimated tokens so far; a counter shows only when > 0. */
  tokensEstimate: number;
  /** Seconds since the in-flight window began. */
  elapsedSecs: number;
  /** Whether the bounded stream body is expanded. */
  expanded: boolean;
  /** Toggle the expanded body. */
  onToggleExpand: () => void;
}

function formatElapsed(secs: number): string {
  return secs < 60
    ? `${secs}s`
    : `${Math.floor(secs / 60)}m ${String(secs % 60).padStart(2, '0')}s`;
}

/**
 * The single in-flight surface for an Elmer turn (tuxlink-h5azu / 06v9s / d5zns).
 * Collapsed by default to a live counter row; expands to a bounded (~10-line)
 * scrolling box that shows the reasoning trace + streaming answer. Owns the whole
 * running->done window, so there is no bubble<->indicator handoff to glitch.
 */
export function StreamingStatusCard({
  verb,
  isResponding,
  answer,
  reasoning,
  tokensEstimate,
  elapsedSecs,
  expanded,
  onToggleExpand,
}: StreamingStatusCardProps) {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const follow = useStreamAutoFollow(bodyRef);

  // Follow the growing stream inside the box, but only while expanded + pinned.
  useEffect(() => {
    if (expanded) follow.followIfPinned();
  }, [answer, reasoning, expanded, follow]);

  const label = isResponding ? 'responding' : verb;

  return (
    <div className="elmer-stream-card" data-testid="elmer-stream-card" data-expanded={expanded}>
      <button
        type="button"
        className="elmer-stream-head"
        data-testid="elmer-stream-card-toggle"
        aria-expanded={expanded}
        aria-label={expanded ? 'Collapse live output' : 'Expand live output'}
        onClick={onToggleExpand}
      >
        <span className="elmer-stream-chev" aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        <span className="elmer-stream-pulse" aria-hidden="true" />
        <span className="elmer-stream-verb" data-testid="elmer-stream-verb">
          Elmer is{' '}
          <span className={isResponding ? 'elmer-stream-verb-em' : undefined}>{label}</span>…
        </span>
        <span className="elmer-stream-spacer" />
        <span className="elmer-stream-metrics">
          {tokensEstimate > 0 && (
            <>
              <span className="elmer-stream-tokens" data-testid="elmer-stream-tokens">
                ~{tokensEstimate.toLocaleString()} tok
              </span>
              <span aria-hidden="true"> · </span>
            </>
          )}
          <span className="elmer-stream-elapsed" data-testid="elmer-stream-elapsed">
            {formatElapsed(elapsedSecs)}
          </span>
        </span>
      </button>

      {expanded && (
        <div
          className="elmer-stream-body"
          data-testid="elmer-stream-body"
          ref={bodyRef}
          onScroll={follow.onScroll}
        >
          {reasoning.length > 0 && (
            <div className="elmer-stream-reasoning" data-testid="elmer-stream-reasoning">
              {reasoning}
            </div>
          )}
          {answer.length > 0 && (
            <span className="elmer-stream-answer">
              {answer}
              <span
                className="elmer-stream-cursor"
                data-testid="elmer-stream-cursor"
                aria-hidden="true"
              />
            </span>
          )}
          {!follow.atBottom && (
            <button
              type="button"
              className="elmer-stream-jump-live"
              data-testid="elmer-stream-jump-live"
              onClick={follow.jumpToLive}
            >
              ↓ Jump to live
            </button>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `pnpm exec vitest run src/elmer/StreamingStatusCard.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/elmer/StreamingStatusCard.tsx src/elmer/StreamingStatusCard.test.tsx
git commit -F - <<'EOF'
feat(elmer): StreamingStatusCard — collapsed counter + bounded expand (h5azu/06v9s/d5zns)

One presentational card owns the whole in-flight window. Collapsed to a live
~N tok counter + elapsed by default; expands to a bounded scrolling body with
reasoning + streaming answer that never auto-collapses. No bubble<->indicator
handoff to glitch.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 5: Wire the card into ElmerPane (delete old surface)

**Files:**
- Modify: `src/elmer/ElmerPane.tsx`

**Interfaces:**
- Consumes: `StreamingStatusCard` (Task 4), `useThinkingPulse` (Task 3), `useStreamAutoFollow` (Task 2).

- [ ] **Step 1: Add imports**

In `src/elmer/ElmerPane.tsx`, with the other `./` imports:
```ts
import { StreamingStatusCard } from './StreamingStatusCard';
import { useThinkingPulse } from './useThinkingPulse';
import { useStreamAutoFollow } from './useStreamAutoFollow';
```

- [ ] **Step 2: Delete the `StreamingBubble` function**

Remove the entire `function StreamingBubble({ answer, reasoning }: { ... }) { ... }` definition (the transient live bubble, under the "Transient streaming bubble (phase 2b)" comment) and its doc comment.

- [ ] **Step 3: Delete the `ThinkingIndicator` function**

Remove the entire `function ThinkingIndicator() { ... }` definition and its doc comment. (`RADIO_VERBS` now lives in `radioVerbs.ts` from Task 1 and is no longer referenced here except via `useThinkingPulse`.)

- [ ] **Step 4: Add derived state inside the `ElmerPane` component**

Just after the `useElmer()` destructure and the existing `useState`/`useRef` declarations, add:
```ts
  // One predicate governs the whole in-flight window — the card owns running->done,
  // so there is no second component to flash in the EV_TURN -> EV_OUTCOME gap.
  const isInFlight =
    phase === 'running' || streamingAnswer.length > 0 || streamingReasoning.length > 0;
  const isResponding = streamingAnswer.length > 0;
  const tokensEstimate = Math.round(
    (streamingAnswer.length + streamingReasoning.length) / 4,
  );
  const { verb, elapsedSecs } = useThinkingPulse(isInFlight);
  // Sticky-per-session expand state for the stream box (default collapsed).
  const [streamExpanded, setStreamExpanded] = useState(false);

  // Transcript auto-follow — replaces the unconditional scrollIntoView effect.
  const messagesRef = useRef<HTMLDivElement>(null);
  const transcriptFollow = useStreamAutoFollow(messagesRef);
```

- [ ] **Step 5: Replace the auto-scroll effect**

Replace the existing effect that reads:
```ts
  useEffect(() => {
    if (typeof listEndRef.current?.scrollIntoView === 'function') {
      listEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [items, streamingAnswer, streamingReasoning]);
```
with:
```ts
  // Follow the transcript only while pinned to the bottom, so the operator can
  // scroll up to supervise tool-call chips mid-stream (tuxlink-06v9s).
  useEffect(() => {
    transcriptFollow.followIfPinned();
  }, [items, streamingAnswer, streamingReasoning, transcriptFollow]);
```
Then delete the now-unused `const listEndRef = useRef<HTMLDivElement>(null);` declaration and remove the `isStreaming` const (`const isStreaming = streamingAnswer.length > 0 || streamingReasoning.length > 0;`) — it is superseded by `isInFlight`/`isResponding`. Keep `isRunning`.

- [ ] **Step 6: Update the message-list render**

In the message-list return branch (the `<div className="elmer-messages" ...>`), add the ref + scroll handler and a transcript jump-to-live pill, and swap the streaming/thinking block for the card. The block becomes:
```tsx
        return (
          <div
            className="elmer-messages"
            data-testid="elmer-messages"
            role="log"
            aria-live="polite"
            ref={messagesRef}
            onScroll={transcriptFollow.onScroll}
          >
            {items.map((item) => (
              <MessageItem key={item.id} item={item} />
            ))}
            {isInFlight && (
              <StreamingStatusCard
                verb={verb}
                isResponding={isResponding}
                answer={streamingAnswer}
                reasoning={streamingReasoning}
                tokensEstimate={tokensEstimate}
                elapsedSecs={elapsedSecs}
                expanded={streamExpanded}
                onToggleExpand={() => setStreamExpanded((v) => !v)}
              />
            )}
            {lastOutcome && (
              <OutcomeCallout
                phase={phase}
                detail={lastOutcome.detail}
                onSwitchProvider={() => { setSwitchProviderFocusTier('paygo'); }}
              />
            )}
            {!transcriptFollow.atBottom && (
              <button
                type="button"
                className="elmer-transcript-jump-live"
                data-testid="elmer-transcript-jump-live"
                onClick={transcriptFollow.jumpToLive}
              >
                ↓ Jump to live
              </button>
            )}
          </div>
        );
```
(Remove the old `{isStreaming && <StreamingBubble .../>}`, `{isRunning && !isStreaming && <ThinkingIndicator />}`, and `<div ref={listEndRef} />` lines.)

- [ ] **Step 7: Typecheck**

Run: `pnpm typecheck`
Expected: PASS — no unused `listEndRef` / `isStreaming` / `StreamingBubble` / `ThinkingIndicator` references remain. (If tsc reports unused symbols, delete them.)

- [ ] **Step 8: Run the Elmer suite (expect the OLD streaming/thinking tests to FAIL)**

Run: `pnpm exec vitest run src/elmer/ElmerPane.test.tsx`
Expected: FAIL — the phase-2b bubble tests and thinking-indicator tests reference removed testids (`elmer-streaming-bubble`, `elmer-streaming-cursor`, `elmer-thinking*`). These are migrated in Task 6. Do NOT commit yet.

---

## Task 6: Migrate + extend the ElmerPane tests

**Files:**
- Modify: `src/elmer/ElmerPane.test.tsx`

- [ ] **Step 1: Replace the thinking-indicator describe blocks**

Delete the three describe blocks that target the old indicator:
- `'<ElmerPane> -- thinking indicator'`
- `'<ElmerPane> -- thinking indicator verb cycling'`
- `'<ElmerPane> -- thinking indicator elapsed timer'`

Replace them with a single block targeting the card's collapsed row (verb cycling + elapsed formatting are now covered by `useThinkingPulse.test.ts`):
```tsx
describe('<ElmerPane> -- in-flight streaming card (collapsed default)', () => {
  it('shows the collapsed streaming card while a run is in progress', async () => {
    render(<ElmerPane />);
    const input = screen.getByTestId('elmer-input') as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: 'hi' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(screen.getByTestId('elmer-stream-card')).toBeTruthy());
    // Collapsed by default: no expanded body.
    expect(screen.queryByTestId('elmer-stream-body')).toBeNull();
    // The verb phrase is a RADIO_VERBS phrase before any answer streams.
    const verbText = screen.getByTestId('elmer-stream-verb').textContent ?? '';
    const verbOnly = verbText.replace(/^Elmer is\s*/, '').replace(/…\s*$/, '').trim();
    expect(RADIO_VERBS).toContain(verbOnly);
  });

  it('the streaming card disappears once EV_OUTCOME arrives', async () => {
    render(<ElmerPane />);
    const input = screen.getByTestId('elmer-input') as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: 'hi' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(screen.getByTestId('elmer-stream-card')).toBeTruthy());

    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, {
      kind: 'outcome', outcomeKind: 'done', detail: '',
    });
    expect(screen.queryByTestId('elmer-stream-card')).toBeNull();
  });
});
```

> Note: use whatever the file's existing helper is for starting a run. If the existing thinking tests started a run by firing an event rather than typing, mirror that. The `elmer-input` testid + Enter is the send path used elsewhere in this file.

- [ ] **Step 2: Replace the phase-2b streaming-bubble + auto-collapse blocks**

Delete:
- `'<ElmerPane> phase 2b -- streaming bubble renders live answer + cursor'`
- `'<ElmerPane> phase 2b -- reasoning auto-collapses when the answer arrives'`

Keep unchanged:
- `'<ElmerPane> phase 2b -- committed item shows a collapsed reasoning toggle that expands'` (the committed-item `ReasoningDisclosure` is unchanged).

Add in their place:
```tsx
describe('<ElmerPane> streaming card -- live stream renders in the expandable body', () => {
  it('assistant deltas appear in the expanded body with a cursor; card carries the token estimate', async () => {
    render(<ElmerPane />);
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Hello ' });
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'world' });

    // Card is present; expand it to see the live text.
    expect(screen.getByTestId('elmer-stream-card')).toBeTruthy();
    fireEvent.click(screen.getByTestId('elmer-stream-card-toggle'));

    const body = screen.getByTestId('elmer-stream-body');
    expect(body.textContent).toContain('Hello world');
    expect(screen.getByTestId('elmer-stream-cursor')).toBeTruthy();
    // 'Hello world' = 11 chars -> round(11/4) = 3.
    expect(screen.getByTestId('elmer-stream-tokens').textContent).toBe('~3 tok');
  });

  it('h5azu-b: at finalize the card is gone and exactly one committed markdown item renders', async () => {
    render(<ElmerPane />);
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Streamed answer' });
    expect(screen.getByTestId('elmer-stream-card')).toBeTruthy();

    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'Streamed answer' });

    expect(screen.queryByTestId('elmer-stream-card')).toBeNull();
    expect(screen.queryByTestId('elmer-stream-cursor')).toBeNull();
    const committed = screen.getAllByTestId('elmer-turn-assistant');
    expect(committed).toHaveLength(1);
    expect(committed[0].textContent).toContain('Streamed answer');
  });

  it('h5azu-reflash: no second in-flight indicator appears between EV_TURN and EV_OUTCOME', async () => {
    render(<ElmerPane />);
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Answer' });
    // Finalize the turn but do NOT yet send the outcome — the old code re-showed
    // the thinking indicator in this window (phase still 'running').
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'Answer' });

    // The card must be gone and nothing re-flashes below the committed answer.
    expect(screen.queryByTestId('elmer-stream-card')).toBeNull();
    expect(screen.getByTestId('elmer-turn-assistant').textContent).toContain('Answer');
  });

  it('h5azu-a: reasoning stays visible when the answer starts (expanded)', async () => {
    render(<ElmerPane />);
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'a long thinking trace' });
    fireEvent.click(screen.getByTestId('elmer-stream-card-toggle')); // expand
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('a long thinking trace');

    // Answer starts — reasoning must NOT disappear.
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'X' });
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('a long thinking trace');
    expect(screen.getByTestId('elmer-stream-body').textContent).toContain('X');
  });
});

describe('<ElmerPane> streaming card -- expand is sticky across turns (d5zns)', () => {
  it('once expanded, a later turn stays expanded', async () => {
    render(<ElmerPane />);
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'first' });
    fireEvent.click(screen.getByTestId('elmer-stream-card-toggle')); // expand
    expect(screen.getByTestId('elmer-stream-body')).toBeTruthy();

    // Finalize turn 1, then start turn 2.
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'first' });
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'second' });

    // Still expanded (sticky) — body visible without another click.
    expect(screen.getByTestId('elmer-stream-body')).toBeTruthy();
  });
});

describe('<ElmerPane> transcript -- scroll-lock released when scrolled up (06v9s)', () => {
  it('does not force-scroll the transcript when the operator has scrolled up', async () => {
    render(<ElmerPane />);
    const list = screen.getByTestId('elmer-messages');
    // Define scroll geometry (jsdom does no layout).
    Object.defineProperty(list, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(list, 'clientHeight', { value: 200, configurable: true });
    let top = 100; // scrolled up
    Object.defineProperty(list, 'scrollTop', {
      get: () => top, set: (v: number) => { top = v; }, configurable: true,
    });
    fireEvent.scroll(list); // register scrolled-up state

    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'token' });
    // The transcript was NOT yanked back to the bottom.
    expect(top).toBe(100);
    // A jump-to-live affordance is offered.
    expect(screen.getByTestId('elmer-transcript-jump-live')).toBeTruthy();
  });
});
```

- [ ] **Step 3: Run the full Elmer suite**

Run: `pnpm exec vitest run src/elmer/ElmerPane.test.tsx src/elmer/StreamingStatusCard.test.tsx src/elmer/useStreamAutoFollow.test.ts src/elmer/useThinkingPulse.test.ts`
Expected: PASS. If a migrated test fails because a run is started differently in this file, align it with the file's existing send/render helpers (see the Task 6 Step 1 note).

- [ ] **Step 4: Commit (code + tests together — the refactor and its test migration are one reviewable unit)**

```bash
git add src/elmer/ElmerPane.tsx src/elmer/ElmerPane.test.tsx
git commit -F - <<'EOF'
feat(elmer): render one StreamingStatusCard; kill the bubble/indicator glitches

ElmerPane now renders a single StreamingStatusCard off an isInFlight predicate,
replacing the transient bubble + standalone thinking indicator. Transcript
auto-follow is gated on pin-to-bottom so tool-call chips stay supervisable
mid-stream. Migrates the phase-2b + thinking-indicator tests and adds
regression coverage for h5azu (single commit, no reflash, reasoning persists),
06v9s (scroll released when scrolled up), and d5zns (sticky expand).

Closes tuxlink-h5azu, tuxlink-06v9s, tuxlink-d5zns.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 7: Styles

**Files:**
- Modify: `src/elmer/ElmerPane.css`

- [ ] **Step 1: Remove obsolete rules**

Delete the `.elmer-streaming-bubble`, `.elmer-streaming-answer`, `.elmer-streaming-cursor` (+ its `@keyframes elmer-blink` if unused elsewhere — grep first), and `.elmer-thinking*` rule blocks. Keep `@keyframes elmer-pulse` (reused below).

- [ ] **Step 2: Add the card styles**

Append to `src/elmer/ElmerPane.css` (token names match the existing file — `--surface-2`, `--border`, `--border-strong`, `--text`, `--text-dim`, `--text-faint`, `--accent`, `--accent-teal`, `--bg-dim`):
```css
/* ===== Streaming status card (tuxlink-h5azu / 06v9s / d5zns) ===== */
.elmer-stream-card {
  border: 1px solid var(--border-strong);
  border-radius: 9px;
  background: var(--surface-2);
  overflow: hidden;
}
.elmer-stream-head {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 9px 11px;
  background: none;
  border: none;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.elmer-stream-head:hover { background: var(--surface-3); }
.elmer-stream-chev { color: var(--text-dim); font-size: 10px; width: 11px; }
.elmer-stream-pulse {
  width: 8px; height: 8px; border-radius: 50%; flex: none;
  background: var(--accent);
  animation: elmer-pulse 1.6s ease-out infinite;
}
.elmer-stream-verb { font-size: 12.5px; color: var(--text); }
.elmer-stream-verb-em { color: var(--accent); }
.elmer-stream-spacer { flex: 1; }
.elmer-stream-metrics {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 11px; color: var(--text-dim);
  font-variant-numeric: tabular-nums; white-space: nowrap;
}
.elmer-stream-tokens { color: var(--accent-teal); }

.elmer-stream-body {
  border-top: 1px solid var(--border);
  max-height: 210px;            /* ~10 lines — tuxlink-06v9s */
  overflow-y: auto;
  padding: 10px 12px;
  background: var(--bg-dim, #0c1017);
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 12px; line-height: 1.55; color: var(--text);
  position: relative;
}
.elmer-stream-reasoning {
  color: var(--text-faint); font-style: italic;
  margin-bottom: 8px; padding-bottom: 8px;
  border-bottom: 1px dashed var(--border);
  white-space: pre-wrap;
}
.elmer-stream-answer { white-space: pre-wrap; }
.elmer-stream-cursor {
  display: inline-block; width: 7px; height: 14px;
  background: var(--accent); vertical-align: text-bottom; margin-left: 1px;
  animation: elmer-blink 1s step-end infinite;
}
@keyframes elmer-blink { 50% { opacity: 0; } }

.elmer-stream-jump-live,
.elmer-transcript-jump-live {
  position: sticky; bottom: 6px; float: right;
  font-size: 10.5px; color: var(--text);
  background: var(--accent); border: none; border-radius: 11px;
  padding: 2px 9px; cursor: pointer;
}
.elmer-transcript-jump-live { margin-left: auto; }
```

> If `@keyframes elmer-blink` already exists in the file, do not duplicate it — reuse the existing one and drop the block above.

- [ ] **Step 3: Lint CSS + confirm nothing else referenced the removed classes**

Run: `grep -rn "elmer-streaming-\|elmer-thinking" src/ || echo "no references"`
Expected: `no references` (all removed). Then:
Run: `pnpm lint:css`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/elmer/ElmerPane.css
git commit -F - <<'EOF'
style(elmer): streaming status card + bounded body + jump-to-live

Bounded ~10-line stream body (max-height 210px, own scroll), collapsed counter
row, jump-to-live pill; removes the obsolete .elmer-streaming-* / .elmer-thinking*
rules.

Agent: basin-juniper-fjord
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Task 8: Full gate + push + PR

**Files:** none (verification + integration).

- [ ] **Step 1: Full local gate**

Run: `pnpm typecheck && pnpm exec vitest run src/elmer && pnpm lint:css`
Expected: PASS across typecheck, the whole `src/elmer` suite, and CSS lint.

- [ ] **Step 2: Broader smoke (AppShell wiring didn't regress)**

Run: `pnpm exec vitest run src/shell/AppShell.elmer.test.tsx`
Expected: PASS.

- [ ] **Step 3: Push the branch** (persistent cwd is the worktree)

```bash
git push -u origin bd-tuxlink-h5azu/elmer-stream-redesign
```

- [ ] **Step 4: Open the PR**

```bash
gh pr create --base main --head bd-tuxlink-h5azu/elmer-stream-redesign \
  --title '[basin-juniper-fjord] Elmer streaming-surface redesign (h5azu+06v9s+d5zns)' \
  --body "$(cat <<'BODY'
Unifies the transient streaming bubble and the standalone thinking indicator into one StreamingStatusCard that owns the whole in-flight lifecycle.

- **tuxlink-h5azu** — no collapse-to-cursor, no plain->markdown reprint, no thinking re-flash (single-component lifecycle; markdown commits once).
- **tuxlink-06v9s** — bounded ~10-line stream box with its own scroll; transcript/box auto-follow only when pinned to bottom, so tool-call chips stay supervisable mid-stream.
- **tuxlink-d5zns** — collapsed to a live `~N tok` counter by default, expandable (sticky per session).

Pure frontend render/layout change: no backend, `useElmer`, or `elmerEvents` changes. New units: `useStreamAutoFollow`, `useThinkingPulse`, `StreamingStatusCard`, `radioVerbs`. Regression tests added for each root cause.

Design spec: `docs/superpowers/specs/2026-07-08-elmer-stream-surface-redesign-design.md`.
Live WebKitGTK verification pending against the operator's build.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
BODY
)"
```

- [ ] **Step 5: Update bd issues**

```bash
bd close tuxlink-h5azu tuxlink-06v9s tuxlink-d5zns
```
(Or leave open until the operator confirms live verification — operator's call.)

---

## Self-Review

**Spec coverage:**
- h5azu collapse-to-cursor → Task 4 (card, no auto-collapse) + Task 6 (h5azu-a test). ✓
- h5azu reprint → Task 5 (card collapsed default; single markdown commit) + Task 6 (h5azu-b test). ✓
- h5azu thinking re-flash → Task 5 (single `isInFlight` predicate) + Task 6 (h5azu-reflash test). ✓
- 06v9s scroll-lock → Task 2 (`useStreamAutoFollow`) + Task 5 (transcript wiring) + Task 6 (scroll test). ✓
- 06v9s bounded box → Task 4 (body) + Task 7 (`max-height: 210px`). ✓
- d5zns collapsed default + counter → Task 4 (collapsed row, `~N tok`) + Task 6 (sticky test). ✓
- Non-streaming providers → Task 4 (counter hidden when estimate 0). ✓
- No backend/hook/event changes → Global Constraints; only `src/elmer/*.tsx/.ts/.css` touched. ✓

**Placeholder scan:** none — every code + test step carries complete content.

**Type consistency:** `StreamAutoFollow`/`useStreamAutoFollow`, `ThinkingPulse`/`useThinkingPulse`, `StreamingStatusCardProps` names and fields are identical across the tasks that define and consume them.

**Out of scope (unchanged):** wgh19 / bx94e / 5io0f (copy/email row), 8asne (security gate).
