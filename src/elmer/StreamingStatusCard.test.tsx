import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { StreamingStatusCard, type StreamingStatusCardProps } from './StreamingStatusCard';

function props(overrides: Partial<StreamingStatusCardProps> = {}): StreamingStatusCardProps {
  return {
    verb: 'chasing DX',
    isResponding: false,
    answer: '',
    reasoning: '',
    tokensEstimate: 0,
    elapsedSecs: 3,
    expanded: false,
    onToggleExpand: () => {},
    ...overrides,
  };
}

describe('StreamingStatusCard', () => {
  it('collapsed by default: shows verb + elapsed, no body', () => {
    render(<StreamingStatusCard {...props()} />);
    expect(screen.getByTestId('elmer-stream-card')).toBeTruthy();
    expect(screen.getByTestId('elmer-stream-verb').textContent).toContain('chasing DX');
    expect(screen.getByTestId('elmer-stream-elapsed').textContent).toContain('3s');
    expect(screen.queryByTestId('elmer-stream-body')).toBeNull();
  });

  it('shows no token counter when the estimate is 0, and ~N tok when > 0', () => {
    const { rerender } = render(<StreamingStatusCard {...props({ tokensEstimate: 0 })} />);
    expect(screen.queryByTestId('elmer-stream-tokens')).toBeNull();
    rerender(<StreamingStatusCard {...props({ tokensEstimate: 1240 })} />);
    expect(screen.getByTestId('elmer-stream-tokens').textContent).toBe('~1,240 tok');
  });

  it('shows "responding" (not a radio verb) once isResponding is true', () => {
    render(<StreamingStatusCard {...props({ isResponding: true, answer: 'Hi' })} />);
    expect(screen.getByTestId('elmer-stream-verb').textContent).toContain('responding');
    expect(screen.getByTestId('elmer-stream-verb').textContent).not.toContain('chasing DX');
  });

  it('toggle button invokes onToggleExpand', () => {
    const onToggleExpand = vi.fn();
    render(<StreamingStatusCard {...props({ onToggleExpand })} />);
    fireEvent.click(screen.getByTestId('elmer-stream-card-toggle'));
    expect(onToggleExpand).toHaveBeenCalledOnce();
  });

  it('expanded: renders the bounded body with reasoning, answer, and cursor', () => {
    render(<StreamingStatusCard {...props({ expanded: true, isResponding: true, reasoning: 'weighing options', answer: 'The answer' })} />);
    const body = screen.getByTestId('elmer-stream-body');
    expect(body).toBeTruthy();
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('weighing options');
    expect(body.textContent).toContain('The answer');
    expect(screen.getByTestId('elmer-stream-cursor')).toBeTruthy();
  });

  it('h5azu-a: reasoning stays visible when the answer starts (no auto-collapse to a cursor)', () => {
    render(<StreamingStatusCard {...props({ expanded: true, isResponding: true, reasoning: 'a very long thinking trace the operator was reading', answer: 'X' })} />);
    expect(screen.getByTestId('elmer-stream-reasoning').textContent).toContain('very long thinking trace');
    expect(screen.getByTestId('elmer-stream-body').textContent).toContain('X');
  });
});
