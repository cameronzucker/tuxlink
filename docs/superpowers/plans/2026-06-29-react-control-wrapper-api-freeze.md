# React Control Wrapper API Freeze — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and freeze typed `Button` / `Select` / `Field` React wrappers over a reconciled `controls.css`, and adopt them on the already-reviewed ribbon + radio-pane surfaces.

**Architecture:** `controls.css` is normalized onto one clean scale (tone × emphasis × size). Three thin React components emit its classes and forward native attributes. Color resolves via a `--ctl-accent` context-token trio (amber in app chrome, green inside `.radio-panel`). Adoption swaps hand-rolled `<button>`/`<select>`/`<input>` markup for the wrappers, producing only the operator-approved normalization deltas.

**Tech Stack:** React 19 + TypeScript, Vite, WebKitGTK 4.1 (render target), vitest + `@testing-library/react` (jsdom), `dev/render-harness/snapshot.py` for WebKitGTK visual re-verify.

**Spec:** [`docs/superpowers/specs/2026-06-29-react-control-wrapper-api-freeze-design.md`](../specs/2026-06-29-react-control-wrapper-api-freeze-design.md)

## Global Constraints

- **Normalized scale, approved deltas only.** The only permitted visual changes are: Connect padding `6/16→6/14` + solid-fg `--bg→--ctl-accent-fg`; Abort padding `5/14→6/14`; Open WebGUI border `currentColor→--border-strong`. Nothing else moves.
- **Component location:** `src/controls/`. `controls.css` stays at `src/styles/controls.css` (already imported globally in `src/App.tsx:8`).
- **Never tokenize the `@media` compact a11y floors** (`min-height:44px`, RadioPanel.css:434) — contract-pinned raw px in `RadioPanel.test.tsx`. Do not touch them.
- **Run the FULL `pnpm vitest run`** after the last CSS change and after each adoption task — scoped runs miss the contract tests (this bit PR #968).
- **WebKitGTK re-verify** uses `dev/render-harness/snapshot.py` (real `libwebkit2gtk-4.1`), NOT Chromium. Env: `WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe`.
- **Git in this worktree:** run a standalone `cd <worktree>` before any `git` op (the main-checkout-race hook false-positives on a stale tracked cwd). Every commit carries `Agent: peregrine-tamarack-sycamore` + the `Co-Authored-By` trailer. `pnpm install` must have run (fresh worktrees have no `node_modules`).
- **The freeze:** after adoption, the `Button`/`Select`/`Field` prop enums are the stable public surface. `controls.css` defines only the classes the mapping needs (YAGNI).

---

### Task 1: Add the `--ctl-accent` context-token trio

**Files:**
- Modify: `src/App.css` (`:root` base token block near line 117; add a `.radio-panel` scope rule)

**Interfaces:**
- Produces: CSS custom properties `--ctl-accent`, `--ctl-accent-soft`, `--ctl-accent-fg`, resolving to the app accent at `:root` and the modem accent inside `.radio-panel`. Consumed by Task 2's `controls.css`.

- [ ] **Step 1: Add the trio to `:root`.** In `src/App.css`, in the base `:root` block (alongside `--ctl-h-*` / `--radius-*` near line 117), add:

```css
  /* Control-accent context trio (tuxlink-3m0vx). Resolves to the app accent by
     default and the modem-dock green inside .radio-panel (rule below). Every
     theme inherits automatically because these reference the primitives. */
  --ctl-accent: var(--accent);
  --ctl-accent-soft: var(--accent-soft);
  --ctl-accent-fg: var(--tux-accent-fg);
```

- [ ] **Step 2: Add the `.radio-panel` scope.** After the `:root` block closes, add a top-level rule:

```css
/* The radio dock keeps its green identity: primary controls inside .radio-panel
   resolve --ctl-accent to the modem accent (tuxlink-3m0vx / -2ief). */
.radio-panel {
  --ctl-accent: var(--modem-accent);
  --ctl-accent-soft: var(--modem-accent-soft);
  --ctl-accent-fg: var(--modem-accent-fg);
}
```

- [ ] **Step 3: Verify build + token presence.**

Run: `pnpm typecheck && grep -c "ctl-accent" src/App.css`
Expected: typecheck passes; grep ≥ 6.

- [ ] **Step 4: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/App.css
git commit -m "feat(design-system): add --ctl-accent context-token trio (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Reconcile `controls.css` onto the normalized scale

**Files:**
- Modify: `src/styles/controls.css` (replace the button rules; keep/extend field+select)

**Interfaces:**
- Consumes: the `--ctl-accent*` trio (Task 1); existing scale tokens (`--type-control` 12px, `--type-body` 13px, `--radius-control` 3px, `--error`, `--tux-danger-surface`, `--text`, `--border-strong`).
- Produces: classes `.tux-btn` + `.tux-btn--{neutral|primary|danger}` + `.tux-btn--{solid|soft|outline}` + `.tux-btn--{xs|sm|md}`; `.tux-field`, `.tux-select` (with baked chevron). Consumed by Tasks 3–5 (wrappers) and Tasks 7–9 (adoption).

- [ ] **Step 1: Replace the button section of `src/styles/controls.css`.** Replace the current `.tux-btn` / `.tux-btn-sm` / `.tux-btn-primary` / `.tux-btn:disabled` rules (lines 5–22) with the normalized matrix:

```css
/* Button base — content-sized (never flex:1), scale-token radius. */
.tux-btn {
  display: inline-flex; align-items: center; justify-content: center;
  width: max-content;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  cursor: pointer;
  text-align: center;
}
.tux-btn:disabled { opacity: 0.6; cursor: default; }

/* Sizes (font · weight · padding) — the normalized density tiers. */
.tux-btn--xs { font-size: var(--type-control); font-weight: 500; padding: 4px 10px; }
.tux-btn--sm { font-size: var(--type-control); font-weight: 600; padding: 6px 14px; }
.tux-btn--md { font-size: var(--type-body);    font-weight: 500; padding: 8px 14px; }

/* Emphasis × tone. Primary resolves via --ctl-accent (amber app / green dock);
   danger via --error; neutral via --text/--border-strong. Only the combinations
   the reviewed surfaces use are defined (YAGNI). */
.tux-btn--primary.tux-btn--solid   { background: var(--ctl-accent); color: var(--ctl-accent-fg); border-color: var(--ctl-accent); }
.tux-btn--primary.tux-btn--soft    { background: var(--ctl-accent-soft); color: var(--ctl-accent); border-color: color-mix(in srgb, var(--ctl-accent) 35%, transparent); }
.tux-btn--primary.tux-btn--outline { background: transparent; color: var(--ctl-accent); border-color: var(--ctl-accent); }
.tux-btn--danger.tux-btn--soft     { background: var(--tux-danger-surface); color: var(--error); border-color: color-mix(in srgb, var(--error) 35%, transparent); }
.tux-btn--danger.tux-btn--outline  { background: transparent; color: var(--error); border-color: var(--error); }
.tux-btn--neutral.tux-btn--outline { background: transparent; color: var(--text); border-color: var(--border-strong); }

/* Hover (reproduce the current per-emphasis feel). */
.tux-btn--solid:hover:not(:disabled)   { filter: brightness(1.12); }
.tux-btn--soft:hover:not(:disabled)    { background: color-mix(in srgb, var(--ctl-accent) 18%, transparent); }
.tux-btn--danger.tux-btn--soft:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 18%, transparent); }
.tux-btn--neutral.tux-btn--outline:hover:not(:disabled) { border-color: var(--modem-accent); color: var(--modem-accent); }
.tux-btn--primary.tux-btn--outline:hover:not(:disabled),
.tux-btn--danger.tux-btn--outline:hover:not(:disabled) { filter: brightness(1.1); }
```

- [ ] **Step 2: Bake the select chevron into `.tux-select`.** Replace the `.tux-select` rule's `appearance` block so the chevron lives in one place:

```css
.tux-select {
  height: var(--ctl-h-sm); padding: 0 26px 0 var(--ctl-pad-x-sm);
  font-size: var(--type-body);
  border: 1px solid var(--border-strong); border-radius: var(--radius-control);
  background: var(--surface-2); color: var(--text);
  appearance: none; -webkit-appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' stroke='%23888' fill='none' stroke-width='1.5'/%3E%3C/svg%3E");
  background-repeat: no-repeat; background-position: right 9px center;
}
```

(Leave `.tux-field` as-is.)

- [ ] **Step 3: Verify build + no stale class names remain.**

Run: `pnpm build 2>&1 | tail -3 && grep -cE "tux-btn-primary|tux-btn-sm\b" src/styles/controls.css`
Expected: build succeeds; grep = 0 (old single-dash classes gone).

- [ ] **Step 4: WebKitGTK spot-check via the comparison mock.** The mock at `dev/render-harness/button-compare.html` already encodes the normalized values; confirm the real `controls.css` matches it by rendering a scratch page later in Task 7. For now, visual verify is deferred to adoption. Commit:

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/styles/controls.css
git commit -m "feat(design-system): reconcile controls.css onto the normalized tone/emphasis/size scale (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `Button` component + tests

**Files:**
- Create: `src/controls/Button.tsx`
- Create: `src/controls/Button.test.tsx`

**Interfaces:**
- Consumes: `controls.css` classes (Task 2).
- Produces: `export function Button(props): JSX.Element`; `export type ButtonTone = 'neutral'|'primary'|'danger'`; `export type ButtonEmphasis = 'solid'|'soft'|'outline'`; `export type ButtonSize = 'xs'|'sm'|'md'`. Consumed by Task 6 (barrel) + Tasks 7–8 (adoption).

- [ ] **Step 1: Write the failing test.** `src/controls/Button.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Button } from './Button';

describe('<Button>', () => {
  it('maps tone/emphasis/size to controls.css classes', () => {
    render(<Button tone="primary" emphasis="soft" size="md">Send</Button>);
    const btn = screen.getByRole('button', { name: 'Send' });
    expect(btn.className).toBe('tux-btn tux-btn--primary tux-btn--soft tux-btn--md');
  });

  it('defaults to neutral / solid / md', () => {
    render(<Button>Go</Button>);
    expect(screen.getByRole('button', { name: 'Go' }).className)
      .toBe('tux-btn tux-btn--neutral tux-btn--solid tux-btn--md');
  });

  it('forwards native attributes and events', () => {
    const onClick = vi.fn();
    render(<Button data-testid="x" disabled onClick={onClick}>Z</Button>);
    const btn = screen.getByTestId('x');
    expect(btn).toBeDisabled();
    fireEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled(); // disabled swallows the click
  });

  it('merges a caller-supplied className', () => {
    render(<Button className="dash-connect">C</Button>);
    expect(screen.getByRole('button', { name: 'C' }).className)
      .toContain('dash-connect');
  });
});
```

- [ ] **Step 2: Run to verify it fails.**

Run: `pnpm vitest run src/controls/Button.test.tsx`
Expected: FAIL — cannot find `./Button`.

- [ ] **Step 3: Write the minimal implementation.** `src/controls/Button.tsx`:

```tsx
import type { ButtonHTMLAttributes } from 'react';

export type ButtonTone = 'neutral' | 'primary' | 'danger';
export type ButtonEmphasis = 'solid' | 'soft' | 'outline';
export type ButtonSize = 'xs' | 'sm' | 'md';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;
  emphasis?: ButtonEmphasis;
  size?: ButtonSize;
}

export function Button({
  tone = 'neutral', emphasis = 'solid', size = 'md',
  className, type = 'button', ...rest
}: ButtonProps) {
  const cls = `tux-btn tux-btn--${tone} tux-btn--${emphasis} tux-btn--${size}`;
  return <button type={type} className={className ? `${cls} ${className}` : cls} {...rest} />;
}
```

- [ ] **Step 4: Run to verify it passes.**

Run: `pnpm vitest run src/controls/Button.test.tsx`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/controls/Button.tsx src/controls/Button.test.tsx
git commit -m "feat(controls): typed Button wrapper over controls.css (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `Field` component + tests

**Files:**
- Create: `src/controls/Field.tsx`
- Create: `src/controls/Field.test.tsx`

**Interfaces:**
- Produces: `export function Field(props): JSX.Element`; `export interface FieldProps extends InputHTMLAttributes<HTMLInputElement> { label?: string }`. Uses `ButtonSize` from `./Button` for its optional `size`.

- [ ] **Step 1: Write the failing test.** `src/controls/Field.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Field } from './Field';

describe('<Field>', () => {
  it('renders a .tux-field input', () => {
    render(<Field aria-label="Cmd port" defaultValue="8515" />);
    expect(screen.getByLabelText('Cmd port').className).toContain('tux-field');
  });
  it('associates a visible label with the input', () => {
    render(<Field label="Cmd port" id="cmd" defaultValue="8515" />);
    expect(screen.getByLabelText('Cmd port')).toHaveValue('8515');
  });
});
```

- [ ] **Step 2: Run to verify it fails.**

Run: `pnpm vitest run src/controls/Field.test.tsx`
Expected: FAIL — cannot find `./Field`.

- [ ] **Step 3: Write the implementation.** `src/controls/Field.tsx`:

```tsx
import { useId, type InputHTMLAttributes } from 'react';

export interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

export function Field({ label, id, className, ...rest }: FieldProps) {
  const auto = useId();
  const fieldId = id ?? auto;
  const cls = className ? `tux-field ${className}` : 'tux-field';
  const input = <input id={fieldId} className={cls} {...rest} />;
  if (!label) return input;
  return (
    <span className="tux-field-wrap">
      <label htmlFor={fieldId} className="tux-field-label">{label}</label>
      {input}
    </span>
  );
}
```

- [ ] **Step 4: Run to verify it passes.**

Run: `pnpm vitest run src/controls/Field.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/controls/Field.tsx src/controls/Field.test.tsx
git commit -m "feat(controls): typed Field wrapper over controls.css (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: `Select` component + tests

**Files:**
- Create: `src/controls/Select.tsx`
- Create: `src/controls/Select.test.tsx`

**Interfaces:**
- Produces: `export function Select(props): JSX.Element`; `export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> { label?: string }`.

- [ ] **Step 1: Write the failing test.** `src/controls/Select.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Select } from './Select';

