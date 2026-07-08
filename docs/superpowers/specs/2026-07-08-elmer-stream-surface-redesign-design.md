# Elmer streaming surface — redesign

**Date:** 2026-07-08
**Agent:** basin-juniper-fjord
**Issues:** tuxlink-h5azu (lead, P1 bug) · tuxlink-06v9s (P1 bug) · tuxlink-d5zns (feature)
**Branch:** `bd-tuxlink-h5azu/elmer-stream-redesign` (off `origin/main`)
**Mock:** `dev/scratch/elmer-stream-redesign/mock-render.png` (local scratch, gitignored)

## Motivation

Three issues, filed separately, converge on one surface — the in-flight streaming
region of the Elmer pane. Fixing them piecemeal would re-touch the same code three
times, so they are one redesign.

The bugs were observed on `origin/main` @ v0.85.0.

### Root causes (from investigation)

The event plumbing in `useElmer.ts` is sound: setState calls are batched, listeners
are de-duped, buffers are cleared on both `EV_TURN` and `EV_OUTCOME`. **The defects
are in the render/layout design in `ElmerPane.tsx`, not the hook.**

1. **tuxlink-h5azu — collapse-to-cursor (symptom a).** `StreamingBubble`
   (`ElmerPane.tsx`, `reasoningOpen = !answerStarted`) hides the entire reasoning
   trace the instant the first answer token lands. With a short first token and the
   `ThinkingIndicator` already gone (`isStreaming` true), the operator is left staring
   at a role label + a blinking cursor. The thinking text visibly collapses to a
   cursor line.

2. **tuxlink-h5azu — dump/clear/reprint (symptom b).** The live bubble renders **plain
   text**; on `EV_TURN` it is swapped for a **markdown** re-render of the same text
   (`AssistantTurnBody`). Plain → markdown reflow reads as a reprint.

3. **tuxlink-h5azu — thinking re-flash.** Between `EV_TURN` (buffers cleared) and
   `EV_OUTCOME` (phase → `done`), `isRunning && !isStreaming` is briefly true, so
   "Elmer is thinking…" flashes *below* the finished answer.

4. **tuxlink-06v9s — scroll-lock.** The auto-scroll effect calls
   `listEndRef.current.scrollIntoView({behavior:'smooth'})` on every change to
   `[items, streamingAnswer, streamingReasoning]`, unconditionally. `streamingAnswer`
   changes on every `EV_DELTA` token, so the viewport is yanked to the bottom many
   times a second and the operator cannot scroll up to supervise tool-call chips.

5. **tuxlink-06v9s — unbounded stream.** `.elmer-streaming-answer` has no max-height;
   the in-flight bubble grows without bound inside the transcript.

## Design

### Core idea

Replace **both** the transient `StreamingBubble` **and** the standalone
`ThinkingIndicator` with a single **`StreamingStatusCard`** that owns the entire
in-flight lifecycle (`thinking → responding → done`). Removing the handoff between two
components is what dissolves the h5azu class of glitches: there is only one component
to be in a consistent state.

### States

**Collapsed (default — tuxlink-d5zns).** While a turn is in flight the operator sees
one compact row:

```
▸  ● Elmer is responding…                    ~840 tok · 0:22
```

- Pulsing dot + verb. Verb is `thinking…` while only reasoning has streamed,
  `responding…` once the first answer token arrives.
- Right-aligned live metrics: estimated token count + elapsed.
- Because **no plain-text stream is on screen**, the final markdown answer simply
  appears at finalize — there is no plain→markdown reprint to witness (fixes symptom b)
  and no accidental bare-cursor collapse (fixes symptom a).
- Tool-call chips (`EV_CHIP`) continue to append to the transcript **above** the card,
  exactly as today.

**Expanded (click chevron — tuxlink-06v9s).** The card grows a body: a bounded
`max-height` box (~210px ≈ 10 lines) with its own `overflow-y: auto`.

- Reasoning is shown dimmed/italic above a dashed rule; the answer streams below with a
  blinking cursor.
- The box has its **own internal auto-follow**: it pins to the bottom as tokens arrive,
  but releases the moment the operator scrolls up inside it. A `↓ Jump to live` pill
  returns them to the tail.
- **Expand state is sticky for the session** — an operator who wants to watch the
  stream stays expanded across turns. Default is collapsed.

**Finalized (tuxlink-h5azu).** On `EV_TURN` the card unmounts and the committed
markdown answer renders **once** in the transcript. No thinking re-flash, because a
single component owns the `running → done` transition (see "Phase coupling" below).
Exact prompt tokens arrive via `EV_CONTEXT` and land in the existing `ContextMeter`.

### Scroll rule (the heart of tuxlink-06v9s)

Auto-follow fires **only when the operator is already pinned to the bottom**, applied
independently in two places:

- **Transcript (`.elmer-messages`):** track scroll position; if the operator has
  scrolled up, suppress the auto-scroll effect until they return to the bottom. This is
  what lets them read tool-call chips mid-stream.
- **Expanded stream box:** same pin-to-bottom logic, scoped to the box.

A small `↓ Jump to live` affordance appears in each region when auto-follow is released.

### Live token counter (tuxlink-d5zns)

The backend only emits exact counts **after** a turn (`EV_CONTEXT`, once per completed
turn). During streaming there is no server-side token count. The live number is
therefore a **frontend estimate**: sum characters across streamed `EV_DELTA` chunks
(reasoning + answer), tokens ≈ `Math.round(chars / 4)`. Rendered with a leading `~`
(`~840 tok`) to signal it is an estimate. The exact count still lands in the
`ContextMeter` at finalize, so the honest/precise number is never lost.

