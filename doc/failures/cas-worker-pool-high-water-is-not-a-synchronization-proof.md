# CAS Worker-Pool High-Water Is Not a Synchronization Proof

## Invalidated Exact Observation

Require an exact worker-pool high-water value when a two-worker connection and a bounded
one-permit scheduler scan may execute concurrently.

## Evidence

The scheduler scan can finish before the connection pair is acquired, or it can overlap that
pair. Both executions preserve the same capacity and ownership contracts, but their observed
high-water values are two and three respectively. Parallel test load exposed both valid
schedules.

## Correction

Tests bound the incidental high-water observation to two through three. They continue to require
exact active and available counts at synchronization points, denial before a connector opens a
socket, and full permit reuse after retirement.

## Reusable Lesson

Use high-water counters to prove a resource bound, not event ordering. Tests that need ordering
must synchronize on the owned event itself.

## Scheduler Worker Counts Do Not Prove Whole-Pool Quiescence

Phase 78 recovery-inventory testing exposed the same distinction at another counter boundary. The
accepted-input scheduler's initial recovered-pending scan can hold a local worker-pool permit while
it performs its source read without spawning a scheduler worker. Its `workers_active` diagnostic is
therefore zero even though the shared admission pool still has active work.

A fixture that injected persistent home failure after observing only the scheduler counter could
race that admitted read. The read then correctly failed closed and made the scheduler fatal, while
the test incorrectly attributed the outcome to inventory conversion. Whole-service quiescence now
requires both a completed scheduler pass and zero active permits in the actual projection worker
pool. Scheduler-local worker counts remain valid only for the child threads they explicitly own.
