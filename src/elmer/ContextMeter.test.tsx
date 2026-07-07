/**
 * ContextMeter tests (T7, tuxlink-65qhn).
 *
 * Covers:
 *   - hidden-before-first-event pattern (consumer renders null guard correctly)
 *   - shown + persistent after context is provided
 *   - formatK: 12000 → "12k", 500 → "500", 32768 → "32k", 0 → "0", 999 → "999"
 *   - label text (left + right)
 *   - pct + color thresholds: <75% → accent, ≥75% → amber, ≥90% → red
 *   - fill width matches computed pct
 *   - numCtx=0 guard (no division-by-zero)
 */
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ContextMeter, formatK } from './ContextMeter';

// ---------------------------------------------------------------------------
// formatK unit tests
// ---------------------------------------------------------------------------

describe('formatK', () => {
  it('returns raw number string for values under 1000', () => {
    expect(formatK(0)).toBe('0');
    expect(formatK(500)).toBe('500');
    expect(formatK(999)).toBe('999');
  });

  it('floors to integer k for values 1000 and above', () => {
    expect(formatK(1000)).toBe('1k');
    expect(formatK(12000)).toBe('12k');
    expect(formatK(32768)).toBe('32k');
    // Floor, not round: 1999 → "1k" not "2k"
    expect(formatK(1999)).toBe('1k');
  });
});

// ---------------------------------------------------------------------------
// ContextMeter render tests
// ---------------------------------------------------------------------------

