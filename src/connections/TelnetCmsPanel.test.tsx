// src/connections/TelnetCmsPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
import { TelnetCmsPanel } from './TelnetCmsPanel';

describe('<TelnetCmsPanel>', () => {
  it('renders in a reading-pane root with the host + transport controls', () => {
    render(<TelnetCmsPanel host="cms-z.winlink.org" transport="Telnet" onPersist={vi.fn()} />);
    const root = screen.getByTestId('telnet-cms-panel-root');
    expect(root.className).toContain('reading-pane');
    expect((screen.getByTestId('conn-host') as HTMLInputElement).value).toBe('cms-z.winlink.org');
  });
  it('commits host on blur via onPersist (trimmed)', () => {
    const onPersist = vi.fn();
    render(<TelnetCmsPanel host="" transport="CmsSsl" onPersist={onPersist} />);
    const input = screen.getByTestId('conn-host');
    fireEvent.change(input, { target: { value: ' server.winlink.org ' } });
    fireEvent.blur(input);
    expect(onPersist).toHaveBeenCalledWith({ host: 'server.winlink.org', transport: 'CmsSsl' });
  });
  it('selecting a transport radio commits immediately', () => {
    const onPersist = vi.fn();
    render(<TelnetCmsPanel host="cms-z.winlink.org" transport="CmsSsl" onPersist={onPersist} />);
    fireEvent.click(screen.getByDisplayValue('Telnet'));   // the Plaintext·8772 radio (value="Telnet")
    expect(onPersist).toHaveBeenCalledWith({ host: 'cms-z.winlink.org', transport: 'Telnet' });
  });
});
