# Request Center — visual redesign (Direction C re-skin)

**bd:** tuxlink-hbbw · follow-up to tuxlink-eymu (shipped PR #513) · **moniker:** dahlia-tamarack-canyon · 2026-06-09

## Why

The Request Center shipped functionally complete and fully tested, but the
visual layer only vaguely resembled the approved request-first mock — the cards
were flat grey label-and-tiny-text boxes in a uniform grid, with no icons, no
hierarchy, and almost no use of the brand accent. Operator smoked it post-merge
and rejected the look. The original mock was a browser-companion artifact never
saved to the repo, so there was no pixel reference to build against (the root
process gap — now fixed: the approved mock is committed).

## Pixel source of truth

[`docs/design/mockups/2026-06-09-request-center-redesign-final.html`](../../design/mockups/2026-06-09-request-center-redesign-final.html)
— **Direction C, rendered 1:1 at the real 1200×820 window.** Operator approved
2026-06-09 ("big improvement; let's go"). The earlier 3-direction exploration is
`2026-06-09-request-center-visual-redesign.html`.

## Scope

**In:** the visual layer only — CSS, component markup, and a shared line-icon
set, across `RequestCenter`, `CatalogBrowse`, `GribForm`, and a small
`sections.ts` data addition (a section "kind" so the home knows which section is
the location hero).

**Out (MUST NOT change):** behavior, the geo/`catalogMap`/`basket`/dispatch
logic, the Tauri command wiring, the menu/AppShell integration, and **every
existing `data-testid` + accessible-name**. The 123 `src/request/` tests + the
`src/shell/chrome` contract tests stay green with at most mechanical
selector-agnostic updates. This is a re-skin, not a rebuild.

## Design system

- Tokens from `src/App.css` (the live dark scheme): `--bg #0d1318`,
  `--surface #141c23`, `--surface-2`, `--border*`, `--text/-dim/-faint`,
  **`--accent #f59f3c`** (warm amber) + `--accent-2`, `--accent-soft`,
  `--success`, `--danger`, `--tux-focus-ring`. No hardcoded colors. The redesign
  uses the amber accent purposefully (location chip, hero edge, primary add +
  Send, active states) where the shipped version barely touched it.
- **Line icons.** A shared inline-SVG set (lucide-style, 1.85 stroke). First
  check whether the app already has an icon component/convention and reuse it;
  otherwise add `src/request/icons.tsx` exporting small `<Icon name=…>`
  components. Icons needed: pin, search, close, plus, arrow, check, radio (the
  RC glyph), weather (sun-cloud), wave (marine), prop (signal arcs), sun
  (solar), aurora, tower (gateways), info, list (browse), map/grid (GRIB),
  basket, trash. Mapping is in the mock.

## Per-view spec (against the mock)

### Header (all views)
Amber-tinted RC glyph + "Request Center" title · the location chip with a pin
icon reading **"Near CN87 · Washington"** (the state *name* — add a USPS→name
lookup; the geo layer already resolves the USPS code; fall back to just
"Near <grid>" when no grid / unknown code, preserving the existing neutral
"Location not set") · the catalog search with a search icon · the close ✕.
Keep testids `request-center-location`, `request-search`, `request-close`.

### Home — request-first (the main change)
- **Location hero:** a bordered amber-edged panel ("For your location · <grid> ·
  <state>") holding the location-tailored requests as **large feat cards** (icon
  tile + title + one-line description + an Add / "Browse <area>" action). These
  are the Weather section's geo-resolved cards (State forecast = addCms; Marine =
  openBrowse). Mark this section in `sections.ts` (e.g. `kind: 'location'`) so
  the renderer heroes it; the national/nearby sections render as chips.
- **Compact chips:** the Propagation & space and Nearby stations sections render
  as a responsive chip grid (`minmax(252px,1fr)`) — icon + title + one-line
  description + the filename in mono + an add/→ control. Descriptions come from
  the existing `card.description`.
- **Reveals:** "Browse full catalog by category (1,477 items · 41 categories)"
  and "GRIB by area (Saildocs)" as the two tertiary buttons.
- Keep the card testids (`request-card-<id>`), section testids
  (`request-section-<id>`), and the add controls' aria-labels.

### Browse — 3-pane
Crumb + Back · category nav (name + count, active = amber rail) · item rows
(mono filename in `--accent-2`, description, size, Add control / "✓ Added" when
already in the basket). Keep `request-browse`, `catalog-browse-*` testids and the
`data-category` attr the openBrowse deep-link asserts.

### GRIB — by area
Crumb + Back · sectioned form (Region with the **real `GridMapPicker`** in a
styled bounded container — the one legitimate fixed-height; Forecast hours;
Parameters as toggle chips; Mode/sub when relevant; Subject) · an "Add to
request" primary button. Keep every `grib-*` field testid + `request-grib` +
`grib-add`/`grib-back`.

### Basket rail (all views)
Header with basket icon + count chip · item rows with a per-rail icon (cms =
green check-tint, saildocs = amber) + label + rail caption + a trash remove ·
per-rail summary line · gradient amber **Send all** (disabled + greyed when
empty) · the arrival note · success/error result block. Keep `request-basket`,
`basket-item-<id>`, `basket-remove-<id>`, `request-basket-summary`,
`request-basket-send`, `request-basket-result`. Add an **empty state** (dashed
ring + "Your basket is empty…") — net-new but inert (no test depends on its
absence).

## Constraints
- **WebKitGTK:** no fixed pixel heights on layout containers (flex + `min-height:0`
  + `overflow-y:auto`); the GRIB map viewport is the one bounded exception.
  Content column caps at **~840px** so a maximized window doesn't stretch it; the
  basket rail stays pinned right.
- **Voice:** formal/declarative, present-indicative, no first person, no temporal
  hedging (matches the shipped copy).
- **Verification:** existing tests green; typecheck 0; then a WebKitGTK grim
  smoke on a warm build (this is the change that most needs the real render —
  do it before declaring done if the dev port is free, else flag for post-merge).

## Out of scope / fast-follows
State-name in the chip beyond the USPS→name map; any new request types; any
behavior or dispatch change; basket draft persistence (unchanged — basket clears
on close).
