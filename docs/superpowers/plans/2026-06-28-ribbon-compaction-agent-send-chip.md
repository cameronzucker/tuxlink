# Ribbon Compaction — Agent-send Chip + Popover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the `EgressArmControl` arm presets into a click-to-open popover anchored to a compact ribbon chip, so the dashboard ribbon stops squishing its items and Connect regains room — while the agent-send state (dot + OFF/ON/countdown/LOCKED) stays glanceable.

**Architecture:** Refactor the existing presentational `EgressArmControl` (`src/shell/EgressArmControl.tsx`) into a chip button + a body-portaled popover. The chip shows state; the popover holds the arm/disarm actions. The popover reuses `IdentitySwitcher`'s mechanism verbatim: local `open` state, measured anchor `coords`, `createPortal` to `document.body` with `position: fixed`, `Esc`-to-close, and document `mousedown` outside-click-to-close. `useEgressArm` stays in `AppShell`; props are unchanged, so neither `AppShell.tsx` nor `DashboardRibbon.tsx` change.

**Tech Stack:** React 18 + TypeScript (Vite, WebKitGTK), `@testing-library/react` + vitest, `react-dom` `createPortal`.

## Global Constraints

- Voice (UI copy): declarative, present-indicative, no first person, no temporal hedging, no defensive self-assertion.
- **This Pi cannot finish a cold `cargo` build/test locally** — but this change is **frontend-only** (no Rust). `npx vitest run <file>` runs locally per file; `npx tsc --noEmit -p tsconfig.json` typechecks.
- Self-contained: touch only `src/shell/EgressArmControl.tsx`, `src/shell/EgressArmControl.test.tsx`, `src/shell/AppShell.css`, and `src/shell/DashboardRibbon.test.tsx`. Do **not** change `src/shell/AppShell.tsx`, `src/shell/DashboardRibbon.tsx`, `src/security/useEgressArm.ts`, or `src/security/egressTypes.ts` — the props contract and types are unchanged.
- Preserve the props interface `EgressArmControlProps { status, onArm, onDisarm, busy?, error? }` and the export name `EgressArmControl` (the import sites depend on both).
- Preserve every existing `egress-*` testid so contracts stay reachable: `egress-arm-control` (root), `egress-state`, `egress-countdown`, `egress-presets`, `egress-arm-{secs}`, `egress-disarm`, `egress-locked`, `egress-error`.
- State→dot mapping is unchanged: `tainted` → dot class `tx` + label `LOCKED`; else `armed` → dot class `''` + label `ON`; else dot class `idle` + label `OFF`. Taint is terminal (wins over armed).
- Branch: `bd-tuxlink-yfezs/ribbon-compaction` (worktree off `origin/main`). Commit trailers: `Agent: <moniker>` + the Co-Authored-By line.
- Spec: `docs/superpowers/specs/2026-06-28-ribbon-compaction-agent-send-chip-design.md`. Mockup: `dev/scratch/ribbon-compaction/`.

---

### Task 1: Restructure `EgressArmControl` into chip + popover

The whole behavior change lives here: chip shows state; clicking it opens a portaled popover holding the per-state actions. The existing unit tests are rewritten to the new interaction model (state visible on the chip; actions reached by opening the popover).

**Files:**
- Modify: `src/shell/EgressArmControl.tsx` (replace the inline body with chip + popover; keep `CountdownCell`, the props interface, and the export name)
- Modify: `src/shell/EgressArmControl.test.tsx` (rewrite to the chip→open→assert model + add open/close tests)
- Modify: `src/shell/AppShell.css` (add chip CSS under `.layout-b .dashboard`; add a top-level `.egress-arm-popover` block — the popover is portaled to `<body>`, so it is OUTSIDE `.dashboard` and the existing `.layout-b .dashboard .egress-*` rules do NOT reach it)

**Interfaces:**
- Consumes: `EgressStatusDto` (`{ armed, armedRemainingSecs, tainted }`), `EGRESS_DURATION_PRESETS` (array of `{ label, secs }`: `15 min`/900, `1 hour`/3600, `4 hours`/14400), `formatEgressRemaining` — all from `src/security/egressTypes.ts`.
- Produces: `EgressArmControl` (unchanged props `{ status: EgressStatusDto; onArm: (secs: number) => void; onDisarm: () => void; busy?: boolean; error?: string | null }`). New internal testids: `egress-chip` (the trigger button), `egress-popover` (the portaled panel).