describe('<Select>', () => {
  it('renders a .tux-select with its options', () => {
    render(
      <Select aria-label="Mode" defaultValue="PKTUSB">
        <option value="PKTUSB">PKTUSB</option>
      </Select>,
    );
    const sel = screen.getByLabelText('Mode');
    expect(sel.className).toContain('tux-select');
    expect(sel).toHaveValue('PKTUSB');
  });
});
```

- [ ] **Step 2: Run to verify it fails.**

Run: `pnpm vitest run src/controls/Select.test.tsx`
Expected: FAIL — cannot find `./Select`.

- [ ] **Step 3: Write the implementation.** `src/controls/Select.tsx`:

```tsx
import { useId, type SelectHTMLAttributes } from 'react';

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
}

export function Select({ label, id, className, children, ...rest }: SelectProps) {
  const auto = useId();
  const selId = id ?? auto;
  const cls = className ? `tux-select ${className}` : 'tux-select';
  const select = <select id={selId} className={cls} {...rest}>{children}</select>;
  if (!label) return select;
  return (
    <span className="tux-field-wrap">
      <label htmlFor={selId} className="tux-field-label">{label}</label>
      {select}
    </span>
  );
}
```

- [ ] **Step 4: Run to verify it passes.**

Run: `pnpm vitest run src/controls/Select.test.tsx`
Expected: PASS (1 test).

- [ ] **Step 5: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/controls/Select.tsx src/controls/Select.test.tsx
git commit -m "feat(controls): typed Select wrapper with baked chevron (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Barrel export (freeze the public surface)

**Files:**
- Create: `src/controls/index.ts`

**Interfaces:**
- Produces: `Button`, `Select`, `Field` + all their types from one import path `../controls` (or `src/controls`). This is the frozen surface.

- [ ] **Step 1: Write the barrel.** `src/controls/index.ts`:

```ts
// Frozen control-wrapper API (tuxlink-3m0vx). These prop enums are the stable
// public surface; changing them is a deliberate, reviewed API change.
export { Button } from './Button';
export type { ButtonProps, ButtonTone, ButtonEmphasis, ButtonSize } from './Button';
export { Field } from './Field';
export type { FieldProps } from './Field';
export { Select } from './Select';
export type { SelectProps } from './Select';
```

- [ ] **Step 2: Verify typecheck.**

Run: `pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/controls/index.ts
git commit -m "feat(controls): freeze the Button/Select/Field barrel export (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Adopt `Button` on the radio-pane footers + `.tux-field-wrap` styles + WebKitGTK re-verify

