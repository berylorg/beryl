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

## Accepted Course Correction

The Operator approved reserving compaction operation capacity before local installation and durable
admission, retaining that reservation through failed-admission disposal, queue handoff, driver
execution, settlement and target cleanup. The existing queue-plus-worker envelope, 72 operations, is the total
custody budget. Completed result-only waiters need no reservation after exact command disposal.
Capacity denial must precede durable admission and preserve existing exact dispatch and settlement
rules. This changes execution admission and is now specified in the
[app live-control authority](../../crates/beryl-app/doc/design-live-control.md#custody-and-release)
and its separate implementation prerequisite is accepted.

Connection-worker retention alone is not a demonstrated replacement: multiple compaction admissions
per connection would also need a bound. An arbitrary observation cutoff or silent omission of
removed callers would not satisfy complete work observation.

Verify paused failed admission, connection retirement and replacement pressure, queue/worker handoff,
driver target cleanup, cancellation and unwind before resuming observation. The driver cleanup tail
already remains within the eight synchronous compaction workers. Successful lifecycle settlement
holds `settlement_fence` through intent removal, durable settlement, reconciliation and disposal,
bounding that moved-out continuation interval to one. This does not establish every remaining
continuation-source bound; that readiness review remains with the observation phase.

## Verification

The reservation now shares a disposal owner with the command permit. A separate admission guard
removes installed local custody and releases the command after projection cleanup on failure or
unwind, independently of retained result waiters. Successful enqueue transfers cleanup to the driver.
The guard distinguishes failed installation from an installed operation, preserving existing intent.

Independent review caught a Rust drop-order trap in the first implementation: local guards unwind
before function parameters, and later locals before earlier locals. Explicit projection/target
rebindings place disposal ahead of command release, including early lifecycle preparation failure.
Poisoned command cleanup recovers the mutex contents to release the exact reservation.

Three real protocol tests perform ordinary execution, compaction admission, connection retirement,
failed target registration or admission unwind, and disposal with a retained result waiter. Pressure
holds the other 71 slots; capacity denial precedes another durable admission despite connection
worker reuse. Driver tests preserve custody through terminal handoff, epoch loss and poisoned unwind.
The existing queue, continuation, settlement and shutdown checks remain passing.

Independent semantic review, production compilation and 37 regressions passed. A final six-case
custody run passed after adding retained-waiter assertions. Changed-file formatting and diff checks
passed. Guarded job memory peaked at 2.10 GiB; owned processes exited and six exact task temporary
directories were removed. The first protocol run was aborted after a test assertion left a pause
controller outside its thread scope; controllers now release during scope unwind. The corrected
bounded run exposed and fixed an unconsumed fixture admission event.

## Continuation Admission Prerequisite

The accepted compaction correction starts too late to bound continuation intent before compaction
admission. Independent source review confirms this separate earlier interval; no live reproduction
is claimed. The compaction reservation acceptance above remains valid.

- `stop/lifecycle.rs::accept_lifecycle_yield` validates exact turn identity and duplicate admission,
  then inserts an `AcceptedLifecycleYield` without a capacity reservation. Its bounded attention
  pool and the separate 256 cancellation markers do not bound accepted continuation owners.
- A public direct ordinary execution can pause after accepting `PhaseContinue`, before
  `ordinary/execute/capture_loop.rs::handle_dynamic_tool` calls `respond_dynamic_tool_call`.
  The callback holds no connection runtime lock. Its `OrdinaryLifecycleGuard` preserves the
  registered intent until the caller progresses or disposes it.
- Connection retirement independently takes the runtime and joins its workers. Repeated direct
  executions on fresh eligible thread identities can retain more intents while reusing connection
  capacity. Direct execution's `ProjectionFlight` is per-thread exclusion in an uncapped map,
  rather than a global capacity reservation.
- Scheduled execution's `ScheduledOrdinaryExecutionLease` does retain a worker permit through
  execution; that separate path cannot establish the direct caller's bound.
- `release_ordinary_lifecycle_yield` removes the accepted value before cancellation and disposal.
  Observation must therefore follow removed cleanup custody as well as registry membership.
  Successful compaction settlement's moved-out intent remains bounded by its settlement fence.

The recommended correction, awaiting Operator direction, is to reserve from the existing 72-slot
budget when a `PhaseContinue` intent is accepted. Share that same counted reservation with its
later compaction, retaining it until both intent disposal and command/target cleanup finish.
Reacquisition at compaction admission could strand a budget already occupied by 72 accepted intents.
Exhaustion must leave the new continuation unaccepted, without new attention or continuation state.
Cancellation retains the reservation through actual disposal; it does not make a retained cancelled
value invisible or prematurely reusable.

This proposal bounds continuation-classified yields, including their later cancelled state. It
does not claim to bound every other lifecycle-yield outcome or change their acceptance policy.
Verify direct-caller suspension, connection reuse, admission pressure, cancellation and removed
cleanup, and shared reservation transfer into compaction before resuming observation. The owning
live-control authority must record this acceptance-lifetime change before its implementation plan.