- [ ] **Step 1: Rewrite the test file to the chip + popover model**

Replace the entire body of the `describe('<EgressArmControl> — ...')` blocks (keep the `formatEgressRemaining (pure)` block and the imports as-is). The pattern: state assertions read the chip directly; action assertions open the popover first via `fireEvent.click(screen.getByTestId('egress-chip'))`.

Replace `src/shell/EgressArmControl.test.tsx` from the first `describe('<EgressArmControl> — disarmed state'` block onward with:

```tsx
function openPopover() {
  fireEvent.click(screen.getByTestId('egress-chip'));
}

describe('<EgressArmControl> — chip (state at a glance)', () => {
  it('disarmed: chip shows OFF, no countdown, popover closed', () => {
    render(<EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} />);
    expect(screen.getByTestId('egress-state').textContent).toContain('OFF');
    expect(screen.queryByTestId('egress-countdown')).toBeNull();
    expect(screen.queryByTestId('egress-popover')).toBeNull();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
  });

  it('armed: chip shows ON + live countdown without opening the popover', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 2535 })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('ON');
    // 2535s = 42:15
    expect(screen.getByTestId('egress-countdown').textContent).toContain('42:15');
  });

  it('tainted: chip shows LOCKED and no countdown', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 999, tainted: true })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-state').textContent).toContain('LOCKED');
    expect(screen.queryByTestId('egress-countdown')).toBeNull();
  });
});

describe('<EgressArmControl> — popover open/close', () => {
  it('clicking the chip opens the popover; Esc closes it', () => {
    render(<EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} />);
    openPopover();
    const pop = screen.getByTestId('egress-popover');
    expect(pop).toBeTruthy();
    fireEvent.keyDown(pop, { key: 'Escape' });
    expect(screen.queryByTestId('egress-popover')).toBeNull();
  });

  it('a mousedown outside the chip and popover closes it', () => {
    render(<EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} />);
    openPopover();
    expect(screen.getByTestId('egress-popover')).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByTestId('egress-popover')).toBeNull();
  });
});

describe('<EgressArmControl> — disarmed actions (in popover)', () => {
  it('popover shows the duration presets', () => {
    render(<EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} />);
    openPopover();
    expect(screen.getByTestId('egress-presets')).toBeTruthy();
    expect(screen.queryByTestId('egress-disarm')).toBeNull();
  });

  it('clicking a preset calls onArm with that duration in seconds', () => {
    const onArm = vi.fn();
    render(<EgressArmControl status={makeStatus()} onArm={onArm} onDisarm={vi.fn()} />);
    openPopover();
    fireEvent.click(screen.getByTestId('egress-arm-3600'));
    expect(onArm).toHaveBeenCalledWith(3600);
  });

  it('disables presets while busy', () => {
    render(<EgressArmControl status={makeStatus()} onArm={vi.fn()} onDisarm={vi.fn()} busy />);
    openPopover();
    expect((screen.getByTestId('egress-arm-900') as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('<EgressArmControl> — armed actions (in popover)', () => {
  it('popover shows Disarm, no presets', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 600 })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    openPopover();
    expect(screen.getByTestId('egress-disarm')).toBeTruthy();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
  });

  it('clicking Disarm calls onDisarm', () => {
    const onDisarm = vi.fn();
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 600 })}
        onArm={vi.fn()}
        onDisarm={onDisarm}
      />,
    );
    openPopover();
    fireEvent.click(screen.getByTestId('egress-disarm'));
    expect(onDisarm).toHaveBeenCalledTimes(1);
  });
});

describe('<EgressArmControl> — chip live countdown ticks down', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('decrements the chip countdown each second', () => {
    render(
      <EgressArmControl
        status={makeStatus({ armed: true, armedRemainingSecs: 65 })}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('egress-countdown').textContent).toContain('01:05');
    act(() => { vi.advanceTimersByTime(2000); });
    expect(screen.getByTestId('egress-countdown').textContent).toContain('01:03');
  });
});

describe('<EgressArmControl> — tainted actions (in popover)', () => {
  it('popover shows the locked explanation, no arm/disarm affordance', () => {
    render(
      <EgressArmControl status={makeStatus({ tainted: true })} onArm={vi.fn()} onDisarm={vi.fn()} />,
    );
    openPopover();
    expect(screen.getByTestId('egress-locked')).toBeTruthy();
    expect(screen.queryByTestId('egress-presets')).toBeNull();
    expect(screen.queryByTestId('egress-disarm')).toBeNull();
  });
});

describe('<EgressArmControl> — error surfacing (in popover)', () => {
  it('renders the error message inside the popover', () => {
    render(
      <EgressArmControl
        status={makeStatus()}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
        error="arm duration must be greater than zero"
      />,
    );
    openPopover();
    expect(screen.getByTestId('egress-error').textContent).toContain(
      'arm duration must be greater than zero',
    );
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npx vitest run src/shell/EgressArmControl.test.tsx`
Expected: FAIL — there is no `egress-chip` / `egress-popover` yet (the current component renders everything inline), so `fireEvent.click(getByTestId('egress-chip'))` throws "Unable to find an element".

