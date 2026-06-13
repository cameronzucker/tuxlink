# APRS Chat — Dock Re-home + Entry Points Implementation Plan (Plan 2 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-home the APRS chat from the `'aprs'` sidebar pseudo-folder into the **shared, switchable right dock** (APRS chat ⇄ Modem console), reached by a glanceable **status-strip APRS control** (entry ①) and **dock tabs** (entry ②), per `docs/superpowers/specs/2026-06-12-aprs-tactical-chat-frontend-ia-design.md`.

**Architecture:** APRS state is **lifted to AppShell** (one `useAprsChat` instance) so both the status-strip control (unread badge + listening) and the dock panel share it. AppShell gains a small dock view-model: `aprsOpen` (has the operator opened chat) + `dockTab` (`'aprs' | 'modem'`). The dock's 4th grid column shows when `radioPanelMode !== null || aprsOpen`; a tab row switches content. `AprsChatPanel` is refactored to receive chat state via props (decoupled from the hook) and exposes an optional `controlStrip` slot — the clean seam for the parallel native-backend's UV-Pro device control (no stub shipped; nothing renders there until the backend lands). Frontend-only.

**Tech Stack:** React + TypeScript, Vitest + @testing-library/react. Local-TDD-able. Run vitest **scoped**; reap workers (`pkill -f vitest`) after interrupted sweeps. Depends on **Plan 1 being merged/applied first** (this plan assumes `AprsChatPanel` already renders timestamps / `Acked HH:MM` / the counter / the cue).

**Commit discipline:** `Agent: <moniker>` trailer on every commit (CLAUDE.md). Under subagent execution the **parent** commits (subagent cwd resets → main-checkout hook denies its in-worktree commit); the subagent codes + gates + STOPs uncommitted. Worktree: `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`.

**Out of scope (deferred follow-ups):**
- **Entry ③ (View-menu `menu:view:aprs_chat`)** — touches the Rust `menu_event_ids` parity contract (`menuModel.ts` header); a separate small cross-language task, not part of this frontend-only plan.
- **UV-Pro device-control strip content** — depends on the parallel native backend's published Tauri contract; this plan only lands the `controlStrip` seam.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/aprs/AprsChatPanel.tsx` | The chat surface | Accept `threads`/`listening`/`send`/`controlStrip` as **props** (stop calling `useAprsChat` internally) |
| `src/aprs/AprsChatPanel.test.tsx` | Panel tests | Render with props instead of relying on the internal hook |
| `src/aprs/aprsUnread.ts` | Unread computation | **New** — pure `countUnread(threads, sinceMs)` helper |
| `src/aprs/aprsUnread.test.ts` | Unread tests | **New** |
| `src/aprs/AprsDockTabs.tsx` | Dock tab switcher | **New** — `[ APRS chat \| Modem ]` tabs with unread badge |
| `src/aprs/AprsDockTabs.test.tsx` | Tab tests | **New** |
| `src/aprs/AprsDockTabs.css` | Tab styles | **New** |
| `src/shell/DashboardRibbon.tsx` | Status strip | Add optional `aprs` prop + the status-strip APRS control (entry ①) |
| `src/shell/DashboardRibbon.test.tsx` | Ribbon tests | + control render/click test |
| `src/shell/AppShell.tsx` | Shell wiring | Lift `useAprsChat`; add `aprsOpen`/`dockTab`/unread; mount panel + tabs in the dock; pass `aprs` to ribbon; remove the `'aprs'` early-return |
| `src/mailbox/FolderSidebar.tsx` | Sidebar nav | Remove the `'aprs'` `ADDRESS_ITEMS` entry |

---

## Task 1: Decouple `AprsChatPanel` from the hook (props in)

**Files:**
- Modify: `src/aprs/AprsChatPanel.tsx` (the `AprsChatPanel` function signature + the `useAprsChat()` call) — keep all rendering identical.
- Modify: `src/aprs/AprsChatPanel.test.tsx`
- Test: `src/aprs/AprsChatPanel.test.tsx`

