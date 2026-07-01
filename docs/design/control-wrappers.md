# Control wrappers — the frozen `Button` / `Select` / `Field` API

**Status:** frozen (`tuxlink-3m0vx`). The prop enums below are the stable public surface for
control markup. New controls use these wrappers instead of hand-rolling `class` + token
combinations. Changing an enum is a deliberate, reviewed API change — not an ad-hoc edit.

Import from the barrel:

```ts
import { Button, Select, Field } from '../controls'; // adjust relative depth per file
```

The wrappers are thin: they emit the `controls.css` classes (`src/styles/controls.css`) and
forward all native attributes. `controls.css` is loaded globally via `src/App.tsx`.

## `Button`

```ts
type ButtonTone     = 'neutral' | 'primary' | 'danger';   // default 'neutral'
type ButtonEmphasis = 'solid' | 'soft' | 'outline';        // default 'solid'
type ButtonSize     = 'xs' | 'sm' | 'md';                  // default 'md'
```

`tone`, `emphasis`, `size` are orthogonal; all native `<button>` attributes forward
(`onClick`, `disabled`, `type` (defaults to `"button"`), `data-testid`, `title`, …). A
caller `className` is appended after the generated classes.

**Size = density tier** (font · weight · padding):

| size | font | weight | padding | role |
|---|---|---|---|---|
| `xs` | `--type-control` (12px) | 500 | 4px 10px | compact inline (↻ refresh / detect) |
| `sm` | `--type-control` (12px) | 600 | 6px 14px | dense chrome (ribbon) |
| `md` | `--type-body` (13px) | 500 | 8px 14px | panel action (dock) |

**Emphasis:** `solid` = filled · `soft` = tinted bg + color-mix border + colored text ·
`outline` = transparent bg + solid border + colored text.

**Color resolves by context, not by hard-code.** `controls.css` defines a `--ctl-accent` /
`--ctl-accent-soft` / `--ctl-accent-fg` token trio: it is the app amber accent at `:root`
and the modem green inside `.radio-panel`. So `tone="primary"` is amber in app chrome and
green in the radio dock automatically. `tone="danger"` uses `--error` / `--tux-danger-surface`;
`tone="neutral"` uses `--text` / `--border-strong`.

**Surface → wrapper mapping** (the reviewed surfaces this freeze adopted):

| Surface | Wrapper |
|---|---|
| Ribbon Connect | `<Button tone="primary" emphasis="solid" size="sm">` |
| Ribbon Abort | `<Button tone="danger" emphasis="outline" size="sm">` |
| Dock Start / Send/Receive | `<Button tone="primary" emphasis="soft" size="md">` |
| Dock Stop | `<Button tone="danger" emphasis="soft" size="md">` |
| Dock Open WebGUI / Tune… | `<Button tone="neutral" emphasis="outline" size="md">` |
| Dock ↻ detect / browse / manage | `<Button tone="neutral" emphasis="outline" size="xs">` |

Only the combinations the reviewed surfaces use are defined in `controls.css` (YAGNI). Adding
a new combination means adding its class there.

## `Field` and `Select`

```ts
interface FieldProps  extends React.InputHTMLAttributes<HTMLInputElement>   { label?: string }
interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> { label?: string }
```

- `Field` renders a `.tux-field` `<input>`; `Select` renders a `.tux-select` `<select>` (with
  the chevron baked into the class). Both forward all native attributes and append a caller
  `className`.
- With a `label`, the control is wrapped in `.tux-field-wrap` and the label is associated via
  `htmlFor`/`id` (a `useId()` fallback when no `id` is given). Without a `label`, the bare
  control is returned.

## Scope of the freeze

Adopted on the already-reviewed surfaces only: the radio-pane footer buttons + config
controls, and the ribbon Connect/Abort. The remaining ~400 hand-rolled controls, the ribbon
segmented controls, and any daylight/high-contrast theme redesign are out of scope (later
work). Daylight is decoupled by construction: the freeze commits to semantics + named tokens,
never to daylight's colors.
