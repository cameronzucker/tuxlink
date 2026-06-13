---
name: wire-walk
description: Use when about to claim a feature is end-to-end shipped, done, complete, working, or reachable — especially after a backend/command/API lands, before marking a PR ready, and at the end of build-robust-features or subagent-driven-development. Use when wiring spans more than one component, phase, or agent.
---

# Wire-Walk

## Overview

A feature is not shipped when its code compiles, its tests pass, and CI is green. It is shipped when a real consumer can reach it. The recurring defect is a **backend that exists, is correct, is CI-green, and is connected to nothing a user can touch** — the seam between components is nobody's component, so nobody wires it.

Wire-walk is the hard gate that catches this. The principle: **building correctly across a seam is unreliable; auditing reachability is reliable.** You convert an unreliable generative claim ("it's all wired up") into a reliable verification ("here is each hop, proven connected, or here is the exact break").

This is a **discipline gate**, not advice. If **any operator-supplied flow** is broken, the feature is not shipped — it is partially-wired, which is a defect. You do not get to re-rank a flow as "secondary" or "edge case" to pass.

## When to use

Run wire-walk **before** any of these: claiming a feature is "done / shipped / end-to-end / complete / working"; marking a PR ready; closing a feature issue; the final step of build-robust-features; the final review of subagent-driven-development. Especially when the wiring crosses a component, a phase, an agent, or a release.

## The Iron Law

**The operator defines the flows. You only get to trace them.**

You MUST NOT generate the flow list yourself and audit against it. An agent's self-generated flows are the flows it already wired — they pass, the blind spot survives, the gate becomes theater. The flow the agent forgot to list is the one most likely to be broken.

## Procedure

### 1. Elicit the flows — GREENFIELD, no draft

Ask the operator, cold, for the key user flows. Do **NOT** propose a draft list first, do **NOT** suggest examples, do **NOT** frame it with your component model. Anchoring the operator with your guess launders your own blind spots into the "ground truth." Like open-ended adversarial review, the value is in what they surface that you did not expect.

Ask plainly:

> "Before I can call this done, list the key things a user actually does with this feature — short, defined task statements (e.g. 'connect a UV-Pro', 'send a message', 'change a channel'). I'll trace each one to code and prove it's wired."

**Pin the starting state for each flow.** A flow that passes against a populated dev environment can dead-end on a real first run — the identity transmit bug was exactly this: "send a message" worked with an identity already in the store and dead-ended on a fresh install where setup never created one. So each flow is traced from a real consumer's actual starting state, and you MUST include the cold-start states explicitly: **fresh install, first launch, empty store, and post-upgrade.** The **setup / onboarding / first-run flow is itself a required flow to trace** — the "setup writes the state that use reads" seam is the highest-risk seam in any feature, and it is invisible if you trace use-flows against pre-seeded state.

Record the elicited flows (in the plan, PR, or issue) so "did you run wire-walk, against which flows" is checkable, not merely asserted. If the flows were captured at feature start as the definition-of-done, use those verbatim instead of re-asking.

**Autonomous / headless / subagent runs with no operator and no recorded flows: STOP and escalate.** Do not self-generate flows and pass yourself. A wire-walk with agent-authored flows is not a wire-walk.

### 2. Trace each flow as a touchpoint chain

For each operator flow, trace the **full chain**, every hop with `file:line`:

```
entry point  →  handler  →  IPC/command/call  →  backend fn  →  effect  →  return path
(UI control /
 CLI flag /
 API route /
 public fn)
```

Trace the flow **verbatim as the operator stated it**, and **from the flow's starting state** (§1). You may not narrow, merge, reinterpret, or substitute a flow to make it pass (see Red Flags).

**Trace gates backward to their producer.** When a hop is a precondition/gate (`active_identity()`, `is_connected`, `has_secret`, a required config field), do NOT stop at "something, somewhere, sets it." Trace the flow that *produces* that state **from this flow's starting state**, and confirm THAT flow is wired. The identity bug passed a naive forward trace because `set_active_identity` *had* a caller (the unlock UI) — but on a fresh install nothing produced an identity to unlock, so the gate was never satisfiable in the state the flow actually starts from. A precondition that is satisfiable "in principle" but not produced on the flow's real starting path is a ❌, not a ✅.

### 3. Verify each hop connects — hunt the break-patterns

These recur across every stack. Grep + read; no build needed.

