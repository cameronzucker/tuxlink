// Tests for the inline ThemeDesigner panel (tuxlink-vgth).
//
// The designer's behavior contract:
//   1. On open it seeds from any saved CustomTheme; falls back to DEFAULT_DARK_TOKENS.
//   2. Editing a token writes inline `--<token>` style on <html> live.
//   3. Save persists the CustomTheme + sets the applied scheme to 'custom'.
//   4. Cancel restores the prior applied scheme; no persistence happens.
//   5. Esc closes (same as Cancel).
//   6. Backdrop click closes (same as Cancel).
//   7. Reset to base re-seeds the working draft from the selected base.

import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ThemeDesigner } from './ThemeDesigner';
import {
  CUSTOM_THEME_TOKENS,
  DEFAULT_DARK_TOKENS,
  DAYLIGHT_TOKENS,
  GITHUB_DARK_TOKENS,
  OFFICE_DARK_TOKENS,
  loadColorScheme,
  loadCustomTheme,
  saveCustomTheme,
  applyColorScheme,
  saveColorScheme,
} from './colorScheme';

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  for (const t of CUSTOM_THEME_TOKENS) {
    document.documentElement.style.removeProperty(`--${t}`);
  }
  document.documentElement.style.removeProperty('color-scheme');
});

describe('ThemeDesigner — open / close', () => {
  it('renders nothing when not open', () => {
    render(<ThemeDesigner open={false} onClose={() => {}} />);
    expect(screen.queryByTestId('theme-designer-panel')).toBeNull();
  });

  it('renders the panel when open', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    expect(screen.getByTestId('theme-designer-panel')).toBeInTheDocument();
  });

  it('seeds from DEFAULT_DARK_TOKENS when no saved theme exists', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    expect(bgText.value).toBe(DEFAULT_DARK_TOKENS.bg);
  });

  it('seeds from the saved custom theme when present', () => {
    const fixture = {
      name: 'Saved field theme',
      mode: 'light' as const,
      tokens: { ...DEFAULT_DARK_TOKENS, bg: '#abcdef' },
    };
    saveCustomTheme(fixture);
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const nameField = screen.getByTestId('theme-designer-name') as HTMLInputElement;
    expect(nameField.value).toBe('Saved field theme');
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    expect(bgText.value).toBe('#abcdef');
  });
});

describe('ThemeDesigner — live preview', () => {
  it('applies tokens to <html> inline style on open', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    expect(document.documentElement.dataset.theme).toBe('custom');
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe(DEFAULT_DARK_TOKENS.bg);
    expect(document.documentElement.style.getPropertyValue('--accent')).toBe(DEFAULT_DARK_TOKENS.accent);
  });

  it('updates the inline style when a token text field changes', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    fireEvent.change(bgText, { target: { value: '#112233' } });
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#112233');
  });

  it('updates the inline style when a color picker changes', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const bgPicker = screen.getByTestId('theme-designer-color-bg') as HTMLInputElement;
    fireEvent.change(bgPicker, { target: { value: '#445566' } });
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#445566');
  });
});

describe('ThemeDesigner — base picker', () => {
  it('re-seeds every token when Start-from changes to Daylight', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const baseSelect = screen.getByTestId('theme-designer-base') as HTMLSelectElement;
    fireEvent.change(baseSelect, { target: { value: 'daylight' } });
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    expect(bgText.value).toBe(DAYLIGHT_TOKENS.bg);
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe(DAYLIGHT_TOKENS.bg);
  });

  it('changes the mode to light when picking a light base', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const baseSelect = screen.getByTestId('theme-designer-base') as HTMLSelectElement;
    fireEvent.change(baseSelect, { target: { value: 'daylight' } });
    const modeSelect = screen.getByTestId('theme-designer-mode') as HTMLSelectElement;
    expect(modeSelect.value).toBe('light');
    expect(document.documentElement.style.colorScheme).toBe('light');
  });

  it('re-seeds from the GitHub dark bundled snapshot', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const baseSelect = screen.getByTestId('theme-designer-base') as HTMLSelectElement;
    fireEvent.change(baseSelect, { target: { value: 'github-dark' } });
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    expect(bgText.value).toBe(GITHUB_DARK_TOKENS.bg);
    expect(document.documentElement.style.getPropertyValue('--accent')).toBe(GITHUB_DARK_TOKENS.accent);
    expect((screen.getByTestId('theme-designer-mode') as HTMLSelectElement).value).toBe('dark');
  });

  it('re-seeds from the Office dark bundled snapshot', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const baseSelect = screen.getByTestId('theme-designer-base') as HTMLSelectElement;
    fireEvent.change(baseSelect, { target: { value: 'office-dark' } });
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    expect(bgText.value).toBe(OFFICE_DARK_TOKENS.bg);
    expect(document.documentElement.style.getPropertyValue('--accent')).toBe(OFFICE_DARK_TOKENS.accent);
    expect((screen.getByTestId('theme-designer-mode') as HTMLSelectElement).value).toBe('dark');
  });
});

