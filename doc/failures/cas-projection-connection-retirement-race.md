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

## Phase 62 Recurrence

Scheduled next-turn execution later repeated the same check-only assumption at a different
boundary. Lease validation sampled service ownership and the connection's retired and detached
flags, released every gate, and then executed the durable accepted-input promotion. Transport or
driver retirement could therefore complete between the last sample and the command, consuming the
next-turn leaf into a pending turn after its exact execution session was already unavailable.

The correction is one non-cloneable promotion reservation acquired under the service lifecycle and
connection-retirement authority immediately before the durable command. Acquisition is bounded and
the gates are released before storage work. If retirement wins, no reservation or promotion is
possible. If the reservation wins, retirement fences new authority but defers final registry
invalidation and detachment until promotion publication and fixed-work reconciliation release it.
The reservation never covers projection establishment or a CAS request.

## Affected Authority

- `doc/plan.md`, Phases 10 and 62.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 loaded-projection correction.
- `doc/systems/cas-live-syndic-transcript/design.md`,
  `crates/beryl-app/doc/design.md`, connection authority, accepted-input scheduling, and focused
  projection tests.
