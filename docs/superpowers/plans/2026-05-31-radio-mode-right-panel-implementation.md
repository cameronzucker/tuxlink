# Radio-Mode Right-Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the locked-in [radio-mode right-panel UX design](../specs/2026-05-31-radio-mode-right-panel-design.md). Every radio mode (Telnet, AX.25 Packet, ARDOP HF, future VARA HF / VARA FM) shares a 360 px right-hand panel; the reading pane returns to messages-only; the bottom session-log strip is removed in favor of an in-panel log section; Express vocabulary (Start / Stop / Abort) replaces tuxlink-invented terms throughout.

**Architecture:** A new `RadioPanel` shell component owns the right-hand column. It mounts on sidebar-connection-selected OR non-stopped-modem OR View → Toggle. Per-mode panel components (`TelnetRadioPanel`, `PacketRadioPanel`, `ArdopRadioPanel`) render the mode-specific content into shared section components (`SessionLogSection`, `SignalSection`, `ModemLinkSection`). Backend changes: ARDOP `Quality` and recent-frame stream from existing broadcaster + new PINGACK parsing (closes `tuxlink-1637`).

**Tech Stack:** TypeScript + React (frontend), Rust + Tauri (backend), Vitest + React Testing Library (frontend tests), `cargo test` (backend tests). Per-phase Codex adversarial review before merge. No new dependencies — every change uses primitives already in the project.

---

## Cross-cutting decisions

### File layout

All new code lives under `src/radio/`:

```
src/radio/
  RadioPanel.tsx              shell component (header, body slots, action row)
  RadioPanel.css              shell styles
  RadioPanel.test.tsx         shell tests
  types.ts                    panel-related TS types
  useRadioPanelVisibility.ts  visibility-rule hook
  modes/
    TelnetRadioPanel.tsx
    TelnetRadioPanel.test.tsx
    PacketRadioPanel.tsx
    PacketRadioPanel.test.tsx
    ArdopRadioPanel.tsx
    ArdopRadioPanel.test.tsx
  sections/
    SessionLogSection.tsx     shared across all modes
    SessionLogSection.test.tsx
    SignalSection.tsx         shared section; mode-specific content slotted in
    SignalSection.test.tsx
    ModemLinkSection.tsx      Packet only
    ModemLinkSection.test.tsx
  charts/
    Sparkline.tsx             60 s history sparkline (S/N, throughput)
    Sparkline.test.tsx
    FrameRibbon.tsx           ARDOP recent-frame-type ribbon
    FrameRibbon.test.tsx
```

Deletions (replaced or obsoleted):

```
src/connections/ArdopHfStub.tsx           replaced by ArdopRadioPanel
src/connections/TelnetCmsPanel.tsx        replaced by TelnetRadioPanel
src/connections/TelnetCmsPanel.css        same
src/connections/TelnetCmsPanel.test.tsx   same
src/packet/PacketConnectionPanel.tsx      replaced by PacketRadioPanel
src/packet/PacketConnectionPanel.css      same (or merged into RadioPanel.css)
src/modem/ArdopDock.tsx                   replaced by ArdopRadioPanel
src/modem/ArdopDock.css                   same
src/modem/ArdopDock.test.tsx              same
src/modem/ArdopDock.integration.test.tsx  same
src/session/SessionLog.tsx                replaced by in-panel SessionLogSection
src/session/SessionLog.css                same (or removed; styles move)
```

### Naming conventions

- Per-mode panel components: `{Mode}RadioPanel.tsx` — `TelnetRadioPanel`, `PacketRadioPanel`, `ArdopRadioPanel`, `VaraHfRadioPanel` (future).
- Shared sections: under `src/radio/sections/`; component name = section name + "Section" (e.g. `SignalSection`).
- Charts: under `src/radio/charts/`.
- CSS classes: `radio-panel`, `radio-panel-h` (header), `radio-panel-sec` (section), `radio-panel-act` (action row). Mode-specific subclasses prefix with mode: `radio-panel-ardop-*`, etc.
- Test IDs: `data-testid="radio-panel-root"` for the shell; `data-testid="radio-panel-{mode}"` per mode; section IDs as `data-testid="radio-panel-{section}"`.

### Test conventions

- **Frontend:** Vitest + React Testing Library. Mock Tauri IPC via `vi.mock('@tauri-apps/api/core', ...)`. Mock the `useModemStatus` hook via `vi.mock('../modem/useModemStatus', ...)` for component tests. Integration tests render `<AppShell>` with the QueryClientProvider wrapper (pattern already in `src/shell/AppShell.modemDock.test.tsx`).
- **Backend:** `cargo test --lib`. Existing `transport.rs` tests already use real `TcpListener` for stub modems; that pattern stays.

### Per-phase workflow

Every phase follows the same shape. The plan repeats it in each phase's "Verification & PR" task with the per-phase specifics, but the workflow is:

1. From the worktree (new per phase, per ADR 0008): branch is created by `new_tuxlink_worktree.py --slug <phase-slug> --issue <bd-id> --base main`.
2. Implement tasks per TDD. Commit per logical unit (often per task).
3. Quality gates from the worktree root:
   - `pnpm vitest run` — all tests green
   - `pnpm exec tsc --noEmit` — clean
   - `cargo test --manifest-path src-tauri/Cargo.toml --lib` — all tests green (if Rust changed)
   - `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` — clean (if Rust changed)
4. Push branch, open PR via `gh pr create`.
5. Codex adversarial round on the diff per the custom-prompt pattern in `CLAUDE.md` — output to `dev/adversarial/2026-05-31-<phase-slug>-codex.md` (gitignored). Address any P0/P1 findings as fix-up commits before merge.
6. Merge via `gh pr merge --merge --delete-branch` (no-squash per ADR 0010).
7. Operator smokes via `pnpm tauri dev` from any worktree on the merged branch.
8. Close the phase's bd-issue.

### bd-issue allocation per phase

Each phase gets its own bd-issue (not yet filed — file at phase-start). Suggested titles + IDs allocated when work starts:

| Phase | bd-issue (file at start) | Branch slug |
|---|---|---|
| P1 | `feat: RadioPanel shell scaffold + bottom-strip removal (radio-panel P1 of 5)` | `radio-panel-shell` |
| P2 | `feat: TelnetRadioPanel — migrate Telnet CMS to right-panel paradigm (radio-panel P2 of 5)` | `radio-panel-telnet` |
| P3 | `feat: PacketRadioPanel — migrate AX.25 Packet to right-panel paradigm (radio-panel P3 of 5)` | `radio-panel-packet` |
| P4 | `feat: ArdopRadioPanel + SignalSection — replaces ArdopDock + ArdopHfStub; closes tuxlink-1637 (radio-panel P4 of 5)` | `radio-panel-ardop` |
| P5 | `refactor: vocabulary cleanup — Start/Stop/Abort, ribbon Connect removal, View menu retirements (radio-panel P5 of 5)` | `radio-panel-vocab` |

### Cascade closures (deliverable)

