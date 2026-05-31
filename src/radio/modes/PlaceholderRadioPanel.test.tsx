// src/radio/modes/PlaceholderRadioPanel.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { PlaceholderRadioPanel } from './PlaceholderRadioPanel';

describe('<PlaceholderRadioPanel>', () => {
  it('renders a "coming soon" placeholder with the mode name', () => {
    render(
      <PlaceholderRadioPanel
        mode={{ kind: 'ardop-hf', intent: 'cms' }}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-placeholder')).toHaveTextContent(
      /Ardop Winlink panel coming soon/i,
    );
  });
});
