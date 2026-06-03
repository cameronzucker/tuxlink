import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { HelpView } from './HelpView';

describe('HelpView (Task 2 skeleton)', () => {
  it('renders the help root container', () => {
    render(<HelpView />);
    expect(screen.getByTestId('tux-help-root')).toBeInTheDocument();
  });
});
