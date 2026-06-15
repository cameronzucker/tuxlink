# Codex UI-declutter prompt (reusable)

A directed prompt for using **Codex as a UI-declutter analyst** on a specific,
already-built surface. Born from the 2026-06-15 APRS post-connect-pane declutter
(tuxlink-l0z5); reusable "going forward" for any cluttered surface.

## Why Codex for this

Claude leads design/taste; Codex is the better *systematic tidier*. Codex
reliably finds: duplicated/redundant elements, vertical-space waste, rows that
could merge, inconsistent spacing/typography tokens, and information-hierarchy
noise — the mechanical declutter wins. It is weaker at inventing a new aesthetic.
So the prompt **aims Codex at consolidation within the existing design tokens /
approved mock**, and forbids aesthetic invention. Taste decisions stay with the
human/Claude lead; Codex supplies a ranked, concrete edit list to react to.

## Division of labor (do NOT skip)

1. **Claude** sets the design intent + names the approved reference (mock/tokens).
2. **Codex** runs the prompt below → ranked declutter findings as `file:line` edits.
3. **Claude** exercises taste: accept / adapt / reject each finding, then implement.
   Codex's output is *input to a design decision*, never an auto-applied patch.

## The prompt (fill the `<…>` slots, pipe via the `codex exec` stdin form)

```
You are a UI-declutter ANALYST, not a redesigner. Read the diff context and the
files below in this read-only worktree. Your job: find concrete ways to reduce
CLUTTER and VERTICAL SPACE on <SURFACE NAME> while preserving every piece of
information and every control currently present. Do NOT invent a new aesthetic,
new colors, or new component types — stay strictly within the existing CSS design
tokens and the approved mock.

Symptom (operator-reported): <SYMPTOM, verbatim>
Approved visual reference: <MOCK PATH> — match its density/structure.
Files to read: <COMPONENT.tsx + COMPONENT.css + adjacent surfaces>
Hard constraints: <e.g. inline-only, no pop-ups; reading-pane width ~Npx;
  RF-honesty (no optimistic state); keep all data-testids; no behavior change>

Produce findings ONLY in this shape, ranked by space saved (most first):
  - [Sx] <one-line declutter> — file:line(s) — est. vertical px saved — risk(low/med)
    rationale: <why it's redundant / mergeable / token-inconsistent>
    concrete change: <the specific merge/removal/restyle, in terms of existing
      classes + tokens; reference the mock element it matches>
Then a 3-line "what NOT to touch" note: information or controls that look
redundant but are load-bearing (and why).
End with nothing else — no preamble, no patch.
```

## Invocation (CLAUDE.md Codex recipe — stdout-tee'd; quota-aware)

```bash
cat /tmp/codex-declutter.txt | npx --yes @openai/codex exec - 2>&1 \
  | tee dev/adversarial/<date>-<surface>-declutter-codex.md
# Verify it ran (not a quota stub / argparse stub):
wc -l dev/adversarial/<date>-<surface>-declutter-codex.md
```

`dev/adversarial/` is gitignored — the raw transcript stays local; summarize the
accepted findings + dispositions in the PR body. A "usage limit … try again at
HH:MM" short file is a **quota defer**, not a skip (see the codex-quota memory):
defer the round, do not substitute a Claude agent for the Codex-specific value.