describe('ContextMeter', () => {
  // -- Visibility / label -------------------------------------------------------

  it('renders the meter element when mounted', () => {
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    expect(screen.getByTestId('elmer-context-meter')).toBeTruthy();
  });

  it('shows left label "Context 12k / 32k" for 12000 / 32000', () => {
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    expect(screen.getByTestId('elmer-context-meter-left').textContent).toBe(
      'Context 12k / 32k',
    );
  });

  it('shows right label with pct and the fixed suffix', () => {
    // 12000 / 32000 = 37.5% → round = 38%
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    const right = screen.getByTestId('elmer-context-meter-right').textContent ?? '';
    expect(right).toContain('38%');
    expect(right).toContain('room for tools + history');
  });

  it('shows raw token count when promptTokens < 1000', () => {
    render(<ContextMeter promptTokens={500} numCtx={32000} />);
    const left = screen.getByTestId('elmer-context-meter-left').textContent ?? '';
    expect(left).toContain('500');
  });

  // -- pct calculation ---------------------------------------------------------

  it('rounds pct correctly: 0 → 0%', () => {
    render(<ContextMeter promptTokens={0} numCtx={32000} />);
    const right = screen.getByTestId('elmer-context-meter-right').textContent ?? '';
    expect(right).toContain('0%');
  });

  it('caps pct at 100 for fill width (does not overflow track)', () => {
    render(<ContextMeter promptTokens={40000} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    // Width must be 100% or less even when promptTokens > numCtx.
    const width = fill.style.width;
    expect(width).toBe('100%');
  });

  // -- numCtx = 0 guard --------------------------------------------------------

  it('does not divide by zero when numCtx is 0 (shows 0%)', () => {
    // Should not throw.
    render(<ContextMeter promptTokens={1000} numCtx={0} />);
    const right = screen.getByTestId('elmer-context-meter-right').textContent ?? '';
    expect(right).toContain('0%');
  });

  // -- Color thresholds ---------------------------------------------------------

  it('uses accent color when pct < 75', () => {
    // 74% → 23680 / 32000
    render(<ContextMeter promptTokens={23680} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    expect(fill.style.background).toContain('var(--accent)');
    // Must NOT contain amber or danger indicators.
    expect(fill.style.background).not.toContain('accent-amber');
    expect(fill.style.background).not.toContain('accent-danger');
  });

  it('uses amber color at exactly 75%', () => {
    // 75% exactly: 24000 / 32000
    render(<ContextMeter promptTokens={24000} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    expect(fill.style.background).toContain('accent-amber');
  });

  it('uses amber color between 75% and 89%', () => {
    // ~80%: 25600 / 32000
    render(<ContextMeter promptTokens={25600} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    expect(fill.style.background).toContain('accent-amber');
  });

  it('uses danger/red color at exactly 90%', () => {
    // 90%: 28800 / 32000
    render(<ContextMeter promptTokens={28800} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    expect(fill.style.background).toContain('accent-danger');
  });

  it('uses danger/red color above 90%', () => {
    // 95%: 30400 / 32000
    render(<ContextMeter promptTokens={30400} numCtx={32000} />);
    const fill = screen.getByTestId('elmer-context-meter-fill') as HTMLElement;
    expect(fill.style.background).toContain('accent-danger');
  });

  // -- Progressbar a11y --------------------------------------------------------

  it('exposes aria-valuenow on the track for screen readers', () => {
    // 38% for 12000 / 32000 (same rounding as label test)
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    const track = screen.getByRole('progressbar');
    expect(track.getAttribute('aria-valuenow')).toBe('38');
    expect(track.getAttribute('aria-valuemin')).toBe('0');
    expect(track.getAttribute('aria-valuemax')).toBe('100');
  });
});

// ---------------------------------------------------------------------------
// Hidden-until-first-event pattern test (consumer integration)
// ---------------------------------------------------------------------------

/**
 * A thin wrapper that mirrors how ElmerPane consumes context from useElmer():
 * the meter is conditionally rendered behind a null guard.
 */
function MeterHost({ context }: { context: { promptTokens: number; numCtx: number } | null }) {
  return (
    <div>
      {context !== null && (
        <ContextMeter promptTokens={context.promptTokens} numCtx={context.numCtx} />
      )}
      <div data-testid="composer-input">composer</div>
    </div>
  );
}

describe('ContextMeter hidden-until-first-event', () => {
  it('is hidden when context is null (no EV_CONTEXT yet)', () => {
    render(<MeterHost context={null} />);
    // The meter must not be present in the DOM.
    expect(screen.queryByTestId('elmer-context-meter')).toBeNull();
    // The composer input is always present (meter absence doesn't break layout).
    expect(screen.getByTestId('composer-input')).toBeTruthy();
  });

  it('is shown after first context event (context becomes non-null)', () => {
    render(<MeterHost context={{ promptTokens: 12000, numCtx: 32000 }} />);
    expect(screen.getByTestId('elmer-context-meter')).toBeTruthy();
  });

  it('remains visible once shown (re-render with updated counts keeps meter)', () => {
    const { rerender } = render(
      <MeterHost context={{ promptTokens: 12000, numCtx: 32000 }} />,
    );
    expect(screen.getByTestId('elmer-context-meter')).toBeTruthy();
    // Update counts on next render — meter must still be visible.
    rerender(<MeterHost context={{ promptTokens: 20000, numCtx: 32000 }} />);
    expect(screen.getByTestId('elmer-context-meter')).toBeTruthy();
    // Updated label reflects new counts.
    const left = screen.getByTestId('elmer-context-meter-left').textContent ?? '';
    expect(left).toContain('20k');
  });
});

// ---------------------------------------------------------------------------
// Counter-mode (numCtx === null): window unknown
// ---------------------------------------------------------------------------

describe('ContextMeter counter-mode (numCtx null)', () => {
  it('counter-mode: renders bare token count when numCtx is null', () => {
    render(<ContextMeter promptTokens={12000} numCtx={null} />);
    expect(screen.getByTestId('elmer-context-meter-left').textContent).toBe('Context 12k');
    // No "/ 32k", no percentage suffix, no fill track.
    expect(screen.queryByTestId('elmer-context-meter-right')).toBeNull();
    expect(screen.queryByTestId('elmer-context-meter-track')).toBeNull();
  });

  it('counter-mode: aria-label states the window is unknown', () => {
    render(<ContextMeter promptTokens={12000} numCtx={null} />);
    const el = screen.getByTestId('elmer-context-meter');
    expect(el.getAttribute('aria-label')).toBe('Context usage: 12k tokens (window unknown)');
  });

  it('windowed mode still renders the bar when numCtx is a number', () => {
    render(<ContextMeter promptTokens={12000} numCtx={32000} />);
    expect(screen.getByTestId('elmer-context-meter-track')).toBeTruthy();
    expect(screen.getByTestId('elmer-context-meter-left').textContent).toBe('Context 12k / 32k');
  });
});