| Break-pattern | Signature | How to find it |
|---|---|---|
| **Orphan producer** | Backend fn/command with zero **consumer** callers | grep its name, then confirm each hit is a real consumer invocation (the `invoke()` / call on the consumer's path) — NOT a registration, test, mock, comment, or re-export. A command "registered in lib.rs" with only registration + test hits is an orphan. Registration ≠ a caller. |
| **Dead control** | UI control rendered but no handler / no-op / handler never passed | read the control's props + its mount site |
| **Empty seam** | A prop/slot *designed* for the feature, passed `undefined`/nothing | grep the slot name at its mount site |
| **Missing variant** | A union/enum/type lacking the new option → can't select/declare it | read the type def vs. what the UI offers |
| **Unsatisfiable gate** | A precondition (`is_connected`, `active_identity`, `has_secret`) not *produced* on the flow's starting path | not "is anything calling the setter?" but "from this flow's starting state (fresh install etc.), does a wired setup flow actually produce it?" Satisfiable-in-principle ≠ produced-in-practice. |
| **Config-only path** | The only way to exercise it is hand-editing config/files | is there a consumer surface at all? |

### 4. Verdict per flow

State, per flow, with the exact break point:

- ✅ **wired** — every hop connects, traced `file:line` to `file:line`.
- ❌ **broken at `file:line`** — name the missing hop and which break-pattern.
- ⚠️ **wired but unverifiable here** — reserved for ONE case: every hop is traced and connected, and the only thing you cannot check is a physical/external **terminal effect** (on-air RF transmission, a live third-party service). It is NOT a bucket for "I couldn't trace this hop" or "I assume something sets it." **An untraceable, missing, or assumed hop is ❌, never ⚠️.** If you reach for ⚠️ because you couldn't follow the chain, that is a ❌.

### 5. Gate

If **any** operator-supplied flow is ❌, the feature is **NOT shipped**. Do not claim done, do not mark the PR ready, do not close the issue. The remaining wiring is the real work, not a follow-up. Report the broken flows with their break points; that list is the spec for finishing the feature.

No outs:
- You may **not re-rank** a broken flow as "secondary / edge / not the primary use" to pass. Every operator flow counts equally; the operator supplied it because it matters.
- You may **not move a broken operator flow to "fast-follow / follow-up / polish"** so the current scope reads as done. Narrowing scope is the operator's call: a flow leaves the gate only when the **operator removes it** from the flow set, never when the agent reclassifies it.
- A ✅ on the other flows does not offset a ❌. One broken flow = not shipped.

## Red Flags — STOP

You are about to defeat the gate if you catch yourself:

- Writing the flow list yourself instead of asking the operator
- Proposing a "draft for you to confirm" (that is anchoring, not greenfield)
- Narrowing a flow ("the operator said 'connect a UV-Pro' but I'll trace the KISS path since that works")
- Counting "the backend command exists and is registered" as a wired flow (registration ≠ a caller)
- Counting "CI is green" or "tests pass" as reachability (they prove the hop, not the chain)
- Treating a ❌ as a follow-up / fast-follow / polish item so you can still say "done"
- Re-ranking a broken flow as "secondary / edge case" so the gate doesn't fire
- Tracing against a populated dev environment / seeded store instead of the flow's real starting state (fresh install, first launch, post-upgrade)
- Marking a hop ⚠️ "unverifiable" when you actually just couldn't trace it
- **"I'm confident it's wired, I don't need to ask for the flows / run this"** — that confidence is the exact failure mode. The identity epic was declared end-to-end with total confidence, from command-reachability, and shipped broken. Confidence is not evidence; run the gate.
- Running it autonomously with self-authored flows because no operator was available

All of these mean: you are grading your own homework. Stop. Get the operator's flows, or escalate.

## Common mistakes

- **Tracing components, not flows.** "The IdentitySwitcher renders" is not a flow. "Configure my callsign, then transmit" is. Flows start from a user goal and cross whatever components they must.
- **Stopping at the first hop.** The break is usually mid-chain (the orphan backend, the empty seam), not at the control you can see.
- **Auditing only the happy path you built.** The motivating flow — the whole point of the feature — is the one to trace hardest, because it is the one most likely to have an unbuilt seam.
- **Verbal reassurance instead of `file:line`.** "It's all wired" is the claim under audit. Cite lines or it didn't happen.

## Why a hard gate (not advice)

This anti-pattern is too strong and consistent to be advisory — it has recurred across multiple features and agents despite a written "ship end-to-end" rule. Prose did not stop it. A gate that produces a per-flow ❌ with a break point, and forbids the "done" claim while one stands, is the enforcement the prose lacked. And because wire-walk is grep + read with no external service, it always runs — it cannot be deferred or skipped for quota/capacity the way an external reviewer can.