- [ ] **Step 1: Write the failing test** — render the panel with injected props (no hook).

Replace the body of `src/aprs/AprsChatPanel.test.tsx` so it imports the types and passes props. Add this helper + a test (keep the existing tests but pass the new required props to each `render`):

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
import { AprsChatPanel } from './AprsChatPanel';
import type { Thread } from './aprsTypes';

const noThreads: Record<string, Thread> = {};
const send = vi.fn().mockResolvedValue('A1');
function renderPanel(over: Partial<Parameters<typeof AprsChatPanel>[0]> = {}) {
  return render(
    <AprsChatPanel threads={noThreads} listening={false} send={send} {...over} />,
  );
}

describe('AprsChatPanel', () => {
  it('renders the composer and a listening indicator', () => {
    renderPanel();
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('renders an injected controlStrip slot when provided', () => {
    renderPanel({ controlStrip: <div data-testid="probe-strip">strip</div> });
    expect(screen.getByTestId('probe-strip')).toBeInTheDocument();
  });

  it('renders a thread with its messages from props', () => {
    const threads: Record<string, Thread> = {
      'W7RPT-9': { callsign: 'W7RPT-9', messages: [
        { id: 'm1', direction: 'in', text: 'ping', msgid: '04', at: Date.now() },
      ] },
    };
    renderPanel({ threads });
    expect(screen.getByText('ping')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: FAIL — `AprsChatPanel` takes no props (TS error) / `controlStrip` unknown.

- [ ] **Step 3: Change the signature to accept props**

In `src/aprs/AprsChatPanel.tsx`:
1. Remove the `useAprsChat` import (line 27) and the `import './AprsChatPanel.css'` stays.
2. Add a props interface above the component and consume props instead of the hook. Replace the line `export function AprsChatPanel() {` and the immediately-following `const { threads, listening, send } = useAprsChat();` (lines 66-67) with:

```tsx
import type { ReactNode } from 'react';

export interface AprsChatPanelProps {
  /// Per-callsign conversation map (owned by AppShell's lifted useAprsChat).
  threads: Record<string, Thread>;
  /// Whether the backend listener is armed (mirrors the backend).
  listening: boolean;
  /// Send `text` to `call`; resolves with the backend msgid (rejects → no bubble).
  send: (call: string, text: string) => Promise<string>;
  /// Optional device-control slot rendered above the composer. The seam for the
  /// UV-Pro native control surface; undefined until the native backend lands.
  controlStrip?: ReactNode;
}

export function AprsChatPanel({ threads, listening, send, controlStrip }: AprsChatPanelProps) {
```

3. Render `controlStrip` just before the `<form className="aprs-composer">` (inside `<main className="aprs-conversation">`, after the `sendError` block):

```tsx
          {controlStrip}
```

Everything else in the component (the listening toggle's `invoke`, the composer, bubbles) is unchanged.

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/aprs/AprsChatPanel.tsx src/aprs/AprsChatPanel.test.tsx
git commit -m "refactor(aprs): AprsChatPanel takes chat state via props + a controlStrip seam"
```

---

## Task 2: `countUnread` helper

**Files:**
- Create: `src/aprs/aprsUnread.ts`
- Test: `src/aprs/aprsUnread.test.ts`

- [ ] **Step 1: Write the failing test** — `src/aprs/aprsUnread.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { countUnread } from './aprsUnread';
import type { Thread } from './aprsTypes';

const t = (msgs: Array<{ dir: 'in' | 'out'; at: number }>): Thread => ({
  callsign: 'W7RPT-9',
  messages: msgs.map((m, i) => ({ id: `m${i}`, direction: m.dir, text: 'x', msgid: null, at: m.at })),
});

describe('countUnread', () => {
  it('counts inbound messages newer than the seen watermark', () => {
    const threads = { 'W7RPT-9': t([{ dir: 'in', at: 100 }, { dir: 'in', at: 300 }, { dir: 'out', at: 400 }]) };
    expect(countUnread(threads, 200)).toBe(1); // only the at:300 inbound
  });
  it('ignores outbound messages', () => {
    const threads = { A: t([{ dir: 'out', at: 500 }]) };
    expect(countUnread(threads, 0)).toBe(0);
  });
  it('returns 0 for an empty thread map', () => {
    expect(countUnread({}, 0)).toBe(0);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/aprsUnread.test.ts`
Expected: FAIL — `countUnread` not found.

- [ ] **Step 3: Implement** — `src/aprs/aprsUnread.ts`:

```ts
// src/aprs/aprsUnread.ts
//
// Pure unread-count helper for the APRS status-strip control. "Unread" = inbound
// messages received after the operator last viewed the chat (the seen watermark,
// an epoch-ms held in AppShell, reset when the APRS dock tab is opened).

import type { Thread } from './aprsTypes';

/// Count inbound messages across all threads with `at` strictly greater than
/// `sinceMs`. Outbound messages never count as unread.
export function countUnread(threads: Record<string, Thread>, sinceMs: number): number {
  let n = 0;
  for (const thread of Object.values(threads)) {
    for (const m of thread.messages) {
      if (m.direction === 'in' && m.at > sinceMs) n += 1;
    }
  }
  return n;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/aprsUnread.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/aprs/aprsUnread.ts src/aprs/aprsUnread.test.ts
git commit -m "feat(aprs): countUnread helper for the status-strip unread badge"
```

---

## Task 3: `AprsDockTabs` switcher component

**Files:**
- Create: `src/aprs/AprsDockTabs.tsx`, `src/aprs/AprsDockTabs.css`
- Test: `src/aprs/AprsDockTabs.test.tsx`

- [ ] **Step 1: Write the failing test** — `src/aprs/AprsDockTabs.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import { AprsDockTabs } from './AprsDockTabs';

describe('AprsDockTabs', () => {
  it('marks the active tab and shows an unread badge on APRS when not active', () => {
    render(<AprsDockTabs active="modem" unread={2} modemEnabled onSelect={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('aprs-dock-tab-aprs-unread')).toHaveTextContent('2');
  });
  it('calls onSelect with the clicked tab', async () => {
    const onSelect = vi.fn();
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled onSelect={onSelect} />);
    await userEvent.click(screen.getByTestId('aprs-dock-tab-modem'));
    expect(onSelect).toHaveBeenCalledWith('modem');
  });
  it('disables the Modem tab when no connection is available', () => {
    render(<AprsDockTabs active="aprs" unread={0} modemEnabled={false} onSelect={() => {}} />);
    expect(screen.getByTestId('aprs-dock-tab-modem')).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsDockTabs.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement** — `src/aprs/AprsDockTabs.tsx`:

```tsx
// src/aprs/AprsDockTabs.tsx
//
// The shared right-dock tab switcher: [ APRS chat | Modem ]. The dock hosts the
// APRS chat (default tenant) or the modem console; these tabs flip between them.
// The Modem tab is disabled when no connection/modem panel is available.

import './AprsDockTabs.css';

export type DockTab = 'aprs' | 'modem';

export interface AprsDockTabsProps {
  active: DockTab;
  unread: number;
  /// Whether the Modem tab can be selected (a radio panel mode is available).
  modemEnabled: boolean;
  onSelect: (tab: DockTab) => void;
}

export function AprsDockTabs({ active, unread, modemEnabled, onSelect }: AprsDockTabsProps) {
  return (
    <div className="aprs-dock-tabs" role="tablist" data-testid="aprs-dock-tabs">
      <button
        type="button"
        role="tab"
        aria-selected={active === 'aprs'}
        className={`aprs-dock-tab ${active === 'aprs' ? 'is-active' : ''}`}
        data-testid="aprs-dock-tab-aprs"
        onClick={() => onSelect('aprs')}
      >
        APRS chat
        {unread > 0 && active !== 'aprs' && (
          <span className="aprs-dock-tab-badge" data-testid="aprs-dock-tab-aprs-unread">{unread}</span>
        )}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={active === 'modem'}
        className={`aprs-dock-tab ${active === 'modem' ? 'is-active' : ''}`}
        data-testid="aprs-dock-tab-modem"
        disabled={!modemEnabled}
        onClick={() => onSelect('modem')}
      >
        Modem
      </button>
    </div>
  );
}
```

`src/aprs/AprsDockTabs.css`:

```css
.aprs-dock-tabs { display: flex; border-bottom: 1px solid var(--border, #2a3140); background: var(--surface-2); }
.aprs-dock-tab {
  flex: 1; padding: 8px 0; font: inherit; font-size: 12px; cursor: pointer;
  background: transparent; border: none; color: var(--text-faint, #94a3b8);
  border-bottom: 2px solid transparent; display: inline-flex; align-items: center; justify-content: center; gap: 6px;
}
.aprs-dock-tab.is-active { color: var(--modem-accent, #4ade80); border-bottom-color: var(--modem-accent, #4ade80); background: var(--modem-accent-soft); }
.aprs-dock-tab:disabled { opacity: 0.45; cursor: not-allowed; }
.aprs-dock-tab-badge { background: var(--modem-accent, #4ade80); color: var(--bg, #0d1318); font-size: 10px; font-weight: 700; border-radius: 9px; padding: 0 5px; }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsDockTabs.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/aprs/AprsDockTabs.tsx src/aprs/AprsDockTabs.css src/aprs/AprsDockTabs.test.tsx
git commit -m "feat(aprs): AprsDockTabs — the shared-dock APRS/Modem switcher"
```

---

## Task 4: Status-strip APRS control (entry ①) in `DashboardRibbon`

**Files:**
- Modify: `src/shell/DashboardRibbon.tsx` — the `DashboardRibbonProps` interface, the destructure (line 99), and the JSX (add the control before the `onConnect` block, ~line 271).
- Test: `src/shell/DashboardRibbon.test.tsx` (follow the existing tests there for provider/prop scaffolding).

- [ ] **Step 1: Write the failing test** — add to `src/shell/DashboardRibbon.test.tsx` (reuse that file's existing render helper / default props; pass the new `aprs` prop):

```tsx
  it('renders the APRS status control and opens chat on click', async () => {
    const onOpen = vi.fn();
    renderRibbon({ aprs: { listening: true, unread: 1, onOpen } });
    const btn = screen.getByTestId('dash-aprs-control');
    expect(btn).toHaveTextContent(/APRS/i);
    expect(screen.getByTestId('dash-aprs-unread')).toHaveTextContent('1');
    await userEvent.click(btn);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });
```

> Use the existing `renderRibbon`/default-props helper in that test file. If none exists, render `<DashboardRibbon {...baseProps} aprs={{ listening: true, unread: 1, onOpen }} />` where `baseProps` mirrors the other tests in the file.

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/shell/DashboardRibbon.test.tsx`
Expected: FAIL — `aprs` prop unknown / no `dash-aprs-control`.

- [ ] **Step 3: Add the prop + control**

In `src/shell/DashboardRibbon.tsx`, add to `DashboardRibbonProps` (find the interface; add this optional field):

```tsx
  /** APRS tactical-chat status control (entry ①). Absent → the control is not
   *  rendered. `unread` drives the badge; `onOpen` brings chat into the dock. */
  aprs?: { listening: boolean; unread: number; onOpen: () => void };
```

Add `aprs` to the destructure on line 99:

```tsx
export const DashboardRibbon = memo(function DashboardRibbon({ data, onConnect, connecting, onAbort, packet, radioConn, ssid, onSsidChange, reviewInbound, onReviewInboundChange, aprs }: DashboardRibbonProps) {
```

In the JSX, immediately before the `{onConnect && (` block (around line 271, so the control sits just left of Connect), add:

```tsx
      {aprs && (
        <>
          <div className="dash-divider" />
          <div className="dash-item dash-aprs">
            <div className="dash-label">APRS</div>
            <button
              type="button"
              className="dash-aprs-control"
              data-testid="dash-aprs-control"
              onClick={aprs.onOpen}
              title="Open APRS tactical chat"
            >
              <span className={`dash-status-dot ${aprs.listening ? 'is-on' : 'is-off'}`} aria-hidden="true" />
              <span className="dash-aprs-state">{aprs.listening ? 'Listening' : 'Off'}</span>
              {aprs.unread > 0 && (
                <span className="dash-aprs-unread" data-testid="dash-aprs-unread">{aprs.unread}</span>
              )}
            </button>
          </div>
        </>
      )}
```

Append styles to `src/shell/DashboardRibbon.css` (or `AppShell.css` if the ribbon styles live there — match where `.connect-button` is defined):

```css
.dash-aprs-control { display: inline-flex; align-items: center; gap: 6px; font: inherit; font-size: 12px; cursor: pointer; background: var(--modem-accent-soft); border: 1px solid color-mix(in srgb, var(--modem-accent) 34%, transparent); color: var(--modem-accent); border-radius: 6px; padding: 4px 10px; }
.dash-aprs-control .dash-status-dot.is-off { background: var(--text-faint); }
.dash-aprs-unread { background: var(--modem-accent); color: var(--bg); font-size: 10px; font-weight: 700; border-radius: 9px; padding: 0 5px; }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/shell/DashboardRibbon.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/shell/DashboardRibbon.tsx src/shell/DashboardRibbon.test.tsx src/shell/DashboardRibbon.css
git commit -m "feat(shell): APRS status-strip control (entry 1) in DashboardRibbon"
```

---

## Task 5: Wire the dock view-model in AppShell + mount the panel/tabs

**Files:**
- Modify: `src/shell/AppShell.tsx` — lift the hook, add state, pass `aprs` to the ribbon, restructure the dock render, remove the `'aprs'` early-return.

This is the integration task. Each step is a focused edit; the whole thing is verified by the App-level test in Task 6.

- [ ] **Step 1: Lift `useAprsChat` + add dock view-model state**

Near the other `useState` hooks in `AppShell` (e.g. after `drawerOpen` at line 295), add:

```tsx
  // APRS tactical chat — lifted here (single instance) so the status-strip
  // control (unread/listening) and the dock panel share one state. (spec §1,§3)
  const aprs = useAprsChat();
  const [aprsOpen, setAprsOpen] = useState(false);
  const [dockTab, setDockTab] = useState<'aprs' | 'modem'>('aprs');
  const [aprsSeenAt, setAprsSeenAt] = useState(0);
  const aprsUnread = countUnread(aprs.threads, aprsSeenAt);
  const openAprsChat = useCallback(() => {
    setAprsOpen(true);
    setDockTab('aprs');
    setAprsSeenAt(Date.now());
  }, []);
```

Add imports at the top of AppShell.tsx (with the other `../aprs` imports / near line 162):

```tsx
import { useAprsChat } from '../aprs/useAprsChat';
import { countUnread } from '../aprs/aprsUnread';
import { AprsDockTabs } from '../aprs/AprsDockTabs';
```

> The existing lazy `AprsChatPanel` import (lines 162-164) stays; the panel now mounts in the dock (Step 4), not the content area.

- [ ] **Step 2: Pass `aprs` to the DashboardRibbon**

In the `<DashboardRibbon .../>` mount (lines 1074-1085), add the prop:

```tsx
          aprs={{ listening: aprs.listening, unread: aprsUnread, onOpen: openAprsChat }}
```

- [ ] **Step 3: Remove the `'aprs'` pseudo-folder early-return (content area)**

Replace the `'aprs'` branch (lines 1114-1121) so the content area no longer hosts APRS — delete the `) : selectedFolder === 'aprs' ? ( … <AprsChatPanel /> … </Suspense>` arm, leaving:

```tsx
        {selectedFolder === 'contacts' ? (
          <ContactsPanel />
        ) : (
          <>
```

- [ ] **Step 4: Make the dock visible for APRS + render tabs/panel**

Change the panes `className` condition (line 1089) so the 4th column also appears when APRS is open:

```tsx
        className={`panes${radioPanelMode !== null || aprsOpen ? ' panes--with-dock' : ''}${drawerOpen ? ' drawer-open' : ''}`}
```

Then change the dock mount. Replace the guard `{radioPanelMode !== null && (` (line 1225) with `{(radioPanelMode !== null || aprsOpen) && (`, and inside the `<RadioDrawer>` body, render the tab row (when APRS has been opened) and switch content. Wrap the existing per-mode panel block so it only renders on the Modem tab, and add the APRS panel for the APRS tab. Concretely, the `<RadioDrawer ...>` children become:

```tsx
            {aprsOpen && (
              <AprsDockTabs
                active={dockTab}
                unread={aprsUnread}
                modemEnabled={radioPanelMode !== null}
                onSelect={(tab) => {
                  setDockTab(tab);
                  if (tab === 'aprs') setAprsSeenAt(Date.now());
                }}
              />
            )}
            {aprsOpen && dockTab === 'aprs' ? (
              <Suspense fallback={null}>
                <AprsChatPanel threads={aprs.threads} listening={aprs.listening} send={aprs.send} />
              </Suspense>
            ) : (
              <>
                {/* existing per-mode radio panels — UNCHANGED, now the Modem tab */}
                {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'cms' && (
                  /* …all the existing TelnetRadioPanel / Packet / ARDOP / VARA / Placeholder blocks, verbatim… */
                )}
              </>
            )}
```

> Keep every existing per-mode block (lines 1242-1305) verbatim inside the `<>…</>` Modem branch — do not alter them. The only structural change is wrapping them in the `dockTab === 'aprs' ? <AprsChatPanel/> : <>…</>` ternary and adding the tab row above.

- [ ] **Step 5: Sanity-run the shell unit tests + typecheck**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/shell/ && pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat typecheck`
Expected: PASS (no `selectedFolder === 'aprs'` references remain to break; `RadioDrawer` still renders for radio modes).

- [ ] **Step 6: Commit**

```bash
git add src/shell/AppShell.tsx
git commit -m "feat(shell): re-home APRS chat into the shared dock (view-model + mount)"
```

---

## Task 6: App-level mount test (production path)

**Files:**
- Test: `src/shell/AppShell.aprs.test.tsx` (new) — follow the existing App-level test harness that wraps the shell in its providers (QueryClientProvider etc.). Mirror the provider scaffolding of the nearest existing `src/**/*.test.tsx` that renders `<AppShell />` or `<App />`.

- [ ] **Step 1: Write the failing test**

```tsx
// Mirror the provider wrapper used by the existing AppShell/App tests
// (QueryClientProvider + any Theme/Router context). Mock @tauri-apps/api so
// invoke/listen are inert.
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }));
// import { renderApp } from '<existing test util>';  // ← use the real harness

describe('APRS dock integration', () => {
  it('opens the chat in the right dock from the status-strip control', async () => {
    renderApp(); // production providers + <AppShell/>
    // No dock APRS panel until opened:
    expect(screen.queryByTestId('aprs-chat-panel')).not.toBeInTheDocument();
    await userEvent.click(screen.getByTestId('dash-aprs-control'));
    expect(await screen.findByTestId('aprs-chat-panel')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-dock-tabs')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it to verify it fails, then passes after wiring is correct**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/shell/AppShell.aprs.test.tsx`
Expected: initially RED if the harness import is stubbed; GREEN once `renderApp` points at the real provider wrapper and Task 5 is in place. (`aprs-chat-panel` is the `data-testid` already on `<section className="aprs-chat">`.)

- [ ] **Step 3: Commit**

```bash
git add src/shell/AppShell.aprs.test.tsx
git commit -m "test(aprs): App-level dock-integration test (production mount path)"
```

---

## Task 7: Remove the `'aprs'` pseudo-folder from the sidebar

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx:45-52` (`ADDRESS_ITEMS`)
- Test: `src/mailbox/FolderSidebar.test.tsx` (if it asserts the APRS row; otherwise add a negative assertion)

- [ ] **Step 1: Write/adjust the failing test** — assert the sidebar no longer offers an APRS row. In `src/mailbox/FolderSidebar.test.tsx` add:

```tsx
  it('does not render an APRS pseudo-folder (chat lives in the dock now)', () => {
    renderSidebar(); // existing helper / default props in that file
    expect(screen.queryByText(/APRS Chat/i)).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: FAIL — the 'APRS Chat' row still renders.

- [ ] **Step 3: Remove the entry** — in `src/mailbox/FolderSidebar.tsx`, delete the `'aprs'` item (and its comment) from `ADDRESS_ITEMS`, leaving:

```tsx
const ADDRESS_ITEMS: readonly PseudoFolderItem[] = [
  { id: 'contacts', label: 'Contacts', icon: '◉', enabled: true },
];
```

> Removing the array entry removes it from all three render paths (desktop nav, compact rail, flyout) since they map `ADDRESS_ITEMS`. If `'aprs'` appears in a union type for `MailboxFolderRef` it can stay (harmless) or be removed in a follow-up; nothing selects it now.

- [ ] **Step 4: Run it to verify it passes, full aprs+shell suites, typecheck**

```bash
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/mailbox/FolderSidebar.test.tsx src/aprs/ src/shell/
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat typecheck
```
Expected: all PASS; `tsc` clean.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/mailbox/FolderSidebar.test.tsx
git commit -m "feat(shell): remove the APRS pseudo-folder (chat now lives in the dock)"
```

---

## Self-Review (against the spec)

- **§1 shared switchable dock** → Task 5 (dock visible on `radioPanelMode || aprsOpen`; tab-switched content). ✅
- **§3 ① status-strip control** → Task 4 + wired in Task 5 Step 2. ✅
- **§3 ② dock tabs** → Task 3 + mounted Task 5 Step 4. ✅
- **§3 ③ View-menu** → explicitly deferred (Rust menu-id parity) — noted in "Out of scope". ✅ (gap is intentional + documented)
- **§5 UV-Pro control seam** → Task 1 `controlStrip` prop (rendered only when provided; AppShell passes nothing yet). ✅
- **§7 remove the pseudo-folder mount** → Task 5 Step 3 (early-return) + Task 7 (sidebar entry). ✅
- **Lifted state for unread** → Task 5 Step 1 (`useAprsChat` in AppShell) + Task 2 (`countUnread`). ✅
- **Type consistency:** `AprsChatPanelProps` (Task 1) used in Tasks 5/6; `DockTab` (Task 3) = `dockTab` state type (Task 5); `aprs` ribbon prop shape (Task 4) matches the object passed in Task 5 Step 2; `countUnread` (Task 2) used in Task 5. No dangling refs. ✅
- **Placeholder scan:** the per-mode radio-panel block in Task 5 Step 4 is referenced as "verbatim, unchanged" with an explicit instruction to keep lines 1242-1305 intact — this is a *preserve existing code* directive, not a placeholder. The App-level harness import (Task 6) and the ribbon/sidebar test helpers (Tasks 4/7) point at existing per-file patterns rather than inventing scaffolding — the one place the implementer must read a neighbor; flagged explicitly each time. ✅

---

## CI gate (after all tasks)

Parent pushes; GitHub CI `verify` (clippy `--all-targets` + full vitest + tsc + vite build, both arches) is authoritative. After Plan 1 + Plan 2 are green AND the operator's on-air smoke passes, PR #642 can be marked ready + merged. The UV-Pro control strip (the `controlStrip` seam) and entry ③ (View-menu) are tracked follow-ups, not blockers for this PR.