describe('ThemeDesigner — Save', () => {
  it('Save persists the draft + applies "custom" scheme + closes', () => {
    let closeCount = 0;
    const onClose = () => { closeCount++; };
    render(<ThemeDesigner open={true} onClose={onClose} />);
    const nameField = screen.getByTestId('theme-designer-name') as HTMLInputElement;
    fireEvent.change(nameField, { target: { value: 'My HF theme' } });
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    fireEvent.change(bgText, { target: { value: '#101010' } });

    fireEvent.click(screen.getByTestId('theme-designer-save'));

    expect(closeCount).toBe(1);
    expect(loadColorScheme()).toBe('custom');
    const persisted = loadCustomTheme();
    expect(persisted).not.toBeNull();
    expect(persisted!.name).toBe('My HF theme');
    expect(persisted!.tokens.bg).toBe('#101010');
  });
});

describe('ThemeDesigner — Cancel', () => {
  it('Cancel restores the prior scheme + does not persist', () => {
    // Prior state: night-red applied + saved.
    saveColorScheme('night-red');
    applyColorScheme('night-red');
    expect(document.documentElement.dataset.theme).toBe('night-red');

    let closeCount = 0;
    const onClose = () => { closeCount++; };
    render(<ThemeDesigner open={true} onClose={onClose} />);

    // Tweak a token (would persist if Save were clicked).
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    fireEvent.change(bgText, { target: { value: '#999999' } });

    fireEvent.click(screen.getByTestId('theme-designer-cancel'));

    expect(closeCount).toBe(1);
    // Persisted scheme remains night-red — saveCustomTheme was NOT called.
    expect(loadColorScheme()).toBe('night-red');
    expect(loadCustomTheme()).toBeNull();
    // Applied scheme restored to night-red.
    expect(document.documentElement.dataset.theme).toBe('night-red');
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('');
  });

  it('Esc behaves the same as Cancel', () => {
    saveColorScheme('default');
    let closeCount = 0;
    render(<ThemeDesigner open={true} onClose={() => { closeCount++; }} />);
    act(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(closeCount).toBe(1);
    expect(loadCustomTheme()).toBeNull();
  });

  it('Backdrop click behaves the same as Cancel', () => {
    saveColorScheme('default');
    let closeCount = 0;
    render(<ThemeDesigner open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('theme-designer-backdrop'));
    expect(closeCount).toBe(1);
    expect(loadCustomTheme()).toBeNull();
  });

  it('clicking inside the panel does NOT close it', () => {
    let closeCount = 0;
    render(<ThemeDesigner open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('theme-designer-panel'));
    expect(closeCount).toBe(0);
  });
});

describe('ThemeDesigner — Reset to base', () => {
  it('Reset re-seeds the working draft from the selected base', () => {
    render(<ThemeDesigner open={true} onClose={() => {}} />);
    const baseSelect = screen.getByTestId('theme-designer-base') as HTMLSelectElement;
    fireEvent.change(baseSelect, { target: { value: 'daylight' } });

    // Tweak a token, then reset.
    const bgText = screen.getByTestId('theme-designer-text-bg') as HTMLInputElement;
    fireEvent.change(bgText, { target: { value: '#abcdef' } });
    expect(bgText.value).toBe('#abcdef');

    fireEvent.click(screen.getByTestId('theme-designer-reset'));
    expect((screen.getByTestId('theme-designer-text-bg') as HTMLInputElement).value).toBe(DAYLIGHT_TOKENS.bg);
  });
});
