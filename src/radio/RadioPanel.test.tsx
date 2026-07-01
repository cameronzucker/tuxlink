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

const CONTROLS_CSS_MODULES = import.meta.glob('../styles/controls.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const controlsCss = CONTROLS_CSS_MODULES['../styles/controls.css'];

describe('<RadioPanel>', () => {
  it('renders the shell with the panel title from the mode', () => {
    render(
      <RadioPanel mode={{ kind: 'ardop-hf', intent: 'cms' }} onClose={() => {}}>
        <div data-testid="child-content">body</div>
      </RadioPanel>,
    );
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('ARDOP Winlink');
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

  // tuxlink-6jpf: RF dial modes get a "Find a gateway" affordance in the panel
  // chrome (opens the station finder). Telnet/P2P do not pass onFindGateway.
  it('renders a Find a gateway button that calls onFindGateway when provided', () => {
    const onFindGateway = vi.fn();
    render(
      <RadioPanel
        mode={{ kind: 'ardop-hf', intent: 'cms' }}
        onClose={() => {}}
        onFindGateway={onFindGateway}
      >
        <div />
      </RadioPanel>,
    );
    const btn = screen.getByTestId('radio-panel-find-gateway');
    expect(btn.closest('.radio-panel-command-row')).toBe(screen.getByTestId('radio-panel-command-row'));
    expect(btn).toHaveTextContent('Find a gateway');
    btn.click();
    expect(onFindGateway).toHaveBeenCalledOnce();
  });

  it('omits the Find a gateway button when onFindGateway is not provided', () => {
    render(
      <RadioPanel mode={{ kind: 'telnet', intent: 'cms' }} onClose={() => {}}>
        <div />
      </RadioPanel>,
    );
    expect(screen.queryByTestId('radio-panel-find-gateway')).toBeNull();
    expect(screen.queryByTestId('radio-panel-command-row')).toBeNull();
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
    // After tuxlink-3m0vx migration, danger-stop styling lives in controls.css
    // as .tux-btn--danger.tux-btn--soft (the Button wrapper's output).
    const badRule = controlsCss.match(/\.tux-btn--danger\.tux-btn--soft\s*\{[^}]+\}/)?.[0] ?? '';
    expect(badRule).toContain('background: var(--tux-danger-surface);');
    expect(badRule).toContain('border-color: color-mix(in srgb, var(--error) 35%, transparent);');
    expect(badRule).toContain('color: var(--error);');
    expect(badRule).not.toContain('background: var(--error);');
    expect(badRule).not.toContain('color: var(--tux-danger-fg);');

    const hoverRule = controlsCss.match(/\.tux-btn--danger\.tux-btn--soft:hover:not\(:disabled\)\s*\{[^}]+\}/)?.[0] ?? '';
    expect(hoverRule).toContain('background: color-mix(in srgb, var(--error) 18%, transparent);');
    expect(hoverRule).not.toContain('filter: brightness');
  });

  it('places Find a gateway in a padded command row instead of loose body content (tuxlink-9525)', () => {
    const rowRule = radioPanelCss.match(/\.radio-panel-command-row\s*\{[^}]+\}/)?.[0] ?? '';
    expect(rowRule).toContain('padding: 8px 12px;');
    expect(rowRule).toContain('border-bottom: 1px dashed');
    expect(rowRule).toContain('display: flex;');

    const buttonRule = radioPanelCss.match(/\.radio-panel-find-gateway\s*\{[^}]+\}/)?.[0] ?? '';
    expect(buttonRule).toContain('display: inline-flex;');
    expect(buttonRule).toContain('min-height: 32px;');
    expect(buttonRule).not.toContain('margin: 0 0 10px');
  });
});

// FZ-M1 compact interior (tuxlink-h7q7 Task 6b)
const RADIO_PANEL_CSS = (
  import.meta.glob('./RadioPanel.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>
)['./RadioPanel.css'];
const MODEM_LINK_CSS = (
  import.meta.glob('./sections/ModemLinkSection.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>
)['./sections/ModemLinkSection.css'];

describe('RadioPanel interior compact CSS (tuxlink-h7q7 Task 6b)', () => {
  it('bumps the segmented tabs to >=44px / 12px in compact (reused by Contacts+Favorites tabs)', () => {
    const block = MODEM_LINK_CSS.slice(MODEM_LINK_CSS.indexOf('@media (max-width: 1365px)'));
    expect(block).toMatch(/\.radio-panel-segmented button \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.radio-panel-segmented button \{[\s\S]*?font-size:\s*12px/);
  });
  it('bumps the close button, chips, and buttons to >=44px in compact', () => {
    const block = RADIO_PANEL_CSS.slice(RADIO_PANEL_CSS.indexOf('@media (max-width: 1365px)'));
    expect(block).toMatch(/\.radio-panel-close \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.radio-panel-chip \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.radio-panel-find-gateway \{[\s\S]*?min-height:\s*44px/);
    expect(block).toMatch(/\.radio-panel \.tux-btn \{[\s\S]*?min-height:\s*44px/);
  });
});

// Codex post-impl review: the small controls the first Task 6b pass missed.
const LISTEN_CSS = (
  import.meta.glob('./sections/ListenSection.css', { eager: true, query: '?raw', import: 'default' }) as Record<string, string>
)['./sections/ListenSection.css'];

describe('RadioPanel interior compact CSS — small controls (Codex post-impl review)', () => {
  it('bumps small buttons, the chip-remove ✕, native radios, and help text', () => {
    // .radio-panel-btn-sm migrated to .tux-btn--xs; floor now covered by
    // .radio-panel .tux-btn in RadioPanel.css (see Task 7 compact a11y floor fix).
    const panel = RADIO_PANEL_CSS.slice(RADIO_PANEL_CSS.indexOf('@media (max-width: 1365px)'));
    expect(panel).toMatch(/\.radio-panel \.tux-btn \{[\s\S]*?min-height:\s*44px/);
    const listen = LISTEN_CSS.slice(LISTEN_CSS.indexOf('@media (max-width: 1365px)'));
    expect(listen).toMatch(/\.radio-panel-chip-x \{[\s\S]*?min-height:\s*44px/);
    expect(listen).toMatch(/\.radio-panel-help \{[\s\S]*?font-size:\s*12px/);
    expect(panel).toMatch(/input\[type='radio'\] \{[\s\S]*?width:\s*22px/);
  });
});