- [ ] **Step 3: Rewrite the component as chip + portaled popover**

Replace the entire contents of `src/shell/EgressArmControl.tsx` with:

```tsx
/**
 * EgressArmControl — the operator ARM surface for agent send-authority
 * (MCP phase 3.6). A compact ribbon chip shows the state at a glance; the
 * arm/disarm actions live in a click-to-open popover so the dashboard ribbon
 * stays uncrowded.
 *
 * States (plain-language, WLE-litmus: forgiving + legible, never cryptic):
 *   - Disarmed → chip "Agent send: OFF"; popover offers duration presets.
 *   - Armed    → chip "Agent send: ON" + a live ticking countdown; popover
 *                offers Disarm.
 *   - Tainted  → chip "Agent send: LOCKED"; popover explains the session is
 *                tainted and authority is locked until restart.
 *
 * Presentational: state + actions come from useEgressArm (AppShell owns the
 * hook instance). The popover reuses IdentitySwitcher's mechanism: measured
 * anchor coords, createPortal to <body> (position:fixed) so it escapes the
 * ribbon's stacking context, Esc-to-close, and document mousedown
 * outside-click-to-close. The live 1-second countdown lives in a scoped
 * subtree (CountdownCell) so the tick does not repaint the rest of the ribbon.
 */

import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  EGRESS_DURATION_PRESETS,
  formatEgressRemaining,
  type EgressStatusDto,
} from '../security/egressTypes';

/**
 * Live countdown cell. Seeds from the polled remaining-seconds and ticks down
 * locally each second; re-seeds whenever a fresh poll changes the value (so a
 * re-arm or clock drift is corrected). Scoped so only this text node repaints.
 */
function CountdownCell({ remainingSecs }: { remainingSecs: number }) {
  const [secs, setSecs] = useState(remainingSecs);

  useEffect(() => {
    setSecs(remainingSecs);
  }, [remainingSecs]);

  useEffect(() => {
    const id = setInterval(() => {
      setSecs((s) => (s > 0 ? s - 1 : 0));
    }, 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <span className="egress-countdown" data-testid="egress-countdown">
      {formatEgressRemaining(secs)} left
    </span>
  );
}

export interface EgressArmControlProps {
  /** Live egress-grant snapshot from useEgressArm. */
  status: EgressStatusDto;
  /** Arm send-authority for the chosen duration (seconds). */
  onArm: (durationSecs: number) => void;
  /** Disarm send-authority immediately. */
  onDisarm: () => void;
  /** True while an arm/disarm round-trip is in flight (disables controls). */
  busy?: boolean;
  /** Last arm/disarm error, or null. Surfaced inline so a failed arm is visible
   *  (an operator must never believe authority is armed when it is not). */
  error?: string | null;
}

export const EgressArmControl = memo(function EgressArmControl({
  status,
  onArm,
  onDisarm,
  busy,
  error,
}: EgressArmControlProps) {
  const { armed, armedRemainingSecs, tainted } = status;

  // Taint is terminal: send-authority is locked regardless of arm state.
  const dotClass = tainted ? 'tx' : armed ? '' : 'idle';
  const stateLabel = tainted ? 'LOCKED' : armed ? 'ON' : 'OFF';

  const [open, setOpen] = useState(false);
  const chipRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const [coords, setCoords] = useState<{ top: number; left: number } | null>(null);

  // Anchor the portaled popover under the chip; re-measure on resize.
  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    function measure() {
      const r = chipRef.current?.getBoundingClientRect();
      if (r) setCoords({ top: r.bottom + 6, left: r.left });
    }
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [open]);

  // Click-outside closes (the popover is portaled out of the chip subtree).
  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      const t = e.target as Node;
      if (!chipRef.current?.contains(t) && !popRef.current?.contains(t)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  return (
    <div className="dash-item dash-egress" data-testid="egress-arm-control">
      <button
        type="button"
        ref={chipRef}
        className="dash-egress-chip"
        data-testid="egress-chip"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={`dash-status-dot ${dotClass}`} aria-hidden="true" />
        <span className="dash-egress-label">Agent send</span>
        <span
          className="dash-egress-state"
          data-testid="egress-state"
          data-armed={armed}
          data-tainted={tainted}
        >
          {stateLabel}
        </span>
        {armed && !tainted && <CountdownCell remainingSecs={armedRemainingSecs} />}
        <span className="dash-egress-caret" aria-hidden="true">
          {open ? '▴' : '▾'}
        </span>
      </button>

      {open &&
        coords &&
        createPortal(
          <div
            ref={popRef}
            className="egress-arm-popover"
            data-testid="egress-popover"
            role="dialog"
            aria-label="Agent send authority"
            tabIndex={-1}
            style={{ top: coords.top, left: coords.left }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setOpen(false);
            }}
          >
            <div className="egress-pop-title">Agent send authority</div>

            {tainted ? (
              <div className="dash-egress-locked" data-testid="egress-locked">
                Session tainted — restart Tuxlink to re-enable agent send.
              </div>
            ) : armed ? (
              <button
                type="button"
                className="egress-disarm-button"
                data-testid="egress-disarm"
                disabled={busy}
                onClick={onDisarm}
              >
                Disarm now
              </button>
            ) : (
              <>
                <div className="egress-arm-label">Arm send-authority for:</div>
                <div
                  className="egress-presets"
                  role="group"
                  aria-label="Arm agent send-authority for a bounded window"
                  data-testid="egress-presets"
                >
                  {EGRESS_DURATION_PRESETS.map((preset) => (
                    <button
                      key={preset.secs}
                      type="button"
                      className="egress-arm-button"
                      data-testid={`egress-arm-${preset.secs}`}
                      disabled={busy}
                      onClick={() => onArm(preset.secs)}
                      title={`Arm agent send-authority for ${preset.label}`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
                <div className="egress-pop-help">
                  While armed, an MCP agent may transmit or change settings. Disarms automatically
                  when the timer ends.
                </div>
              </>
            )}

            {error && (
              <div className="dash-egress-error" role="alert" data-testid="egress-error">
                {error}
              </div>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
});
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/shell/EgressArmControl.test.tsx`
Expected: PASS (all blocks, including `formatEgressRemaining (pure)`).

