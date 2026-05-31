// src/radio/modes/TelnetRadioPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TelnetRadioPanel } from './TelnetRadioPanel';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
}));

describe('<TelnetRadioPanel>', () => {
  it('renders the Telnet Winlink panel with endpoint and transport', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
    // "cms.winlink.org" appears in both the header sub and the Endpoint
    // readonly field; the port disambiguates to the Endpoint field.
    expect(screen.getByText(/cms\.winlink\.org:8773/)).toBeInTheDocument();
    expect(screen.getByText(/CMS-SSL/)).toBeInTheDocument();
  });

  it('renders the Session log section', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders Start and Stop actions', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByRole('button', { name: /Start/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/i })).toBeInTheDocument();
  });

  it('clicking Start fires cms_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Start/i }));
    expect(invoke).toHaveBeenCalledWith('cms_connect');
  });
});
