# Invalidated Approach

Call Codex App Server `thread/rollback` against a historical valid CAS binding selected while the
current Syndic binding is `Unbound`, then publish the rolled-back prefix as a new valid binding.
On a returned error or a later local publication failure, publish a stale binding to retire the CAS
thread.

# Why It Failed

`thread/rollback` has no idempotency key and mutates the source CAS thread in place. Beryl can exit
or crash after CAS applied the rollback but before either the new valid binding or the stale
retirement reaches Fjall.

The current `Unbound` binding is not a sufficient crash fence. Native-lineage discovery deliberately
uses immutable terminal CAS-turn correlation and the CAS-thread reverse index to recover a
historical valid source binding behind that current head. After restart, the same planner can
therefore rediscover the pre-rollback durable source proof and issue the rollback again, removing
too many CAS turns.

Post-call error handling cannot close a process-crash cut that occurs before that handling runs.
Treating the old durable count as if it described the remotely mutated thread would also violate the
exact-prefix authority required by the CAS/Syndic boundary.

# Required Course Correction

Do not dispatch in-place rollback through the production projection coordinator until one clean
architecture is selected and documented:

- Persist an exact pre-dispatch rollback intent/fence that blocks every later historical-source
  reuse. Exact success must atomically publish the target valid binding and close that intent;
  restart with an unresolved intent must retire the source and recover without replaying rollback.
- Or remove in-place rollback from production projection selection and use crash-safe native
  inclusive fork for a nonempty target and native fresh start for an empty target. A failed or
  ambiguously completed creation then only orphans a new CAS thread and cannot mutate existing
  durable authority.

The second choice increases retained CAS-thread count until future garbage collection. The first
choice expands Syndic's durable binding/attempt schema and restart recovery surface.

# Related Evidence

- `doc/memory/topic/codex-app-server/native-lineage-0.144.1.md` records that fork and rollback have
  no idempotency key and must not be retried blindly after ambiguous completion.
- `doc/plan.md` Phase 10 currently requires native rollback but does not assign a durable
  pre-dispatch rollback fence.
- `crates/syndic-storage/src/native_projection.rs` can rediscover historical valid lineage while
  the current selected-path binding is unbound.

# Resolution

The Operator selected the crash-safe second course correction. Production projection orchestration
uses inclusive native fork for a nonempty earlier prefix and fresh native lineage for an empty
prefix. It never dispatches in-place rollback. The normalized backend primitive may remain an
app-neutral capability, but it is not durable Syndic lineage authority or a coordinator path.
