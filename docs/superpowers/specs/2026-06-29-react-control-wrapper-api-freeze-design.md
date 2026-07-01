# React control wrapper API freeze — design spec

- **Issue:** `tuxlink-3m0vx` (unblocked by the `tuxlink-ppnui` screenshot-review gate)
- **Predecessor:** `tuxlink-zj9se` (PR #968 — radio-pane + non-enumerated-ribbon token migration), `tuxlink-9q6ly` (Phase 0 + `controls.css` foundation)
- **Plan of record:** [`docs/superpowers/plans/2026-06-29-frontend-cohesion-design-system.md`](../plans/2026-06-29-frontend-cohesion-design-system.md)
- **Status:** design — pending operator review before implementation planning.

## Problem

`src/styles/controls.css` ships the shared control primitives (`.tux-btn` / `-sm` /
`-primary`, `.tux-field`, `.tux-select`) on the scale tokens, but **nothing in the app
adopts them** and **no React wrappers exist**. The design-system plan deferred the
`Button` / `Select` / `Field` wrapper API until the ribbon and radio panes survived
WebKitGTK screenshot review. That review passed (`tuxlink-ppnui`), so the wrappers are
now eligible to be built and frozen.

Two facts discovered while grounding this design shape the work:

1. **A frozen wrapper that nothing uses is the "registered but reachable by nobody"
   anti-pattern the wire-walk gate exists to catch.** So the freeze must include
   adoption on at least the already-reviewed surfaces — not a pure-additive
   zero-adopter drop.

2. **The reviewed surfaces use two intentionally different primary treatments, and
   `controls.css` only models one of them:**
   - **Ribbon Connect** = solid `--accent` (amber) fill — the single loud CTA.
   - **Radio-dock Start/Send** (`.radio-panel-btn-primary`) = `--modem-accent-soft`
     tint + `--modem-accent` (green) border/text — a *soft-outlined* primary, chosen
     deliberately so the action "is visible at rest without becoming the only filled
     action in the row."
   - **Radio-dock Stop** (`.radio-panel-btn-bad`) = the same soft-outlined pattern in
     `--error` red. `controls.css` has **no danger variant at all**.
   - `controls.css` `.tux-btn-primary` is a *solid amber* fill — naively adopting it on
     the panes would be a triple regression: amber-not-green, solid-not-outlined, and
     Stop has nowhere to map.

The freeze is therefore not "wrap `controls.css` as-is." It is: **reconcile
`controls.css` with the two real emphasis levels, build a typed API over the reconciled
foundation, and adopt it on the reviewed surfaces as a pure visual refactor.**

## Scope

**In scope**
- Build three React components — `Button`, `Select`, `Field` — over `controls.css`.
- Reconcile `controls.css` onto one clean scale: the `solid` / `soft` / `outline`
  emphases, `neutral` / `primary` / `danger` tones, `xs` / `sm` / `md` sizes, and the
  context-token trio (`--ctl-accent` / `-soft` / `-fg`); bake the select chevron in one
  place (resolving the "chevron per-surface for now" TODO).
- Freeze the prop API (the names/enums below become the stable surface).
- Adopt the wrappers on the **already-reviewed surfaces only**: the radio-pane footer
  button family (`.radio-panel-btn` / `-sm` / `-primary` / `-bad`) and the ribbon
  Connect/Abort buttons, plus the radio-pane config `Select`/`Field` controls. Exact
  call-site list enumerated at plan time.
- The small, operator-approved normalization deltas (see Reviewed normalization).

**Out of scope**
- The remaining ~400 hand-rolled `<button>` / `<select>` / `<input>` call-sites (a later
  migration plan).
- The ribbon segmented controls (GPS/MANUAL, Review/Download) — a distinct
  segmented-control pattern, not a plain `Button`.
- **Any daylight / high-contrast theme redesign.** The daylight theme is unfinished and
  parked; this freeze does not touch its color decisions (see Theming). The one exception
  is the solid-fg token fix, which *improves* daylight (near-white → white on fills) and
  adds no debt — it rode the approved normalization mock.

## Decision: Hybrid (normalize the drift)

Grounding revealed the reviewed buttons are **hand-tuned per instance** — three
different box models, and `controls.css .tux-btn` (a speculative Phase-0 guess) matched
none of them. A `current`-vs-`normalized` mock rendered in real WebKitGTK
(`dev/render-harness/button-compare.html`) showed the divergence is **cosmetically
negligible**: five of six button roles are indistinguishable normalized-vs-current, and
the only visible change is quieting the neutral `Open WebGUI` border (`currentColor` →
`--border-strong`). The operator reviewed that mock and approved the normalized column.

Therefore this freeze **normalizes onto one clean scale** (Hybrid) rather than enshrining
the drift: `controls.css` becomes a genuine unified foundation, wrappers emit its classes,
and the small approved deltas ship with it. The mock served as the pre-implementation
visual review; a final WebKitGTK re-verify on the actual adopted surfaces confirms it.

## API

### `Button`

```ts
type ButtonTone     = 'neutral' | 'primary' | 'danger';
type ButtonEmphasis = 'solid' | 'soft' | 'outline';
type ButtonSize     = 'xs' | 'sm' | 'md';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;         // default 'neutral'
  emphasis?: ButtonEmphasis; // default 'solid'
  size?: ButtonSize;         // default 'md'
}
```

`tone`, `emphasis`, `size` are orthogonal. The three sizes carry the normalized
density (font + weight + padding):

| size | font | weight | padding | role |
|---|---|---|---|---|
| `xs` | `--type-control` (12px) | 500 | 4px 10px | compact inline (↻ refresh/detect) |
| `sm` | `--type-control` (12px) | 600 | 6px 14px | dense chrome (ribbon) |
| `md` | `--type-body` (13px) | 500 | 8px 14px | panel action (dock) |

The three emphases: `solid` (filled), `soft` (tinted bg + color-mix border + colored
text), `outline` (transparent bg + solid border + colored text). Every reviewed button
maps cleanly:

| Surface | Wrapper call |
|---|---|
| Ribbon Connect | `<Button tone="primary" emphasis="solid" size="sm">` |
| Ribbon Abort | `<Button tone="danger" emphasis="outline" size="sm">` |
| Dock Start / Send/Receive | `<Button tone="primary" emphasis="soft" size="md">` |
| Dock Stop | `<Button tone="danger" emphasis="soft" size="md">` |
| Dock Open WebGUI / Tune… | `<Button tone="neutral" emphasis="outline" size="md">` |
| Dock ↻ detect / browse / manage | `<Button tone="neutral" emphasis="outline" size="xs">` |

Renders `class="tux-btn tux-btn--{tone} tux-btn--{emphasis} tux-btn--{size}"` plus all
native `<button>` attributes (`onClick`, `disabled`, `type`, `data-testid`, `title`, …)
forwarded via `...rest`. Not every tone×emphasis×size combination has a call-site (YAGNI);
`controls.css` defines only the classes the mapping above needs.

### `Select` and `Field`

```ts
interface FieldProps  extends React.InputHTMLAttributes<HTMLInputElement>   { label?: string; size?: ButtonSize; }
interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> { label?: string; size?: ButtonSize; }
```

- `Field` renders a labeled `.tux-field` text input; label associated via `htmlFor`/`id`.
- `Select` renders a labeled `.tux-select` with the chevron **baked into the class once**
  (CSS background-image), replacing the per-surface chevron the panes hand-roll today.
- Both forward native attributes. Lower-stakes than `Button`; sensible defaults, no novel
  variants.

## Color resolution — context tokens, not hard-codes

`controls.css` gains a context-token **trio** for the `primary` tone:

```css
:root        { --ctl-accent: var(--accent);       --ctl-accent-soft: var(--accent-soft);       --ctl-accent-fg: var(--tux-accent-fg); }
.radio-panel { --ctl-accent: var(--modem-accent); --ctl-accent-soft: var(--modem-accent-soft); --ctl-accent-fg: var(--modem-accent-fg); }
```

Then `tone="primary"` resolves to **amber in app chrome and green in the dock
automatically** — no prop, no per-call-site branching. The three emphases (primary tone
shown; `danger` swaps `--ctl-accent*` → `--error` / `--tux-danger-surface`; `neutral`
uses `--text` / `--border-strong`):

```css
.tux-btn--primary.tux-btn--solid   { background: var(--ctl-accent); color: var(--ctl-accent-fg);
                                     border-color: var(--ctl-accent); }
.tux-btn--primary.tux-btn--soft    { background: var(--ctl-accent-soft); color: var(--ctl-accent);
                                     border-color: color-mix(in srgb, var(--ctl-accent) 35%, transparent); }
.tux-btn--primary.tux-btn--outline { background: transparent; color: var(--ctl-accent);
                                     border-color: var(--ctl-accent); }
.tux-btn--neutral.tux-btn--outline { background: transparent; color: var(--text);
                                     border-color: var(--border-strong); }
.tux-btn--danger.tux-btn--soft     { background: var(--tux-danger-surface); color: var(--error);
                                     border-color: color-mix(in srgb, var(--error) 35%, transparent); }
.tux-btn--danger.tux-btn--outline  { background: transparent; color: var(--error);
                                     border-color: var(--error); }
```

The `soft` background uses the `--ctl-accent-soft` token (= `--modem-accent-soft` in the
dock) directly, **not** a `color-mix` of `--ctl-accent` — the shipped `*-soft` tokens use
a different, hand-tuned green base (`#22c55e`) than `--modem-accent` (`#4ade80`), so a
`color-mix` would not reproduce the reviewed green. The `soft` border uses
`color-mix(--ctl-accent 35%)`, matching `.radio-panel-btn-primary`. Exact hover/disabled
rules per class are captured at plan time against the live values.

This also **fixes a latent daylight bug**, and does so within the approved normalization:
today the solid ribbon Connect sets `color: var(--bg)` (near-white on light themes, fragile
on a colored fill). Routing it through `--ctl-accent-fg` (`= --tux-accent-fg`) flips it to
white in daylight. In dark this is an imperceptible `#0d1318` → `#1a0e02` shift — visible in
the approved normalized mock, not a regression.

## Theming — daylight stays decoupled

The high-contrast / daylight theme is **unfinished and parked**, and this freeze must not
touch it. It does not, by construction:

- The frozen API commits to **semantics** (`tone` / `emphasis` / `size`) and **named
  tokens** (`--ctl-accent` / `--ctl-accent-fg`), never to daylight's colors or contrast.
- Daylight already varies buttons through **token values only** ("colors only, same
  shapes", `tuxlink-c22r` / `tuxlink-fwse`): it deepens `--accent` → `#a83800`,
  `--modem-accent` → `#0a6b2e`, and flips the `*-fg` tokens to white. The wrappers inherit
  all of that automatically.
- When daylight's eventual rework happens, it adjusts token values (and *optionally* adds
  one daylight-scoped `emphasis` rule if it wants buttons bolder than soft) with **zero
  wrapper-API churn**. The "force solid in daylight" question is explicitly left to that
  pass.

The defined `tone`/`emphasis`/`--ctl-accent` vocabulary this spec establishes is itself
the missing language that made the prior daylight round a multi-day litigation — this work
is upstream of fixing daylight, not in tension with it.

## Reviewed normalization (not byte-for-byte)

Adoption is a **normalization**, not a byte-for-byte refactor: the reconciled `controls.css`
classes bring the reviewed buttons onto one clean scale, producing the small, operator-
approved deltas below. Everything not listed is unchanged.

| Button | Approved delta |
|---|---|
| Ribbon Connect | padding `6px 16px` → `6px 14px`; solid fg `--bg` → `--ctl-accent-fg` (imperceptible in dark, correct in daylight) |
| Ribbon Abort | padding `5px 14px` → `6px 14px` |
| Dock Open WebGUI | border `currentColor` (bright) → `--border-strong` (quiet, matches ghost) |
| Start/Send, Stop, ↻ ghost | unchanged |

These deltas were rendered current-vs-normalized in real WebKitGTK
(`dev/render-harness/button-compare.html`) and approved. A final WebKitGTK re-verify on the
actual adopted surfaces (ribbon + all radio panes, dark theme; plus a `daylight` snapshot to
confirm the fg fix reads correctly and nothing else moved) closes the loop.

## Testing & verification

- **Full `pnpm vitest run`** after the last CSS change — scoped runs miss the
  `RadioPanel.test.tsx` contract tests (this bit PR #968).
- **Never tokenize the `@media` compact a11y floors** — they are contract-pinned raw px
  (`RadioPanel.test.tsx`). The wrappers do not touch them.
- `pnpm typecheck`, `pnpm build`.
- **WebKitGTK render-harness re-verify** of the adopted surfaces via `snapshot.py`
  (`?view=ribbon|radio-ardop|radio-vara|radio-telnet`, both stopped and `?running=1`
  states using the merged harness fixture). Include a `data-theme='daylight'` snapshot —
  **not** to judge daylight, only to prove the refactor changed nothing there.
- New unit tests for the three wrappers (variant→class mapping, native-attr forwarding,
  label association).

## "Freeze" semantics

After this lands, the `Button` / `Select` / `Field` prop names and enums are the **stable
public surface** for new control work; new controls use the wrappers rather than
hand-rolling class+token combos. `controls.css` is the backing foundation. Changing the
frozen prop enums later is a deliberate, reviewed API change — not an ad-hoc edit.

## Risks

- **Computed-style drift on adoption** — mitigated by the byte-for-byte non-regression
  requirement + WebKitGTK re-verify (dark + daylight) + full vitest.
- **`color-mix` support in WebKitGTK** — already used by the shipped `.radio-panel-btn*`
  rules, so it is known-good in the target engine.
- **Scope creep into the other ~400 call-sites** — explicitly out of scope; the freeze
  proves the API on the reviewed surfaces only.

## Agent

Agent: peregrine-tamarack-sycamore
