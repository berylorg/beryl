# Compaction Admission Custody Bound

## Invalidated Assumption

The existing 64-entry compaction queue and eight synchronous workers do not bound all admitted
command custody. Revision-bound observation cannot derive its complete retained-owner bound from
those stages alone. This finding precedes observation implementation and is supported by independent
source review; no live reproduction is claimed.

## Decisive Evidence

All source paths below are relative to `crates/beryl-app/src/cas_projection`.

- `context_compaction/coordinator.rs::compact_thread` obtains a command permit before calling
  `admit_manual`. `LocalCompaction` stores that permit before queue admission.
- `context_compaction/coordinator/admission.rs::admit_manual` installs the local operation,
  publishes durable admission, reads it back and registers its target before calling `enqueue`.
  Retirement can make target registration fail.
- That failure branch settles `CancelledBeforeDispatch`, calls `fail_local`, then returns.
  Suspend the caller after `fail_local` and before returning. `complete_local` has removed its
  registry entry, but the caller still owns its `LocalCompaction` and command permit.
- `LocalCompaction::complete` only sets the result and wakes waiters. `release_command` belongs to
  `CompactionDriverGuard`, which this prequeue caller never reached. Its ordinary stack disposal
  eventually releases the permit, but the caller can remain paused before that disposal.
- `LoadedProjectionLease` and `LiveEventTarget` retain a connection `Arc`, not its worker permits.
  `connection/lifecycle.rs::execute_ordinary_shutdown` takes the connection runtime and joins its
  workers independently of the paused caller. The connection capacity becomes reusable.
- Repeating on fresh eligible thread and connection identities accumulates removed caller custody
  without occupying any compaction queue entry or worker. Neither the per-thread local registry nor
  the connection worker count bounds this interval.

Do not substitute an unproven autonomous-loss schedule: ordinary connection retirement has not
been shown to invoke compaction settlement while the sole target consumer is paused before enqueue.
The target-registration failure and paused return suffice.

## Proposed Correction Awaiting Operator Direction

Reserve compaction operation capacity before local installation and durable admission, and retain
that reservation through failed-admission disposal, queue handoff, driver execution, settlement and
target cleanup. Use the existing queue-plus-worker envelope, 72 operations, as the proposed total
custody budget. Completed result-only waiters need no reservation after exact command disposal.
Capacity denial must precede durable admission and preserve existing exact dispatch and settlement
rules. This changes execution admission and requires an explicit decision in the
[app live-control authority](../../crates/beryl-app/doc/design-live-control.md#custody-and-release)
before planning its implementation.

Connection-worker retention alone is not a demonstrated replacement: multiple compaction admissions
per connection would also need a bound. An arbitrary observation cutoff or silent omission of
removed callers would not satisfy complete work observation.

Verify paused failed admission, connection retirement and replacement pressure, queue/worker handoff,
driver target cleanup, cancellation and unwind before resuming observation. The driver cleanup tail
already remains within the eight synchronous compaction workers. Successful lifecycle settlement
holds `settlement_fence` through intent removal, durable settlement, reconciliation and disposal,
bounding that moved-out continuation interval to one. This does not establish every remaining
continuation-source bound; that readiness review remains with the observation phase.
