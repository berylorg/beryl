# CAS Projection Flights Scoped To One Coordinator Instance

Phase 10 initially put the same-thread projection-flight registry inside each
`CasProjectionCoordinator`. The coordinator constructor remained publicly repeatable for the same
healthy Beryl-home generation, so two instances could both admit the same Syndic thread and issue
duplicate remote start, fork, or recovery-injection work before one durable publication lost its
revision race.

Durable expected revisions make the second publication fail, but they do not undo duplicate remote
work and therefore cannot prove the required single coordinator winner or exactly-once injection
attempt. Making callers remember one preferred coordinator would be duplicated process authority,
not an architectural guarantee.

The invalid scope is replaced by one process-wide flight registry keyed by exact Beryl-home id,
healthy home generation, and Syndic thread id. Acquisition remains a short bounded mutation; the
non-cloneable guard physically removes its key on drop, and no storage or backend work occurs while
the registry mutex is held. Separate homes, recovered home generations, and different threads stay
independent.

Affected implementation is `crates/beryl-app/src/cas_projection/service.rs`. Controlling authority
is Phase 10 of `doc/plan.md` and the concurrency contract in
`doc/systems/cas-live-syndic-transcript/design.md`.
