# Accepted-Next Worker Must Not Arm Its Own Flight Retry

## Scope

Phase 62 parked accepted-next workers and same-thread flight release.

## Invalidated Approach

The scheduler excluded `WorkerCompleted` from next-pass wake authority and assumed that was
sufficient to prevent a parked worker from retrying itself.

## Evidence

Under a deterministic redundant execution-ready wake, the scheduler rescanned the still-durable
candidate while its first worker was paused. The rescan collided with that worker's same-thread
flight and armed a flight-release waiter. When an ambiguous precommit promotion reconciled to
`Prior`, dropping the parked worker's lease released the flight and started a second worker.

## Why It Failed

The retry loop was indirect: worker completion did not open a pass, but a waiter created against
the scheduler's own active worker converted that same worker's mandatory flight release into
fresh-authority wake evidence.

## Required Course Correction

- Track each active accepted-next worker by both worker thread and Syndic thread.
- If a scan reaches a source already owned by an active accepted-next worker, retain only the
  compact scan position and stop without acquiring or waiting on the same-thread flight.
- Remove that ownership only when the worker is joined.
- Let the worker's typed disposition decide whether completion authorizes immediate continuation;
  a parked disposition cannot manufacture another wake through its own resource release.

## Affected Authority

- `doc/plan.md` Phase 62
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-app/tests/phase62_accepted_next_scheduler/promotion_faults.rs`
