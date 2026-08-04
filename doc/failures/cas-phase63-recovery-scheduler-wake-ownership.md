# CAS Phase 63 Recovery Scheduler Wake Ownership

## Invalidated Whole-Scan Reset

Reset the recovered-pending scan to the beginning after every Syndic revision change.

## Evidence

A recovered worker may leave an exactly rejected or otherwise safely undispatched turn pending.
Its own durable convergence changes the Syndic revision. An unconditional whole-scan reset then
rediscovers that same source without any fresh execution authority and can issue it repeatedly.

## Correction

The current sweep advances its physical thread floor before the worker runs and may rebind that
floor across work-neutral or own-worker revision drift. Provider-unavailable work also advances
for the current sweep. Neither worker completion nor its permit release opens a fresh complete
attempt.

## Invalidated Unowned Cursor Rebase

Treat physical-cursor rebinding as a complete response to arbitrary concurrent revision drift.

## Evidence

A concurrent mutation can make a thread behind the retained floor newly eligible. Rebinding alone
would skip that work for the lifetime of the service.

## Correction

Every independent owner capable of creating recovered-pending eligibility must publish
`ExecutionReady`. That wake discards the retained floor and starts a complete scan. A wake already
in the current batch reset before the read; a wake arriving during stale handling remains pending
because stale handling returns immediately. The storage rebind API documents this caller-owned
precondition and carries no source authority.

## Invalidated Duplicate Durable Wake

Publish accepted-next readiness both from durable terminal or loss convergence and again from the
scheduled worker completion that observed the same outcome.

## Evidence

Those independently queued reasons can each open a full pass. Under low capacity they produced
duplicate provider attempts after the durable state had already supplied the required progress
wake.

## Correction

The durable terminal or loss publisher owns its readiness wake. Worker completion owns bounded
join bookkeeping and only reports typed cross-lane continuation when a retained waiter actually
needs the released authority.

## Invalidated Completion Publication Order

Drop the scheduled-worker lease and worker permit before publishing the fixed-capacity completion
record.

## Evidence

Releasing the permit allows another worker to start and finish while the earlier completion has
not entered the queue. The number of unpublished or queued completions can then exceed the queue
capacity derived from live worker permits.

## Correction

Each scheduled worker publishes its completion while still holding the lease and permit, then
drops the lease, and only then wakes the scheduler. The permit bound therefore also bounds every
completion not yet drained.

## Invalidated First-Idle Test Observation

Treat the first observation of zero active workers as proof that independently published durable-
loss and capacity-release wakes have both been consumed.

## Evidence

The two causal wakes may coalesce under light load or be processed in separate passes under a
larger parallel test run. Sampling after the first pass made the second legitimate provider
attempt look like a self-retry.

## Correction

The cross-lane test now allows at most the two causally owned attempts, waits until that bounded
counter is quiescent, and then proves it remains stable. Any third causal excess or continuing
self-retry still fails the test.

## Reusable Lesson

A coalesced scheduler signal needs one causal owner per durable transition, and a bounded handoff
must retain the capacity token until publication. Cursor progress, retry eligibility, durable
readiness, and physical resource release are different facts and must not substitute for one
another.
