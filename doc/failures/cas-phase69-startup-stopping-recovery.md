# CAS Phase 69 Startup Stopping Recovery

## Invalidated Approach

Treat a durable `Stopping` gate as an impossible app-startup case, then model its correction after
generic active-binding abandonment followed by a separately published source-less terminal.

## Evidence

Independent Phase 69 review found that `ProjectionConnectionService` failed construction whenever
startup classification returned `DeliveryRecoveryCase::Stopping`. The first recovery integration
attempt also failed because exact stop abandonment had already atomically published the
source-less `AuthorityLost` terminal that the generic active path publishes separately.

## Why It Failed

Both admitted and dispatch-claimed stop records are valid crash states. Neither recreates dispatch
authority, and stop abandonment owns a stronger atomic successor than generic binding
abandonment: it consumes the stop, retires the projection, and records incomplete terminal
authority in one mutation.

## Required Course Correction

Startup builds `AbandonStopOperation` only from the authenticated stopping recovery case, commits
and reconciles that exact mutation once, then resumes terminal-history convergence from its
already-published terminal successor. It never replays interruption or publishes another terminal.

## Affected Work

Root `doc/plan.md` Phase 69, app startup delivery recovery, stop publication reconciliation, and
admitted/claimed restart coverage own the correction.
