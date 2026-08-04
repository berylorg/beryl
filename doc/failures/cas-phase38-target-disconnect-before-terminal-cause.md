# CAS Phase 38 Target Disconnect Before Terminal Cause

## Invalidated Approach

Treat an intermittent `WorkerStopped` outcome in the definitive source-publication fault case as
test-fixture timing or imprecise fault targeting.

## Evidence

The complete Phase 38 residency binary reproduced `TargetClosed(WorkerStopped)` while the focused
failure run could also produce the expected `TargetClosed(SourcePublicationFailed)`. The scoped
home-store controller was FIFO and the raw peer remained connected, ruling out both suspected
causes.

`close_target` recorded `publication_closing`, dropped the target sender, and only then wrote the
shared terminal signal. A blocked `TargetRegistration::poll` could therefore wake on channel
disconnection while the signal was still `Open` and select its unexpected-worker-stop fallback.

## Why It Failed

Channel disconnection is observable immediately. Publishing a typed close cause after dropping the
last sender creates a real production race in which the consumer can permanently preserve a weaker
fallback taxonomy.

## Required Course Correction

Every planned target close must publish its terminal cause before disconnecting the receiver. The
retained source-loss branch now follows the same cause-before-disconnect order as router retirement,
terminal success, and target removal. Receiver-loss convergence remains unchanged because its open
disconnect has different semantics.

The Phase 38 integration fault remains the mounted regression: it holds the completed checked-user
event after durability, arms the exact terminal publication fault, and requires the precise
`SourcePublicationFailed` outcome.

## Affected Work

`doc/plan.md` Phase 38, the live-event router target owner, and the submitted-input failure suite own
the correction.

## Later Phase 80 Settlement Distinction

The first persistent-failure cut-order regressions assumed that an authoritative `thread/closed`
always removes the target row. Focused nextest runs disproved that assumption on both sides of the
failure freeze: an exact pending-activation target deliberately remains retained for loss
settlement.

Closure still publishes `ThreadClosed` before disconnect, fences the remote thread lane, and makes
the frozen proof non-dispatchable. The retained row is no longer live routing authority; its frozen
guard remains unspent and its changed `publication_closing` state makes old dispatch authentication
fail. Tests and recovery code must distinguish externally observable terminal closure from final
loss-owner settlement instead of using target-row absence as the closure proof.
