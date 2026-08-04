# Scope

Provider-broker terminal ownership and Phase 35 ambiguous-write verification.

# Invalidated Approach

When the ingester itself produces a terminal result, publish the broker's shared cancellation flag
before installing the terminal reply, while allowing fault tests to begin reading durable state as
soon as a storage frontier became observable.

# Evidence

Independent Phase 43 review found that `Ingester::run` stored terminal cancellation before
`AckSlot::complete` returned the exact operation or completion. The Phase 35 `AfterPersist` paths
then waited on storage before checking the broker's exact submitted/acknowledged and in-flight
counters. A second review established that an idle counter snapshot was still insufficient: it
could be sampled after server transmission but before the new observation entered broker
accounting, and channel delivery preceded the old `begin_submission` metric.

# Why It Failed

The ingester-generated cancellation is externally observable lifecycle state, but the ingester
still owns the synchronous operation until its acknowledgement slot contains the
ownership-preserving reply. Publishing those facts in the opposite order exposes a terminal
connection whose caller has not recovered its exact operation or completion. Independent shutdown
may still signal cancellation early, but the acknowledgement wait deliberately retains ownership
until the ingester returns it. A storage frontier alone cannot prove that ambiguous-write
verification and terminal broker settlement have finished.

# Course Correction

Terminal completion now atomically installs the exact reply and closes later admission under the
acknowledgement-slot mutex. Only after that linearization point may the ingester publish
cancellation and retire. Test accounting acquires its in-flight guard before channel delivery, then
marks only a successful send as submitted. Each acknowledged provider seal advances a dedicated
counter. Fault tests capture the next seal target before releasing or sending an observation and
require that exact target, zero in-flight and staged work, and `submitted == acked` before
inspecting durable state.

# Affected Authority

- `crates/beryl-app/doc/design.md`
- `doc/plan.md`, Phase 43
- `crates/beryl-app/tests/phase35_provider_residency`
