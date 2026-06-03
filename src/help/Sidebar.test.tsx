import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Sidebar } from './Sidebar';
import { getTopicBySlug } from './topics';

describe('Sidebar', () => {
  it('renders all four section headers', () => {
    const { container } = render(
      <Sidebar activeSlug="01-getting-started" onSelect={() => {}} />,
    );
    const headers = Array.from(
      container.querySelectorAll('.tux-help-sb-section-title'),
    ).map((el) => el.textContent);
    expect(headers).toEqual([
      'Getting started',
      'Using Tuxlink',
      'Configuration',
      'Reference',
    ]);
  });

  it('renders every topic with its 2-digit number', () => {
    render(<Sidebar activeSlug="01-getting-started" onSelect={() => {}} />);
    // Each `.tux-help-sb-num` shows a 2-digit prefix.
    expect(screen.getByText('01')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('marks the active topic with aria-current=page', () => {
    render(<Sidebar activeSlug="02-connections" onSelect={() => {}} />);
    const active = screen.getByRole('link', { current: 'page' });
    expect(active.textContent).toMatch(/Connections/);
  });

  it('calls onSelect with the slug when a topic is clicked', () => {
    const onSelect = vi.fn();
    render(<Sidebar activeSlug="01-getting-started" onSelect={onSelect} />);
    // The mailbox topic's actual displayName is "The mailbox" (parsed from
    // the # heading in docs/user-guide/03-mailbox.md).
    const mailbox = getTopicBySlug('03-mailbox')!;
    fireEvent.click(screen.getByText(mailbox.displayName));
    expect(onSelect).toHaveBeenCalledWith('03-mailbox');
  });
});