**Files:**
- Modify: the radio-pane components using `radio-panel-btn*` (enumerate — see Step 1)
- Modify: `src/radio/RadioPanel.css` (add `.tux-field-wrap` / `.tux-field-label` layout; remove now-dead `.radio-panel-btn*` rules ONLY after all call-sites migrate)
- Test: existing `src/radio/**/*.test.tsx` (must stay green)

**Interfaces:**
- Consumes: `Button` (Task 3), `controls.css` classes (Task 2).

- [ ] **Step 1: Enumerate the call-sites.**

Run: `grep -rn "radio-panel-btn" src/radio --include=*.tsx`
Expected: the ~58 sites across `ArdopRadioPanel.tsx`, `VaraRadioPanel.tsx`, `TelnetRadioPanel.tsx`, and section components. Record the list.

- [ ] **Step 2: Migrate each call-site** using this exact mapping (className → wrapper props):

| Current className | Wrapper |
|---|---|
| `radio-panel-btn radio-panel-btn-primary` | `<Button tone="primary" emphasis="soft" size="md">` |
| `radio-panel-btn radio-panel-btn-bad` | `<Button tone="danger" emphasis="soft" size="md">` |
| `radio-panel-btn` (plain neutral) | `<Button tone="neutral" emphasis="outline" size="md">` |
| `radio-panel-btn-sm` | `<Button tone="neutral" emphasis="outline" size="xs">` |

