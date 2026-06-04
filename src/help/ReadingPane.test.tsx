import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReadingPane } from './ReadingPane';
import { getTopicBySlug } from './topics';
import type { HelpTopic } from './topics';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));

const intro = getTopicBySlug('01-getting-started')!;
const conn = getTopicBySlug('02-connections')!;
const mailbox = getTopicBySlug('03-mailbox')!;

/** Build a minimal synthetic HelpTopic so tests aren't tied to real topic content. */
function makeTopic(slug: string, body: string): HelpTopic {
  return {
    slug,
    number: slug.slice(0, 2),
    displayName: slug,
    body,
    sectionId: 'using',
  };
}

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

describe('extended link interceptor', () => {
  it('scrolls to #anchor without navigating', () => {
    const onNavigate = vi.fn();
    // Topic body contains a named heading and a same-topic anchor link.
    const topic = makeTopic(
      '05-anchors',
      '# Anchor test\n\n## VARA HF {#vara-hf}\n\nSee [VARA HF](#vara-hf) for details.',
    );
    const { container } = render(<ReadingPane topic={topic} onNavigate={onNavigate} />);

    const anchorLink = container.querySelector('a[href="#vara-hf"]');
    expect(anchorLink, 'expected a same-topic #anchor link in the rendered topic').not.toBeNull();
    fireEvent.click(anchorLink!);

    // onNavigate must NOT be called — native browser scroll handles this case.
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it('navigates to slug + schedules scroll to anchor on combined link', () => {
    const onNavigate = vi.fn();
    // Spy on requestAnimationFrame so we can flush it synchronously.
    const rafCallbacks: FrameRequestCallback[] = [];
    const rafSpy = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      rafCallbacks.push(cb);
      return rafCallbacks.length;
    });
    // jsdom doesn't implement scrollIntoView — define a no-op stub so vi.spyOn can wrap it.
    if (!Element.prototype.scrollIntoView) {
      Element.prototype.scrollIntoView = () => {};
    }
    // Spy on Element.prototype.scrollIntoView so we can verify it fires.
    const scrollSpy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});

    // Topic body contains a cross-topic combined link (slug + anchor).
    const topic = makeTopic(
      '05-anchors',
      '# Anchor test\n\nSee [VARA HF](02-connections.md#vara-hf) for details.',
    );
    const { container } = render(<ReadingPane topic={topic} onNavigate={onNavigate} />);

    // Plant an element with id="vara-hf" so document.querySelector finds it.
    const targetEl = document.createElement('h2');
    targetEl.id = 'vara-hf';
    container.appendChild(targetEl);

    const combinedLink = container.querySelector('a[href="02-connections.md#vara-hf"]');
    expect(combinedLink, 'expected a combined .md#anchor link in the rendered topic').not.toBeNull();
    fireEvent.click(combinedLink!);

    // onNavigate must be called with the topic slug (without extension or anchor).
    expect(onNavigate).toHaveBeenCalledWith('02-connections');

    // Flush the requestAnimationFrame callback to assert the scroll fires.
    for (const cb of rafCallbacks) cb(0);
    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'auto', block: 'start' });

    rafSpy.mockRestore();
    scrollSpy.mockRestore();
  });
});
