import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StubPanel } from './StubPanel';

describe('<StubPanel>', () => {
  it('renders the coming-soon pane with the session-type + protocol labels', () => {
    render(<StubPanel sessionType="radio-only" protocol="packet" />);
    const root = screen.getByTestId('stub-panel-root');
    expect(root.className).toContain('reading-pane');
    expect(root.textContent).toMatch(/Radio-only/);
    expect(root.textContent).toMatch(/Packet/);
    expect(root.textContent).toMatch(/soon|not yet/i);
  });
});
