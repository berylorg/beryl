# Generated Media Read Drift

The first coordinator treated every source/admission read failure as a fatal worker failure.
Its fatal path cancels the fixed cohort, so ordinary concurrent capture or another generated
worker's publication could strand all subsequent generated output until graph replacement.

Syndic intentionally returns `ConcurrentChange` when its domain revision advances during a bounded
source proof read. State admission reads originally returned the same `Rejected` value for revision
drift and disagreeing target/metadata facts. Generic error conversion erased the distinction the
service needs to preserve ongoing convergence. Publication also performs an admission read inside
State's mutation factory, so correcting only App's direct read sites was insufficient.

State now distinguishes read revision drift from disagreement. App defers typed concurrent reads,
releases the compact resource flight and uses the already registered Home mutation wake to recheck.
Discovery retains the exact finite sweep/high-water on page deferral. A drifting descriptor does
not reset that cursor or prevent later descriptors from progressing. Genuine integrity and Home
failures retain their separate failure behavior.

The deterministic State regression blocks the initial revision read at Home's existing read
confirmation boundary, commits actual Asset publication, then verifies `ConcurrentChange` and a
successful fresh Published read. Coordinator coverage must also preserve continuation and verify
that the nested publication read follows the same deferred path. The owning contracts remain
[State assets](../../crates/beryl-state/doc/design-assets.md) and
[generated convergence](../systems/image-assets/generated-finalization.md); current acceptance
status belongs to the [active plan](../plan.md).

The whole-activation Home revision fence initially translated a cancelled candidate read into
Unavailable and attempted final confirmation after failed bounded preparation. The unchanged
candidate cancellation regression exposed both effects. Fence reads now check cancellation before
and after I/O before translating errors; failed preparation propagates its original error without
another confirmation read. Successful reads still require equal Home revisions. Cancellation,
ordinary stale snapshots and genuine Home failures must remain distinct at this boundary.
