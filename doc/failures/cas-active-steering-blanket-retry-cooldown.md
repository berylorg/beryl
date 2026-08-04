# CAS Active-Steering Blanket Retry Cooldown

## Invalid Approach

Treat every durable `Retryable` steering candidate as automatic work and pace repeated attempts
with one process-global capped cooldown.

## Evidence

`Retryable` records only proven non-dispatch and continued legality. The app's current retry causes
combine cancellation, home and target authority drift, lifecycle readiness, deterministic source
validation and serialization, and backend `ProvenNotDispatched`. The backend outcome proves only
that no request byte reached the transport; it has no transient classification.

The apparently transient transport-write, closed-transport, and WebSocket failures invalidate the
exact connection. They therefore converge through target loss instead of remaining scheduler-visible
retry work. The target-current causes that remain are cancellation, deterministic preconditions,
source disagreement, or opaque availability failures with no evidence that elapsed time repairs
them.

## Why It Fails

A cooldown prevents a tight loop but still repeats deterministic work forever, spends bounded
worker capacity without new authority, and can delay later accepted input. Worker and
connection-attempt release are capacity facts, not evidence that the failed condition changed.

## Course Correction

Durable `Retryable` remains dispatch-safety authority only. Admitted work is immediately
schedulable. Cancellation parks until an explicit fresh cancellation-lifecycle or recovery wake;
ordinary readiness and capacity wakes do not clear that gate. Every other current target-current
authority, lifecycle, source, validation, or serialization failure first restores safe retryable
storage, then fails the exact active projection closed through atomic target loss so the
undispatched input becomes ordered next-turn work. Failure of that convergence stops the scheduler
service. Connection-invalidating or possibly dispatched work uses its existing loss disposition.

The scheduler retains only compact pass cursors and one process-global retry-eligibility state. A
future explicitly typed transient and proven-nondispatch disposition may arm one capped global
deadline, but the current production taxonomy arms no timer.

## Affected Authority

This correction governs Phase 57 in `doc/plan.md`,
`doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, and the affected package design documents.
