# Accepted-Next Promotion Reservation Must Win Over Later Shutdown

## Scope

Exact accepted-next promotion, connection retirement and process shutdown admission.

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

## Process Admission Settlement

The process-fence regression exposed a second settlement gap: the worker installed an
indeterminate promotion's reconciliation handle and immediately classified the command as a
persistent home failure. A healthy `AfterCommitBeforePersist` fault then reconciled successfully,
but service close still failed with `SchedulerShutdown`. Installation alone did not settle the
winning promotion or justify a home-failure classification.

The worker now reconciles the exact installed handle while retaining both connection and process
reservations. Exact publication proceeds through existing promotion proof; exact nonpublication
parks the accepted candidate. Collision and unresolved failure retain their failure paths. Release
of a concurrently retired connection must not mask an already classified command failure.

The focused process-admission tests cover the reserved-but-uninstalled gap, worker-owned healthy
reconciliation, and failed promotion concurrent with connection retirement. Provider dispatch
fencing remains a separate boundary.

## Affected Authority

- `doc/plan.md` accepted-input promotion boundary
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-app/tests/accepted_next_scheduler/shutdown.rs`
- `crates/beryl-app/tests/accepted_next_scheduler/process_admission.rs`
