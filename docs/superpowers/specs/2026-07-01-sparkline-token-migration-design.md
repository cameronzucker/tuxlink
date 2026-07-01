# Sparkline gradient → token migration (tuxlink-ivzut)

Per-issue design record under the frontend-cohesion design-system epic. Parent
spec: [`2026-06-29-frontend-cohesion-design-system-design.md`](2026-06-29-frontend-cohesion-design-system-design.md)
(line-147 prescription: *"Sparkline.css candy gradients + raw hex → semantic
solid bars / `color-mix` from `--success`/`--accent`/`--error`"*).

## Problem

[`src/radio/charts/Sparkline.css`](../../../src/radio/charts/Sparkline.css) paints
its three bar palettes with raw hex gradients:

| bar state | current |
|---|---|
| default (good) | `linear-gradient(0deg, #4ade80, rgba(74,222,128,0.2))` |
| `.warn` | `linear-gradient(0deg, #fbbf24, rgba(251,191,36,0.2))` |
| `.bad` | `linear-gradient(0deg, #f87171, rgba(248,113,113,0.2))` |

This is not merely off-scale cosmetics. Every other status surface (SessionLog,
connection dots, FrameRibbon) reads `--success` / `--accent-2` / `--error`, which
the theme layer deliberately overrides per theme — most notably `night-red`
collapses them to red-luminance for night-vision preservation. Because the
Sparkline hard-codes vivid green/amber, it **punches through that contract**:
under `night-red` its bars stay vivid green while the rest of the app is
monochrome red. Tokenizing fixes a real theming defect, not just a style nit.

## Decision

Migrate the three bar backgrounds onto the semantic status tokens, keeping the
existing subtle bottom-fade sourced from the token via `color-mix` (**Option A**,
operator-selected 2026-07-01 after a real-WebKitGTK side-by-side of A vs a flat
solid Option B):

```css
.sparkline-bar        { background: linear-gradient(0deg, var(--success),
                          color-mix(in srgb, var(--success) 20%, transparent)); }
.sparkline-bar.warn   { background: linear-gradient(0deg, var(--accent-2),
                          color-mix(in srgb, var(--accent-2) 20%, transparent)); }
.sparkline-bar.bad    { background: linear-gradient(0deg, var(--error),
                          color-mix(in srgb, var(--error) 20%, transparent)); }
```

Rationale for the specifics:

- **Token family = semantic status**, not the `--modem-accent` chrome family: the
  sparkline encodes threshold state (good/warn/bad), so it must track
  success/warn/danger, and inherit each theme's deliberate behavior (incl.
  night-red monochrome).
- **`--accent-2` for warn** (not the spec's loose `--accent`): matches the
  established radio-subsystem convention (`SessionLogSection.css`,
  `RadioPanel.css` use `var(--accent-2)` for the warn/connecting amber). `--tux-warn`
  aliases `--accent-2`; using the base token matches surrounding code.
- **`color-mix(in srgb, … 20%, transparent)`** reproduces the shipped 20%-alpha
  bottom stop from the token. Already used in 5+ files (AppShell, Catalog, GRIB,
  Search), so WebKitGTK-on-Pi support is confirmed, not assumed.
- No new tokens are introduced (parent-spec constraint: *add NO new token without
  an explicit scale decision*).

## Scope / non-goals

- **One file's styling**: `Sparkline.css` lines 17/23/27. No `.tsx` change.
- **No `@media` rules exist in this file**, so the contract-pinned compact-a11y
  floors (RadioPanel, keyed to `.radio-panel .tux-btn`) are untouched — but the
  full `pnpm vitest run` still runs after the last CSS edit (scoped runs miss
  contract tests).
- FrameRibbon shares the same raw hexes; out of scope here (its own follow-up).

## Verification

- Render harness `view=sparkline` (added to `dev/render-harness/harness.tsx`)
  renders Current / A / B side-by-side; before/after snapshots in real WebKitGTK,
  default-dark + night-red. Evidence: `dev/scratch/sparkline-compare-*.png`.
- `Sparkline.test.tsx` asserts structural contract only (bar count, threshold
  classes, heights) — no background assertions — so the color change is
  test-safe; full `pnpm vitest run` confirms no contract regressions elsewhere.
