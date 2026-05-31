// src/radio/RadioPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RadioPanel } from './RadioPanel';

describe('<RadioPanel>', () => {
  it('renders the shell with the panel title from the mode', () => {
    render(
      <RadioPanel mode={{ kind: 'ardop-hf', intent: 'cms' }} onClose={() => {}}>
        <div data-testid="child-content">body</div>
      </RadioPanel>,
    );
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Ardop Winlink');
    expect(screen.getByTestId('child-content')).toBeInTheDocument();
  });

  it('renders a close button that calls onClose', () => {
    const onClose = vi.fn();
    render(
      <RadioPanel mode={{ kind: 'telnet', intent: 'cms' }} onClose={onClose}>
        <div />
      </RadioPanel>,
    );
    const close = screen.getByTestId('radio-panel-close');
    close.click();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('renders the state dot with the data-state attribute for CSS theming', () => {
    render(
      <RadioPanel
        mode={{ kind: 'ardop-hf', intent: 'cms' }}
        state="connected"
        onClose={() => {}}
      >
        <div />
      </RadioPanel>,
    );
    expect(screen.getByTestId('radio-panel-dot')).toHaveAttribute('data-state', 'connected');
  });
});