- [ ] **Step 5: Add the chip + popover CSS**

In `src/shell/AppShell.css`, find the existing egress block (search `.layout-b .dashboard .dash-egress-row`). REPLACE the `.layout-b .dashboard .dash-egress-row` rule (the old inline row) with the chip rules below, and APPEND the top-level popover block. The popover is portaled to `<body>`, so its rules MUST be top-level (not nested under `.dashboard`), or they will not apply.

Add/adjust these rules (keep the existing `.dash-status-dot`, `.dash-egress-state`, `.egress-countdown`, `.egress-presets`, `.egress-arm-button`, `.egress-disarm-button`, `.dash-egress-locked`, `.dash-egress-error` definitions, but ALSO mirror the interactive ones under `.egress-arm-popover` since the portaled panel is outside `.dashboard`):

```css
/* Agent-send chip (compact ribbon trigger). Replaces the old inline egress row. */
.layout-b .dashboard .dash-egress-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 10px;
  background: var(--surface-2);
  border: 1px solid var(--border-strong);
  border-radius: 7px;
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
}
.layout-b .dashboard .dash-egress-chip:hover,
.layout-b .dashboard .dash-egress-chip[aria-expanded='true'] {
  border-color: var(--accent);
}
.layout-b .dashboard .dash-egress-label {
  color: var(--text-dim);
}
.layout-b .dashboard .dash-egress-caret {
  color: var(--text-faint);
  font-size: 10px;
}

/* Portaled popover (anchored under the chip; position:fixed via the inline
   top/left from measured coords). Top-level — the panel lives on <body>. */
.egress-arm-popover {
  position: fixed;
  z-index: 200;
  min-width: 240px;
  background: var(--bg);
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
  padding: 13px 14px 14px;
  font-size: 12px;
  color: var(--text);
}
.egress-arm-popover .egress-pop-title {
  font-size: 11px;
  color: var(--text-faint);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  margin-bottom: 10px;
}
.egress-arm-popover .egress-arm-label {
  font-size: 11px;
  color: var(--text-dim);
  margin-bottom: 7px;
}
.egress-arm-popover .egress-presets {
  display: flex;
  gap: 7px;
}
.egress-arm-popover .egress-arm-button {
  flex: 1;
  text-align: center;
  font-size: 12px;
  padding: 7px 0;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--surface);
  color: var(--text);
  cursor: pointer;
}
.egress-arm-popover .egress-arm-button:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent-2);
}
.egress-arm-popover .egress-disarm-button {
  width: 100%;
  font-size: 12px;
  font-weight: 600;
  padding: 8px 0;
  border: 1px solid var(--error);
  border-radius: 6px;
  background: transparent;
  color: var(--error);
  cursor: pointer;
}
.egress-arm-popover .egress-arm-button:disabled,
.egress-arm-popover .egress-disarm-button:disabled {
  opacity: 0.55;
  cursor: default;
}
.egress-arm-popover .egress-pop-help {
  font-size: 11px;
  color: var(--text-faint);
  line-height: 1.45;
  margin-top: 10px;
}
.egress-arm-popover .dash-egress-locked {
  font-size: 12px;
  color: var(--error);
  line-height: 1.4;
}
.egress-arm-popover .dash-egress-error {
  margin-top: 10px;
  color: var(--error);
  font-size: 11px;
}
```

