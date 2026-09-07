# Scope

Keep GUI thread switching clean without overengineering. The Operator now authorizes the two
reported non-GUI corrections: marker admission and healthy scheduler-conflict handling. Keep each
in its own acceptance boundary and reuse existing mechanisms. Broader process-provider composition
and other deferred work remain outside this authorization.
The controlling contracts are [conversation threads](features/conversation-threads/design.md),
[backend recovery](features/backend-runtime-recovery/design.md), and the
[app package](../crates/beryl-app/doc/design.md). Complete process composition, Running threads,
final-window shutdown and other broader background-work requirements remain separate rework work.

The Operator authorizes continuous implementation until a blocker requires attention, with a
commit after each accepted phase. Temporary-directory deletion and obsolete-directory cleanup
remain authorized; verify exact targets and preserve unrelated or concurrent work. The active
Beryl-home architectural replacement remains tracked by [REWORK.md](rework/beryl-home/REWORK.md).
The former window-owned stop/wait plan is superseded. Previously accepted exact-stop,
continuation-cancellation, draft-flush and session primitives are reusable evidence, not authority
for stopping a background thread when a nonfinal view closes.

Apply the [simplification audit](audits/code-simplification/report.md) selectively within each
owning acceptance boundary. It is evidence, not authority or a second plan; preserve its baseline
estimates and record actual dispositions only for accepted selected findings. Separate an
independently implementable simplification or material scope growth before work begins. Keep
source names behavior-based and follow the canonical single GPUI graph. No compatibility shell,
universal resource governor, or compile-only substitute fulfills target behavior.

The final shell mounts every declared slot and feature contribution using accepted target
services and widgets. Leave deferred contributions visibly absent or unavailable until their
bounded implementation is accepted. Startup, restoration, Exit/close mounting, catalog,
transcript, Running threads, attention, approval-policy reconciliation, Settings, repair, recovery,
branch, assets and bootstrap remain explicit rework checkpoints. Preserve their separate gates.

# Phase 335: Preserve Typed Marker Admission Refusals (finished)

Accepted typed operation-limit, shared-capacity, storage-failure, and committed-unavailable results
through readiness, assignment and reconciliation under the
[draft-storage contract](../crates/syndic-storage/doc/design-draft-storage.md). Postcommit readiness
retry retains its exclusive exact attempt; known durable outcomes survive local cleanup failures.
Independent semantic and adversarial review passed after correcting retry capability duplication
and failure-provenance loss. The final 41 submission, assignment and cleanup tests passed after all
changes; the earlier 113-test marker regression run and locked local Syndic/HomeStore checks passed.
The accepted widget evidence, fresh readiness, and admitted target resolver prerequisites remain
ready for production composition.

# Phase 336: Reconcile Durable Staged Build Outcomes (pending)

Proposed prerequisite after production integration exposed a missing public Syndic boundary.
The current public build reconciliation requires replay of original fragments from ordinal one;
the host retains authenticated durable staging authority and has no public bounded reader for
that replay. A terminal status read omits committed local finalization and writer progress or
settlement release. Establish the owning Syndic contract for bounded staged-build outcome
reconciliation before activation. It must authenticate the exact staged build, consume local
finalization once, preserve typed command and ambiguous-outcome evidence, and perform the existing
writer progress, settlement, and cleanup transitions without an app-owned full-edit buffer.
Verify real multi-page fresh and mixed edits, ExactOld/ExactNew, finalization failure, stale or
substituted authority, and terminal reclamation. Obtain independent semantic and adversarial review.
Implementation awaits Operator direction under the technical-plan stop rule.

# Phase 329: Connect Production Marker Admission (wip)

Blocked after partial app implementation: staged build outcome reconciliation cannot be composed
from the accepted public APIs. The [failure record](failures/composer-marker-admission.md) records
the exact boundary. Preserve the uncommitted app implementation and tests; do not accept this phase
or substitute status reads for required finalization. The proposed prerequisite above needs
Operator direction before work resumes.

Compose accepted widget evidence and Syndic fresh-asset readiness before production storage
MutationBegin. Preserve generic HomeStore proof composition, exact selection and operation,
bounded replay, cancellation and reconciliation. Remove the test-only admission restriction and
verify fresh insertion, replacement, moves, removal, and the three previously failing mounted GUI
cases without weakening their assertions. Broader process-provider work remains deferred.
Use existing bounded local producers, replayable propagated cut, and the admitted-Asset marker
entry point. The mounted rich/large clipboard event currently has no producer consumer; its
composition remains part of the separate clipboard/image-assets rework checkpoint. Acceptance
here does not claim end-to-end mounted clipboard image decoding or large/rich paste.

# Phase 324: Own Scheduled Execution Sessions In The Process (pending)

Deferred independent scheduler composition work. The uncommitted provider has no production GUI
switching caller and is not a prerequisite for Phase 327. Preserve its unaccepted source separately;
do not include it in the GUI phase's commit or infer it is required merely to detach a view.
Its concurrency test exposed [existing healthy-conflict fatalization](failures/cas-phase13-global-revision-publication.md).
The narrow scheduler correction is authorized separately in Phase 330; that authorization does not
activate this provider work. Re-establish readiness before resuming this phase.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Establish the bounded revision-bound work inventory for shutdown and
Running threads without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Implement one admission fence and exact all-work convergence boundary shared by final-window close
and explicit Exit. Preserve accepted queues, prevent successor dispatch, retain exact pending and
noninterruptible targets through terminal history or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.
