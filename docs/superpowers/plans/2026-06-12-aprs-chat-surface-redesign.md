# APRS Chat — Surface Redesign Implementation Plan (Plan 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the honest, legible surface affordances to the existing APRS chat panel — per-message timestamps, an `Acked HH:MM` delivery state, the `n / 67` airtime counter, and a quiet open-channel cue — in tuxlink's clean Office register, without moving where the panel is mounted.

**Architecture:** Frontend-only, all within `src/aprs/`. The data already carries `ChatMessage.at` (client-stamped on RX/TX); this plan surfaces it and adds one new field (`ackedAt`) captured on the `acked` transition. No backend, types-bridge, or shell changes. Plan 2 (dock re-home + entry points) is the companion plan; this plan leaves the panel mounted in its current `'aprs'` pseudo-folder slot and is independently shippable + green.

**Tech Stack:** React + TypeScript, Vitest + @testing-library/react. The whole plan is local-TDD-able (vitest is fast JS; no cargo). Run vitest **scoped** to `src/aprs/` and reap workers after (`pkill -f vitest` if a sweep is interrupted) per the project's vitest-zombie guidance.

**Commit discipline:** Every commit needs the `Agent: <moniker>` trailer (CLAUDE.md). Under subagent-driven execution, the **parent** runs the commit (a dispatched subagent's cwd resets to the repo root each Bash call, so the main-checkout hook denies its in-worktree commit) — the subagent codes + gates + STOPs uncommitted. Work in the existing worktree `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/aprs/aprsTypes.ts` | Wire/UI types | Add `ackedAt?: number` to `ChatMessage` |
| `src/aprs/useAprsChat.ts` | Event hook + thread state | Stamp `ackedAt` when a message transitions to `acked` |
| `src/aprs/useAprsChat.test.ts` | Hook tests | + test for the `ackedAt` stamp |
| `src/aprs/AprsChatPanel.tsx` | The panel surface | Add timestamp display, `Acked HH:MM`, `n/67` counter, open-channel cue; export `formatTime` |
| `src/aprs/AprsChatPanel.css` | Panel styles | Styles for the new meta row, counter, cue (Office register) |
| `src/aprs/AprsChatPanel.test.tsx` | Panel tests | Switch to a handler-capturing event mock; + tests for the new surface |

---

## Task 1: Capture the ACK timestamp

**Files:**
- Modify: `src/aprs/aprsTypes.ts:31-38`
- Modify: `src/aprs/useAprsChat.ts:60-79` (`applyState`) and `:120-122` (the state handler)
- Test: `src/aprs/useAprsChat.test.ts`

- [ ] **Step 1: Write the failing test**

Add this `it(...)` block inside the `describe('useAprsChat', ...)` in `src/aprs/useAprsChat.test.ts` (after the existing acked test, before the closing `});`):

```ts
  it('stamps ackedAt when a message transitions to acked', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello'); });
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    const msg = result.current.threads['KK6XYZ'].messages.find((x) => x.msgid === 'A1');
    expect(msg?.state).toBe('acked');
    expect(typeof msg?.ackedAt).toBe('number');
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/useAprsChat.test.ts`
Expected: FAIL — `expected "undefined" to be "number"` (no `ackedAt` is set yet).

- [ ] **Step 3: Add the `ackedAt` field to the type**

In `src/aprs/aprsTypes.ts`, change the `ChatMessage` interface (the `at` field block) to add `ackedAt`:

```ts
export interface ChatMessage {
  id: string;
  direction: 'in' | 'out';
  text: string;
  msgid: string | null;
  state?: DeliveryState;
  /// Local epoch-ms when tuxlink received (inbound) or sent (outbound) this
  /// message. Honest client-stamp — NOT a claimed origin time.
  at: number;
  /// Local epoch-ms when the `acked` transition arrived. Set only on ACK so the
  /// UI can show "Acked HH:MM" (the round-trip close time). Undefined otherwise.
  ackedAt?: number;
}
```

- [ ] **Step 4: Stamp `ackedAt` in the hook**

In `src/aprs/useAprsChat.ts`, change `applyState` to accept the transition time and set `ackedAt` on an `acked` transition. Replace the signature + the `messages[idx] = ...` line:

```ts
function applyState(
  threads: Record<string, Thread>,
  msgid: string,
  state: StateChangeDto['state'],
  at: number,
): Record<string, Thread> {
  const next: Record<string, Thread> = {};
  let changed = false;
  for (const [call, thread] of Object.entries(threads)) {
    const idx = thread.messages.findIndex((m) => m.msgid === msgid);
    if (idx === -1) {
      next[call] = thread;
      continue;
    }
    const messages = thread.messages.slice();
    messages[idx] = {
      ...messages[idx],
      state,
      ...(state === 'acked' ? { ackedAt: at } : {}),
    };
    next[call] = { callsign: thread.callsign, messages };
    changed = true;
  }
  return changed ? next : threads;
}
```

Then update the call site (the `aprs-message:state` subscriber, currently around line 120):

```ts
    subscribe<StateChangeDto>('aprs-message:state', (payload) => {
      setThreads((prev) => applyState(prev, payload.msgid, payload.state, Date.now()));
    });
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/useAprsChat.test.ts`
Expected: PASS (all 4 tests, including the new one).

- [ ] **Step 6: Commit**

```bash
git add src/aprs/aprsTypes.ts src/aprs/useAprsChat.ts src/aprs/useAprsChat.test.ts
git commit -m "feat(aprs): capture ACK timestamp (ackedAt) on the acked transition"
```

---

## Task 2: Display per-message timestamps + export `formatTime`

**Files:**
- Modify: `src/aprs/AprsChatPanel.tsx:53-64` (`Bubble`) + add `formatTime`
- Modify: `src/aprs/AprsChatPanel.css` (meta row)
- Test: `src/aprs/AprsChatPanel.test.tsx`

- [ ] **Step 1: Switch the panel test to a handler-capturing event mock + write the failing test**

Replace the two `vi.mock(...)` lines at the top of `src/aprs/AprsChatPanel.test.tsx` (lines 3-4) with a handler-capturing event mock (mirrors `useAprsChat.test.ts`) so tests can inject inbound messages, and import `act`:

```ts
import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => { delete handlers[name]; });
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));
import { AprsChatPanel, formatTime } from './AprsChatPanel';
```

Then add these tests inside the `describe`:

```ts
  it('formats an epoch-ms timestamp as a short HH:MM time', () => {
    expect(formatTime(new Date(2026, 5, 12, 14, 8).getTime())).toMatch(/\b\d{1,2}:\d{2}\b/);
  });

  it('renders a timestamp on each message bubble', async () => {
    render(<AprsChatPanel />);
    await act(async () => {});
    act(() => { handlers['aprs-message:new']?.({ payload: { sender: 'KK6XYZ', text: 'ping', msgid: '04' } }); });
    expect(await screen.findByTestId('aprs-bubble-time')).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: FAIL — `formatTime` is not exported / `aprs-bubble-time` testid not found.

- [ ] **Step 3: Add `formatTime` and the timestamp to `Bubble`**

In `src/aprs/AprsChatPanel.tsx`, add the exported helper above `DeliveryChip` (after the `CHIP` const):

```tsx
/// Format a local epoch-ms timestamp as a short HH:MM clock time, honoring the
/// operator's locale. Exported for unit testing.
export function formatTime(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
```

Replace the `Bubble` function (lines 53-64) with a version that renders a meta row carrying the time (and the delivery chip for outbound):

```tsx
function Bubble({ msg }: { msg: ChatMessage }) {
  return (
    <div
      className={`aprs-bubble aprs-bubble-${msg.direction}`}
      data-testid="aprs-bubble"
      data-direction={msg.direction}
    >
      <span className="aprs-bubble-text">{msg.text}</span>
      <span className="aprs-bubble-meta">
        <span className="aprs-bubble-time" data-testid="aprs-bubble-time">
          {formatTime(msg.at)}
        </span>
        {msg.direction === 'out' && msg.state && <DeliveryChip state={msg.state} msg={msg} />}
      </span>
    </div>
  );
}
```

> Note: `DeliveryChip` gains a `msg` prop in Task 3; for this task it is passed but unused. To keep Task 2 green on its own, update the `DeliveryChip` signature now to accept and ignore it: change `function DeliveryChip({ state }: { state: DeliveryState })` to `function DeliveryChip({ state }: { state: DeliveryState; msg?: ChatMessage })` (the extra prop is optional, so passing `msg` typechecks; Task 3 uses it).

- [ ] **Step 4: Add meta-row styles (Office register)**

In `src/aprs/AprsChatPanel.css`, replace the `.aprs-bubble-text` rule with the text rule plus a meta row, keeping the existing clean look:

```css
.aprs-bubble-text {
  white-space: pre-wrap;
}
.aprs-bubble-meta {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  align-self: flex-end;
}
.aprs-bubble-in .aprs-bubble-meta {
  align-self: flex-start;
}
.aprs-bubble-time {
  font-size: 10px;
  color: var(--text-faint, #94a3b8);
  font-variant-numeric: tabular-nums;
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: PASS (the 3 original tests + the 2 new ones).

- [ ] **Step 6: Commit**

```bash
git add src/aprs/AprsChatPanel.tsx src/aprs/AprsChatPanel.css src/aprs/AprsChatPanel.test.tsx
git commit -m "feat(aprs): show per-message timestamps in the chat surface"
```

---

## Task 3: `Acked HH:MM` delivery state

**Files:**
- Modify: `src/aprs/AprsChatPanel.tsx:33-51` (`CHIP` + `DeliveryChip`)
- Test: `src/aprs/AprsChatPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

Add inside the `describe` in `src/aprs/AprsChatPanel.test.tsx`:

```ts
  it('shows the ACK time on an acked outbound bubble', async () => {
    render(<AprsChatPanel />);
    await act(async () => {});
    await act(async () => {
      const sendBtn = screen.getByRole('button', { name: /send/i });
      const call = screen.getByTestId('aprs-composer-callsign');
      const text = screen.getByTestId('aprs-composer-text');
      // drive a send so an outbound bubble (msgid A1) exists
      (call as HTMLInputElement).value = 'KK6XYZ';
      call.dispatchEvent(new Event('input', { bubbles: true }));
      (text as HTMLInputElement).value = 'hello';
      text.dispatchEvent(new Event('input', { bubbles: true }));
    });
    // NOTE: simpler — inject the acked state directly via the captured handler
    // after the optimistic bubble is in place:
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    expect(await screen.findByText(/^Acked \d{1,2}:\d{2}$/)).toBeInTheDocument();
  });
```

> If driving the composer via raw DOM events proves flaky under the controlled inputs, prefer firing the send through `@testing-library/user-event` (already a dev dep): `await userEvent.type(screen.getByTestId('aprs-composer-callsign'), 'KK6XYZ')`, etc., then click Send. Keep the final assertion (`/^Acked \d{1,2}:\d{2}$/`) identical.

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: FAIL — the chip reads `Acked` (no time).

- [ ] **Step 3: Render the ACK time in `DeliveryChip`**

In `src/aprs/AprsChatPanel.tsx`, replace `DeliveryChip` (and keep `CHIP` as-is) so the `acked` chip appends its time when known:

```tsx
function DeliveryChip({ state, msg }: { state: DeliveryState; msg?: ChatMessage }) {
  const chip = CHIP[state];
  const label =
    state === 'acked' && msg?.ackedAt != null
      ? `${chip.label} ${formatTime(msg.ackedAt)}`
      : chip.label;
  return (
    <span
      className={`aprs-chip aprs-chip-${chip.variant}`}
      data-testid="aprs-delivery-chip"
      data-state={state}
    >
      {label}
    </span>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/aprs/AprsChatPanel.tsx src/aprs/AprsChatPanel.test.tsx
git commit -m "feat(aprs): show ACK round-trip time (Acked HH:MM) on delivered bubbles"
```

---

## Task 4: The `n / 67` airtime counter

**Files:**
- Modify: `src/aprs/AprsChatPanel.tsx` (the composer `<form>`, lines ~206-245) + a `const APRS_TEXT_MAX = 67;`
- Modify: `src/aprs/AprsChatPanel.css` (counter)
- Test: `src/aprs/AprsChatPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

Add inside the `describe`:

```ts
  it('shows a live n/67 character counter for the message field', async () => {
    const { default: userEvent } = await import('@testing-library/user-event');
    render(<AprsChatPanel />);
    await userEvent.type(screen.getByTestId('aprs-composer-text'), 'hello');
    expect(screen.getByTestId('aprs-char-count')).toHaveTextContent('5 / 67');
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: FAIL — no `aprs-char-count` element.

- [ ] **Step 3: Add the counter constant + element**

In `src/aprs/AprsChatPanel.tsx`, add near the top (after imports):

```tsx
/// APRS message text budget — the per-message character cap that makes bounded
/// airtime real (matches the backend codec's ≤67 text limit).
const APRS_TEXT_MAX = 67;
```

In the composer `<form>`, immediately before the submit `<button>` (the Send button), add the counter:

```tsx
            <span
              className={`aprs-char-count ${text.length > APRS_TEXT_MAX ? 'aprs-char-count-over' : ''}`}
              data-testid="aprs-char-count"
              aria-live="polite"
            >
              {text.length} / {APRS_TEXT_MAX}
            </span>
```

- [ ] **Step 4: Style the counter**

Append to `src/aprs/AprsChatPanel.css`:

```css
.aprs-char-count {
  align-self: center;
  flex-shrink: 0;
  font-size: 11px;
  color: var(--text-faint, #94a3b8);
  font-variant-numeric: tabular-nums;
  padding: 0 2px;
}
.aprs-char-count-over {
  color: var(--accent-2, #fbbf24);
  font-weight: 600;
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/aprs/AprsChatPanel.tsx src/aprs/AprsChatPanel.css src/aprs/AprsChatPanel.test.tsx
git commit -m "feat(aprs): add the 67-char airtime counter to the composer"
```

---

## Task 5: The quiet open-channel cue

**Files:**
- Modify: `src/aprs/AprsChatPanel.tsx` (panel header region, after the listening indicator)
- Modify: `src/aprs/AprsChatPanel.css` (cue)
- Test: `src/aprs/AprsChatPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

Add inside the `describe`:

```ts
  it('shows a quiet open-channel honesty cue', () => {
    render(<AprsChatPanel />);
    const cue = screen.getByTestId('aprs-open-channel');
    expect(cue).toBeInTheDocument();
    expect(cue).toHaveTextContent(/heard by all stations in range/i);
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: FAIL — no `aprs-open-channel` element.

- [ ] **Step 3: Add the cue to the header**

In `src/aprs/AprsChatPanel.tsx`, inside the `<header className="aprs-chat-h">`, after the `aprs-listening` indicator `<span>` and before the Start/Stop `<button>`, add:

```tsx
        <span className="aprs-open-channel" data-testid="aprs-open-channel" title="APRS is received by every station in range and digipeated — not a private channel.">
          Heard by all stations in range
        </span>
```

- [ ] **Step 4: Style the cue (quiet, not a loud badge)**

Append to `src/aprs/AprsChatPanel.css`:

```css
.aprs-open-channel {
  font-size: 11px;
  color: var(--text-faint, #94a3b8);
  font-style: italic;
  white-space: nowrap;
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/AprsChatPanel.test.tsx`
Expected: PASS.

- [ ] **Step 6: Run the full aprs suite + typecheck, then commit**

```bash
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat vitest run src/aprs/
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-2f2n-aprs-tactical-chat typecheck
git add src/aprs/AprsChatPanel.tsx src/aprs/AprsChatPanel.css src/aprs/AprsChatPanel.test.tsx
git commit -m "feat(aprs): add the open-channel honesty cue to the chat header"
```

Expected: all `src/aprs/` tests PASS; `tsc --noEmit` clean. (Reap any vitest workers: `pkill -f vitest` if a run was interrupted.)

---

## Self-Review (against the spec)

- **Spec §4 timestamps** → Task 2. ✅
- **Spec §4 `Acked HH:MM`** → Tasks 1 + 3. ✅
- **Spec §4 `n/67` counter** → Task 4. ✅
- **Spec §4 open-channel cue** → Task 5. ✅
- **Spec §4 "no fabricated frequency"** → not introduced here (no frequency element exists in the panel; the dock control strip is Plan 2 + native backend). ✅
- **Spec §4 retain four honest states + no-bubble-on-rejected** → untouched (`CHIP` keeps all four; `useAprsChat.send` reject path unchanged). ✅
- **Type consistency** → `ackedAt?: number` defined in Task 1 is the only new field; `DeliveryChip`'s `msg?: ChatMessage` prop is introduced in Task 2's note and used in Task 3. `formatTime` exported in Task 2, reused in Task 3. `APRS_TEXT_MAX` defined in Task 4. No dangling references. ✅
- **Placeholder scan** → no TBD/TODO; every code step shows the code. ✅
- **Out of scope (correctly deferred to Plan 2)** → dock re-home, status-strip control, dock tabs, View-menu item, removing the `'aprs'` pseudo-folder, the UV-Pro control strip. The panel stays mounted in the `'aprs'` pseudo-folder for this plan, so all surface work is verifiable in isolation.

---

## CI gate (after all tasks)

The parent pushes the branch; GitHub CI runs `verify` (clippy `--all-targets` + full vitest + tsc + vite build) on both arches. The `verify` gate, not the local scoped runs, is authoritative. Do not mark PR #642 ready on this plan alone — Plan 2 (dock re-home + entry points) and the operator on-air smoke remain.
