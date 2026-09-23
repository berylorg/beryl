# Discussion Steering After Handoff Admission

## Invalidated Assumption

Checking for zero future-turn input at resolution admission is insufficient to guarantee that
the discussion remains free of queued input until successful archive. Phase 516 cannot accept
complete mutation enforcement without a disposition for steering already accepted at admission.

## Evidence

The [handoff system](../systems/branch-discussion-handoff/design.md#atomic-admission-against-queued-input)
allows steering targeting the resolving turn without deferring resolution. Its coordination
contract preserves delivery and terminal convergence while forbidding later discussion successors.

Two ordinary storage transitions can subsequently produce queued child work:

- `mutation/accepted/transition.rs::next_records` converts rejected steering into
  `NextTurn(SteeringRejected)`, preserving its accepted content and increasing the next-turn count.
- `mutation/live/terminal/gate.rs::terminal_gate_effect` reclassifies remaining ready or retryable
  steering into `NextTurn(TerminalHistory)` when the resolving turn terminates. Delivering work
  must settle first, but ready work need not have reached CAS.

Thus a user message accepted just before the resolution tool call can miss that resolving turn.
The queued message remains durable, but pending/archive guards prevent its promotion. The current
handoff contract still permits parent execution and requires archive on parent success. Independent
inventory review confirmed that no existing authority specifies this post-admission child queue.
The generic invariant-failure rule does not resolve this expressly permitted race.

## Accepted Correction

Before creating any parent handoff input, wait for already-admitted child steering to settle and
for resolving-turn terminal convergence. If it produces future child input, terminally fail that
handoff attempt and release its discussion gate atomically, leaving the discussion unarchived and
the queued input eligible for ordinary processing. Require a later model call for a fresh attempt.
If no child input remains, proceed with the same admitted handoff. Operator approved this correction
on 2026-09-24. Feature and system contracts now require it; admission continues to permit existing
steering. The coordinator must enforce the exact child-queue proof before leaving
`waiting_resolving_turn`, including recovery, and never classify this ordinary race as corruption.

## Implementation State

The combined mutation fix is accepted at `7718ecf4`. Local mutation gates passed nine handoff cases
and seventeen focused regressions, app compilation and independent review. Steering rejection and
terminal reclassification remain available and preserve queued input. Typed child settlement proof,
State disposition and coordinator composition remain required before parent handoff can run.