For **non-streaming providers** (no `EV_DELTA`), the card shows verb + elapsed only, no
token count — it degrades to the old `ThinkingIndicator` behavior with no regression.

## Components & boundaries

New/changed units in `src/elmer/`:

- **`StreamingStatusCard.tsx` (new).** Pure presentational component.
  - Props: `{ phase, answer, reasoning, tokensEstimate, elapsedSecs, expanded,
    onToggleExpand }`.
  - Owns: collapsed row, expanded bounded box, internal scroll/auto-follow +
    jump-to-live pill for the box.
  - Does NOT own elapsed-time ticking or token estimation (passed in) — keeps it
    trivially testable.
- **`useStreamAutoFollow.ts` (new, small hook).** Encapsulates the pin-to-bottom
  detection for a scroll container: returns `{ atBottom, onScroll, jumpToLive }` given a
  ref. Reused by both the transcript and the stream box. Single, tested unit.
- **`ElmerPane.tsx` (changed).**
  - Delete `StreamingBubble` and the `{isRunning && !isStreaming && <ThinkingIndicator/>}`
    branch; render `<StreamingStatusCard/>` for the whole in-flight window
    (`isRunning || isStreaming`).
  - Replace the unconditional auto-scroll effect with `useStreamAutoFollow` gating on
    the transcript container.
  - Compute `tokensEstimate` and `elapsedSecs` (lift the elapsed ticker out of the old
    `ThinkingIndicator`; keep the ham-radio verb rotation for the collapsed verb).
  - Persist `expanded` in component state (session-sticky).
- **`ElmerPane.css` (changed).** Styles for the card (collapsed row, bounded body with
  `max-height`), the jump-to-live pill, and the blinking cursor moved onto the card.
  Remove `.elmer-streaming-bubble` / `.elmer-streaming-answer` rules that no longer apply.

The `ThinkingIndicator`'s ham-radio verb list (`RADIO_VERBS`) is retained and reused for
the card's collapsed verb.

## Data flow

Unchanged in the hook. `useElmer` still exposes `streamingAnswer`,
`streamingReasoning`, `phase`, `items`, `context`. `ElmerPane` derives:

- `isInFlight = phase === 'running' || streamingAnswer || streamingReasoning`
- `tokensEstimate = round((streamingAnswer.length + streamingReasoning.length) / 4)`
- verb from `streamingAnswer.length > 0 ? 'responding' : 'thinking'`

### Phase coupling — killing the thinking re-flash

Today the flash happens because the card's visibility keys off two different signals
(`isStreaming` for the bubble, `isRunning && !isStreaming` for the indicator) that briefly
disagree in the `EV_TURN → EV_OUTCOME` gap. The card keys off **one** predicate,
`isInFlight`, and renders its own internal `thinking`/`responding` sub-state from the
buffers. When `EV_TURN` fires, the committed item is appended in the **same** React
commit that clears the buffers; the card stays mounted (phase still `running`) showing a
brief `responding…` with the final token count until `EV_OUTCOME`, then unmounts. No
second component appears, so there is no flash.

## Testing (vitest / jsdom — runs on this Pi)

The existing suite (`ElmerPane.test.tsx`, `useElmer.test.tsx`) runs under vitest+jsdom
locally. Regression tests, one per root cause:

1. **h5azu-a (no collapse-to-cursor):** stream reasoning, then a 1-char answer delta;
   assert the reasoning text is still reachable (not hidden) and no bare-cursor-only
   render — collapsed card shows the verb/counter, expanded shows reasoning + answer.
2. **h5azu-b (single markdown commit):** stream deltas, fire `EV_TURN`; assert exactly
   one assistant markdown node renders and the transient card is gone (no duplicate).
3. **h5azu-reflash:** after `EV_TURN` and before `EV_OUTCOME`, assert no
   `ThinkingIndicator`/second in-flight element is present.
4. **06v9s-scroll:** simulate the operator scrolled up (mock `scrollTop`/`scrollHeight`);
   push a delta; assert `scrollIntoView`/programmatic scroll is **not** called (follow
   released). Then simulate pinned-to-bottom; assert it **is** called.
5. **06v9s-bounded:** assert the expanded stream body carries the bounded/scroll class.
6. **d5zns-counter:** stream N chars; assert the collapsed row shows `~round(N/4) tok`.
   Non-streaming path (EV_TURN with no deltas): assert no token count is shown.
7. **d5zns-default-collapsed / sticky:** assert collapsed by default; after toggling
   expand, a second turn stays expanded.

Live WebKitGTK visual verification (the actual pixels) is out of scope for this Pi — the
full Tauri app builds in CI, and the operator verifies the resulting build against the
Spark endpoint (`qwen3.5-122b` reasoning model exercises the reasoning path;
`qwen3-coder-next` exercises the answer-only path).

## Out of scope (explicit)

- **tuxlink-wgh19 / bx94e / 5io0f** — code-block copy icon, condensed copy control,
  save-to-md / inline-email. The finalized panel draws the icon-row *slot* these fill,
  but they are separate work (Phase 2).
- **tuxlink-8asne** — shell/internet system-prompt toggles. Security design gate; no
  build until a dedicated brainstorm + adversarial pass.
- No changes to the Rust backend, event contract (`elmerEvents.ts` / `events.rs`), or
  `useElmer` state shape. This is a pure frontend render/layout redesign.

## Decisions locked (operator-approved 2026-07-08)

1. Collapsed-by-default, expand sticky per session.
2. Live counter = `chars/4` estimate, labeled `~N tok`; exact count via ContextMeter.
3. Bounded stream box ~210px (~10 lines) with internal scroll + jump-to-live pill.
