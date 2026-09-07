# Scope

Ordinary-turn publication, convergence, and cross-domain accepted-input promotion across
concurrent Syndic threads. The original publication correction was implemented during Phase 13;
the process-session integration in Phase 324 exposed the remaining cross-domain case.

# Invalidated approach

The app sampled the home and Syndic domain revisions before entering
`HomeStore::execute`, built a revision-checked command, and reconciled the result after the writer
returned. Multi-record orchestration snapshots likewise retried whenever the whole Syndic domain
revision changed.

# Evidence

`HomeStore::execute` acquires the process-wide writer only after the caller has built its command.
An unrelated thread can therefore commit between the app's revision reads and writer admission.
The resulting ordinary command reports a revision conflict even though every request-owned record
still has the exact expected revision. Live-event capture treated that conflict as execution loss,
and its cleanup used the same racy path. Whole-domain stable-read loops could also retry forever
while unrelated threads continued committing.

# Why it failed

Home/domain revisions serialize physical commands, but they do not express logical ownership of one
thread. Sampling them outside writer admission coupled independent threads and violated the accepted
cross-thread concurrency contract. Blind retry would remain starvation-prone and could not make a
cleanup command authoritative.

# Course correction

The publication correction added a permanent typed home-store boundary that captures current physical revisions only after
serialized writer admission while preserving each domain mutation's exact record-level validation.
It serves ordinary publication and convergence. Stabilize read-only orchestration snapshots by
re-reading their exact mutable anchor records, never by waiting for the entire domain to become
quiet. Add adversarial tests with sustained unrelated-thread commits.

# Cross-Domain Scheduler Recurrence

The process-session integration test
`process_sessions_dispatch_and_capture_independent_threads_without_views` holds one thread's
terminal delivery while admitting another exact thread. Its first thread can capture user events
between construction and execution of the second thread's accepted-promotion command. The observed
result was `NotCommitted` with a Syndic domain revision conflict: expected 245, current 248.

`crates/beryl-app/src/cas_projection/accepted_input_scheduler/next_turn/worker.rs` returns
`WorkerDisposition::CommandNotCommitted` before promotion reconciliation.
`accepted_input_scheduler/workers.rs` groups that disposition with committed and indeterminate
command failures and calls `fail_closed(PersistentHomeFailure)`. The observed scheduler became
fatal and the process provider closed while the persistent failure cut remained armed. This is
ordinary unrelated-thread contention, not lost session authority or unhealthy storage.

The failing scheduler code predates the process-session provider and has no GUI dependency.
Concurrent threads are already supported; view detachment introduces no new concurrency
requirement. The new test exposes a defect in an existing queued-input promotion path, not evidence
that concurrent execution generally lacked support.

The existing `CurrentDomainCommand`/`execute_current` correction cannot simply be reused:
[atomic-command authority](../../crates/beryl-home-store/doc/design-atomic-commands.md) explicitly
limits it to one domain with no sidecar token. Accepted promotion must atomically combine Syndic
promotion and Asset-owner transition through the caller-fenced `HomeCommand` boundary. This limits
that particular remedy; it does not establish that a compound writer-time boundary is necessary.

Home-store conflict classification explicitly treats `CommandError::Conflict` as no health
failure. The scheduler already permits fresh next-lane scans through typed continuation after
ordinary candidate drift. A narrow correction may handle a healthy, definitely uncommitted
physical conflict through that existing exact revalidation path. Its cursor, wake, generation,
resource-release and continued-progress behavior still needs verification; a blind retry loop is
not an accepted correction.

Phase 324 remains unaccepted. The earlier conclusion that this result required new cross-domain
storage architecture was premature and is withdrawn. Determine the narrow correction from the
existing concurrency and command-outcome contracts before proposing an architectural change.
Preserve exact logical-record validation, atomic ownership transfer, typed failure provenance,
generation and cancellation cuts, and bounded progress. Neither blind retries nor test timing
that avoids the conflict establishes these guarantees. The provider work and failing integration
test remain uncommitted; this record does not claim a completed correction for promotion.

# Affected authority

`doc/plan.md`, `doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/beryl-home-storage/design.md`, and the owning home-store, Syndic, Asset and app package
documents control the corrected boundary. The deferred scheduler issue is recorded in Phase 324.
