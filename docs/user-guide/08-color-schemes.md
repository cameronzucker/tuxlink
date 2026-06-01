# Color schemes

The color scheme controls the entire UI's appearance — surfaces, text,
accents, semantic state colors (success / error / info). Schemes are
purely presentational; switching does not touch the operator's identity,
mailbox, or any configuration.

## Picking a preset

View → Color Scheme lists the six bundled presets:

- **Default (dark).** The cool-slate dark theme; the design baseline.
- **Daylight (light).** A soft off-white theme with a warm-amber accent.
  Designed for moderate-bright indoor and outdoor use.
- **High contrast (light).** Pure white surfaces with near-black text
  and deep accents. For harsh direct-sun LCD viewing where Daylight
  still washes out.
- **Paper (warm light).** Warm beige surfaces with a saddle-brown
  accent. Reads like a printed sheet.
- **Night / tactical (red).** Deep-red surfaces with brighter red text.
  Night-vision-preserving; designed for after-dark net operations.
- **Grayscale.** Hueless. Pairs with an external red-gel or NVG filter
  that retints the entire screen.

The choice persists between sessions.

## Customizing

View → Color Scheme → Customize… opens the inline Theme Designer. Pick a
base preset to start from, then tweak any token via the native color
picker or by typing a hex / rgb / oklch value in the text input. The
preview is live — the whole app re-paints as edits land.

Token groups in the designer:

- **Surfaces.** The window background and the elevation ladder.
- **Borders.** The three tiers of dividing lines.
- **Text.** Primary, dim (labels), faint (help text).
- **Accent.** The highlight / link / button color, plus the matching
  on-accent text color.
- **Status / semantic.** Unread dot, success, error (and its on-error
  text color), info, form-tag.

Saving persists the theme as "My custom theme" — it appears in the View
→ Color Scheme list. Cancel, Esc, or backdrop-click restores the
previously-applied scheme without saving.

## Light vs dark mode

Each preset declares a CSS `color-scheme` (light or dark). This affects
the browser's native form controls — scrollbars, select dropdowns,
selection highlights — so they match the theme on WebKitGTK. The
designer's Mode toggle does the same for custom themes.

## Where next

- [Settings](07-settings.md) — every non-color preference.
- [The mailbox](03-mailbox.md) — what the colored indicators mean.
