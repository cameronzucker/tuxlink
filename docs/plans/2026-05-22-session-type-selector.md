# Session-type connection selector — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Connections sidebar's flat connection list with a two-level accordion (session **type** → **protocol**) that routes each selection to a rich connection-management pane in the reading-pane slot.

**Architecture:** Frontend-only (React/TS). The `Connections` section of `FolderSidebar` becomes an accordion of the five routing intents; expanding lists that intent's protocols; selecting a protocol sets `selectedConnection` to a `{sessionType, protocol}` key; `AppShell` dispatches the correct pane. The Telnet-CMS pane hosts the CMS host/transport/login controls relocated from `SettingsPanel` (reusing PR #122's `config_set_connect`/`config_read` backend, unchanged). Packet panes reuse `PacketConnectionPanel`. Unbuilt intents render with `soon` protocols + a stub pane.

**Tech Stack:** React 18 + TypeScript, Vitest + @testing-library/react, Tauri `invoke`. No backend/Rust changes (PR #122 backend reused as-is).

**Spec:** `docs/design/2026-05-22-session-type-selector-design.md`

---

## Reconciliation with PR #122 (`tuxlink-3o0`) — do this first

PR #122's branch `bd-tuxlink-3o0/cms-switcher` carries the CMS backend (`config_set_connect`, `resolve_cms_host`, the connect-exercise test) **and** the SettingsPanel CMS fieldset. This epic reuses the backend and *removes* the SettingsPanel fieldset (relocating it to the Telnet-CMS pane). Per operator decision (2026-05-22), #122 is **superseded by this epic's PR**, not merged on its own.

### Task 0: Base the epic branch on #122's backend

**Files:** none (git topology)

- [ ] **Step 1:** From the epic worktree, merge the #122 branch to inherit the backend:
```bash
git -C worktrees/bd-tuxlink-3pb-session-selector merge --no-ff origin/bd-tuxlink-3o0/cms-switcher \
  -m "merge: inherit tuxlink-3o0 CMS backend into session-selector epic (supersedes #122 UI)"
```
- [ ] **Step 2:** Verify the backend is present:
```bash
git -C worktrees/bd-tuxlink-3pb-session-selector grep -l "config_set_connect" src-tauri/src/ui_commands.rs
```
Expected: the file path prints (command exists on the branch).
- [ ] **Step 3:** Confirm baseline gates green before changing anything:
```bash
pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run
cargo test --lib --manifest-path worktrees/bd-tuxlink-3pb-session-selector/src-tauri/Cargo.toml
```
Expected: 437 frontend / 314 backend pass (the #122 baseline).
- [ ] **Step 4:** Add the bd dependency edge so the tracker reflects the supersession:
```bash
bd dep add tuxlink-3pb tuxlink-3o0   # 3pb consumes 3o0's backend
```

---

## File structure

- **Create** `src/connections/sessionTypes.ts` — the session-type/protocol catalog + `ConnectionKey` type. One responsibility: the static model of intents × protocols + build status.
- **Create** `src/connections/sessionTypes.test.ts`
- **Create** `src/connections/TelnetCmsPanel.tsx` — Winlink-CMS→Telnet pane (relocated CMS controls). Container + presentational.
- **Create** `src/connections/TelnetCmsPanel.test.tsx`
- **Create** `src/connections/TelnetCmsPanel.css`
- **Create** `src/connections/StubPanel.tsx` — "coming soon" pane for unbuilt intents.
- **Modify** `src/mailbox/FolderSidebar.tsx` — Connections section → accordion (generalize `ConnectionKey`).
- **Modify** `src/mailbox/FolderSidebar.test.tsx`
- **Modify** `src/shell/AppShell.tsx` — `selectedConnection: ConnectionKey | null`; pane dispatch.
- **Modify** `src/shell/AppShell.test.tsx`
- **Modify** `src/shell/SettingsPanel.tsx` — remove the CMS Server fieldset (+ its state/handlers/constants).
- **Modify** `src/shell/SettingsPanel.test.tsx` — drop CMS-fieldset tests.
- **Modify** `src/packet/PacketConnectionPanel.tsx` — accept an `intent` prop (`'cms-gateway' | 'p2p'`) gating Listen + secure-login copy.

---

## Task 1: Session-type / protocol catalog

**Files:**
- Create: `src/connections/sessionTypes.ts`
- Test: `src/connections/sessionTypes.test.ts`

- [ ] **Step 1: Write the failing test**
```typescript
// src/connections/sessionTypes.test.ts
import { describe, it, expect } from 'vitest';
import { SESSION_TYPES, protocolsFor, isBuilt, type ConnectionKey } from './sessionTypes';

describe('session-type catalog', () => {
  it('lists the five routing intents in order', () => {
    expect(SESSION_TYPES.map((s) => s.id)).toEqual([
      'cms', 'radio-only', 'post-office', 'p2p', 'network-po',
    ]);
  });
  it('CMS offers Telnet (built) and Packet (built); VARA shown but not built', () => {
    const protos = protocolsFor('cms');
    expect(protos.find((p) => p.id === 'telnet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'packet')?.built).toBe(true);
    expect(protos.find((p) => p.id === 'vara-hf')?.built).toBe(false);
  });
  it('isBuilt is false for any protocol under an unbuilt intent (radio-only)', () => {
    const key: ConnectionKey = { sessionType: 'radio-only', protocol: 'packet' };
    expect(isBuilt(key)).toBe(false);
  });
  it('isBuilt is true for cms+telnet, cms+packet, p2p+packet', () => {
    expect(isBuilt({ sessionType: 'cms', protocol: 'telnet' })).toBe(true);
    expect(isBuilt({ sessionType: 'cms', protocol: 'packet' })).toBe(true);
    expect(isBuilt({ sessionType: 'p2p', protocol: 'packet' })).toBe(true);
  });
});
```
- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run src/connections/sessionTypes.test.ts`
Expected: FAIL — cannot resolve `./sessionTypes`.

- [ ] **Step 3: Write minimal implementation**
```typescript
// src/connections/sessionTypes.ts
export type SessionTypeId = 'cms' | 'radio-only' | 'post-office' | 'p2p' | 'network-po';
export type ProtocolId = 'telnet' | 'packet' | 'vara-hf' | 'vara-fm';
export interface ConnectionKey { sessionType: SessionTypeId; protocol: ProtocolId; }

export interface ProtocolEntry { id: ProtocolId; label: string; built: boolean; }
export interface SessionTypeEntry {
  id: SessionTypeId; label: string; blurb: string; built: boolean; protocols: ProtocolEntry[];
}

const PKT = { id: 'packet' as const, label: 'Packet (AX.25)' };
const TEL = { id: 'telnet' as const, label: 'Telnet' };
const VHF = { id: 'vara-hf' as const, label: 'VARA HF' };
const VFM = { id: 'vara-fm' as const, label: 'VARA FM' };

// `built` on a protocol = the (sessionType, protocol) pane has UI + backend today.
export const SESSION_TYPES: SessionTypeEntry[] = [
  { id: 'cms', label: 'Winlink (CMS)', blurb: 'Sync your global mailbox. Credentialed secure-login.', built: true,
    protocols: [{ ...TEL, built: true }, { ...PKT, built: true }, { ...VHF, built: false }, { ...VFM, built: false }] },
  { id: 'radio-only', label: 'Radio-only', blurb: 'RF-only Hybrid network (pool R).', built: false,
    protocols: [{ ...TEL, built: false }, { ...PKT, built: false }, { ...VHF, built: false }, { ...VFM, built: false }] },
  { id: 'post-office', label: 'Post Office', blurb: 'Local RMS Relay store-and-forward (pool L).', built: false,
    protocols: [{ ...TEL, built: false }, { ...PKT, built: false }] },
  { id: 'p2p', label: 'Peer-to-peer', blurb: 'Direct station — no creds.', built: true,
    protocols: [{ ...PKT, built: true }, { ...TEL, built: false }, { ...VHF, built: false }, { ...VFM, built: false }] },
  { id: 'network-po', label: 'Network Post Office', blurb: 'Local RMS Relay network.', built: false,
    protocols: [{ ...TEL, built: false }] },
];

export function protocolsFor(id: SessionTypeId): ProtocolEntry[] {
  return SESSION_TYPES.find((s) => s.id === id)?.protocols ?? [];
}
export function isBuilt(key: ConnectionKey): boolean {
  return protocolsFor(key.sessionType).find((p) => p.id === key.protocol)?.built ?? false;
}
```
- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run src/connections/sessionTypes.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**
```bash
git -C worktrees/bd-tuxlink-3pb-session-selector add src/connections/sessionTypes.ts src/connections/sessionTypes.test.ts
git -C worktrees/bd-tuxlink-3pb-session-selector commit -m "feat(connections): session-type/protocol catalog (tuxlink-3pb)

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Connections sidebar accordion

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (Connections section + `ConnectionKey` import)
- Test: `src/mailbox/FolderSidebar.test.tsx`

Generalize the sidebar's `ConnectionKey` (drop the local `'packet'` type; import from `sessionTypes`). Replace the static `CONNECTION_ITEMS` + single Packet button with an accordion: each `SESSION_TYPES` entry is an expandable header (`data-testid="sess-<id>"`, `aria-expanded`); expanding renders its protocols (`data-testid="proto-<sessType>-<protoId>"`). Clicking a built protocol calls `onSelectConnection({sessionType, protocol})`; `soon` protocols are `disabled`. The active protocol row carries `aria-current` + the existing transport-state dot.

- [ ] **Step 1: Write the failing test** (append to `FolderSidebar.test.tsx`)
```typescript
import { SESSION_TYPES } from '../connections/sessionTypes';

describe('FolderSidebar — Connections accordion', () => {
  it('renders a header per session type, collapsed by default', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    for (const s of SESSION_TYPES) {
      expect(screen.getByTestId(`sess-${s.id}`)).toHaveAttribute('aria-expanded', 'false');
    }
  });
  it('expands a session type to reveal its protocols', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('proto-cms-telnet')).toBeInTheDocument();
    expect(screen.getByTestId('proto-cms-packet')).toBeInTheDocument();
  });
  it('selecting a built protocol calls onSelectConnection with the key', () => {
    const onSelectConnection = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'telnet' });
  });
  it('a "soon" protocol is disabled and does not fire selection', () => {
    const onSelectConnection = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    const vara = screen.getByTestId('proto-cms-vara-hf');
    expect(vara).toBeDisabled();
    fireEvent.click(vara);
    expect(onSelectConnection).not.toHaveBeenCalled();
  });
});
```
- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: FAIL — `sess-cms` testid not found.

- [ ] **Step 3: Implement the accordion** in `FolderSidebar.tsx`.
  - Change `export type ConnectionKey = 'packet'` → `import type { ConnectionKey } from '../connections/sessionTypes'` and re-export.
  - Keep the `Mailbox` section unchanged.
  - Replace the `Connections` block with: for each `SESSION_TYPES` entry render a `<button data-testid={`sess-${s.id}`} aria-expanded={expanded[s.id]} onClick={toggle}>` header (chevron + label); when expanded, map `s.protocols` to `<button data-testid={`proto-${s.id}-${p.id}`} disabled={!p.built} aria-current={isActive} onClick={() => onSelectConnection?.({sessionType: s.id, protocol: p.id})}>` rows. Track `expanded` in `useState<Record<SessionTypeId, boolean>>`. Active = `selectedConnection?.sessionType===s.id && .protocol===p.id`. Keep the green `packetState` dot on the active row (now keyed off `selectedConnection`).
  - Styling: reuse `.nav` classes in `AppShell.css`; add `.nav.proto { padding-left: 20px }` and a chevron span.
- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run src/mailbox/FolderSidebar.test.tsx`
Expected: PASS (existing Mailbox tests + 4 new).

- [ ] **Step 5: Commit**
```bash
git -C worktrees/bd-tuxlink-3pb-session-selector add src/mailbox/FolderSidebar.tsx src/mailbox/FolderSidebar.test.tsx
git -C worktrees/bd-tuxlink-3pb-session-selector commit -m "feat(connections): session-type accordion in the sidebar (tuxlink-3pb)

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: AppShell pane dispatch

**Files:**
- Modify: `src/shell/AppShell.tsx` (`selectedConnection` type + reading-pane branch)
- Test: `src/shell/AppShell.test.tsx`

Change `selectedConnection` state to `ConnectionKey | null`. Replace the `selectedConnection === 'packet' ? <PacketConnectionPanelContainer/> : <MessageView/>` ternary with a dispatcher: built `cms+telnet` → `<TelnetCmsPanelContainer/>`; `cms+packet` → `<PacketConnectionPanelContainer baseCall=… intent="cms-gateway"/>`; `p2p+packet` → `<PacketConnectionPanelContainer … intent="p2p"/>`; any unbuilt key → `<StubPanel sessionType=… protocol=…/>`; null → `<MessageView/>`.

- [ ] **Step 1: Write the failing test** (append to `AppShell.test.tsx`, following its existing render/mocks)
```typescript
it('renders the Telnet-CMS pane when cms+telnet is selected', async () => {
  renderAppShell();                                   // existing helper
  fireEvent.click(screen.getByTestId('sess-cms'));
  fireEvent.click(screen.getByTestId('proto-cms-telnet'));
  expect(await screen.findByTestId('telnet-cms-panel-root')).toBeInTheDocument();
});
it('renders a stub pane for an unbuilt key (radio-only+telnet)', async () => {
  renderAppShell();
  fireEvent.click(screen.getByTestId('sess-radio-only'));
  // radio-only protocols are all "soon" → disabled; assert the stub via a forced-select unit
  // (covered in StubPanel.test); here assert the row is disabled
  expect(screen.getByTestId('proto-radio-only-telnet')).toBeDisabled();
});
```
- [ ] **Step 2: Run** `pnpm -C … exec vitest run src/shell/AppShell.test.tsx` → FAIL (`telnet-cms-panel-root` absent).
- [ ] **Step 3:** Implement the dispatcher in `AppShell.tsx` (import `TelnetCmsPanelContainer`, `StubPanel`, `isBuilt`). Note: `TelnetCmsPanelContainer` lands in Task 4 — if executing strictly in order, stub it as `() => <div data-testid="telnet-cms-panel-root"/>` here and replace in Task 4. (Subagent-driven execution: do Task 4 first, then this dispatch.)
- [ ] **Step 4: Run** the test → PASS.
- [ ] **Step 5: Commit** (`feat(connections): AppShell pane dispatch on {sessionType,protocol} (tuxlink-3pb)` + trailers).

---

## Task 4: Telnet-CMS pane (relocate PR #122 controls)

**Files:**
- Create: `src/connections/TelnetCmsPanel.tsx`, `.test.tsx`, `.css`
- Reference (move from): `src/shell/SettingsPanel.tsx:191-248` (the CMS Server fieldset + `persistConnect` + `CMS_HOST_QUICK_PICKS` + `CMS_TRANSPORT_OPTIONS`).

Container/presentational split (mirror `PacketConnectionPanel`): `TelnetCmsPanelContainer` loads `config_read` on mount → `{host, transport}`, persists via `invoke('config_set_connect', {host, transport})`; `TelnetCmsPanel` is the controlled view (host input commits on blur/Enter, transport radios commit immediately). Root `data-testid="telnet-cms-panel-root"`, class `reading-pane`. Reuse testids `conn-host`, `name="cms-transport"` so the relocated coverage is continuous.

- [ ] **Step 1: Write the failing test**
```typescript
// src/connections/TelnetCmsPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
import { invoke } from '@tauri-apps/api/core';
import { TelnetCmsPanel } from './TelnetCmsPanel';

describe('<TelnetCmsPanel>', () => {
  it('renders in a reading-pane root with the host + transport controls', () => {
    render(<TelnetCmsPanel host="cms-z.winlink.org" transport="Telnet" onPersist={vi.fn()} />);
    const root = screen.getByTestId('telnet-cms-panel-root');
    expect(root.className).toContain('reading-pane');
    expect((screen.getByTestId('conn-host') as HTMLInputElement).value).toBe('cms-z.winlink.org');
  });
  it('commits host on blur via onPersist', () => {
    const onPersist = vi.fn();
    render(<TelnetCmsPanel host="" transport="CmsSsl" onPersist={onPersist} />);
    const input = screen.getByTestId('conn-host');
    fireEvent.change(input, { target: { value: 'server.winlink.org' } });
    fireEvent.blur(input);
    expect(onPersist).toHaveBeenCalledWith({ host: 'server.winlink.org', transport: 'CmsSsl' });
  });
  it('selecting a transport radio commits immediately', () => {
    const onPersist = vi.fn();
    render(<TelnetCmsPanel host="cms-z.winlink.org" transport="CmsSsl" onPersist={onPersist} />);
    fireEvent.click(screen.getByDisplayValue('Telnet'));   // the Plaintext·8772 radio (value="Telnet")
    expect(onPersist).toHaveBeenCalledWith({ host: 'cms-z.winlink.org', transport: 'Telnet' });
  });
});
```
- [ ] **Step 2: Run** `pnpm -C … exec vitest run src/connections/TelnetCmsPanel.test.tsx` → FAIL (module missing).
- [ ] **Step 3:** Implement `TelnetCmsPanel.tsx` by relocating the JSX/handlers from `SettingsPanel.tsx:191-248` into the controlled presentational component + a container that wires `config_read`/`config_set_connect` (copy the `persistConnect` logic verbatim — host trim, both-fields-together write). Add the `CMS_HOST_QUICK_PICKS` / `CMS_TRANSPORT_OPTIONS` consts to this file.
- [ ] **Step 4: Run** the test → PASS (3 tests).
- [ ] **Step 5: Commit** (`feat(connections): Telnet-CMS connection pane (relocated from SettingsPanel) (tuxlink-3pb)` + trailers).

---

## Task 5: Remove the CMS fieldset from SettingsPanel

**Files:**
- Modify: `src/shell/SettingsPanel.tsx` (delete CMS fieldset + `host`/`transport` state, `persistConnect`, `CMS_HOST_QUICK_PICKS`, `CMS_TRANSPORT_OPTIONS`; drop `host`/`transport` from the local `SettingsView`)
- Modify: `src/shell/SettingsPanel.test.tsx` (delete the "CMS Server fieldset" tests, ~lines 73-110)

- [ ] **Step 1:** Delete the CMS-fieldset tests from `SettingsPanel.test.tsx`.
- [ ] **Step 2: Run** `pnpm -C … exec vitest run src/shell/SettingsPanel.test.tsx` → PASS (GPS/privacy tests only; the deleted tests are gone, none failing).
- [ ] **Step 3:** Remove the CMS fieldset + its state/handlers/constants from `SettingsPanel.tsx`. Keep GPS state + precision fieldsets untouched. `config_read` may still return `host`/`transport`; simply stop consuming them here.
- [ ] **Step 4: Run** `pnpm -C … exec vitest run src/shell/SettingsPanel.test.tsx` and `pnpm -C … exec tsc --noEmit` → PASS / clean.
- [ ] **Step 5: Commit** (`refactor(settings): drop CMS fieldset — relocated to Telnet-CMS pane (tuxlink-3pb)` + trailers).

---

## Task 6: Packet panes — intent parameter (CMS-gateway vs P2P)

**Files:**
- Modify: `src/packet/PacketConnectionPanel.tsx` (add `intent?: 'cms-gateway' | 'p2p'`)
- Test: `src/packet/PacketConnectionPanel.test.tsx`

P2P shows the Listen control + "no login" copy; CMS-gateway hides Listen (CMS is connect-only) and notes secure-login. Default `intent='p2p'` preserves today's behavior.

- [ ] **Step 1: Write the failing test**
```typescript
it('hides the Listen control under the cms-gateway intent', () => {
  render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" intent="cms-gateway" />);
  expect(screen.queryByTestId('listen-action')).toBeNull();
});
it('shows the Listen control under the p2p intent (default)', () => {
  render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" intent="p2p" />);
  expect(screen.getByTestId('listen-action')).toBeInTheDocument();
});
```
- [ ] **Step 2: Run** → FAIL (Listen always present today).
- [ ] **Step 3:** Add the `intent` prop; gate the Status/Listen block on `intent !== 'cms-gateway'`. Pass `intent` from `PacketConnectionPanelContainer`.
- [ ] **Step 4: Run** → PASS (+ existing packet tests still green).
- [ ] **Step 5: Commit** (`feat(packet): intent prop gates Listen for cms-gateway vs p2p (tuxlink-3pb)` + trailers).

---

## Task 7: Stub pane for unbuilt intents

**Files:**
- Create: `src/connections/StubPanel.tsx`, `src/connections/StubPanel.test.tsx`

A minimal `reading-pane` pane: `data-testid="stub-panel-root"`, shows the session-type label + protocol + a "coming soon — backend not yet built" line. Used by AppShell for any `!isBuilt(key)` selection (defensive; built rows are the normal path).

- [ ] **Step 1: Write the failing test**
```typescript
import { render, screen } from '@testing-library/react';
import { StubPanel } from './StubPanel';
it('renders the coming-soon pane with the session-type + protocol labels', () => {
  render(<StubPanel sessionType="radio-only" protocol="packet" />);
  const root = screen.getByTestId('stub-panel-root');
  expect(root.className).toContain('reading-pane');
  expect(root.textContent).toMatch(/Radio-only/);
  expect(root.textContent).toMatch(/soon|not yet/i);
});
```
- [ ] **Step 2: Run** → FAIL (module missing).
- [ ] **Step 3:** Implement `StubPanel.tsx` (look up labels via `SESSION_TYPES`).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** (`feat(connections): stub pane for not-yet-built session types (tuxlink-3pb)` + trailers).

---

## Task 8: Full-suite gate + browser smoke + PR

- [ ] **Step 1: Full gates**
```bash
pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec tsc --noEmit
pnpm -C worktrees/bd-tuxlink-3pb-session-selector exec vitest run
cargo test --lib --manifest-path worktrees/bd-tuxlink-3pb-session-selector/src-tauri/Cargo.toml
```
Expected: tsc clean; vitest all pass (437 baseline + new); cargo 314 pass (unchanged — no Rust edits).
- [ ] **Step 2: Operator browser smoke (UI feature — required before merge):** `cd worktrees/bd-tuxlink-3pb-session-selector && pnpm tauri dev` → expand **Winlink (CMS)** → **Telnet**, confirm the host/quick-picks/8772-8773 radios render + persist; confirm the accordion expand/collapse + selection persistence; confirm Packet panes (CMS-gateway hides Listen, P2P shows it).
- [ ] **Step 3: Push + PR (supersedes #122)**
```bash
git -C worktrees/bd-tuxlink-3pb-session-selector push
gh pr create --base main --head bd-tuxlink-3pb/session-selector \
  --title "feat(connections): session-type accordion selector + per-intent panes (tuxlink-3pb)" \
  --body "Implements docs/design/2026-05-22-session-type-selector-design.md. Supersedes #122 (its CMS backend is merged in; the CMS UI is relocated from SettingsPanel into the Telnet-CMS pane). Closes tuxlink-3pb."
gh pr close 122 --comment "Superseded by the session-selector epic PR (backend preserved, UI relocated)."
```
- [ ] **Step 4:** `bd close tuxlink-3pb` after merge.

---

## Self-review notes (author)

- **Spec coverage:** accordion (§3.1) → Task 2; rich pane + selection persistence (§3.2) → Tasks 3/4/6; matrix + soon (§3.3) → Tasks 1/7; code mapping (§4) → Tasks 2-6; #122 relocation (§6) → Tasks 0/4/5. Radio-only/Post-Office/Network-PO backends (R/L pools) are out of v0.1 (§5/§7) — represented as stubs only. ✓
- **Type consistency:** `ConnectionKey {sessionType, protocol}`, `config_set_connect({host, transport})`, testids `telnet-cms-panel-root` / `conn-host` / `listen-action` used consistently across tasks. ✓
- **Note for executor:** Task 3 depends on Task 4's component — do Task 4 first under subagent-driven execution, or use the inline stub noted in Task 3 Step 3.
- **`<SESSION-MONIKER>`** placeholders in commit trailers are intentional — the executing agent substitutes its own moniker (the commit hook rejects the literal placeholder).
