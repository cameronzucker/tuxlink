/**
 * ContextMeter — slim context-usage progress bar rendered above the Elmer
 * composer input (T7, tuxlink-65qhn).
 *
 * Hidden until the first EV_CONTEXT event arrives (context prop is null).
 * Once shown, persists for the rest of the session — context never goes
 * null after the first event.
 *
 * Layout matches the approved mock (elmer-native-ollama-mock-v2.png):
 *   Left  — "Context 12k / 32k"
 *   Right — "38% · room for tools + history"
 *   Track — slim fill bar; color shifts at 75% (amber) and 90% (red).
 *
 * Color tokens used (same as ElmerPane.css):
 *   default: var(--accent)
 *   ≥75%:    var(--accent-amber, var(--accent))
 *   ≥90%:    var(--accent-danger, var(--accent))
 */

/**
 * Format a token count as a "k" string.
 * - ≥ 1000: floor to integer k (12000 → "12k", 32768 → "32k").
 * - < 1000: raw number as string ("500" → "500").
 */
export function formatK(tokens: number): string {
  if (tokens >= 1000) {
    return `${Math.floor(tokens / 1000)}k`;
  }
  return String(tokens);
}

interface ContextMeterProps {
  promptTokens: number;
  numCtx: number;
}

export function ContextMeter({ promptTokens, numCtx }: ContextMeterProps) {
  // Guard: if numCtx is zero (unexpected), avoid division-by-zero.
  const pct = numCtx > 0 ? Math.round((promptTokens / numCtx) * 100) : 0;
  const fillPct = Math.min(pct, 100);

  // Color thresholds: ≥90% → danger/red, ≥75% → amber warning, else accent.
  // `--accent-amber` / `--accent-danger` are NOT defined in the theme palette
  // (only --accent / --accent-soft / --accent-edge / --error are). The rest of
  // the codebase (WeatherGlyph.css, MessageView.css) uses these same var names
  // with HARDCODED hex fallbacks so the color still resolves. We do the same —
  // falling back to --accent would leave the bar one color at every level and
  // silently kill the truncation warning (the whole point of the meter).
  let fillColor: string;
  if (pct >= 90) {
    fillColor = 'var(--accent-danger, #f87171)';
  } else if (pct >= 75) {
    fillColor = 'var(--accent-amber, #f0c674)';
  } else {
    fillColor = 'var(--accent)';
  }

  return (
    <div
      className="elmer-context-meter"
      data-testid="elmer-context-meter"
      aria-label={`Context usage: ${pct}% of ${formatK(numCtx)} tokens`}
    >
      <div className="elmer-context-meter-labels">
        <span
          className="elmer-context-meter-left"
          data-testid="elmer-context-meter-left"
        >
          Context {formatK(promptTokens)} / {formatK(numCtx)}
        </span>
        <span
          className="elmer-context-meter-right"
          data-testid="elmer-context-meter-right"
        >
          {pct}% · room for tools + history
        </span>
      </div>
      <div
        className="elmer-context-meter-track"
        data-testid="elmer-context-meter-track"
        role="progressbar"
        aria-valuenow={pct}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className="elmer-context-meter-fill"
          data-testid="elmer-context-meter-fill"
          style={{ width: `${fillPct}%`, background: fillColor }}
        />
      </div>
    </div>
  );
}
