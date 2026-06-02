/**
 * ThemeDesigner — inline (in-webview) color theme editor (tuxlink-vgth).
 *
 * UX: View → Color Scheme → Customize… opens this panel. The operator picks a
 * base preset (Default dark, Daylight, etc.), tweaks individual token colors
 * via native `<input type=color>` pickers, and clicks Save. The preview is
 * LIVE — token edits write straight to <html style="--bg: …"> so every
 * surface in the app re-paints with the new color while the dialog is open.
 *
 * On Save: the theme is persisted to localStorage as CustomTheme JSON, the
 * applied scheme is set to "custom", and the panel closes. The saved theme
 * appears under View → Color Scheme as "My custom theme."
 * On Cancel: the prior applied scheme is restored, no persistence happens.
 *
 * NOT a separate OS window — inline overlay per feedback_inline_ui_no_window_clutter.
 *
 * Note on the live preview's interaction with applyColorScheme: while the
 * designer is open we bypass the persistence path and write tokens directly
 * via setProperty so the operator's edits are visible without a Save. On
 * unmount (cancel) we restore the prior scheme via applyColorScheme(prior).
 */

import { useState, useEffect, useMemo, useRef } from 'react';
import {
  CUSTOM_THEME_TOKENS,
  COLOR_SCHEMES,
  applyColorScheme,
  saveColorScheme,
  saveCustomTheme,
  loadCustomTheme,
  loadColorScheme,
  tokensForBase,
  DEFAULT_DARK_TOKENS,
  DAYLIGHT_TOKENS,
  type CustomTheme,
  type CustomThemeToken,
  type PresetScheme,
} from './colorScheme';
import './ThemeDesigner.css';

export interface ThemeDesignerProps {
  open: boolean;
  onClose: () => void;
}

/** Group the editable tokens for the UI. The grouping isn't load-bearing for
 *  the model (the model is a flat token map) — it's just so the operator
 *  doesn't see a 20-row undifferentiated list. */
interface TokenGroup {
  label: string;
  help: string;
  tokens: { id: CustomThemeToken; label: string; help?: string }[];
}

const GROUPS: TokenGroup[] = [
  {
    label: 'Surfaces',
    help: 'The base background and the elevation ladder (chrome panes, hover states).',
    tokens: [
      { id: 'bg', label: 'Background', help: 'The window background — what shows through everywhere else.' },
      { id: 'surface', label: 'Surface', help: 'Cards, panels, the message list.' },
      { id: 'surface-2', label: 'Surface 2', help: 'Inputs, hovered rows, secondary surfaces.' },
      { id: 'elevated', label: 'Elevated', help: 'Dropdowns, popovers, the active row.' },
    ],
  },
  {
    label: 'Borders',
    help: 'The three tiers of dividing lines — soft (subtle), regular, strong (focus).',
    tokens: [
      { id: 'border-soft', label: 'Border (soft)' },
      { id: 'border', label: 'Border' },
      { id: 'border-strong', label: 'Border (strong)' },
    ],
  },
  {
    label: 'Text',
    help: 'Primary text, dim text (labels), faint text (help).',
    tokens: [
      { id: 'text', label: 'Text' },
      { id: 'text-dim', label: 'Text (dim)' },
      { id: 'text-faint', label: 'Text (faint)' },
    ],
  },
  {
    label: 'Accent',
    help: 'The orange/red/etc. that highlights selected items, links, and buttons.',
    tokens: [
      { id: 'accent', label: 'Accent' },
      { id: 'accent-2', label: 'Accent (bright)', help: 'Hovers + brighter highlights.' },
      { id: 'tux-accent-fg', label: 'On-accent text', help: 'The text color drawn on top of accent (button labels).' },
    ],
  },
  {
    label: 'Radio dock',
    help: 'The radio panel chrome — separate from the project accent so the dock can keep a green identity even when accent is amber/brown.',
    tokens: [
      { id: 'modem-accent', label: 'Modem accent', help: 'MODEM title, Connect button, ARQ on-state.' },
      { id: 'modem-accent-2', label: 'Modem accent (bright)', help: 'Hover state for the Connect button.' },
      { id: 'modem-accent-soft', label: 'Modem accent (soft)', help: 'Radio panel header background tint.' },
      { id: 'modem-accent-fg', label: 'On-modem-accent text', help: 'The text color drawn on top of the modem accent.' },
    ],
  },
  {
    label: 'Status / semantic',
    help: 'Success/error/info colors used in dots, badges, alerts.',
    tokens: [
      { id: 'unread-dot', label: 'Unread dot' },
      { id: 'success', label: 'Success' },
      { id: 'error', label: 'Error' },
      { id: 'tux-danger-fg', label: 'On-error text' },
      { id: 'info', label: 'Info' },
      { id: 'form-tag', label: 'Form-tag color', help: 'Highlight color for HTML-form messages.' },
    ],
  },
];