> Leave the existing `.dash-status-dot`, `.dash-egress-state`, and `.egress-countdown` rules in place — those classes render on the chip (inside `.dashboard`), so the existing scoped selectors still apply to them.

- [ ] **Step 6: Run the full check, then commit**

Run: `npx vitest run src/shell/EgressArmControl.test.tsx` and `npx tsc --noEmit -p tsconfig.json`
Expected: tests PASS; typecheck clean (exit 0).

```bash
git add src/shell/EgressArmControl.tsx src/shell/EgressArmControl.test.tsx src/shell/AppShell.css
git commit  # subject: feat(shell): compact Agent-send into a ribbon chip + popover
```

---

### Task 2: DashboardRibbon integration guard — Connect not crowded by inline presets

A focused integration assertion that, with the chip in place, the ribbon renders the egress chip and the preset row is NOT inline (it only appears when the chip is opened). This is the regression guard for the actual bug (presets squishing the ribbon).

**Files:**
- Modify: `src/shell/DashboardRibbon.test.tsx` (add one `describe` block; reuse the file's existing `render` helper and `makeData` factory)

**Interfaces:**
- Consumes: the `DashboardRibbon` component and the test file's existing `render()` + `makeData()` helpers. The ribbon takes an optional `egress` prop shaped `{ status: EgressStatusDto; onArm: (secs: number) => void; onDisarm: () => void; busy?: boolean; error?: string | null }` (the same object `AppShell` passes).

- [ ] **Step 1: Read the test file's helpers**

Open `src/shell/DashboardRibbon.test.tsx` and confirm the signatures of the local `render(ui, options?)` helper (around line 55) and `makeData(overrides)` factory. Note how other tests pass props to `<DashboardRibbon data={makeData(...)} ... />`. You will pass an `egress` prop alongside `data`.

- [ ] **Step 2: Write the failing integration test**

Add this block at the end of `src/shell/DashboardRibbon.test.tsx` (inside the top-level scope, after the last `describe`). Import any missing symbols at the top of the file: `fireEvent` from `@testing-library/react`, and `EGRESS_STATUS_DISARMED` from `../security/egressTypes` (add to the existing imports; do not duplicate).

```tsx
describe('<DashboardRibbon> — Agent-send chip is compact (tuxlink-yfezs)', () => {
  const egress = {
    status: EGRESS_STATUS_DISARMED,
    onArm: vi.fn(),
    onDisarm: vi.fn(),
  };

  it('renders the Agent-send chip with no inline preset row', () => {
    render(<DashboardRibbon data={makeData()} egress={egress} />);
    expect(screen.getByTestId('egress-chip')).toBeTruthy();
    // The presets must NOT be inline in the ribbon — they live in the popover.
    expect(screen.queryByTestId('egress-presets')).toBeNull();
  });

  it('opens the preset popover only when the chip is clicked', () => {
    render(<DashboardRibbon data={makeData()} egress={egress} />);
    fireEvent.click(screen.getByTestId('egress-chip'));
    expect(screen.getByTestId('egress-presets')).toBeTruthy();
  });
});
```

> If `screen` / `vi` are not already imported in this file, add them to the existing `vitest` / `@testing-library/react` import lines (do not create duplicate import statements). If the file already imports `EGRESS_STATUS_DISARMED` or `fireEvent`, reuse them.

- [ ] **Step 3: Run it**

Run: `npx vitest run src/shell/DashboardRibbon.test.tsx`
Expected: PASS — Task 1 already implemented the chip + popover, so the ribbon renders the chip and the presets appear only on click. (If the ribbon gates the egress item behind a truthy `egress` prop, the explicit `egress={egress}` satisfies it. If the assertion fails because the ribbon does not render egress without additional props, read `DashboardRibbon.tsx`'s egress-render guard and pass exactly what it requires — do NOT modify `DashboardRibbon.tsx`.)

- [ ] **Step 4: Run the affected suite + typecheck, then commit**

Run: `npx vitest run src/shell/DashboardRibbon.test.tsx src/shell/EgressArmControl.test.tsx` and `npx tsc --noEmit -p tsconfig.json`
Expected: PASS + clean typecheck.

```bash
git add src/shell/DashboardRibbon.test.tsx
git commit  # subject: test(shell): guard the ribbon Agent-send chip stays compact
```

---

## Self-Review

- **Spec coverage:** chip shows state + caret (T1) ✓; popover holds presets/disarm/locked/error (T1) ✓; armed countdown on the chip (T1, `armed && !tainted` CountdownCell) ✓; reuse IdentitySwitcher popover mechanism — coords + portal + Esc + outside-click (T1) ✓; presentational, props/types unchanged (T1, no AppShell/DashboardRibbon/hook edits) ✓; all `egress-*` testids preserved (T1) ✓; ribbon-compaction regression guard (T2) ✓; option B (overflow menu) excluded ✓.
- **Placeholder scan:** none — full component, full test file body, full CSS, and the integration test are inline. The only conditional instruction (T2 Step 3 "if the ribbon gates egress") is a read-and-match directive with an explicit no-modify boundary, not a behavior gap.
- **Type consistency:** `EgressArmControlProps { status, onArm, onDisarm, busy?, error? }` unchanged across T1; `EgressStatusDto { armed, armedRemainingSecs, tainted }` used consistently; `EGRESS_STATUS_DISARMED` reused in T2; testids (`egress-chip`, `egress-popover`, `egress-state`, `egress-countdown`, `egress-presets`, `egress-arm-{secs}`, `egress-disarm`, `egress-locked`, `egress-error`) consistent T1↔T2.
- **CSS scoping caught:** the portaled popover is outside `.dashboard`, so its interactive children are styled under a top-level `.egress-arm-popover` scope (T1 Step 5), not the old `.layout-b .dashboard .egress-*` selectors.
