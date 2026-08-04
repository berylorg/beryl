# CAS Phase 54 Target Loss Overtakes Steering Disposition

## Invalidated Approach

Authorize an exact active steering target immediately before `turn/steer`, then rely on the
existing delayed-lifecycle publication permit and a later durable accepted-route mutation to
settle the request.

## Evidence

The router's delayed-steering permit protects only the bounded ingress verification and in-memory
checked-result publication. It clears `publication_in_flight` before the delivery owner consumes
the terminal lifecycle and commits the Phase 53 disposition.

`acquire_target_loss` waits only for that transient router publication. It can therefore acquire
loss authority after an exact response and checked Completed lifecycle but before
`CompleteAcceptedInputDelivery` becomes durable.

## Why It Failed

A pre-dispatch authorization is a point check, not ownership of the whole non-idempotent delivery
attempt. Generic target loss can win the uncovered interval, atomically classify the still
Delivering input as delivery-unknown, and remove the target even though the delivery worker holds
the exact success proof.

The same race can erase the named-input distinction for an exact rejection without a closed
machine verdict. Publishing a standalone rejection before generic abandonment would replace the
race with a forbidden two-commit crash cut.

## Required Course Correction

One non-cloneable exact steering-attempt permit must be acquired from the ready target before the
Ready-to-Delivering claim and span dispatch through durable disposition. Delayed lifecycle ingress
remains independently able to publish under that attempt. Target close may request loss, but loss
publication cannot overtake the attempt owner.

Success, proven non-dispatch, and structured rejection release the attempt only after their exact
durable disposition. Generic completion-unknown and unconfirmed exact rejection atomically
transfer the attempt into generic or named target-loss authority. Repeated convergence for the
same target must observe a bounded idempotent loss receipt rather than report a false unavailable
target.

## Affected Work

`doc/plan.md` Phase 54, the app connection router, the checked steering-result owner, the provider
loss publisher, and Phase 54 lifecycle-versus-loss race coverage own this correction.
