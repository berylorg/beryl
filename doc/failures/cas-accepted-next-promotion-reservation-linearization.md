# Accepted-Next Promotion Reservation Must Win Over Later Shutdown

## Scope

Phase 62 exact accepted-next promotion and connection-service shutdown.

## Invalidated Approach

The first implementation reused the scheduler's ordinary “service is accepting and generation is
current” predicate both before and after acquiring the noncloneable connection promotion
reservation.

## Evidence

A deterministic regression paused the promotion immediately after reservation acquisition and
then started service shutdown. Connection retirement correctly waited for the reservation, but
the worker's later validation observed the shutdown acceptance fence and abandoned the already
won promotion.

## Why It Failed

Reservation acquisition is the linearization point between promotion and shutdown. Once the
worker has acquired that reservation, a later shutdown must wait for command execution and
durable reconciliation. Rechecking the acceptance fence afterward lets the losing shutdown path
revoke the winner.

## Required Course Correction

- Before reservation, validate both service acceptance and exact home-generation authority.
- Acquire the promotion reservation while holding the connection registry authority.
- After reservation, revalidate home identity, health, generation, and Syndic storage authority,
  but do not reapply a later service-acceptance fence.
- Keep the reservation through command execution and durable reconciliation, then release it
  before projection execution.

## Affected Authority

- `doc/plan.md` Phase 62
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-app/tests/phase62_accepted_next_scheduler/shutdown.rs`
