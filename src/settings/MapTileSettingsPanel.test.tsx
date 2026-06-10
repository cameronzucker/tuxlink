import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { MapTileSettingsPanel } from './MapTileSettingsPanel';

// Raw sources for the CSS-codesplit regression (tuxlink-jgom). The panel reuses
// the shared `.tux-settings-*` overlay chrome but is lazy-loaded into its OWN
// chunk; the positioning rule (`.tux-settings-backdrop { position: fixed }`)
// lives in src/shell/SettingsPanel.css, imported only by the GPS SettingsPanel —
// a DIFFERENT lazy chunk. Opening Map tiles without first opening GPS Settings
// left the backdrop unpositioned, so it rendered as an in-flow block that shoved
// the whole app under the bottom bar. The panel MUST ship the chrome positioning
// with its own chunk.
const PANEL_SRC = import.meta.glob('./MapTileSettingsPanel.tsx', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const CSS_RAW = import.meta.glob(['./*.css', '../shell/SettingsPanel.css'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

describe('MapTileSettingsPanel', () => {
  it('ships the .tux-settings-backdrop overlay positioning via its own CSS imports (tuxlink-jgom)', () => {
    const src = PANEL_SRC['./MapTileSettingsPanel.tsx'];
    // CSS specifiers the component imports (relative paths only).
    const specs = [...src.matchAll(/import\s+['"](\.[^'"]+\.css)['"]/g)].map((m) => m[1]);
    const importedCss = specs.map((s) => CSS_RAW[s]).filter(Boolean).join('\n');
    // The overlay backdrop MUST be position:fixed within the panel's own import
    // graph — otherwise it renders inline and compresses the app (the #548 regression).
    const backdrop = importedCss.match(/\.tux-settings-backdrop\s*\{[^}]*\}/);
    expect(
      backdrop,
      'MapTileSettingsPanel must import the .tux-settings-backdrop chrome CSS, not rely on the GPS SettingsPanel chunk being loaded first',
    ).toBeTruthy();
    expect(backdrop![0]).toMatch(/position:\s*fixed/);
  });

  it('renders nothing when closed', () => {
    const { container } = render(<MapTileSettingsPanel open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders the LAN tile-source settings section when open', () => {
    render(<MapTileSettingsPanel open onClose={vi.fn()} />);
    expect(screen.getByTestId('map-tile-source-settings')).toBeInTheDocument();
    // It rides inside a labelled dialog (matches the other inline overlays).
    expect(screen.getByRole('dialog', { name: /map tiles/i })).toBeInTheDocument();
  });

  it('calls onClose on the close button, on Escape, and on backdrop click', () => {
    const onClose = vi.fn();
    render(<MapTileSettingsPanel open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('map-tile-settings-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(2);
    fireEvent.click(screen.getByTestId('map-tile-settings-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(3);
  });

  it('does not call onClose when the panel body is clicked', () => {
    const onClose = vi.fn();
    render(<MapTileSettingsPanel open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('map-tile-source-settings'));
    expect(onClose).not.toHaveBeenCalled();
  });
});