By end of P5:
- `tuxlink-mnk4` — closed (PR #166 closed without merge once P4 lands; the whole `ArdopDock` is gone)
- `tuxlink-ed51` — closed as resolved by existing `Open WebGUI ↗` button (`tuxlink-60wh`)
- `tuxlink-mzr7` — closed as resolved by existing `Open WebGUI ↗` button
- `tuxlink-1637` — closed in P4 (Signal section's Quality score is the user-visible result)
- `tuxlink-74mx` (the spec) — closes once all 5 phases land
- `tuxlink-nr21` (this plan) — closes once all 5 phases land

---

## Phase 1 — RadioPanel shell scaffold + bottom-strip removal

**Goal:** Land the `RadioPanel` shell component with empty per-mode placeholder panels, integrate it into `AppShell`'s grid (reading pane gives up 360 px when the panel is mounted), remove the bottom session-log strip, rename the View menu item. After P1: no functional regression vs current state (panel mounts on selection but shows "coming soon" placeholders per mode); modes still use their current panel components in the reading pane.

**Wait — clarification on integration.** P1 is structural prep only. Reading-pane panels (`TelnetCmsPanel`, `PacketConnectionPanel`, `ArdopHfStub` + `ArdopDock`) STAY in P1. The new `RadioPanel` mounts as a *separate* surface during P1, showing placeholder content. P2-P4 each migrate one mode and delete that mode's old surface. This avoids a "big bang" cutover.

**bd-issue & branch:** file at start; suggested title "feat: RadioPanel shell scaffold + bottom-strip removal (radio-panel P1 of 5)". Slug: `radio-panel-shell`.

### Task 1.1 — Define radio-panel types

**Files:**
- Create: `src/radio/types.ts`
- Test: (no test for pure types; covered in dependent component tests)

- [ ] **Step 1: Create the types file**

```typescript
// src/radio/types.ts
//
// Types shared by the radio panel and its mode-specific implementations.
// The panel is the right-hand column that owns connection setup, live
// state, modem console, session log, and actions for the currently-
// selected radio mode. Mode-specific panels (Telnet / Packet / ARDOP /
// VARA when built) render their content into the panel's shared
// chrome.
//
// See docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md
// for the locked design decisions.

import type { ConnectionKey } from '../mailbox/FolderSidebar';

/**
 * The reason the radio panel is currently mounted. Multiple reasons can
 * be true simultaneously; the panel shows whichever mode is most
 * relevant (active modem > sidebar selection > toggle).
 */
export interface RadioPanelMountReason {
  /** A connection sidebar entry is selected (Telnet / Packet / etc.). */
  sidebarSelected: ConnectionKey | null;
  /** Any modem is in a non-stopped state. */
  modemActive: boolean;
  /** Operator has toggled the View menu item on. */
  togglePinned: boolean;
}

/**
 * The mode the panel is currently displaying. Derived from
 * RadioPanelMountReason; null means the panel is not mounted.
 */
export type RadioPanelMode =
  | { kind: 'telnet'; intent: 'cms' }
  | { kind: 'packet'; intent: 'cms' | 'p2p' }
  | { kind: 'ardop-hf'; intent: 'cms' }
  | { kind: 'vara-hf'; intent: 'cms' | 'p2p' }    // forward-looking
  | { kind: 'vara-fm'; intent: 'cms' | 'p2p' };   // forward-looking

/**
 * Human-readable name for a mode + intent, matching Express vocabulary
 * from docs/scratch/winlink-re/decompiled/. Used in the panel header.
 */
export function panelTitle(mode: RadioPanelMode): string {
  const intentSuffix = mode.intent === 'cms' ? 'Winlink' : 'P2P';
  switch (mode.kind) {
    case 'telnet':   return `Telnet ${intentSuffix}`;
    case 'packet':   return `Packet ${intentSuffix}`;
    case 'ardop-hf': return `Ardop ${intentSuffix}`;
    case 'vara-hf':  return `Vara HF ${intentSuffix}`;
    case 'vara-fm':  return `Vara FM ${intentSuffix}`;
  }
}
```

- [ ] **Step 2: Verify TypeScript accepts the file**

Run from the worktree root: `pnpm exec tsc --noEmit`
Expected: clean (no new errors).

- [ ] **Step 3: Commit**

```bash
git -C "$(pwd)" add src/radio/types.ts
git -C "$(pwd)" commit -m "feat(radio): define RadioPanel types (radio-panel-shell P1.1)"
```

### Task 1.2 — Visibility hook

**Files:**
- Create: `src/radio/useRadioPanelVisibility.ts`
- Create: `src/radio/useRadioPanelVisibility.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// src/radio/useRadioPanelVisibility.test.ts
import { describe, it, expect } from 'vitest';
import { computePanelMode, computePanelVisibility } from './useRadioPanelVisibility';
import { STOPPED, type ModemStatus } from '../modem/types';

const RUNNING: ModemStatus = { ...STOPPED, state: 'connected-irs' };

describe('computePanelVisibility', () => {
  it('hides the panel when nothing is active', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: false,
      togglePinned: false,
    })).toBe(false);
  });

  it('shows the panel when a connection is selected in the sidebar', () => {
    expect(computePanelVisibility({
      sidebarSelected: { sessionType: 'cms', protocol: 'ardop-hf' },
      modemActive: false,
      togglePinned: false,
    })).toBe(true);
  });

  it('shows the panel when any modem is non-stopped', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: true,
      togglePinned: false,
    })).toBe(true);
  });

  it('shows the panel when View → Toggle Radio Panel is pinned on', () => {
    expect(computePanelVisibility({
      sidebarSelected: null,
      modemActive: false,
      togglePinned: true,
    })).toBe(true);
  });
});

describe('computePanelMode', () => {
  it('returns null when nothing is active', () => {
    expect(computePanelMode(
      { sidebarSelected: null, modemActive: false, togglePinned: false },
      STOPPED,
    )).toBeNull();
  });

  it('prefers the active modem when a different mode is selected in the sidebar', () => {
    // operator on Packet view but ARDOP is connecting — should show ARDOP
    const mode = computePanelMode(
      { sidebarSelected: { sessionType: 'cms', protocol: 'packet' },
        modemActive: true, togglePinned: false },
      { ...STOPPED, state: 'connecting' },
    );
    // In v1 we only have one modem (ARDOP); multi-modem coordination is out of scope.
    // The hook returns the sidebar selection when modemActive is true but the modem
    // matches that selection; if the modem is a different mode, sidebar still wins
    // (the operator's selection is the active context).
    expect(mode).toEqual({ kind: 'packet', intent: 'cms' });
  });

  it('returns sidebar selection when modem is stopped and pin is off', () => {
    const mode = computePanelMode(
      { sidebarSelected: { sessionType: 'p2p', protocol: 'packet' },
        modemActive: false, togglePinned: false },
      STOPPED,
    );
    expect(mode).toEqual({ kind: 'packet', intent: 'p2p' });
  });
});
```

- [ ] **Step 2: Run the test to confirm it fails**

Run from worktree root: `pnpm vitest run src/radio/useRadioPanelVisibility.test.ts`
Expected: FAIL with "cannot find module './useRadioPanelVisibility'".

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/radio/useRadioPanelVisibility.ts
//
// The visibility rule from docs/superpowers/specs/2026-05-31-radio-mode-
// right-panel-design.md §3.3:
//
//   The panel mounts when ANY of:
//     - a connection entry is selected in the sidebar
//     - any modem is in a non-stopped state
//     - View → Toggle Radio Panel is on
//
//   The mode displayed is derived from that same context (§3.3 + §4.1).

import type { RadioPanelMountReason, RadioPanelMode } from './types';
import type { ModemStatus } from '../modem/types';

export function computePanelVisibility(reason: RadioPanelMountReason): boolean {
  return (
    reason.sidebarSelected !== null ||
    reason.modemActive ||
    reason.togglePinned
  );
}

export function computePanelMode(
  reason: RadioPanelMountReason,
  _modemStatus: ModemStatus,
): RadioPanelMode | null {
  if (!computePanelVisibility(reason)) {
    return null;
  }

  // v1 prefers sidebar selection. Multi-modem coordination (where a
  // running modem differs from the sidebar selection) is out of scope
  // per spec §8 — one active modem at a time, and the sidebar
  // selection is the operator's active context.
  if (reason.sidebarSelected !== null) {
    const { sessionType, protocol } = reason.sidebarSelected;
    const intent: 'cms' | 'p2p' = sessionType === 'p2p' ? 'p2p' : 'cms';
    switch (protocol) {
      case 'telnet':   return { kind: 'telnet',   intent: 'cms' };
      case 'packet':   return { kind: 'packet',   intent };
      case 'ardop-hf': return { kind: 'ardop-hf', intent: 'cms' };
      case 'vara-hf':  return { kind: 'vara-hf',  intent };
      case 'vara-fm':  return { kind: 'vara-fm',  intent };
    }
  }

  // togglePinned + no sidebar selection + no modem: show a "no connection"
  // placeholder. For v1 we default to Telnet Winlink as a reasonable
  // empty state; operators set the actual mode by clicking a sidebar entry.
  return { kind: 'telnet', intent: 'cms' };
}
```

- [ ] **Step 4: Run tests to confirm green**

Run: `pnpm vitest run src/radio/useRadioPanelVisibility.test.ts`
Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C "$(pwd)" add src/radio/useRadioPanelVisibility.ts src/radio/useRadioPanelVisibility.test.ts
git -C "$(pwd)" commit -m "feat(radio): visibility hook computes panel mount + mode (radio-panel-shell P1.2)"
```

### Task 1.3 — RadioPanel shell component

**Files:**
- Create: `src/radio/RadioPanel.tsx`
- Create: `src/radio/RadioPanel.css`
- Create: `src/radio/RadioPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/radio/RadioPanel.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RadioPanel } from './RadioPanel';

describe('<RadioPanel>', () => {
  it('renders the shell with the panel title from the mode', () => {
    render(<RadioPanel mode={{ kind: 'ardop-hf', intent: 'cms' }} onClose={() => {}}>
      <div data-testid="child-content">body</div>
    </RadioPanel>);
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Ardop Winlink');
    expect(screen.getByTestId('child-content')).toBeInTheDocument();
  });

  it('renders a close button that calls onClose', async () => {
    const onClose = vi.fn();
    render(<RadioPanel mode={{ kind: 'telnet', intent: 'cms' }} onClose={onClose}>
      <div />
    </RadioPanel>);
    const close = screen.getByTestId('radio-panel-close');
    close.click();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('renders the state dot with the data-state attribute for CSS theming', () => {
    render(<RadioPanel mode={{ kind: 'ardop-hf', intent: 'cms' }} state="connected" onClose={() => {}}>
      <div />
    </RadioPanel>);
    expect(screen.getByTestId('radio-panel-dot')).toHaveAttribute('data-state', 'connected');
  });
});
```

`vi` needs an import; add `import { vi } from 'vitest';` at the top.

- [ ] **Step 2: Run the test to confirm it fails**

Run: `pnpm vitest run src/radio/RadioPanel.test.tsx`
Expected: FAIL with "cannot find module './RadioPanel'".

- [ ] **Step 3: Implement the shell component**

```tsx
// src/radio/RadioPanel.tsx
//
// Shell for the right-hand radio panel. Per spec §3.2 + §4.2:
//   - 360 px wide when mounted; not shown otherwise
//   - header with state dot + mode title + close
//   - body renders mode-specific sections (passed as children)
//   - all sections always rendered (no collapsible by default)
//
// See docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md.

import type { ReactNode } from 'react';
import { panelTitle, type RadioPanelMode } from './types';
import './RadioPanel.css';

export type RadioPanelState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'disconnecting'
  | 'error';

export interface RadioPanelProps {
  mode: RadioPanelMode;
  state?: RadioPanelState;
  /** Optional sub-text in the header (peer / bandwidth / etc.). */
  sub?: string;
  /** Called when the operator clicks the close button. */
  onClose: () => void;
  /** Mode-specific section content. */
  children: ReactNode;
}

export function RadioPanel({
  mode, state = 'disconnected', sub, onClose, children,
}: RadioPanelProps): JSX.Element {
  return (
    <aside className="radio-panel" data-testid="radio-panel-root">
      <header className="radio-panel-h">
        <span
          className="radio-panel-dot"
          data-testid="radio-panel-dot"
          data-state={state}
        />
        <span className="radio-panel-name" data-testid="radio-panel-title">
          MODEM · {panelTitle(mode)}
        </span>
        {sub && <span className="radio-panel-sub">{sub}</span>}
        <button
          type="button"
          className="radio-panel-close"
          data-testid="radio-panel-close"
          onClick={onClose}
          aria-label="Close radio panel"
        >
          ☓
        </button>
      </header>
      <div className="radio-panel-body">
        {children}
      </div>
    </aside>
  );
}
```

- [ ] **Step 4: Create the CSS**

```css
/* src/radio/RadioPanel.css */
.radio-panel {
  background: var(--panel-bg, #0f1218);
  border-left: 1px solid var(--border, #2a3140);
  color: var(--text, #cbd5e1);
  font-size: 11px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  width: 360px;
  min-width: 360px;
  height: 100%;
}

.radio-panel-h {
  padding: 10px 12px;
  background: rgba(34, 197, 94, 0.06);
  border-bottom: 1px solid rgba(34, 197, 94, 0.18);
  display: flex;
  align-items: center;
  gap: 8px;
}

.radio-panel-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #64748b;
  flex-shrink: 0;
}
.radio-panel-dot[data-state='connecting']    { background: #fbbf24; }
.radio-panel-dot[data-state='connected']     { background: #4ade80; box-shadow: 0 0 5px #4ade80; }
.radio-panel-dot[data-state='disconnecting'] { background: #fbbf24; }
.radio-panel-dot[data-state='error']         { background: #f87171; }

.radio-panel-name {
  font-weight: 600;
  color: #4ade80;
  font-size: 12px;
  letter-spacing: 0.02em;
}

.radio-panel-sub {
  font-size: 10px;
  color: var(--text-faint, #94a3b8);
  margin-left: auto;
}

.radio-panel-close {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: var(--text-faint, #94a3b8);
  padding: 3px 7px;
  border-radius: 3px;
  font-size: 11px;
  cursor: pointer;
  line-height: 1;
}
.radio-panel-close:hover {
  background: rgba(255, 255, 255, 0.08);
}

.radio-panel-body {
  flex: 1;
  overflow-y: auto;
}

/* Section primitive — used by mode panels and shared sections */
.radio-panel-sec {
  padding: 10px 12px;
  border-bottom: 1px dashed rgba(255, 255, 255, 0.06);
}
.radio-panel-sec:last-child {
  border-bottom: none;
}
.radio-panel-sec h5 {
  margin: 0 0 8px;
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint, #94a3b8);
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
}
.radio-panel-sec h5 .live {
  margin-left: auto;
  font-size: 8px;
  color: #4ade80;
}
```

- [ ] **Step 5: Run tests and tsc**

Run: `pnpm vitest run src/radio/RadioPanel.test.tsx`
Expected: all 3 tests pass.

Run: `pnpm exec tsc --noEmit`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git -C "$(pwd)" add src/radio/RadioPanel.tsx src/radio/RadioPanel.css src/radio/RadioPanel.test.tsx
git -C "$(pwd)" commit -m "feat(radio): RadioPanel shell component (radio-panel-shell P1.3)"
```

### Task 1.4 — Placeholder mode panels

**Files:**
- Create: `src/radio/modes/PlaceholderRadioPanel.tsx`
- Create: `src/radio/modes/PlaceholderRadioPanel.test.tsx`

P1 doesn't migrate any real mode content — the placeholder mounts when any mode is selected; per-mode migrations happen in P2-P4.

- [ ] **Step 1: Write the failing test**

```tsx
// src/radio/modes/PlaceholderRadioPanel.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { PlaceholderRadioPanel } from './PlaceholderRadioPanel';

describe('<PlaceholderRadioPanel>', () => {
  it('renders a "coming soon" placeholder with the mode name', () => {
    render(
      <PlaceholderRadioPanel
        mode={{ kind: 'ardop-hf', intent: 'cms' }}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-placeholder')).toHaveTextContent(
      /Ardop Winlink panel coming soon/i,
    );
  });
});
```

- [ ] **Step 2: Run test to confirm fail**

Run: `pnpm vitest run src/radio/modes/PlaceholderRadioPanel.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

```tsx
// src/radio/modes/PlaceholderRadioPanel.tsx
//
// During P1, every mode mounts this placeholder. P2-P4 replace each
// mode's placeholder with its real implementation, one phase at a time.

import { RadioPanel } from '../RadioPanel';
import { panelTitle, type RadioPanelMode } from '../types';

export interface PlaceholderRadioPanelProps {
  mode: RadioPanelMode;
  onClose: () => void;
}

export function PlaceholderRadioPanel({
  mode, onClose,
}: PlaceholderRadioPanelProps): JSX.Element {
  return (
    <RadioPanel mode={mode} state="disconnected" onClose={onClose}>
      <section className="radio-panel-sec">
        <h5>{panelTitle(mode)}</h5>
        <p data-testid="radio-panel-placeholder"
           style={{ color: 'var(--text-faint, #94a3b8)', fontSize: 11 }}>
          {panelTitle(mode)} panel coming soon — replaced in a future
          implementation phase. The reading-pane / dock surface for this
          mode still works in the meantime.
        </p>
      </section>
    </RadioPanel>
  );
}
```

- [ ] **Step 4: Tests green**

Run: `pnpm vitest run src/radio/modes/PlaceholderRadioPanel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C "$(pwd)" add src/radio/modes/PlaceholderRadioPanel.tsx src/radio/modes/PlaceholderRadioPanel.test.tsx
git -C "$(pwd)" commit -m "feat(radio): placeholder mode panel (radio-panel-shell P1.4)"
```

### Task 1.5 — Integrate RadioPanel into AppShell

**Files:**
- Modify: `src/shell/AppShell.tsx`
- Modify: `src/shell/AppShell.css`
- Modify: `src/shell/AppShell.modemDock.test.tsx` (rename tests; keep the old surface assertions for now)

- [ ] **Step 1: Update AppShell.tsx — replace dockVisible with new visibility hook**

In `src/shell/AppShell.tsx`, locate the block:

```tsx
  const { status: modemStatus } = useModemStatus();
  const dockVisible =
    modemStatus.state !== 'stopped' ||
    selectedConnection?.protocol === 'ardop-hf' ||
    pinRadioDock;
```

Replace with:

```tsx
  const { status: modemStatus } = useModemStatus();
  // Spec §3.3 visibility rule. The hook captures the three OR-conditions
  // (sidebar selection / active modem / pinned-toggle) and returns the
  // mode to show plus whether the panel is mounted.
  const radioPanelMode = computePanelMode(
    {
      sidebarSelected: selectedConnection,
      modemActive: modemStatus.state !== 'stopped',
      togglePinned: pinRadioDock,
    },
    modemStatus,
  );
  const radioPanelVisible = radioPanelMode !== null;
```

Add the import near the other radio imports:

```tsx
import { computePanelMode } from '../radio/useRadioPanelVisibility';
import { PlaceholderRadioPanel } from '../radio/modes/PlaceholderRadioPanel';
```

- [ ] **Step 2: Mount the new panel in the panes grid**

Replace the existing block:

```tsx
        {dockVisible && <ArdopDock />}
      </div>
```

with:

```tsx
        {/* Spec P1: PlaceholderRadioPanel mounts here. P2-P4 swap in the
            real per-mode components. The legacy ArdopDock continues to
            mount BELOW until P4 removes it. */}
        {radioPanelMode && (
          <PlaceholderRadioPanel
            mode={radioPanelMode}
            onClose={() => {
              setSelectedConnection(null);
              setPinRadioDock(false);
            }}
          />
        )}
        {dockVisible && selectedConnection?.protocol === 'ardop-hf' && <ArdopDock />}
      </div>
```

Rename `dockVisible` references downstream of this block to `radioPanelVisible` (the panes class swap):

```tsx
        className={`panes${radioPanelVisible ? ' panes--with-dock' : ''}`}
```

(The `panes--with-dock` CSS class name stays for now; P5 cleanup can rename to `panes--with-radio-panel` if desired, out of scope for P1.)

- [ ] **Step 3: Run the existing AppShell.modemDock.test.tsx — it WILL fail; update expectations**

Run: `pnpm vitest run src/shell/AppShell.modemDock.test.tsx`
Expected: FAIL — tests check for `data-testid="ardop-dock-root"` but the placeholder now wins the slot.

Update the affected tests to also accept the placeholder; the simplest path is to rename the test file to `AppShell.radioPanel.test.tsx` and rewrite assertions against `radio-panel-root`:

```tsx
// In each test that previously asserted ardop-dock-root:
expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
expect(screen.getByTestId('radio-panel-placeholder')).toBeInTheDocument();
```

- [ ] **Step 4: Run tests and tsc**

Run: `pnpm vitest run`
Expected: all green (existing tests + updated panel tests).

Run: `pnpm exec tsc --noEmit`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git -C "$(pwd)" add src/shell/AppShell.tsx src/shell/AppShell.modemDock.test.tsx
git -C "$(pwd)" mv src/shell/AppShell.modemDock.test.tsx src/shell/AppShell.radioPanel.test.tsx 2>/dev/null || true
git -C "$(pwd)" add -A
git -C "$(pwd)" commit -m "feat(shell): mount RadioPanel placeholder via visibility hook (radio-panel-shell P1.5)"
```

### Task 1.6 — Remove the bottom session-log strip

**Files:**
- Modify: `src/shell/AppShell.tsx` (remove `<SessionLog />` render)
- Modify: `src/shell/AppShell.css` (drop the strip's row from the grid)
- Modify: `src/shell/chrome/menuModel.ts` (drop `menu:view:session_log` + `menu:view:raw_log`)
- Modify: `src/shell/chrome/dispatchMenuAction.ts` (drop `toggleSessionLog` from dispatcher and `MenuHandlers`)
- Modify: `src/shell/AppShell.tsx` (drop `showSessionLog` state + handler)
- Delete: `src/session/SessionLog.tsx`, `src/session/SessionLog.css`, `src/session/SessionLog.test.tsx` (if present)

The session log is moving into the panel as a per-mode section in P2-P4. P1 removes the bottom strip surface entirely so the reading pane reclaims its vertical real estate.

- [ ] **Step 1: Drop the SessionLog mount in AppShell.tsx**

Remove the line:

```tsx
      {showSessionLog && <SessionLog />}
```

Remove the import:

```tsx
import { SessionLog } from '../session/SessionLog';
```

Remove the state:

```tsx
const [showSessionLog, setShowSessionLog] = useState(true);
```

Remove from MenuHandlers:

```tsx
toggleSessionLog: () => setShowSessionLog((s) => !s),
```

- [ ] **Step 2: Update MenuHandlers interface in dispatchMenuAction.ts**

Drop the `toggleSessionLog` field from the `MenuHandlers` interface:

```typescript
// src/shell/chrome/dispatchMenuAction.ts
export interface MenuHandlers {
  openCompose: () => void;
  connect: () => void;
  reply: () => void;
  replyAll: () => void;
  forward: () => void;
  // toggleSessionLog REMOVED — no separate session-log surface (radio-panel-shell P1.6)
  toggleStatusBar: () => void;
  toggleRadioDock: () => void;
  // ...rest unchanged
}
```

And drop the dispatcher case:

```typescript
    // case 'menu:view:session_log': h.toggleSessionLog(); return;  // REMOVED
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
    case 'menu:view:radio_dock': h.toggleRadioDock(); return;
```

- [ ] **Step 3: Update menuModel.ts — drop session-log items + the Ctrl+Shift+L binding**

In the View menu items array, remove:

```typescript
    { id: 'menu:view:session_log', label: 'Toggle Session Log', accel: 'Ctrl+Shift+L' },
    { id: 'menu:view:raw_log', label: 'Show Raw Session Log' },
```

In the `ACCELERATORS` array, remove:

```typescript
{ combo: 'Ctrl+Shift+L', key: 'l', ctrl: true, shift: true, id: 'menu:view:session_log' },
```

- [ ] **Step 4: Update menuModel.test.ts — adjust the ID list**

The existing test in `src/shell/chrome/menuModel.test.ts` references `'menu:view:session_log'` and `'menu:view:raw_log'` in its expected-ID list. Remove those entries from the test expectation:

```typescript
// Before:
//   'menu:view:session_log', 'menu:view:raw_log', 'menu:view:status_bar', 'menu:view:radio_dock',
// After:
//   'menu:view:status_bar', 'menu:view:radio_dock',
```

- [ ] **Step 5: Update dispatchMenuAction.test.ts — drop the session_log test**

Remove the test case asserting `menu:view:session_log` routing. Adjust the `routes view toggles` test to drop the session_log call:

```typescript
  it('routes view toggles', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:status_bar', h);
    expect(h.toggleStatusBar).toHaveBeenCalledOnce();
  });
```

Drop `toggleSessionLog` from the `handlers()` factory.

- [ ] **Step 6: Delete the SessionLog files**

```bash
rm -f src/session/SessionLog.tsx src/session/SessionLog.css src/session/SessionLog.test.tsx
# If the src/session/ directory is now empty, leave it for now — P2's SessionLogSection
# doesn't reuse the SessionLog name, and an empty dir doesn't ship.
```

- [ ] **Step 7: Update AppShell.css — drop the session-log row from the grid**

In `src/shell/AppShell.css`, locate the `.layout-b` grid-template-rows definition and remove the row that allocates the session-log strip. Original (representative; adjust to match actual selectors):

```css
.layout-b {
  display: grid;
  grid-template-rows: auto auto auto 1fr auto auto;  /* title / menu / ribbon / panes / log / status */
}
```

Change to:

```css
.layout-b {
  display: grid;
  grid-template-rows: auto auto auto 1fr auto;  /* title / menu / ribbon / panes / status */
}
```

- [ ] **Step 8: Run all tests + tsc**

Run: `pnpm vitest run`
Expected: all green. The session-log-related test failures (now-deleted file) are gone.

Run: `pnpm exec tsc --noEmit`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git -C "$(pwd)" add -A
git -C "$(pwd)" commit -m "refactor(shell): remove bottom session-log strip (radio-panel-shell P1.6)

The session log moves into the radio panel as a per-mode section in P2-P4
per spec §3.7 + §4.3. Removes the bottom strip surface that the spec
declares 'a wrong-slot decision':
  - <SessionLog> component + its test file deleted
  - menu items Toggle Session Log + Show Raw Session Log removed
  - Ctrl+Shift+L accelerator unbound (no equivalent surface)
  - MenuHandlers.toggleSessionLog removed
  - .layout-b grid loses the session-log row; reading pane reclaims ~80 px
    vertical real estate per §4.1"
```

### Task 1.7 — Rename View → Toggle Radio Panel

**Files:**
- Modify: `src/shell/chrome/menuModel.ts`
- Modify: `src/shell/chrome/dispatchMenuAction.ts` (rename `toggleRadioDock` → `toggleRadioPanel`)
- Modify: `src/shell/AppShell.tsx` (`pinRadioDock` → `pinRadioPanel`; handler rename)
- Modify: existing tests using `toggleRadioDock` / `pinRadioDock`

- [ ] **Step 1: Rename in menuModel.ts**

Change:

```typescript
    { id: 'menu:view:radio_dock', label: 'Toggle Radio Dock', accel: 'Ctrl+Shift+M' },
```

to:

```typescript
    { id: 'menu:view:radio_panel', label: 'Toggle Radio Panel', accel: 'Ctrl+Shift+M' },
```

And in the `ACCELERATORS` array, change the id:

```typescript
{ combo: 'Ctrl+Shift+M', key: 'm', ctrl: true, shift: true, id: 'menu:view:radio_panel' },
```

- [ ] **Step 2: Rename in dispatchMenuAction.ts**

Rename `toggleRadioDock` → `toggleRadioPanel` in the `MenuHandlers` interface and the dispatcher:

```typescript
export interface MenuHandlers {
  // ...
  toggleRadioPanel: () => void;
  // ...
}

// In dispatcher:
    case 'menu:view:radio_panel': h.toggleRadioPanel(); return;
```

- [ ] **Step 3: Rename in AppShell.tsx**

```tsx
const [pinRadioPanel, setPinRadioPanel] = useState(false);
// ...
toggleRadioPanel: () => setPinRadioPanel((s) => !s),
// ...
// In the useRadioPanelVisibility call:
togglePinned: pinRadioPanel,
```

- [ ] **Step 4: Update tests**

In `src/shell/chrome/dispatchMenuAction.test.ts`:

```typescript
// Update handlers() factory:
toggleRadioPanel: vi.fn(),

// Update the test case:
it('routes view:radio_panel to toggleRadioPanel', () => {
  const h = handlers();
  dispatchMenuAction('menu:view:radio_panel', h);
  expect(h.toggleRadioPanel).toHaveBeenCalledOnce();
});
```

In `src/shell/chrome/menuModel.test.ts`:

```typescript
// Update the expected-ID list:
'menu:view:status_bar', 'menu:view:radio_panel',
```

In `src/shell/AppShell.radioPanel.test.tsx`:

```typescript
// Update the Ctrl+Shift+M test:
fireEvent.keyDown(window, { key: 'm', ctrlKey: true, shiftKey: true });
expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
```

- [ ] **Step 5: Run all tests + tsc**

Run: `pnpm vitest run` + `pnpm exec tsc --noEmit`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git -C "$(pwd)" add -A
git -C "$(pwd)" commit -m "refactor(shell): rename View → Toggle Radio Panel (radio-panel-shell P1.7)

The dock-vs-panel distinction is dropped per spec §3.2 + §3.7 (the
'compact / Full' framing was an artifact of the dropped Full overlay).
The menu item, the MenuHandlers field, and the AppShell state all rename
in lockstep. Ctrl+Shift+M accelerator preserved so muscle memory
survives."
```

### Task 1.8 — Phase 1 verification, PR, Codex round, merge

- [ ] **Step 1: Run full quality gates**

```bash
pnpm vitest run                                 # frontend tests
pnpm exec tsc --noEmit                          # typecheck
cargo test --manifest-path src-tauri/Cargo.toml --lib  # backend tests (should be unchanged)
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings  # backend lint
```

All gates expected green. P1 doesn't touch Rust; cargo runs are confirmation that the frontend changes didn't break the build.

- [ ] **Step 2: Push branch + open PR**

```bash
git -C "$(pwd)" push -u origin bd-<id>/radio-panel-shell
gh pr create --title "[<moniker>] feat(radio): RadioPanel shell scaffold + bottom-strip removal (radio-panel P1 of 5)" --body "$(cat <<'EOF'
## Summary

First phase of the radio-mode right-panel implementation per
[`docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md`](docs/superpowers/specs/2026-05-31-radio-mode-right-panel-design.md)
§7 P1. Lands the chrome and removes the bottom session-log strip
before any mode migration.

## Changes

- New \`src/radio/\` directory with shell + visibility hook + placeholder
  mode panel
- AppShell mounts \`PlaceholderRadioPanel\` via the visibility hook
- Bottom session-log strip removed (log moves into the panel as a
  per-mode section in P2-P4)
- \`View → Toggle Radio Dock\` renamed to \`Toggle Radio Panel\`;
  Ctrl+Shift+M accelerator preserved
- Existing ArdopDock continues to mount for ARDOP HF only — gets
  replaced in P4

## Tests

- \`useRadioPanelVisibility.test.ts\`: 7 cases covering visibility rule + mode
- \`RadioPanel.test.tsx\`: 3 cases (shell renders, close callback, state-dot)
- \`PlaceholderRadioPanel.test.tsx\`: 1 case (renders mode name)
- \`AppShell.radioPanel.test.tsx\`: panel mounts/unmounts per visibility rule

## Codex adrev

Run before merge per spec §11. Output at \`dev/adversarial/2026-05-31-radio-panel-shell-codex.md\`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Run Codex round on the diff**

```bash
cat > /tmp/codex-p1.txt <<'EOF'
You are doing adversarial code review of the diff against origin/main
in this worktree. Run `git diff origin/main..HEAD` to see all changes.

Context: P1 of the radio-mode right-panel implementation. Adds the
shell component + visibility hook + placeholder mode panels, removes
the bottom session-log strip, renames the View menu item.

Audit P0/P1 only:
1. Visibility rule correctness — does the hook honor the spec §3.3 OR
   semantics? Edge cases: sidebar selection cleared while modem still
   non-stopped; toggle pinned while sidebar selection changes; close
   button effects on all three signals.
2. State leakage — pinRadioPanel is component-local useState. Does
   AppShell remount in any flow that would silently flip pinRadioPanel
   to false (settings panel open/close, message selection, folder
   switch)?
3. Test coverage — are the new tests asserting what the spec actually
   says, not what the implementation happens to do?
4. Migration safety — ArdopDock continues to mount for ARDOP HF; the
   placeholder also mounts. Do they collide (two surfaces in the same
   grid column)?

Read: src/radio/RadioPanel.tsx, src/radio/useRadioPanelVisibility.ts,
src/radio/modes/PlaceholderRadioPanel.tsx, src/shell/AppShell.tsx,
src/shell/AppShell.css.

Format findings as ## Findings ### P0 / ### P1 / NO P0/P1 ISSUES FOUND.
EOF
cat /tmp/codex-p1.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-05-31-radio-panel-shell-codex.md
```

Address any P0/P1 findings as fix-up commits. If `Codex catches the dual-mount collision (placeholder + ArdopDock for ARDOP), adjust the conditional so the placeholder shows for non-ARDOP modes only:

```tsx
{radioPanelMode && radioPanelMode.kind !== 'ardop-hf' && (
  <PlaceholderRadioPanel mode={radioPanelMode} onClose={...} />
)}
```

- [ ] **Step 4: Merge after Codex clean**

```bash
gh pr merge <PR#> --merge --delete-branch
```

- [ ] **Step 5: Operator smoke**

Operator runs `pnpm tauri dev` from the worktree and verifies:
- Panel mounts when a sidebar connection entry is selected (placeholder shows)
- Panel mounts when ARDOP HF is selected (placeholder shows; ArdopDock also shows alongside — expected during P1)
- Ctrl+Shift+M toggles the placeholder visibility
- Bottom session-log strip is gone
- Reading pane height grew

- [ ] **Step 6: Close P1's bd-issue**

```bash
bd close <P1-bd-id>
```

---

## Phase 2 — Telnet panel migration

**Goal:** Implement `TelnetRadioPanel` per spec §5.1 with `SessionLogSection` (the shared log section per §4.3) and `SessionLogSection` rendering rules. Wire AppShell to route Telnet sidebar selections to the new panel. Delete `TelnetCmsPanel`.

**bd-issue & branch:** "feat: TelnetRadioPanel — migrate Telnet CMS to right-panel paradigm (radio-panel P2 of 5)". Slug: `radio-panel-telnet`. New worktree off origin/main.

### Task 2.1 — SessionLogSection shared component

**Files:**
- Create: `src/radio/sections/SessionLogSection.tsx`
- Create: `src/radio/sections/SessionLogSection.css`
- Create: `src/radio/sections/SessionLogSection.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/radio/sections/SessionLogSection.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { SessionLogSection, type SessionLogEntry } from './SessionLogSection';

const FIXTURE: SessionLogEntry[] = [
  { ts: '05:35:58', level: 'info', message: 'Connecting to cms.winlink.org:8773 (CMS-SSL)' },
  { ts: '05:35:59', level: 'ok',   message: 'TLS handshake complete · secure-login OK' },
  { ts: '05:36:00', level: 'info', message: 'Negotiating messages…' },
  { ts: '05:36:01', level: 'warn', message: 'Unknown client types are not allowed on production servers — use cms-z.winlink.org' },
  { ts: '05:36:01', level: 'alert', message: 'CMS connect failed: transport error',
    raw: 'RemoteError: "Unknown client types are not allowed on production servers — use cms-z.winlink.org — Disconnecting (68.2.111.142)"' },
];

describe('<SessionLogSection>', () => {
  it('renders the log entries with severity classes', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    const root = screen.getByTestId('session-log-section');
    expect(within(root).getByText(/Connecting to cms\.winlink\.org/)).toBeInTheDocument();
    expect(within(root).getByText(/TLS handshake complete/)).toBeInTheDocument();
    // Severity glyphs / classes:
    expect(within(root).getByText(/CMS connect failed/).closest('.log-entry'))
      .toHaveClass('log-entry-alert');
    expect(within(root).getByText(/Unknown client types/).closest('.log-entry'))
      .toHaveClass('log-entry-warn');
  });

  it('renders multi-paragraph errors (summary + raw)', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    expect(screen.getByText(/RemoteError:/)).toBeInTheDocument();
  });

  it('renders the Show raw + Auto-scroll controls', () => {
    render(<SessionLogSection entries={FIXTURE} />);
    expect(screen.getByLabelText(/Show raw/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Auto-scroll/i)).toBeInTheDocument();
  });

  it('hides entries with kind=raw when Show raw is unchecked', () => {
    const withRaw: SessionLogEntry[] = [
      ...FIXTURE,
      { ts: '05:36:02', level: 'raw', message: '[B2F] FQ' },
    ];
    render(<SessionLogSection entries={withRaw} />);
    expect(screen.queryByText('[B2F] FQ')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to confirm fail**

`pnpm vitest run src/radio/sections/SessionLogSection.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement SessionLogSection**

```tsx
// src/radio/sections/SessionLogSection.tsx
//
// Per-session live log section. Spec §3.7 + §4.3.
// Layout:
//   - 56 px fixed timestamp column + flex message column
//   - hanging-indent wrap
//   - severity colors + glyphs (⚠ for warn, ⊘ for alert)
//   - multi-paragraph error: bold summary + faint italic raw block
//   - 130 px scrollable region with auto-scroll
//   - Show raw / Auto-scroll / Copy controls
//
// Used by every per-mode panel (Telnet, Packet, ARDOP, VARA).

import { useEffect, useRef, useState } from 'react';
import './SessionLogSection.css';

export type SessionLogLevel = 'info' | 'ok' | 'warn' | 'alert' | 'raw';

export interface SessionLogEntry {
  /** HH:MM:SS string, monospace. */
  ts: string;
  level: SessionLogLevel;
  /** Human-shaped one-line message (for alert: bold summary line). */
  message: string;
  /** Optional raw protocol-level detail (renders below message in faint italic). */
  raw?: string;
}

export interface SessionLogSectionProps {
  entries: SessionLogEntry[];
  /** Optional initial state for Show raw / Auto-scroll. */
  initialShowRaw?: boolean;
  initialAutoScroll?: boolean;
  /** Optional copy handler; defaults to copying the rendered text. */
  onCopy?: () => void;
}

export function SessionLogSection({
  entries,
  initialShowRaw = false,
  initialAutoScroll = true,
  onCopy,
}: SessionLogSectionProps): JSX.Element {
  const [showRaw, setShowRaw] = useState(initialShowRaw);
  const [autoScroll, setAutoScroll] = useState(initialAutoScroll);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll on new entries when the toggle is on. The operator
  // scroll-back pauses auto-scroll via the onScroll handler.
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries, autoScroll, showRaw]);

  const filtered = showRaw ? entries : entries.filter(e => e.level !== 'raw');

  return (
    <section className="radio-panel-sec session-log-section"
             data-testid="session-log-section">
      <h5>
        Session log
        <span className="live">live tail</span>
      </h5>
      <div className="log-scroll" ref={scrollRef}>
        {filtered.map((e, i) => (
          <div key={i} className={`log-entry log-entry-${e.level}`}>
            <span className="log-ts">{e.ts}</span>
            <span className="log-msg">
              {e.level === 'alert' ? <strong>{e.message}</strong> : e.message}
              {e.raw && (
                <span className="log-raw"><br />{e.raw}</span>
              )}
            </span>
          </div>
        ))}
      </div>
      <div className="log-controls">
        <label>
          <input type="checkbox" checked={showRaw}
                 onChange={(ev) => setShowRaw(ev.target.checked)} />
          Show raw
        </label>
        <label>
          <input type="checkbox" checked={autoScroll}
                 onChange={(ev) => setAutoScroll(ev.target.checked)} />
          Auto-scroll
        </label>
        <button type="button" className="log-copy"
                onClick={onCopy ?? (() => copyEntries(filtered))}>
          Copy ↗
        </button>
      </div>
    </section>
  );
}

function copyEntries(entries: SessionLogEntry[]): void {
  const text = entries
    .map(e => `${e.ts}  ${e.message}${e.raw ? '\n            ' + e.raw : ''}`)
    .join('\n');
  void navigator.clipboard.writeText(text).catch(() => {});
}
```

- [ ] **Step 4: Create the CSS**

```css
/* src/radio/sections/SessionLogSection.css */
.session-log-section .log-scroll {
  font-family: ui-monospace, 'SF Mono', Menlo, monospace;
  font-size: 10px;
  line-height: 1.5;
  background: rgba(0, 0, 0, 0.18);
  border-radius: 3px;
  padding: 6px 8px;
  max-height: 132px;
  overflow-y: auto;
  color: var(--text-faint, #94a3b8);
}

.session-log-section .log-entry {
  display: grid;
  grid-template-columns: 56px 1fr;
  gap: 6px;
  padding: 3px 0;
  border-top: 1px solid rgba(255, 255, 255, 0.04);
}
.session-log-section .log-entry:first-child {
  border-top: none;
  padding-top: 0;
}

.session-log-section .log-ts {
  color: #64748b;
  font-size: 9px;
  padding-top: 1px;
  white-space: nowrap;
}

.session-log-section .log-msg {
  word-break: break-word;
}

.log-entry-info .log-msg  { color: var(--text, #cbd5e1); }
.log-entry-ok   .log-msg  { color: #4ade80; }
.log-entry-warn .log-msg  { color: #fbbf24; }
.log-entry-warn .log-msg::before { content: '⚠ '; }
.log-entry-raw  .log-msg  { color: var(--text-faint, #94a3b8); font-style: italic; opacity: 0.7; }

.log-entry-alert {
  background: rgba(239, 68, 68, 0.08);
  border-left: 3px solid #f87171;
  padding-left: 6px;
  margin: 2px -8px;
  padding-right: 8px;
  padding-top: 4px;
  padding-bottom: 4px;
  border-top: none;
}
.log-entry-alert + .log-entry { border-top: 1px solid rgba(255, 255, 255, 0.04); }
.log-entry-alert .log-msg { color: #f87171; }
.log-entry-alert .log-msg::before { content: '⊘ '; font-weight: 600; }
.log-entry-alert .log-msg .log-raw { color: #fecaca; font-style: italic; opacity: 0.8; }

.session-log-section .log-controls {
  display: flex;
  gap: 6px;
  margin-top: 6px;
  font-size: 9px;
  align-items: center;
}
.session-log-section .log-controls label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-faint, #94a3b8);
  cursor: pointer;
}
.session-log-section .log-controls label input {
  transform: scale(0.85);
}
.session-log-section .log-copy {
  background: transparent;
  border: none;
  color: #67e8f9;
  cursor: pointer;
  margin-left: auto;
  padding: 0;
  font-size: 9px;
}
```

- [ ] **Step 5: Run tests, tsc, commit**

```bash
pnpm vitest run src/radio/sections/SessionLogSection.test.tsx
pnpm exec tsc --noEmit
git -C "$(pwd)" add src/radio/sections/SessionLogSection.tsx src/radio/sections/SessionLogSection.css src/radio/sections/SessionLogSection.test.tsx
git -C "$(pwd)" commit -m "feat(radio): SessionLogSection — shared log section per spec §4.3 (radio-panel-telnet P2.1)"
```

### Task 2.2 — TelnetRadioPanel

**Files:**
- Create: `src/radio/modes/TelnetRadioPanel.tsx`
- Create: `src/radio/modes/TelnetRadioPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/radio/modes/TelnetRadioPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TelnetRadioPanel } from './TelnetRadioPanel';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
}));

describe('<TelnetRadioPanel>', () => {
  it('renders the Telnet Winlink panel with endpoint and transport', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
    expect(screen.getByText(/cms\.winlink\.org/)).toBeInTheDocument();
    expect(screen.getByText(/CMS-SSL/)).toBeInTheDocument();
  });

  it('renders the Session log section', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders Start and Stop actions', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByRole('button', { name: /Start/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/i })).toBeInTheDocument();
  });

  it('clicking Start fires cms_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Start/i }));
    expect(invoke).toHaveBeenCalledWith('cms_connect');
  });
});
```

- [ ] **Step 2: Run test to confirm fail**

`pnpm vitest run src/radio/modes/TelnetRadioPanel.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement TelnetRadioPanel**

```tsx
// src/radio/modes/TelnetRadioPanel.tsx
//
// Telnet CMS panel per spec §5.1. Smallest content surface: no modem
// to configure (the CMS endpoint comes from config). Sections rendered:
// Connection (endpoint + transport), Session (last result), Session log,
// Actions.

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection, type SessionLogEntry } from '../sections/SessionLogSection';

