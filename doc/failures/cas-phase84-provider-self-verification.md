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

## Durable Rule

When one process owner has sole authority to verify shared storage health, mounted workers may
signal or join that owner's exact flight but may not run substitute verification. Any synchronous
worker that must preserve ambiguous durable ownership across the flight needs a stored, exact-epoch
completion outcome with missed-wake protection; a one-way signal is not a completion protocol.
