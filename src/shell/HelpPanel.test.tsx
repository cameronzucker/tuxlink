// Tests for the inline HelpPanel (tuxlink-35g0). The panel renders bundled
// user-guide markdown via the minimal block parser. The markdown is loaded
// at module import via `import.meta.glob`, so we don't try to fake the
// filesystem — the assertions hit real, bundled topic content.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { HelpPanel } from './HelpPanel';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(() => Promise.resolve()),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('HelpPanel', () => {
  it('renders nothing when not open', () => {
    render(<HelpPanel open={false} onClose={() => {}} />);
    expect(screen.queryByTestId('help-panel')).toBeNull();
  });

  it('renders the topic TOC + content when open', () => {
    render(<HelpPanel open={true} onClose={() => {}} />);
    expect(screen.getByTestId('help-panel')).toBeInTheDocument();
    expect(screen.getByTestId('help-content')).toBeInTheDocument();
    // The bundled topics include Getting started — the first topic by
    // filename prefix (01-getting-started).
    expect(screen.getByTestId('help-topic-01-getting-started')).toBeInTheDocument();
  });

  it('opens on the first topic by default', () => {
    render(<HelpPanel open={true} onClose={() => {}} />);
    const firstTopic = screen.getByTestId('help-topic-01-getting-started');
    expect(firstTopic.className).toContain('active');
  });

  it('clicking a different topic swaps the active content', () => {
    render(<HelpPanel open={true} onClose={() => {}} />);
    const settingsTopic = screen.getByTestId('help-topic-07-settings');
    fireEvent.click(settingsTopic);
    expect(settingsTopic.className).toContain('active');
    // Content has the Settings H1 heading.
    expect(screen.getByTestId('help-content').textContent).toContain('Settings');
  });

  it('Esc closes the panel', () => {
    let closeCount = 0;
    render(<HelpPanel open={true} onClose={() => { closeCount++; }} />);
    act(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(closeCount).toBe(1);
  });

  it('Backdrop click closes the panel', () => {
    let closeCount = 0;
    render(<HelpPanel open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('help-backdrop'));
    expect(closeCount).toBe(1);
  });

  it('clicking inside the panel does NOT close it', () => {
    let closeCount = 0;
    render(<HelpPanel open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('help-panel'));
    expect(closeCount).toBe(0);
  });
});