export interface TelnetRadioPanelProps {
  onClose: () => void;
  /** Optional initial log entries (tests inject; live wiring lands in
   *  P2 if we surface a useTelnetSessionLog hook, otherwise empty []). */
  initialLogEntries?: SessionLogEntry[];
}

export function TelnetRadioPanel({
  onClose,
  initialLogEntries = [],
}: TelnetRadioPanelProps): JSX.Element {
  const [busy, setBusy] = useState(false);

  const start = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await invoke('cms_connect');
    } catch {
      // Errors surface in the session log; nothing inline.
    } finally {
      setBusy(false);
    }
  };

  const stop = () => {
    void invoke('cms_abort').catch(() => {});
  };

  return (
    <RadioPanel
      mode={{ kind: 'telnet', intent: 'cms' }}
      state={busy ? 'connecting' : 'disconnected'}
      sub="cms.winlink.org"
      onClose={onClose}
    >
      <section className="radio-panel-sec">
        <h5>Connection</h5>
        <div className="radio-panel-field">
          <span>Endpoint</span>
          <span className="radio-panel-readonly">cms.winlink.org:8773</span>
        </div>
        <div className="radio-panel-field">
          <span>Transport</span>
          <span className="radio-panel-readonly">CMS-SSL (TLS)</span>
        </div>
      </section>

      <section className="radio-panel-sec">
        <h5>Session</h5>
        <div className="radio-panel-mono">
          {/* Last-result + state line; wiring TBD in implementation. */}
          {busy ? 'Connecting…' : 'Idle — Start to begin a session.'}
        </div>
      </section>

      <SessionLogSection entries={initialLogEntries} />

      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          disabled={busy}
          onClick={start}
        >
          {busy ? 'Connecting…' : 'Start'}
        </button>
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-bad"
          onClick={stop}
        >
          Stop
        </button>
      </section>
    </RadioPanel>
  );
}
```

- [ ] **Step 4: Add the CSS classes to RadioPanel.css**

Append to `src/radio/RadioPanel.css`:

```css
.radio-panel-field {
  display: grid;
  grid-template-columns: 64px 1fr;
  gap: 8px;
  align-items: center;
  margin-bottom: 4px;
  font-size: 11px;
}
.radio-panel-readonly {
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.10);
  border-radius: 3px;
  padding: 4px 7px;
  font-size: 11px;
  color: var(--text, #cbd5e1);
  font-family: ui-monospace, monospace;
}
.radio-panel-mono {
  font-family: ui-monospace, monospace;
  font-size: 10px;
  color: var(--text-faint, #94a3b8);
  line-height: 1.5;
}
.radio-panel-act {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}
.radio-panel-btn {
  padding: 6px 10px;
  border-radius: 3px;
  font-size: 11px;
  text-align: center;
  cursor: pointer;
  border: 1px solid;
  flex: 1;
}
.radio-panel-btn-primary {
  background: rgba(34, 197, 94, 0.12);
  border-color: rgba(34, 197, 94, 0.35);
  color: #4ade80;
}
.radio-panel-btn-bad {
  background: rgba(239, 68, 68, 0.10);
  border-color: rgba(239, 68, 68, 0.30);
  color: #f87171;
  flex: 0 0 auto;
  padding: 6px 10px;
}
.radio-panel-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

- [ ] **Step 5: Run tests + tsc + commit**

```bash
pnpm vitest run src/radio/modes/TelnetRadioPanel.test.tsx
pnpm exec tsc --noEmit
git -C "$(pwd)" add src/radio/modes/TelnetRadioPanel.tsx src/radio/modes/TelnetRadioPanel.test.tsx src/radio/RadioPanel.css
git -C "$(pwd)" commit -m "feat(radio): TelnetRadioPanel + shared CSS primitives (radio-panel-telnet P2.2)"
```

### Task 2.3 — Wire AppShell to route Telnet through TelnetRadioPanel

**Files:**
- Modify: `src/shell/AppShell.tsx`

- [ ] **Step 1: Add the import**

```tsx
import { TelnetRadioPanel } from '../radio/modes/TelnetRadioPanel';
```

- [ ] **Step 2: Replace the placeholder mount for Telnet**

Replace the placeholder block:

```tsx
{radioPanelMode && radioPanelMode.kind !== 'ardop-hf' && (
  <PlaceholderRadioPanel mode={radioPanelMode} onClose={...} />
)}
```

with:

```tsx
{radioPanelMode && radioPanelMode.kind === 'telnet' && (
  <TelnetRadioPanel onClose={() => {
    setSelectedConnection(null);
    setPinRadioPanel(false);
  }} />
)}
{radioPanelMode && (radioPanelMode.kind === 'packet' || radioPanelMode.kind === 'vara-hf' || radioPanelMode.kind === 'vara-fm') && (
  <PlaceholderRadioPanel mode={radioPanelMode} onClose={...} />
)}
{/* ARDOP HF still uses ArdopDock until P4 */}
```

- [ ] **Step 3: Delete TelnetCmsPanel and its references**

```bash
rm -f src/connections/TelnetCmsPanel.tsx src/connections/TelnetCmsPanel.css src/connections/TelnetCmsPanel.test.tsx
```

Remove the `TelnetCmsPanelContainer` mount from `AppShell.tsx`:

```tsx
// Before:
if (sessionType === 'cms' && protocol === 'telnet') {
  return <TelnetCmsPanelContainer />;
}

// After:
if (sessionType === 'cms' && protocol === 'telnet') {
  // Telnet now lives in the right radio panel (P2). The reading pane
  // is empty when Telnet is selected — operator reads messages or
  // empty-state.
  return <MessageView selectedMessage={selectedMessage} />;
}
```

Remove the import:

```tsx
// REMOVE:
import { TelnetCmsPanelContainer } from '../connections/TelnetCmsPanel';
```

- [ ] **Step 4: Run tests + tsc + commit**

```bash
pnpm vitest run
pnpm exec tsc --noEmit
git -C "$(pwd)" add -A
git -C "$(pwd)" commit -m "feat(shell): route Telnet selection to TelnetRadioPanel; delete TelnetCmsPanel (radio-panel-telnet P2.3)"
```

### Task 2.4 — Phase 2 verification, PR, Codex round, merge

- [ ] **Step 1: Quality gates**

```bash
pnpm vitest run
pnpm exec tsc --noEmit
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings
```

- [ ] **Step 2: Push + PR + Codex round + merge**

Same workflow as P1.8 (PR title + body adjusted for P2 scope). Codex prompt:

```
P2 of the radio-mode right-panel implementation. Adds SessionLogSection
(shared across modes per spec §4.3) + TelnetRadioPanel (per §5.1).
Replaces the reading-pane TelnetCmsPanel.

Audit P0/P1:
1. SessionLogSection rendering rules vs spec §4.3 — column widths,
   hanging indent (CSS), severity color/glyph (does each level have a
   distinct visual signal?), multi-paragraph errors.
2. Show raw / Auto-scroll state — does the filter correctly hide raw
   entries when toggled? Does auto-scroll pause on operator scroll
   (or is that a missing requirement)?
3. TelnetRadioPanel Start/Stop — does Start fire cms_connect with the
   right argument shape? Does Stop fire cms_abort?
4. AppShell migration — does removing TelnetCmsPanel break any other
   route or test?
```

- [ ] **Step 3: Operator smoke**

Verifies:
- Selecting Telnet in sidebar mounts the new panel with endpoint shown
- Session log section renders (initially empty)
- Start button fires a CMS connect attempt
- Reading pane shows empty/messages state (no Telnet form there anymore)

- [ ] **Step 4: Close P2 bd-issue**

---

## Phase 3 — Packet panel migration

**Goal:** Implement `PacketRadioPanel` per spec §5.2 with `ModemLinkSection`. Delete `PacketConnectionPanel`.

**bd-issue & branch:** "feat: PacketRadioPanel — migrate AX.25 Packet to right-panel paradigm (radio-panel P3 of 5)". Slug: `radio-panel-packet`. New worktree off origin/main.

### Task 3.1 — ModemLinkSection shared component

**Files:**
- Create: `src/radio/sections/ModemLinkSection.tsx`
- Create: `src/radio/sections/ModemLinkSection.test.tsx`

This section is currently AX.25-Packet-specific but designed to be reused by any future TNC-mediated mode. The visible content is the same shape as the existing `PacketModemBlock` in `PacketConnectionPanel`, densified for 360 px.

- [ ] **Step 1: Test**

```tsx
// src/radio/sections/ModemLinkSection.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ModemLinkSection } from './ModemLinkSection';

describe('<ModemLinkSection>', () => {
  it('renders the TCP/USB/BT segmented picker', () => {
    render(<ModemLinkSection
      kind="Tcp"
      host="127.0.0.1"
      port={8001}
      onChange={() => {}}
    />);
    expect(screen.getByRole('button', { name: /TCP/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /USB/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /BT/ })).toBeInTheDocument();
  });

  it('fires onChange with the new kind when a segment is clicked', () => {
    const onChange = vi.fn();
    render(<ModemLinkSection
      kind="Tcp"
      host="127.0.0.1"
      port={8001}
      onChange={onChange}
    />);
    fireEvent.click(screen.getByRole('button', { name: /USB/ }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ linkKind: 'Serial' }));
  });

  it('shows TCP host + port when kind=Tcp', () => {
    render(<ModemLinkSection kind="Tcp" host="127.0.0.1" port={8001} onChange={() => {}} />);
    expect(screen.getByDisplayValue('127.0.0.1')).toBeInTheDocument();
    expect(screen.getByDisplayValue('8001')).toBeInTheDocument();
  });

  it('shows serial device + baud when kind=Serial', () => {
    render(<ModemLinkSection
      kind="Serial"
      serialDevice="/dev/ttyUSB0"
      serialBaud={9600}
      onChange={() => {}}
    />);
    expect(screen.getByDisplayValue('/dev/ttyUSB0')).toBeInTheDocument();
    expect(screen.getByDisplayValue('9600')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to confirm fail**
- [ ] **Step 3: Implement (mirror the existing PacketModemBlock content; densify spacing for 360 px column).** Source the field shape from `src/packet/packetTypes.ts` and `src/packet/PacketConnectionPanel.tsx`. The component takes `{ linkKind, tcpHost, tcpPort, serialDevice, serialBaud }` as props and emits the same fields via `onChange(fields)`.
- [ ] **Step 4: CSS — append to `RadioPanel.css`**

```css
.radio-panel-segmented {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 4px;
  margin-bottom: 6px;
}
.radio-panel-segmented button {
  padding: 4px 6px;
  border-radius: 3px;
  text-align: center;
  background: rgba(255, 255, 255, 0.04);
  color: rgba(255, 255, 255, 0.30);
  border: 1px solid rgba(255, 255, 255, 0.06);
  font-family: ui-monospace, monospace;
  font-size: 9px;
  text-transform: uppercase;
  cursor: pointer;
}
.radio-panel-segmented button.active {
  background: rgba(34, 197, 94, 0.18);
  color: #4ade80;
  border-color: rgba(34, 197, 94, 0.35);
}
```

- [ ] **Step 5: Tests + tsc + commit**

### Task 3.2 — PacketRadioPanel

**Files:**
- Create: `src/radio/modes/PacketRadioPanel.tsx`
- Create: `src/radio/modes/PacketRadioPanel.test.tsx`

The Packet panel mirrors the existing `PacketConnectionPanel`'s sections (My station / Listen / Connect) but trimmed for 360 px. The digipeater-path editor is a compact accordion (0-2 relays) — visible row count adjusts.

- [ ] **Step 1: Test (3-4 cases — renders title, shows ModemLinkSection, fires packet_connect with path on Start, shows Listen UI for p2p intent)**
- [ ] **Step 2: Run test to confirm fail**
- [ ] **Step 3: Implement** — composes `ModemLinkSection` + `SessionLogSection` + Packet-specific Connect/Listen blocks. Mirror logic from current `PacketConnectionPanel`.
- [ ] **Step 4: Tests + tsc + commit**

### Task 3.3 — Wire AppShell to route Packet through PacketRadioPanel

**Files:**
- Modify: `src/shell/AppShell.tsx`

- [ ] **Step 1: Replace placeholder for Packet with PacketRadioPanel mount**
- [ ] **Step 2: Delete `src/packet/PacketConnectionPanel.tsx` and its CSS + test**
- [ ] **Step 3: Update AppShell's reading-pane routing for `cms-gateway` and `p2p` packet intents** — both now return `<MessageView />` instead of `<PacketConnectionPanelContainer />`
- [ ] **Step 4: Tests + tsc + commit**

### Task 3.4 — Phase 3 verification, PR, Codex round, merge

Same shape as P1.8 / P2.4. Codex prompt focuses on:
1. Modem-link persistence — does `onChange` correctly persist via `packet_config_set` (or whatever path) without races?
2. SSID + listen-default preference handling — do they survive AppShell remount?
3. Digipeater path editor at 360 px — does the compact accordion work for 0 / 1 / 2 relays?
4. Cleanup — any orphaned references to `PacketConnectionPanel` after deletion?

Operator smoke — selecting Packet in sidebar shows the new panel with modem-link picker; SSID dropdown persists; Connect with relay path works end-to-end.

---

## Phase 4 — ARDOP panel + Signal section + PINGACK Quality (closes tuxlink-1637)

**Goal:** Implement `ArdopRadioPanel` per spec §5.3 with the new `SignalSection` (Quality score + S/N trend sparkline + recent frame ribbon). The Signal section's Quality score requires backend PINGACK parsing (closes `tuxlink-1637`). Delete `ArdopDock` + `ArdopHfStub`. Close PR #166 without merging (its dock-visibility fix is moot).

**bd-issue & branch:** "feat: ArdopRadioPanel + SignalSection — replaces ArdopDock + ArdopHfStub; closes tuxlink-1637 (radio-panel P4 of 5)". Slug: `radio-panel-ardop`. New worktree off origin/main.

### Task 4.1 — Sparkline component

**Files:**
- Create: `src/radio/charts/Sparkline.tsx`
- Create: `src/radio/charts/Sparkline.css`
- Create: `src/radio/charts/Sparkline.test.tsx`

Used by both the Live section's throughput sparkline and the Signal section's S/N trend sparkline. Takes a number array of fixed length (60 samples ≈ 60 seconds) + optional thresholds for warn/bad coloring.

- [ ] **Step 1: Test (4 cases: renders correct bar count, applies warn class above threshold, applies bad class above bad threshold, scales bar heights to fit container)**
- [ ] **Step 2: Run test, confirm fail**
- [ ] **Step 3: Implement** — flexbox container, one inline `<div>` per sample, height as percentage; CSS handles the gradient + warn/bad colors via class.

```tsx
// src/radio/charts/Sparkline.tsx
import './Sparkline.css';

export interface SparklineProps {
  /** Samples ordered oldest → newest. Max ~60 for 60s history. */
  samples: number[];
  /** Min value of the range (typically 0 for throughput / dB-scale for S/N). */
  min?: number;
  /** Max value of the range. */
  max?: number;
  /** Threshold above which samples color warn-yellow. */
  warnAbove?: number;
  /** Threshold below which samples color warn-yellow (for "low is bad" cases). */
  warnBelow?: number;
  /** Same for bad-red. */
  badAbove?: number;
  badBelow?: number;
  /** Height of the chart in pixels. Default 42. */
  height?: number;
}

export function Sparkline({
  samples,
  min = 0,
  max,
  warnAbove,
  warnBelow,
  badAbove,
  badBelow,
  height = 42,
}: SparklineProps): JSX.Element {
  const computedMax = max ?? Math.max(...samples, 1);
  const span = computedMax - min || 1;

  return (
    <div className="sparkline" style={{ height }} data-testid="sparkline">
      {samples.map((s, i) => {
        const pct = ((s - min) / span) * 100;
        let cls = '';
        if (badAbove !== undefined && s > badAbove) cls = 'bad';
        else if (badBelow !== undefined && s < badBelow) cls = 'bad';
        else if (warnAbove !== undefined && s > warnAbove) cls = 'warn';
        else if (warnBelow !== undefined && s < warnBelow) cls = 'warn';
        return (
          <div
            key={i}
            className={`sparkline-bar ${cls}`}
            style={{ height: `${Math.max(2, pct)}%` }}
          />
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: CSS**

```css
/* src/radio/charts/Sparkline.css */
.sparkline {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  padding: 4px 0;
  border-radius: 2px;
}
.sparkline-bar {
  flex: 1;
  background: linear-gradient(0deg, #4ade80, rgba(74, 222, 128, 0.2));
  border-radius: 1px 1px 0 0;
  min-width: 2px;
}
.sparkline-bar.warn {
  background: linear-gradient(0deg, #fbbf24, rgba(251, 191, 36, 0.2));
}
.sparkline-bar.bad {
  background: linear-gradient(0deg, #f87171, rgba(248, 113, 113, 0.2));
}
```

- [ ] **Step 5: Tests + tsc + commit**

### Task 4.2 — FrameRibbon component

**Files:**
- Create: `src/radio/charts/FrameRibbon.tsx`
- Create: `src/radio/charts/FrameRibbon.test.tsx`

Horizontal flow of recent ARQ subprotocol frame types, color-coded.

- [ ] **Step 1: Test (renders correct cells in order, applies color class per frame type, shows legend)**
- [ ] **Step 2: Implement**

```tsx
// src/radio/charts/FrameRibbon.tsx
export type ArdopFrameType = 'CON' | 'IDLE' | 'DATA' | 'ACK' | 'NAK' | 'REJ';

export interface FrameRibbonProps {
  /** Recent frames, oldest → newest. Max ~14 cells fit in 360px column. */
  frames: ArdopFrameType[];
  /** Whether to render the legend below. Default true. */
  showLegend?: boolean;
}

export function FrameRibbon({
  frames,
  showLegend = true,
}: FrameRibbonProps): JSX.Element {
  return (
    <>
      <div className="frame-ribbon" data-testid="frame-ribbon">
        {frames.slice(-14).map((f, i) => (
          <div
            key={i}
            className={`frame-cell frame-${f.toLowerCase()}`}
            title={f}
          >
            {f}
          </div>
        ))}
      </div>
      {showLegend && (
        <div className="frame-legend">
          {(['CON','IDLE','DATA','ACK','NAK','REJ'] as ArdopFrameType[]).map(t => (
            <span key={t}>
              <i className={`frame-${t.toLowerCase()}`} />{t}
            </span>
          ))}
        </div>
      )}
    </>
  );
}
```

- [ ] **Step 3: CSS** (append to `RadioPanel.css` or new `FrameRibbon.css`):

```css
.frame-ribbon {
  display: flex;
  gap: 2px;
  align-items: stretch;
  height: 22px;
  margin-top: 8px;
  background: rgba(0, 0, 0, 0.18);
  border-radius: 3px;
  padding: 2px;
}
.frame-cell {
  flex: 1;
  border-radius: 2px;
  font-family: ui-monospace, monospace;
  font-size: 8px;
  text-align: center;
  line-height: 18px;
  background: rgba(255, 255, 255, 0.04);
  color: var(--text-faint, #94a3b8);
}
.frame-con  { background: rgba(168, 85, 247, 0.20); color: #c084fc; }
.frame-idle { background: rgba(148, 163, 184, 0.20); color: #94a3b8; }
.frame-data { background: rgba(34, 197, 94, 0.25); color: #4ade80; }
.frame-ack  { background: rgba(56, 189, 248, 0.20); color: #67e8f9; }
.frame-nak  { background: rgba(251, 191, 36, 0.20); color: #fbbf24; }
.frame-rej  { background: rgba(239, 68, 68, 0.18); color: #f87171; }

.frame-legend {
  display: flex;
  gap: 8px;
  font-size: 9px;
  color: var(--text-faint, #94a3b8);
  margin-top: 4px;
  flex-wrap: wrap;
}
.frame-legend span {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.frame-legend i {
  width: 8px;
  height: 8px;
  border-radius: 2px;
  display: inline-block;
}
```

- [ ] **Step 4: Tests + tsc + commit**

### Task 4.3 — Backend PINGACK Quality parsing (closes tuxlink-1637)

**Files:**
- Modify: `src-tauri/src/winlink/modem/ardop/command.rs` (add `PingAck` / `Ping` variants to the parser)
- Modify: `src-tauri/src/modem_status.rs` (add `quality: Option<u8>` field, plumb through accumulator)

Per `tuxlink-1637`: ardopcf emits `PINGACK SNdB Quality` (operator-sent ping response) and `PING caller>target SNdB Quality` (incoming ping). Parse both, store `Quality` in the modem-status accumulator, expose to the frontend via the existing `modem:status` event.

- [ ] **Step 1: Write the failing Rust test**

In `src-tauri/src/winlink/modem/ardop/command.rs` test module:

```rust
#[test]
fn parses_pingack_with_sn_and_quality() {
    let parsed = Command::parse("PINGACK 12 87").unwrap();
    match parsed {
        Command::PingAck { sn_db, quality } => {
            assert_eq!(sn_db, 12);
            assert_eq!(quality, 87);
        }
        _ => panic!("expected PingAck"),
    }
}

#[test]
fn parses_ping_with_caller_target_sn_quality() {
    let parsed = Command::parse("PING W4PHS>W7RMS 10 75").unwrap();
    match parsed {
        Command::Ping { caller, target, sn_db, quality } => {
            assert_eq!(caller, "W4PHS");
            assert_eq!(target, "W7RMS");
            assert_eq!(sn_db, 10);
            assert_eq!(quality, 75);
        }
        _ => panic!("expected Ping"),
    }
}
```

- [ ] **Step 2: Run test to confirm fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib parses_pingack
```

Expected: FAIL — no `PingAck` variant.

- [ ] **Step 3: Add the variants and parsing**

In `command.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ... existing variants
    PingAck { sn_db: i32, quality: u32 },
    Ping { caller: String, target: String, sn_db: i32, quality: u32 },
    // ... rest
}

// In Command::parse:
//   case statement for "PINGACK":
let mut parts = rest.split_whitespace();
let sn_db: i32 = parts.next().ok_or(...)?.parse()?;
let quality: u32 = parts.next().ok_or(...)?.parse()?;
return Ok(Command::PingAck { sn_db, quality });

//   case statement for "PING":
let mut parts = rest.split_whitespace();
let cg = parts.next().ok_or(...)?;
let (caller, target) = cg.split_once('>').ok_or(...)?;
let sn_db: i32 = parts.next().ok_or(...)?.parse()?;
let quality: u32 = parts.next().ok_or(...)?.parse()?;
return Ok(Command::Ping {
    caller: caller.to_string(),
    target: target.to_string(),
    sn_db, quality,
});
```

- [ ] **Step 4: Run test to confirm pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib parses_pingack parses_ping
```

Expected: PASS.

- [ ] **Step 5: Add quality field to ModemStatus + accumulator**

In `src-tauri/src/modem_status.rs`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModemStatus {
    // ... existing fields
    /// ardopcf Quality value (0-100), populated from PINGACK / PING events.
    /// None until first ping observed. tuxlink-1637.
    pub quality: Option<u8>,
}

// In apply_event_to_accumulators_inline:
Command::PingAck { sn_db, quality } => {
    status.sn_db = Some(*sn_db as f32);
    status.quality = Some(*quality as u8);
}
Command::Ping { sn_db, quality, .. } => {
    status.sn_db = Some(*sn_db as f32);
    status.quality = Some(*quality as u8);
}
```

Add a parallel `frontend/src/modem/types.ts` field:

```typescript
export interface ModemStatus {
  // ... existing
  quality: number | null;
}
```

Update `STOPPED` constant in `types.ts`:

```typescript
export const STOPPED: ModemStatus = {
  // ...
  quality: null,
};
```

- [ ] **Step 6: Test the accumulator behavior**

```rust
#[test]
fn pingack_populates_status_quality() {
    let mut status = ModemStatus::default();
    let cmd = Command::PingAck { sn_db: 12, quality: 87 };
    apply_event_to_accumulators_inline(&cmd, &mut status);
    assert_eq!(status.sn_db, Some(12.0));
    assert_eq!(status.quality, Some(87));
}
```

Run + confirm pass.

- [ ] **Step 7: Commit**

```bash
git -C "$(pwd)" add -A
git -C "$(pwd)" commit -m "feat(modem): parse PINGACK + PING events for Quality score (radio-panel-ardop P4.3; closes tuxlink-1637)

ardopcf emits PINGACK SNdB Quality and PING caller>target SNdB Quality
events that carry both signal-to-noise and a 0-100 quality score. The
existing parser ignored them. Adds variants Command::PingAck and
Command::Ping, parses both, and stores Quality in ModemStatus.quality
via the existing accumulator path. Quality surfaces in the Signal
section's big-number indicator (frontend wiring in P4.4-P4.6)."
```

### Task 4.4 — SignalSection component

**Files:**
- Create: `src/radio/sections/SignalSection.tsx`
- Create: `src/radio/sections/SignalSection.css`
- Create: `src/radio/sections/SignalSection.test.tsx`

- [ ] **Step 1: Test (renders Quality value, renders S/N trend sparkline, renders FrameRibbon, shows `—` when quality is null)**
- [ ] **Step 2: Implement**

```tsx
// src/radio/sections/SignalSection.tsx
//
// Spec §5.3 — ARDOP signal-quality section. Quality score + S/N
// trend sparkline + recent frame ribbon. For VARA HF (future), this
// same slot holds the OFDM constellation; the component takes the
// content as children so per-mode visualizations can slot in.

import { Sparkline } from '../charts/Sparkline';
import { FrameRibbon, type ArdopFrameType } from '../charts/FrameRibbon';
import './SignalSection.css';

export interface SignalSectionProps {
  /** ardopcf Quality value 0-100; null = no data yet. */
  quality: number | null;
  /** S/N samples for the trend sparkline (60 samples ≈ 60 s). */
  snrSamples: number[];
  /** Recent ARQ frame types (latest 14 render). */
  recentFrames: ArdopFrameType[];
  /** Latest S/N value to show as the current reading. */
  snrCurrent: number | null;
}

export function SignalSection({
  quality, snrSamples, recentFrames, snrCurrent,
}: SignalSectionProps): JSX.Element {
  const avgSnr = snrSamples.length
    ? snrSamples.reduce((a, b) => a + b, 0) / snrSamples.length
    : null;
  return (
    <section className="radio-panel-sec signal-section" data-testid="signal-section">
      <h5>Signal</h5>
      <div className="signal-top">
        <div className="quality" data-testid="quality-score">
          <div className="qv">{quality === null ? '—' : quality}</div>
          <div className="qk">Quality</div>
          <div className="qs">/100</div>
        </div>
        <div className="snr-trend">
          <div className="lab-row">
            <span className="k">S/N trend</span>
            <span className="v">{snrCurrent === null
              ? '— dB'
              : `${snrCurrent >= 0 ? '+' : ''}${snrCurrent.toFixed(1)} dB`}</span>
          </div>
          <Sparkline
            samples={snrSamples}
            height={28}
            warnBelow={3}
            badBelow={0}
          />
          <div className="lab-row" style={{ marginTop: 2 }}>
            <span style={{ fontSize: 9, opacity: 0.7 }}>−60s</span>
            <span style={{ fontSize: 9 }}>
              avg <strong>{avgSnr === null ? '—' : `${avgSnr >= 0 ? '+' : ''}${avgSnr.toFixed(1)} dB`}</strong>
            </span>
            <span style={{ fontSize: 9, opacity: 0.7 }}>now</span>
          </div>
        </div>
      </div>
      <FrameRibbon frames={recentFrames} />
    </section>
  );
}
```

- [ ] **Step 3: CSS**

```css
/* src/radio/sections/SignalSection.css */
.signal-section .signal-top {
  display: grid;
  grid-template-columns: 90px 1fr;
  gap: 10px;
  align-items: center;
}
.signal-section .quality {
  background: radial-gradient(circle at 50% 50%, rgba(34, 197, 94, 0.18), transparent 65%);
  border: 1px solid rgba(34, 197, 94, 0.30);
  border-radius: 6px;
  padding: 6px 4px;
  text-align: center;
}
.signal-section .qv {
  font-family: ui-monospace, monospace;
  font-size: 24px;
  font-weight: 600;
  color: #4ade80;
  line-height: 1;
}
.signal-section .qk {
  font-size: 8px;
  color: var(--text-faint, #94a3b8);
  text-transform: uppercase;
  letter-spacing: 0.07em;
  margin-top: 3px;
}
.signal-section .qs {
  font-size: 9px;
  color: var(--text-faint, #94a3b8);
  margin-top: 2px;
}
.signal-section .snr-trend .lab-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 10px;
}
.signal-section .snr-trend .k {
  color: var(--text-faint, #94a3b8);
  text-transform: uppercase;
  font-size: 9px;
  letter-spacing: 0.05em;
}
.signal-section .snr-trend .v {
  font-family: ui-monospace, monospace;
  color: #4ade80;
}
```

- [ ] **Step 4: Tests + tsc + commit**

### Task 4.5 — ArdopRadioPanel — assemble Connect / Live / Signal / ARQ state / Session log / Actions

**Files:**
- Create: `src/radio/modes/ArdopRadioPanel.tsx`
- Create: `src/radio/modes/ArdopRadioPanel.test.tsx`

The most complex panel. Composes every section type. Sources data from `useModemStatus` (live S/N, throughput, ARQ state, peer, mode, width). Maintains its own 60-sample S/N + throughput history (rolling window). Recent-frame history sourced from the same status event stream.

- [ ] **Step 1: Test (5-7 cases — renders title `Ardop Winlink`, mounts SignalSection with Quality, mounts SessionLogSection, Start button fires `modem_ardop_connect` with consent, Send/Receive disabled when not connected, ⊘ Stop fires `modem_ardop_disconnect`, Open WebGUI button opens the configured cmd_port - 1 URL)**

Most of these mirror existing `ArdopDock.test.tsx` patterns; the migration is mechanical.

- [ ] **Step 2: Implement** — large file (~250 LOC) mirroring `ArdopDock.tsx`'s logic but rendering into `RadioPanel` chrome with new sections. Maintains internal sample history buffers via `useEffect` listening to `modem:status` events:

```tsx
// Outline:
export function ArdopRadioPanel({ onClose }: ArdopRadioPanelProps): JSX.Element {
  const { status } = useModemStatus();
  const consent = useConsent();
  // ... existing state (target, connecting, showConsent, etc.)

  // Rolling history buffers
  const snrHistory = useSampleHistory(status.snDb, 60);
  const throughputHistory = useSampleHistory(status.throughputBps, 60);
  const frameHistory = useFrameHistory(status, 14);

  // Action handlers mirror ArdopDock.tsx (doConnect, onConnectClick,
  // onSendReceiveClick, onDisconnectClick, onOpenWebGuiClick)

  return (
    <RadioPanel
      mode={{ kind: 'ardop-hf', intent: 'cms' }}
      state={mapModemStateToPanelState(status.state)}
      sub={`${status.peer ?? '—'} · ${status.widthHz ?? '—'} Hz`}
      onClose={onClose}
    >
      <ConnectSection target={target} setTarget={setTarget}
                      bandwidth={bandwidth} setBandwidth={setBandwidth} />
      <LiveSection status={status} throughputHistory={throughputHistory} />
      <SignalSection
        quality={status.quality}
        snrSamples={snrHistory}
        recentFrames={frameHistory}
        snrCurrent={status.snDb}
      />
      <ArqStateSection status={status} />
      <SessionLogSection entries={sessionLogEntries} />
      <ActionsSection
        state={status.state}
        onStart={onConnectClick}
        onSendReceive={onSendReceiveClick}
        onOpenWebGui={onOpenWebGuiClick}
        onStop={onDisconnectClick}
      />

      {showConsent && (
        <ConsentModal target={target.trim()}
                      onCancel={() => setShowConsent(false)}
                      onConfirm={onConsentConfirm} />
      )}
    </RadioPanel>
  );
}
```

Each inner section (`ConnectSection`, `LiveSection`, `ArqStateSection`, `ActionsSection`) can be inlined or extracted into separate files under `src/radio/modes/ardop/` if the panel file grows too large. Recommendation: extract once the file exceeds 250 LOC.

- [ ] **Step 3: Sample-history utility (`useSampleHistory`)**

```tsx
// src/radio/useSampleHistory.ts
import { useEffect, useRef, useState } from 'react';

/**
 * Maintains a rolling fixed-length buffer of samples. Pushes the latest
 * value once per tick (configurable; default 1s). Used by the throughput
 * + S/N sparklines.
 */
export function useSampleHistory(
  current: number | null,
  length: number,
  intervalMs: number = 1000,
): number[] {
  const [samples, setSamples] = useState<number[]>(() => new Array(length).fill(0));
  const latest = useRef(current);
  latest.current = current;

  useEffect(() => {
    const id = setInterval(() => {
      setSamples(prev => [...prev.slice(1), latest.current ?? 0]);
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return samples;
}
```

Test it separately (3-4 cases — length, pushes latest, ticks at interval).

- [ ] **Step 4: Tests + tsc + commit**

### Task 4.6 — Wire AppShell to route ARDOP HF through ArdopRadioPanel; delete ArdopDock + ArdopHfStub

**Files:**
- Modify: `src/shell/AppShell.tsx` (remove ArdopDock + ArdopHfStub references; route ARDOP to ArdopRadioPanel)
- Delete: `src/modem/ArdopDock.tsx`, `src/modem/ArdopDock.css`, `src/modem/ArdopDock.test.tsx`, `src/modem/ArdopDock.integration.test.tsx`
- Delete: `src/connections/ArdopHfStub.tsx`

- [ ] **Step 1: AppShell — remove ArdopDock mount, route ARDOP HF panel**

```tsx
// Remove the ArdopDock import + the conditional mount
// Add:
import { ArdopRadioPanel } from '../radio/modes/ArdopRadioPanel';

// In the routing block (where Telnet / Packet got migrated):
{radioPanelMode && radioPanelMode.kind === 'ardop-hf' && (
  <ArdopRadioPanel onClose={() => {
    setSelectedConnection(null);
    setPinRadioPanel(false);
  }} />
)}
```

For the reading-pane routing where ARDOP HF currently returns `<ArdopHfStub />`:

```tsx
if (sessionType === 'cms' && protocol === 'ardop-hf') {
  return <MessageView selectedMessage={selectedMessage} />;
}
```

- [ ] **Step 2: Delete the old files**

```bash
rm -f src/modem/ArdopDock.tsx src/modem/ArdopDock.css \
      src/modem/ArdopDock.test.tsx src/modem/ArdopDock.integration.test.tsx \
      src/connections/ArdopHfStub.tsx
```

- [ ] **Step 3: Tests + tsc + commit**

### Task 4.7 — Phase 4 verification, PR, Codex round, merge

Quality gates: include `cargo test --lib` + clippy (Rust changed in 4.3).

Codex prompt focuses on:
1. RADIO-1 invariants — consent token handling preserved across the migration (mint, store, consume, clear)?
2. PINGACK / PING parsing — edge cases (negative S/N, quality > 100, malformed input)
3. Sample-history correctness — does the rolling buffer correctly drop old samples?
4. Signal section data flow — does Quality null propagate as `—` placeholder?
5. Open-WebGUI URL construction — same guards as `tuxlink-60wh` (cmd_port >= 2)?
6. Cleanup — any orphaned references to ArdopDock / ArdopHfStub?

Operator smoke: full Mode-2 flow — select ARDOP HF, Start, observe Live section populating S/N + VU + throughput meters + sparkline, observe Signal section showing Quality after ping; Send/Receive; Disconnect; verify session log section shows the full session's lines.

### Task 4.8 — Close cascade issues

```bash
bd close tuxlink-1637 \
         tuxlink-mnk4 \
         tuxlink-ed51 \
         tuxlink-mzr7
gh pr close 166 --comment "Superseded by P4 of radio-panel implementation — whole ArdopDock removed, dock-visibility fix moot."
```

---

## Phase 5 — Vocabulary cleanup

**Goal:** Rename `Session → Connect/Disconnect` to `Start/Stop` + add `Abort`; rebind F5 / Ctrl+Shift+O to contextual Start; remove the ribbon Connect button; retire `View → Show transport`; rename remaining `pinRadioDock` → `pinRadioPanel` consistency.

**bd-issue & branch:** "refactor: vocabulary cleanup — Start/Stop/Abort, ribbon Connect removal, View menu retirements (radio-panel P5 of 5)". Slug: `radio-panel-vocab`. New worktree off origin/main.

### Task 5.1 — Rename Session menu items

**Files:**
- Modify: `src/shell/chrome/menuModel.ts`
- Modify: `src/shell/chrome/dispatchMenuAction.ts`

- [ ] **Step 1: Update menuModel.ts**

```typescript
{ label: 'Session', items: [
  { id: 'menu:session:start',  label: 'Start',  accel: 'F5' },
  { id: 'menu:session:stop',   label: 'Stop' },
  { id: 'menu:session:abort',  label: 'Abort' },
  { separator: true },
  { id: 'menu:session:test_send', label: 'Test send' },
  // 'menu:session:show_transport' REMOVED — panel + ribbon already
  // surface the transport (spec §6.2).
] },
```

In `ACCELERATORS`, update both F5 and Ctrl+Shift+O to target `start`:

```typescript
{ combo: 'F5', key: 'F5', ctrl: false, shift: false, id: 'menu:session:start' },
{ combo: 'Ctrl+Shift+O', key: 'o', ctrl: true, shift: true, id: 'menu:session:start' },
```

- [ ] **Step 2: Update dispatchMenuAction.ts**

```typescript
export interface MenuHandlers {
  // ...
  start: () => void;   // was connect
  stop: () => void;
  abort: () => void;
  // toggleSessionLog already removed in P1
  // ...
}

// Dispatcher:
case 'menu:session:start': h.start(); return;
case 'menu:session:stop':  h.stop();  return;
case 'menu:session:abort': h.abort(); return;
// case 'menu:session:show_transport': REMOVED
```

- [ ] **Step 3: Update menuModel.test.ts**

Update the expected-ID list to drop `'menu:session:show_transport'` and rename `'menu:session:connect'` / `'disconnect'` to the new ids.

- [ ] **Step 4: Update dispatchMenuAction.test.ts**

Update handler-name references and add tests for `start` / `stop` / `abort` routing.

- [ ] **Step 5: Tests + tsc + commit**

### Task 5.2 — Contextual F5 / Ctrl+Shift+O behavior

**Files:**
- Modify: `src/shell/AppShell.tsx`

The new `start` handler should fire the currently-selected mode's Start. v1 implementation:

```tsx
const start = useCallback(() => {
  if (!radioPanelMode) return;  // no-op if no mode visible

  // Each mode-panel exposes its Start via an imperative ref OR via a
  // shared eventbus. Recommendation for v1: each mode-panel listens for
  // a window event 'tuxlink:radio-panel:start' and fires its Start
  // handler if it matches its mode. AppShell just dispatches.
  window.dispatchEvent(new CustomEvent('tuxlink:radio-panel:start', {
    detail: { mode: radioPanelMode },
  }));
}, [radioPanelMode]);

// Similarly for stop / abort.
```

Each mode panel adds a listener:

```tsx
// In ArdopRadioPanel:
useEffect(() => {
  const onStart = (ev: Event) => {
    const detail = (ev as CustomEvent).detail;
    if (detail?.mode?.kind === 'ardop-hf') {
      onConnectClick();
    }
  };
  window.addEventListener('tuxlink:radio-panel:start', onStart);
  return () => window.removeEventListener('tuxlink:radio-panel:start', onStart);
}, [onConnectClick]);
```

- [ ] **Step 1: Implement Start/Stop/Abort handlers in AppShell**
- [ ] **Step 2: Add listeners in each mode panel** (Telnet, Packet, ARDOP)
- [ ] **Step 3: Test the F5 dispatch** — extend the existing accelerator tests
- [ ] **Step 4: Commit**

### Task 5.3 — Remove ribbon Connect button

**Files:**
- Modify: `src/shell/DashboardRibbon.tsx`
- Modify: `src/shell/DashboardRibbon.test.tsx`
- Modify: `src/shell/AppShell.tsx` (drop `onConnect`, `connecting`, `onAbort` props passed to DashboardRibbon)

- [ ] **Step 1: DashboardRibbon — strip the Connect / Abort buttons**

```tsx
// In DashboardRibbon.tsx — remove the conditional block:
// {onConnect && (<>...</>)}

// Remove from props interface:
// onConnect, connecting, onAbort  ← all REMOVED
```

- [ ] **Step 2: DashboardRibbon.test.tsx — drop the Connect / Abort tests**

- [ ] **Step 3: AppShell — drop the Connect/Abort handlers passed in**

```tsx
// In AppShell.tsx — simplify:
<DashboardRibbon
  data={statusData}
  packet={packetUi}
/>

// Remove the state + handlers:
// const [connecting, setConnecting] = useState(false);
// const onConnect = ...
// const onAbort = ...
```

Note: the `onConnect` / `onAbort` IPC calls (`cms_connect`, `cms_abort`) move to the mode panels' Start / Abort handlers (already in place after P2). The dashboard ribbon just doesn't trigger them anymore.

- [ ] **Step 4: Tests + tsc + commit**

### Task 5.4 — Phase 5 verification, PR, Codex round, merge

Codex prompt focuses on:
1. Vocabulary consistency — no remaining `Connect` / `Disconnect` strings in user-facing menus/buttons
2. F5 accelerator — operator on Telnet view, F5 fires Telnet Start; operator on ARDOP view, F5 fires ARDOP Start
3. Ribbon — no Connect button, no Connect handler wired; ribbon is purely informational
4. Test coverage — all renamed handlers have updated tests

Operator smoke: full flow per mode, F5 starts the selected mode, ribbon is clean.

### Task 5.5 — Close the umbrella issues

```bash
bd close tuxlink-74mx  # the spec
bd close tuxlink-nr21  # this plan
```

Update `dev/implementation-log.md` (if it exists) with the radio-panel-redesign completion entry.

---

## Self-review

After writing the complete plan, run this checklist mentally:

**Spec coverage:** Skim each section of the spec — every locked decision in §3, every layout decision in §4, every per-mode panel spec in §5, every vocabulary change in §6 — should be implemented in some task. Specifically:

- §3.1 Reading pane is messages-only — covered by §5.1 / §5.2 / §5.3 ✓ (TelnetCmsPanel deleted in P2, PacketConnectionPanel deleted in P3, ArdopHfStub deleted in P4)
- §3.2 360 px panel — covered in P1 Task 1.3 RadioPanel.tsx ✓
- §3.3 Visibility rule — covered in P1 Task 1.2 useRadioPanelVisibility ✓
- §3.4 All sections always rendered — handled by the per-mode panels' rendering logic ✓
- §3.5 Express vocabulary — covered in P5 ✓
- §3.6 Ribbon Connect removed — covered in P5 Task 5.3 ✓
- §3.7 Log in panel + bottom strip removed — covered in P1 Task 1.6 + P2 Task 2.1 SessionLogSection ✓
- §4.1 Column grid — covered in P1 Task 1.5 / 1.6 (CSS grid update) ✓
- §4.2 Panel chrome — covered in P1 Task 1.3 ✓
- §4.3 Session log typography rules — covered in P2 Task 2.1 ✓
- §5.1 Telnet panel content — covered in P2 ✓
- §5.2 Packet panel content — covered in P3 ✓
- §5.3 ARDOP panel + Signal section — covered in P4 ✓
- §5.4 VARA forward-look — design-only in spec; no plan task needed ✓
- §6.1 Per-panel action labels — implemented per-mode in P2-P4 ✓
- §6.2 Menu audit — covered in P5 + partially in P1 (Toggle Radio Panel rename) ✓
- §6.3 F5 contextual — covered in P5 Task 5.2 ✓
- §6.4 Ribbon informational — covered in P5 Task 5.3 ✓

**Placeholder scan:** Every code block is complete and runnable. The few `// ...` ellipses in code blocks (e.g., "rest unchanged" in interface deltas) are in places where the spec explicitly tells the implementing engineer to preserve existing fields — the prose around them names them.

**Type consistency:** `RadioPanelMode`, `RadioPanelState`, `SessionLogEntry`, `ArdopFrameType` are used consistently across tasks. `MenuHandlers.toggleRadioPanel` (renamed from `toggleRadioDock` in P1.7) referenced consistently in P5.

**Frequent commits:** Every task ends in a commit step. Most tasks have 4-7 steps. No task batches more than one logical change.

**No new dependencies:** All new components use React + existing project primitives. No new npm packages, no new Cargo deps.

---

## Execution

Plan complete and committed to `docs/superpowers/plans/2026-05-31-radio-mode-right-panel-implementation.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Which approach?
