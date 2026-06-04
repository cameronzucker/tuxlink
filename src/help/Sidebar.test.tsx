import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Sidebar } from './Sidebar';
import { getTopicBySlug } from './topics';
import type { DocsHit } from './useHelpSearch';

// Helper: minimal default props for the grouped-list mode.
function groupedProps(active = '01-what-is-tuxlink') {
  return {
    activeSlug: active,
    onSelect: vi.fn(),
    searchQuery: '',
    onSearchChange: vi.fn(),
    hits: undefined as DocsHit[] | undefined,
  };
}

describe('Sidebar (grouped-list mode)', () => {
  it('renders all eight section headers', () => {
    const { container } = render(<Sidebar {...groupedProps()} />);
    const headers = Array.from(
      container.querySelectorAll('.tux-help-sb-section-title'),
    ).map((el) => el.textContent);
    expect(headers).toEqual([
      'Quickstart',
      'Winlink fundamentals',
      'Radio integration',
      'Digital modes',
      'Using tuxlink',
      'Operating practices',
      'Reference',
      'Migration',
    ]);
  });

  it('renders every topic with its 2-digit number', () => {
    render(<Sidebar {...groupedProps()} />);
    expect(screen.getByText('01')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('marks the active topic with aria-current=page', () => {
    render(<Sidebar {...groupedProps('02-first-launch-wizard')} />);
    const active = screen.getByRole('link', { current: 'page' });
    expect(active.textContent).toMatch(/First-launch wizard/);
  });

  it('calls onSelect with the slug when a topic is clicked', () => {
    const props = groupedProps();
    render(<Sidebar {...props} />);
    const mailbox = getTopicBySlug('18-the-mailbox')!;
    fireEvent.click(screen.getByText(mailbox.displayName));
    expect(props.onSelect).toHaveBeenCalledWith('18-the-mailbox');
  });
});

describe('Sidebar (search input)', () => {
  it('renders a search input with placeholder', () => {
    render(<Sidebar {...groupedProps()} />);
    expect(screen.getByPlaceholderText(/Search topics/i)).toBeInTheDocument();
  });

  it('calls onSearchChange as the operator types', () => {
    const props = groupedProps();
    render(<Sidebar {...props} />);
    fireEvent.change(screen.getByPlaceholderText(/Search topics/i), {
      target: { value: 'ardop' },
    });
    expect(props.onSearchChange).toHaveBeenCalledWith('ardop');
  });

  it('renders the hit list (replacing the grouped list) when query is non-empty', () => {
    const hits: DocsHit[] = [
      { slug: '02-connections', title: 'Connections', snippet: 'About <mark>ARDOP</mark>' },
    ];
    const { container } = render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="ardop"
        onSearchChange={vi.fn()}
        hits={hits}
      />,
    );
    expect(screen.getByText('Connections')).toBeInTheDocument();
    // Grouped section headers are not rendered in hit-list mode.
    expect(container.querySelector('.tux-help-sb-section-title')).toBeNull();
  });

  it('renders the snippet markers as real <mark> elements (XSS-safe)', () => {
    const hits: DocsHit[] = [
      { slug: '02-connections', title: 'Connections', snippet: 'About <mark>ARDOP</mark>' },
    ];
    const { container } = render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="ardop"
        onSearchChange={vi.fn()}
        hits={hits}
      />,
    );
    const mark = container.querySelector('.tux-help-sb-hit-snippet mark');
    expect(mark?.textContent).toBe('ARDOP');
  });

  it('does NOT execute literal <script> tags in the snippet (XSS-safe)', () => {
    const hits: DocsHit[] = [
      {
        slug: 'evil',
        title: 'Evil',
        // If renderSnippet ever fell back to dangerouslySetInnerHTML, this
        // would create a <script> node. With the split-and-render pattern,
        // it's rendered as plain text and no <script> element exists.
        snippet: 'hi <script>alert(1)</script> bye',
      },
    ];
    const { container } = render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="x"
        onSearchChange={vi.fn()}
        hits={hits}
      />,
    );
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('.tux-help-sb-hit-snippet')?.textContent).toContain(
      'hi <script>alert(1)</script> bye',
    );
  });

  it('renders a "Searching…" status when hits is undefined and query is non-empty', () => {
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="ardop"
        onSearchChange={vi.fn()}
        hits={undefined}
      />,
    );
    expect(screen.getByText(/Searching/)).toBeInTheDocument();
  });

  it('renders "No matches." when hits is an empty array', () => {
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="zzz"
        onSearchChange={vi.fn()}
        hits={[]}
      />,
    );
    expect(screen.getByText(/No matches/)).toBeInTheDocument();
  });

  it('renders a clear (×) button when the search has text and calls onSearchChange("")', () => {
    const onSearchChange = vi.fn();
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={vi.fn()}
        searchQuery="ardop"
        onSearchChange={onSearchChange}
        hits={undefined}
      />,
    );
    const clear = screen.getByRole('button', { name: /clear search/i });
    fireEvent.click(clear);
    expect(onSearchChange).toHaveBeenCalledWith('');
  });

  it('calls onSelect with the hit slug when a hit is clicked', () => {
    const onSelect = vi.fn();
    const hits: DocsHit[] = [
      { slug: '02-connections', title: 'Connections', snippet: 'About ARDOP' },
    ];
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={onSelect}
        searchQuery="ardop"
        onSearchChange={vi.fn()}
        hits={hits}
      />,
    );
    fireEvent.click(screen.getByText('Connections'));
    expect(onSelect).toHaveBeenCalledWith('02-connections');
  });
});
