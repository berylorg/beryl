# CAS Phase 84 Provider Self-Verification

## Scope

Provider-observation staging and frame-build reconciliation after an ambiguous home command in a
mounted running-session service epoch.

## Invalidated Approach

Each provider committer called `HomeStore::verify_health` itself after an ambiguous command, then
performed its exact durable point read. This preserved the provider's batch identity and frontier,
but created additional same-generation verifiers beside the process recovery supervisor.

## Evidence

Both callers are live production paths. The ordered provider ingester reaches
`StageCommitter::stage_batch` for begin, control, fragment, and seal operations. Provider publication
reaches `FrameCommitter` while retaining the exact route and source-publication permit. Each caller
already holds an operation-scoped live-command permit connected to the exact persistent-failure
notification.

The existing notification flight was one-way: it coalesced a signal for the supervisor but exposed
no completion epoch or outcome to mounted workers. Removing the local verifier without adding a
completion join would either abandon an ambiguously committed durable frontier, misclassify a
healthy service as failed, or require polling. Returning a generic staging conflict or rotating the
observation identity would contradict the exact point-read reconciliation authority established in
Phase 13.

## Course Correction

The exact service notification owns a monotonic multi-waiter verification-completion epoch.
Provider committers atomically register their home, home generation, and service generation, signal
or join the supervisor flight, and wait without holding gate or process-slot locks. The supervisor
publishes verified-current, failed, stale, or shutdown to every waiter before a failed-service cut
can drain their live-command permits.

Verified-current resumes the existing exact batch or build point-read reconciliation on the same
stack-owned frontier. Every other outcome returns typed authority loss and lets the persistent cut
settle the operation. The scheduler continues to use its dedicated nonblocking lane-resume signal;
the multi-waiter provider completion is not encoded as another consumable scheduler bit.

The same join applies when the exact provider epoch is already `verifying` before dispatch. If the
reconciliation point-read itself observes a later `verifying` epoch, the committer joins that new
completion and repeats the same read while retaining the original batch or build frontier.

## Invalidated Blanket Preparation Retry

Phase 86 briefly wrapped ordinary provider preparation in one generic verification helper. After
`VerifiedCurrent`, it retained every successful preparation but retried every error. That was not a
valid reconciliation boundary: `VerifiedCurrent` proves the exact flight completed, but does not
classify cancellation, lifecycle, source, frame, or staging errors as retryable. Applying the helper
to provider begin could also redispatch a non-idempotent staging command.

Scoped architectural review and actual broker-path tests established the required split. Provider
begin holds one witness around health, generation, and storage reacquisition, repeats only that
read-only authority point read after verified completion, and dispatches the generated observation
identity exactly once through the existing stage committer. Provider seal retains its exact source
permit, observation identity, route, and frontier; it reopens and repeats preparation only for a
typed health-gate ambiguity, while unrelated errors retain their original disposition. The source
publication permit remains the synchronized cancellation and target-retirement fence throughout
preparation and publication.

Completion review found the same blanket policy still present in source activation and checked-user
preparation: heterogeneous errors had been collapsed to `()`, and any error concurrent with
`VerifiedCurrent` was retried. The first typed correction also carried the pre-verification
`SyndicStorage` handle into later source-event publication and opened an unnecessary second join
against the already completed flight. Real-path faults showed one durable activation dispatch but
no visible continuation events, followed by `Unavailable` from the retired flight.

Source activation now carries typed authority, health-gate, domain, record, publication, and target
failures. Each durability-sensitive command dispatches once. After verified completion, the owner
reacquires the exact current storage handle and performs the reconciliation point read directly; it
joins again only when that read itself reports a new exact same-generation `verifying` epoch. The
fresh handle continues into checked-user preparation and publication. Direct activation callers
preserve authority loss separately from ordinary target failure instead of collapsing the new type.

A final review found two remaining provenance collapses. Source activation asked the publication
permit for an `Option` generation and then resampled mutable home health to guess why it was absent;
provider begin similarly reduced identity, generation, reacquisition, registration, poison, and
storage-health failures to `Option`. Both could mask a non-health failure concurrent with verified
completion or invalidate a target after the home had already returned healthy. The permit now
matches its immutable retained generation directly, and provider begin uses a typed authority-read
error. After verified completion both paths perform one direct fresh read; only a newly carried
exact same-generation `verifying` gate opens another join.

Provider-seal preparation applies the same boundary to the carried storage error itself: only
`verifying` at the ingester's immutable expected generation is ambiguous. `Failed`, `Reopening`,
foreign-generation health gates, and non-health failures keep their original disposition and may
not reopen or repeat preparation after `VerifiedCurrent`.

Helper-only tests were insufficient because they could encode the invalid retry policy without
exercising durable staging or target ownership. Focused coverage must drive the real broker begin
and seal paths and prove exact identity and route reuse, no staging redispatch, typed terminal
completion before permit drain, exact acknowledgement, and no target invalidation.

## Durable Rule

When one process owner has sole authority to verify shared storage health, mounted workers may
signal or join that owner's exact flight but may not run substitute verification. Any synchronous
worker that must preserve ambiguous durable ownership across the flight needs a stored, exact-epoch
completion outcome with missed-wake protection; a one-way signal is not a completion protocol.