Preserve every other attribute (`onClick`, `disabled`, `data-testid`, `title`, `type`). Example — `src/radio/modes/ArdopRadioPanel.tsx` footer:

```tsx
// before:
<button type="button" className="radio-panel-btn radio-panel-btn-bad"
  data-testid="ardop-stop-btn" disabled={disconnecting} onClick={onStopClick}>
  {disconnecting ? 'Stopping…' : 'Stop'}
</button>
// after:
<Button tone="danger" emphasis="soft" size="md"
  data-testid="ardop-stop-btn" disabled={disconnecting} onClick={onStopClick}>
  {disconnecting ? 'Stopping…' : 'Stop'}
</Button>
```

Add `import { Button } from '../../controls';` (adjust depth per file).

- [ ] **Step 3: Add `.tux-field-wrap` layout to `src/radio/RadioPanel.css`** (so `Field`/`Select` labels in Task 9 lay out like the current `.radio-panel-field`; reproduce that rule's flex/gap). Inspect the existing `.radio-panel-field` rule first and mirror it:

```css
/* Wrapper for labeled Field/Select controls (tuxlink-3m0vx) — mirrors the
   existing .radio-panel-field row layout. */
.tux-field-wrap { display: flex; flex-direction: column; gap: 4px; }
.tux-field-label { font-size: var(--type-meta); color: var(--text-dim); }
```

- [ ] **Step 4: Run the FULL vitest suite.**

Run: `pnpm vitest run`
Expected: PASS — including `RadioPanel.test.tsx` contract tests. If a test asserts on `radio-panel-btn*` classes, update the assertion to the wrapper output (the button is the same DOM `<button>` with new classes) — do NOT change the compact a11y-floor assertions.

- [ ] **Step 5: WebKitGTK re-verify (stopped + running).** Start the dev server (`pnpm dev`), then snapshot each pane in both states and confirm the footers match the approved normalization:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
  "http://localhost:1420/dev/render-harness/harness.html?view=radio-ardop&running=1" \
  /tmp/ardop-running.png 900 1700 3000
```

Repeat for `radio-vara&running=1`, `radio-telnet`, and the stopped variants. Read each PNG; confirm Start/Send (green soft), Stop (red soft), Open WebGUI (`--border-strong` border), ghost buttons render per the mock.

- [ ] **Step 6: Remove the now-dead `.radio-panel-btn*` rules** from `RadioPanel.css` ONLY if Step 1's grep now returns zero `.tsx` matches. Re-run `pnpm vitest run` (full) after the CSS deletion.

- [ ] **Step 7: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/radio
git commit -m "refactor(radio): adopt Button wrapper on radio-pane footers (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Adopt `Button` on the ribbon Connect/Abort + WebKitGTK re-verify

**Files:**
- Modify: `src/shell/DashboardRibbon.tsx` (the Connect + Abort buttons)
- Modify: `src/shell/AppShell.css` (`.connect-button` / `.abort-button` → remove after migration; keep `.dash-connect` layout wrapper)

**Interfaces:**
- Consumes: `Button` (Task 3).

- [ ] **Step 1: Migrate Connect + Abort** in `src/shell/DashboardRibbon.tsx`:

```tsx
// Connect (was className="connect-button"):
<Button tone="primary" emphasis="solid" size="sm" className="connect-button"
  onClick={onConnect} disabled={connecting}>
  {connecting ? 'Connecting…' : 'Connect'}
</Button>
// Abort (was className="abort-button"):
<Button tone="danger" emphasis="outline" size="sm" className="abort-button"
  onClick={onAbort}>Abort</Button>
```

Keep the `connect-button`/`abort-button` classes ONLY for the `.dash-connect` positioning selectors if any target them; the visual rules now come from `.tux-btn*`. Add `import { Button } from '../controls';`.

- [ ] **Step 2: Trim `AppShell.css`.** In `.layout-b .dashboard .connect-button` / `.abort-button`, remove the now-duplicated visual props (font/color/background/border/padding/radius) — they come from `.tux-btn*`. Keep only any layout/positioning the `.dash-connect` flow needs. Verify against Step 4's render.

- [ ] **Step 3: Run the ribbon tests.**

Run: `pnpm vitest run src/shell/DashboardRibbon`
Expected: PASS. Update any assertion on `connect-button` visual classes if present (the DOM `<button>` + testids are unchanged).

- [ ] **Step 4: WebKitGTK re-verify the ribbon (idle + connecting).**

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
  "http://localhost:1420/dev/render-harness/harness.html?view=ribbon&connecting=1" \
  /tmp/ribbon-connecting.png 1920 140 2800
```

Read it; confirm Connect (amber solid, pad 6/14) and Abort (red outline, pad 6/14) match the approved normalization.

- [ ] **Step 5: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/shell
git commit -m "refactor(shell): adopt Button wrapper on the ribbon Connect/Abort (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Adopt `Select`/`Field` on radio-pane config controls + full re-verify (dark + daylight)

**Files:**
- Modify: the radio-pane config controls (`ArdopRadioPanel.tsx`, `VaraRadioPanel.tsx`, `TelnetRadioPanel.tsx` selects/inputs — enumerate)
- Test: existing radio tests stay green

**Interfaces:**
- Consumes: `Select`, `Field` (Tasks 4–5).

- [ ] **Step 1: Enumerate.**

Run: `grep -rnE "<select|<input" src/radio/modes --include=*.tsx | grep -vE "type=\"(checkbox|radio)\""`
Expected: the config selects (Capture/Playback/PTT/Mode/CAT/Bandwidth) + text inputs (Cmd port/Binary/CAT baud/Host/ports). Record the list. Leave checkboxes untouched (not in scope).

- [ ] **Step 2: Migrate** each `<select className="...">` → `<Select ...>` and each text `<input className="...">` → `<Field ...>`, preserving `value`/`onChange`/`disabled`/`data-testid`/`placeholder`. Keep any existing wrapper class via `className`. Example:

```tsx
// before: <input className="radio-panel-input" value={cmdPort} onChange={...} />
// after:  <Field value={cmdPort} onChange={...} />
```

Add `import { Select, Field } from '../../controls';`.

- [ ] **Step 3: Run the FULL vitest suite.**

Run: `pnpm vitest run`
Expected: PASS (all contract + radio tests).

- [ ] **Step 4: Final WebKitGTK re-verify — dark AND daylight.** Render all panes + ribbon in dark, then repeat one representative pane under daylight to confirm the fg fix reads correctly and nothing else moved. The harness needs a theme hook: append `document.documentElement.dataset.theme = params.get('theme') ?? ''` in `dev/render-harness/harness.tsx` (one line, before `createRoot`), then:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
  "http://localhost:1420/dev/render-harness/harness.html?view=radio-ardop&running=1&theme=daylight" \
  /tmp/ardop-daylight.png 900 1700 3000
```

Read both; confirm no unexpected movement.

- [ ] **Step 5: Commit.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add src/radio dev/render-harness/harness.tsx
git commit -m "refactor(radio): adopt Select/Field wrappers on radio-pane config controls (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Freeze documentation + final gates

**Files:**
- Create: `docs/design/control-wrappers.md` (the frozen API reference)
- Modify: `dev/implementation-log.md` (if present) — top entry

**Interfaces:** none (docs).

- [ ] **Step 1: Write `docs/design/control-wrappers.md`** — the frozen `Button`/`Select`/`Field` prop reference + the tone×emphasis×size → surface mapping table (copy from the spec's API section), plus a one-line "new controls use these; changing the enums is a reviewed API change."

- [ ] **Step 2: Run the full gate set.**

Run: `pnpm typecheck && pnpm vitest run && pnpm build`
Expected: all PASS.

- [ ] **Step 3: Commit + push + open PR.**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-3m0vx-wrapper-api-freeze
git add docs/design/control-wrappers.md
git commit -m "docs(design-system): freeze the control-wrapper API reference (tuxlink-3m0vx)

Agent: peregrine-tamarack-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
git push
gh pr create --base main --title "[peregrine-tamarack-sycamore] feat(design-system): freeze Button/Select/Field wrapper API (tuxlink-3m0vx)" --body "Implements docs/superpowers/plans/2026-06-29-react-control-wrapper-api-freeze.md. Normalized-scale reconciliation of controls.css + typed wrappers adopted on the reviewed ribbon + radio-pane surfaces. WebKitGTK re-verified (dark + daylight). Closes tuxlink-3m0vx."
```

- [ ] **Step 4: Watch CI; merge when green** (`gh pr merge <#> --merge`, no `--auto`), then dispose the worktree per the ADR 0009 ritual and `bd close tuxlink-3m0vx`.

## Self-Review

- **Spec coverage:** API (Tasks 3–6) · controls.css reconciliation + tokens (Tasks 1–2) · adoption on reviewed surfaces (Tasks 7–9) · reviewed-normalization deltas verified in WebKitGTK dark+daylight (Tasks 7–9) · freeze semantics (Tasks 6, 10) · full-vitest/contract-test discipline (Tasks 7, 9, 10). Covered.
- **Placeholder scan:** call-site lists are produced by exact `grep` commands (concrete, not stale hardcoded lists); every code step shows complete code. No TBD/TODO.
- **Type consistency:** `ButtonTone`/`ButtonEmphasis`/`ButtonSize` defined in Task 3, re-exported in Task 6, consumed in Tasks 7–8. `Field`/`Select` label+id pattern consistent across Tasks 4–5. Class names (`tux-btn--{tone|emphasis|size}`) consistent between Task 2 (CSS), Task 3 (component), and Task 7/8 (mapping).
