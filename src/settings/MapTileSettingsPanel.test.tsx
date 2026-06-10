import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { MapTileSettingsPanel } from './MapTileSettingsPanel';

describe('MapTileSettingsPanel', () => {
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