/** The bases the operator can start their custom theme from. The label is
 *  human; the id maps to colorScheme.ts's tokensForBase(). */
const BASE_OPTIONS: { id: PresetScheme; label: string }[] = COLOR_SCHEMES.map((s) => ({
  id: s.id,
  label: s.label,
}));

/** Map a CSS color string to a #rrggbb suitable for `<input type=color>`. The
 *  browser color input only accepts hex; we coerce non-hex inputs (rgba/oklch)
 *  to a neutral fallback so the picker has SOMETHING to show. The underlying
 *  custom theme keeps the operator's original string — the picker is a
 *  surface, not the model. */
function toHex(value: string): string {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed.toLowerCase();
  if (/^#[0-9a-fA-F]{3}$/.test(trimmed)) {
    // #abc → #aabbcc
    const [, a, b, c] = /^#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])$/.exec(trimmed)!;
    return `#${a}${a}${b}${b}${c}${c}`.toLowerCase();
  }
  // Non-hex (rgba, oklch, named): fall back to a mid-gray for the picker. The
  // operator's typed value still drives the live preview via the text input.
  return '#888888';
}

export function ThemeDesigner({ open, onClose }: ThemeDesignerProps) {
  // Snapshot the previously-applied scheme so Cancel restores it. Captured
  // when the panel opens; the live preview otherwise overwrites the inline
  // style and there's no way to recover the prior visual state from the DOM.
  const priorSchemeRef = useRef<ReturnType<typeof loadColorScheme> | null>(null);

  // Seed-from picker. Changing this rewrites every token from the chosen
  // base; the operator's manual edits since the last seed are lost (we'd
  // prompt-confirm but that's window clutter — the picker is the explicit
  // gesture, the operator knows what they're doing).
  const [base, setBase] = useState<PresetScheme>('default');

  // The working theme. Initialised on open from any saved custom theme;
  // otherwise from the chosen base.
  const [draft, setDraft] = useState<CustomTheme | null>(null);

  // Open-side-effects: snapshot the prior scheme + load any saved custom
  // theme + capture initial base.
  useEffect(() => {
    if (!open) return;
    priorSchemeRef.current = loadColorScheme();
    const saved = loadCustomTheme();
    if (saved) {
      setDraft(saved);
      // The "base" picker doesn't try to detect which preset the saved theme
      // originated from (lossy after edits) — default it to dark.
      setBase('default');
    } else {
      setBase('default');
      setDraft({
        name: 'My theme',
        mode: 'dark',
        tokens: { ...DEFAULT_DARK_TOKENS },
      });
    }
  }, [open]);

  // Live-preview: every token edit applies as inline style so the WHOLE app
  // re-paints. Drives the visual feedback loop the designer is about. We
  // toggle data-theme to 'custom' so any CSS that watches the attribute
  // (none currently, but defensive against future code) also reacts.
  useEffect(() => {
    if (!open || !draft) return;
    const root = document.documentElement;
    root.dataset.theme = 'custom';
    for (const t of CUSTOM_THEME_TOKENS) {
      root.style.setProperty(`--${t}`, draft.tokens[t]);
    }
    root.style.colorScheme = draft.mode;
  }, [open, draft]);

  // Escape closes (matches SettingsPanel + the rest of the chrome).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') handleCancel();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
    // handleCancel is stable enough — its only dep is the prior-scheme ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Heuristic: "is the base light?" — used to default the mode toggle on the
  // first base swap so daylight base → light mode automatically.
  const baseIsLight = useMemo(() => {
    const opt = COLOR_SCHEMES.find((s) => s.id === base);
    return opt?.mode === 'light';
  }, [base]);

  function handleSeedFromBase(next: PresetScheme) {
    setBase(next);
    const seed = tokensForBase(next);
    const opt = COLOR_SCHEMES.find((s) => s.id === next);
    setDraft((prev) => ({
      name: prev?.name ?? 'My theme',
      mode: opt?.mode ?? 'dark',
      tokens: { ...seed },
    }));
  }

  function handleTokenChange(token: CustomThemeToken, color: string) {
    setDraft((prev) =>
      prev
        ? { ...prev, tokens: { ...prev.tokens, [token]: color } }
        : prev,
    );
  }

  function handleSave() {
    if (!draft) return;
    saveCustomTheme(draft);
    saveColorScheme('custom');
    applyColorScheme('custom');
    onClose();
  }

  function handleCancel() {
    // Restore the previously-applied scheme so the operator's edits don't
    // bleed past the cancel. applyColorScheme strips our inline style.
    const prior = priorSchemeRef.current ?? 'default';
    applyColorScheme(prior);
    onClose();
  }

  function handleResetToBase() {
    handleSeedFromBase(base);
  }

  if (!open || !draft) return null;

  return (
    <div
      className="tux-theme-designer-backdrop"
      data-testid="theme-designer-backdrop"
      onClick={handleCancel}
    >
      <div
        className="tux-theme-designer-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Customize color scheme"
        data-testid="theme-designer-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-theme-designer-header">
          <h2 className="tux-theme-designer-title">Customize color scheme</h2>
          <button
            type="button"
            className="tux-theme-designer-close"
            data-testid="theme-designer-close"
            aria-label="Close customize panel"
            onClick={handleCancel}
          >
            ×
          </button>
        </div>

        <div className="tux-theme-designer-help">
          Tweak any token below — the preview is live. Save persists this as
          “{draft.name || 'My theme'}” under <em>View → Color Scheme</em>.
        </div>

        <fieldset className="tux-theme-designer-group">
          <legend>Theme</legend>
          <label className="tux-theme-designer-field">
            <span className="tux-theme-designer-field-label">Name</span>
            <input
              type="text"
              className="tux-theme-designer-text-input"
              data-testid="theme-designer-name"
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              maxLength={48}
            />
          </label>
          <label className="tux-theme-designer-field">
            <span className="tux-theme-designer-field-label">Start from</span>
            <select
              className="tux-theme-designer-text-input"
              data-testid="theme-designer-base"
              value={base}
              onChange={(e) => handleSeedFromBase(e.target.value as PresetScheme)}
            >
              {BASE_OPTIONS.map((b) => (
                <option key={b.id} value={b.id}>{b.label}</option>
              ))}
            </select>
          </label>
          <label className="tux-theme-designer-field">
            <span className="tux-theme-designer-field-label">Mode</span>
            <select
              className="tux-theme-designer-text-input"
              data-testid="theme-designer-mode"
              value={draft.mode}
              onChange={(e) => setDraft({ ...draft, mode: e.target.value as 'light' | 'dark' })}
            >
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
            <span className="tux-theme-designer-field-help">
              {baseIsLight
                ? 'Base preset is light — drives WebKitGTK native control rendering.'
                : 'Base preset is dark.'}
            </span>
          </label>
        </fieldset>

        {GROUPS.map((g) => (
          <fieldset className="tux-theme-designer-group" key={g.label}>
            <legend>{g.label}</legend>
            <div className="tux-theme-designer-group-help">{g.help}</div>
            {g.tokens.map((t) => (
              <div className="tux-theme-designer-row" key={t.id}>
                <label
                  className="tux-theme-designer-row-label"
                  htmlFor={`theme-designer-${t.id}`}
                >
                  <span className="tux-theme-designer-token-label">{t.label}</span>
                  {t.help && (
                    <span className="tux-theme-designer-token-help">{t.help}</span>
                  )}
                </label>
                <div className="tux-theme-designer-row-controls">
                  <input
                    type="color"
                    id={`theme-designer-${t.id}`}
                    data-testid={`theme-designer-color-${t.id}`}
                    className="tux-theme-designer-color"
                    value={toHex(draft.tokens[t.id])}
                    onChange={(e) => handleTokenChange(t.id, e.target.value)}
                    aria-label={`${t.label} color picker`}
                  />
                  <input
                    type="text"
                    className="tux-theme-designer-color-text"
                    data-testid={`theme-designer-text-${t.id}`}
                    value={draft.tokens[t.id]}
                    onChange={(e) => handleTokenChange(t.id, e.target.value)}
                    aria-label={`${t.label} color value`}
                  />
                </div>
              </div>
            ))}
          </fieldset>
        ))}

        <div className="tux-theme-designer-actions">
          <button
            type="button"
            className="tux-theme-designer-button"
            data-testid="theme-designer-reset"
            onClick={handleResetToBase}
          >
            Reset to base
          </button>
          <div className="tux-theme-designer-actions-right">
            <button
              type="button"
              className="tux-theme-designer-button"
              data-testid="theme-designer-cancel"
              onClick={handleCancel}
            >
              Cancel
            </button>
            <button
              type="button"
              className="tux-theme-designer-button tux-theme-designer-button-primary"
              data-testid="theme-designer-save"
              onClick={handleSave}
            >
              Save
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// Re-export so AppShell + tests can use these without a separate import path.
export { DAYLIGHT_TOKENS, DEFAULT_DARK_TOKENS };
