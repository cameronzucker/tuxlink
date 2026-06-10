// Tests for the inline About Tuxlink dialog (tuxlink-35g0).

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { AboutDialog } from './AboutDialog';

// Mock @tauri-apps/plugin-shell so link clicks don't try to spawn xdg-open
// in the test environment.
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(() => Promise.resolve()),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('AboutDialog', () => {
  it('renders nothing when not open', () => {
    render(<AboutDialog open={false} onClose={() => {}} />);
    expect(screen.queryByTestId('about-panel')).toBeNull();
  });

  it('renders the panel with product name, version, and the disclaimer', () => {
    render(<AboutDialog open={true} onClose={() => {}} />);
    expect(screen.getByTestId('about-panel')).toBeInTheDocument();
    expect(screen.getByText('Tuxlink')).toBeInTheDocument();
    expect(screen.getByTestId('about-version').textContent ?? '').toMatch(/^v\d+\.\d+\.\d+/);
    // The Alpha caution is load-bearing: the operator must see the status.
    expect(screen.getByText(/Alpha release/i)).toBeInTheDocument();
    expect(screen.getByText(/not for operational deployment/i)).toBeInTheDocument();
    expect(screen.queryByText(/pre-alpha/i)).toBeNull();
    expect(screen.getByText(/version\.txt via release-please/i)).toBeInTheDocument();
  });

  it('renders links for license, repo, releases, changelog, issues', () => {
    render(<AboutDialog open={true} onClose={() => {}} />);
    expect(screen.getByTestId('about-license-link')).toHaveTextContent('GPL-3.0-or-later');
    expect(screen.getByTestId('about-license-link')).toHaveAttribute(
      'href',
      'https://github.com/cameronzucker/tuxlink/blob/main/LICENSE',
    );
    expect(screen.getByTestId('about-repo-link')).toBeInTheDocument();
    expect(screen.getByTestId('about-releases-link')).toHaveAttribute(
      'href',
      'https://github.com/cameronzucker/tuxlink/releases',
    );
    expect(screen.getByTestId('about-changelog-link')).toBeInTheDocument();
    expect(screen.getByTestId('about-issues-link')).toBeInTheDocument();
    expect(screen.getByText(/released under GPL-3\.0-or-later/i)).toBeInTheDocument();
    expect(screen.queryByText(/^MIT$/i)).toBeNull();
    expect(screen.queryByText(/under the MIT License/i)).toBeNull();
  });

  it('clicking an outbound link calls shell-open with the URL (not in-app navigation)', async () => {
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    render(<AboutDialog open={true} onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('about-repo-link'));
    expect(shellOpenMock).toHaveBeenCalledWith('https://github.com/cameronzucker/tuxlink');
  });

  it('Close button calls onClose', () => {
    let closeCount = 0;
    render(<AboutDialog open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('about-ok'));
    expect(closeCount).toBe(1);
  });

  it('Esc closes the dialog', () => {
    let closeCount = 0;
    render(<AboutDialog open={true} onClose={() => { closeCount++; }} />);
    act(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(closeCount).toBe(1);
  });

  it('Backdrop click closes the dialog', () => {
    let closeCount = 0;
    render(<AboutDialog open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('about-backdrop'));
    expect(closeCount).toBe(1);
  });

  it('clicking inside the panel does NOT close it', () => {
    let closeCount = 0;
    render(<AboutDialog open={true} onClose={() => { closeCount++; }} />);
    fireEvent.click(screen.getByTestId('about-panel'));
    expect(closeCount).toBe(0);
  });
});
