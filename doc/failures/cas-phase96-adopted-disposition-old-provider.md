# Scope

Phase 96 terminal disposition of an adopted-but-unpublished recovery attempt.

# Invalidated Approach

Treat `AdoptedUnpublishedProjectionConnectionService::dispose_after_recovery_failure` as a complete
whole-attempt consuming disposition while it only makes the attempt inert, disposes connection and
candidate owners, and shuts down the replacement service.

# Evidence

The focused Phase 96 proof reaches successful closed-fence adoption with the old scheduler joined,
one retained connection, and one retained loaded-projection token. Before explicit disposition the
old scheduled-execution provider shutdown count is correctly zero. Source inspection shows the
disposition disarms recovery-inventory re-escrow and then drops the old retained service without
calling `PersistentFailureRetainedService::shutdown_old_service_epoch`.

# Why It Failed

The scheduled-execution provider contract requires an explicit `shutdown()` after scheduler workers
join and before the service releases its owned home. Successful publication reaches that boundary
through old-epoch retirement, and terminal quarantine retirement reaches it through terminal
retirement. Adopted-unpublished recovery-failure disposition reaches neither path. Provider `Drop`
is not the lifecycle authority and can mask the gap when a wrapper happens to shut itself down.

# Required Course Correction

Design one owning, exactly-once terminal retirement for the old recovery inventory that remains
available to the adopted attempt. It must preserve exact cut, fence, escrow, connection, candidate,
worker, and home ownership; invoke old-service shutdown only after scheduler quiescence; propagate
failure without losing the remaining owner; and prevent both re-escrow and repeated shutdown.

The Phase 96 proof must assert old-provider shutdown count zero before consuming disposition and one
after it. Do not replace this with a weaker assertion or a Drop-based cleanup.

# Affected Work

- `doc/plan.md` Phase 96.
- `crates/beryl-app/src/cas_projection/service/adoption/disposition.rs`.
- `crates/beryl-app/src/cas_projection/persistent_failure/retention/inventory`.
- `crates/beryl-app/tests/unit/service_epoch_adoption/command_frontier.rs`.

# Later Course Correction

The required course correction above belonged to the retained-service adoption design and is no
longer the target. The missing provider shutdown demonstrated that whole-attempt disposition still
did not own every old-generation lifecycle obligation after successful unpublished adoption. It is
therefore evidence against the adoption topology, not a reason to add another exactly-once
retirement layer to it.

Current recovery closes the failed service and all derived runtime authority, then creates fresh
services, connections, and projections from durable authority. Phase 96 and the later adoption
phases must be deleted or replaced accordingly. The zero-before/one-after shutdown assertion
remains evidence for the abandoned path; it is not an acceptance gate for a repaired adoption
disposition.
