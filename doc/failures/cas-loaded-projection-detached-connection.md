# CAS Loaded Projection Detached From Its Connection

Phase 10 initially represented a loaded CAS projection as copied identity facts while the admitted
backend session remained a separate borrowed argument. Its process-local registry keyed entries by
runtime, managed-process generation, and CAS thread, but not by the exact client connection that
owned the CAS subscription.

That boundary is invalid for recovered injection. CAS start and fork subscribe the calling client
connection, and the injected prefix is authorized only while the exact loaded session remains
proven. A registry lookup from one connection could therefore succeed while the coordinator sent a
recovered-source fork through a different connection to the same CAS process. The copied result
could also outlive its subscription, and the registry retained released keys as tombstones.

The invalid approach is removed rather than patched with a thread-count cap or a connection-id
comparison at one call site. Phase 10 now requires one process-owned connection service, an exact
connection identity, process-wide loaded-generation allocation, and non-cloneable per-thread
subscription leases. A recovered source can execute or fork only through its owning live lease.
Releasing a lease physically removes local authority before connection-scoped unsubscribe; loss
revokes every affected lease, and recovered binding retirement must become durable before a fresh
injected replacement is admitted.

Native persistent lineage may later resume after lease loss because CAS owns its rollout. Recovered
injected lineage cannot use resume as proof that its synthetic prefix survived. A second connection
to the same runtime and process is not independently a substitute for the original recovered lease.
The later narrative-mismatch design permits only an explicit overlapping handoff in which the old
lease remains as a non-execution subscription anchor until the fresh connection has joined that
exact in-memory thread.

Affected implementation is `crates/beryl-app/src/cas_projection`, with durable recovered-retirement
support in `syndic-storage`. Controlling authority is Phase 10 of `doc/plan.md` and the loaded
projection lease section of `doc/systems/cas-live-syndic-transcript/design.md`.
