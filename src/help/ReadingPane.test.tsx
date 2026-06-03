import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReadingPane } from './ReadingPane';
import { getTopicBySlug } from './topics';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));

const intro = getTopicBySlug('01-getting-started')!;
const conn = getTopicBySlug('02-connections')!;
const mailbox = getTopicBySlug('03-mailbox')!;

describe('ReadingPane', () => {
  it('renders the topic displayName as an h1', () => {
    render(<ReadingPane topic={intro} onNavigate={() => {}} />);
    expect(
      screen.getByRole('heading', { level: 1, name: intro.displayName }),
    ).toBeInTheDocument();
  });

  it('renders rendered markdown body content', () => {
    render(<ReadingPane topic={conn} onNavigate={() => {}} />);
    // The connections topic mentions ARDOP repeatedly (paragraph + headings).
    // Confirm the parser surfaced at least one match.
    const matches = screen.getAllByText(/ARDOP/i);
    expect(matches.length).toBeGreaterThan(0);
  });

  it('intercepts inter-topic .md links and calls onNavigate with the slug', () => {
    const onNavigate = vi.fn();
    // Pick a topic with explicit inter-topic links — the mailbox topic
    // bottom-section links to several other topics by .md path.
    const target = mailbox;
    const { container } = render(<ReadingPane topic={target} onNavigate={onNavigate} />);

    // Find an inter-topic .md link by attribute and click it.
    const mdLink = container.querySelector('a[href$=".md"]');
    expect(mdLink, 'expected at least one inter-topic .md link in the rendered topic').not.toBeNull();
    fireEvent.click(mdLink!);

    expect(onNavigate).toHaveBeenCalled();
    // The slug derived from the link's href should match the digits-name pattern.
    const slug = (onNavigate.mock.calls[0][0] as string);
    expect(slug).toMatch(/^\d{2}-[a-z-]+$/);
  });
});
