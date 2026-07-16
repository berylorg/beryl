# CAS Projection Connection Retirement Race

## Scope

Checkpoint 3 Phase 10 process-owned loaded-projection connection authority.

## Invalidated Approach

`ProjectionConnection::register_new` and `acquire_existing` checked the connection's retired flag
before mutating the process-wide loaded-thread registry, while `retire` set that flag and removed
the connection's entries through a separate unsynchronized registry operation.

## Evidence And Failure

Source review of `crates/beryl-app/src/cas_projection/connection.rs` found an interleaving where a
registration observes an active connection, retirement then removes its current entries, and the
registration finally inserts a new entry for that already-retired connection. The resulting entry
cannot authorize execution because its lease observes retirement, but it remains physically present
and can block later ownership of the same exact CAS thread.

This contradicts the process-owned connection authority, connection-wide revocation, and
no-tombstone requirements in Phase 10.

## Course Correction

Serialize connection retirement with only the bounded retired-check-plus-registry-acquisition
critical section. The gate must never cover backend or storage work. Retirement wins before a later
acquisition, or removes an acquisition that linearized first; no insertion may linearize after the
connection became retired.

Add deterministic concurrency coverage for both possible orderings and retain the existing
physical-removal and generation-ABA tests.

## Affected Authority

- `doc/plan.md`, Phase 10.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 loaded-projection correction.
- `crates/beryl-app/src/cas_projection/connection.rs` and focused projection tests.
