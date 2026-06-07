// src/radio/RadioPanel.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RadioPanel } from './RadioPanel';

const RADIO_PANEL_CSS_MODULES = import.meta.glob('./RadioPanel.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const radioPanelCss = RADIO_PANEL_CSS_MODULES['./RadioPanel.css'];

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

  it('styles Stop actions as outlined danger at rest, matching primary button posture', () => {
    const badRule = radioPanelCss.match(/\.radio-panel-btn-bad\s*\{[^}]+\}/)?.[0] ?? '';
    expect(badRule).toContain('background: var(--tux-danger-surface);');
    expect(badRule).toContain('border-color: color-mix(in srgb, var(--error) 35%, transparent);');
    expect(badRule).toContain('color: var(--error);');
    expect(badRule).not.toContain('background: var(--error);');
    expect(badRule).not.toContain('color: var(--tux-danger-fg);');

    const hoverRule = radioPanelCss.match(/\.radio-panel-btn-bad:hover:not\(:disabled\)\s*\{[^}]+\}/)?.[0] ?? '';
    expect(hoverRule).toContain('background: color-mix(in srgb, var(--error) 18%, transparent);');
    expect(hoverRule).toContain('border-color: var(--error);');
    expect(hoverRule).not.toContain('filter: brightness');
  });
});
