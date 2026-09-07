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

# Phase 337: Continue Bounded Sequence Range Repairs (pending)

The Operator approved the split-height repair and subsequent outcome/app completion. Readiness
proved that repairing whole-fragment surgery inside one command cannot satisfy the existing
aggregate byte ceiling for every supported tree. A canonical counterexample requires at least
4,251,960 acquired/emitted reference bytes before record overhead. The
[budget evidence](failures/syndic-draft-piece-range-budget.md) records both the rejected local join
strategy and the stronger one-command lower bound. No tree-algorithm changes have been made.

Proposed revised boundary, awaiting Operator direction and owning-design readiness: make partial
range surgery resumable through canonical intermediate working roots and an exact remaining range
or repair cursor. Preserve actual subtree heights, canonical occupancy and immutable reuse, and
charge all selected reads/emissions before exceeding the existing 256-record/4,194,304-byte limits.
Each quantum must commit authenticated progress; rejection of the same unchanged frontier is not
continuation. Define exact cursor/receipt/head binding, replay, cancellation, corruption rejection,
and final transition to insertion in the owning draft-storage and V7 schema authority before
implementation. Whether the existing fields can carry that cursor remains an explicit design
question; the plan does not authorize a format assumption.

Verify the [height-transition reproduction](failures/syndic-draft-piece-split-height.md), both range
edges, UTF-8 and marker gaps, exact text/root/history results, restart and replay across partial
repair, measured whole-quantum bounds, and the preserved 257-fragment operation. Obtain independent
semantic and package-required adversarial review before acceptance and resumed outcome verification.

# Phase 336: Reconcile Durable Staged Build Outcomes (pending)

Blocked after partial implementation by the approved persistent split repair above.
The initial locked production check passed. The latest focused run excluding the separate long
reproduction passed 5 tests and failed 3: settlement replay returns `EmptyContribution`, mixed-marker
advance reports `InvalidGapWitness`, and a fixture expects one window where two are observed.
Independent static review found no demonstrated blocker in capture, reconciliation, or charged
reference verification; runtime evidence and final review remain incomplete. The long-operation
test fails in the existing tree algorithm, and dynamic settlement, cleanup ambiguity, corruption,
successor/collision and long-prefix bounds still need evidence. No implementation is accepted or
committed. Preserve the source and tests; do not weaken the long-operation requirement.

The Operator approved this prerequisite and subsequent production integration. Independent
architecture readiness passed for the owning
[staged-build outcome contract](../crates/syndic-storage/doc/design-draft-storage.md#staged-build-command-outcomes),
[referenced-closure bounds](../crates/syndic-storage/doc/design-schema-v7.md#v7-bounds-and-canonical-encoding),
and shared [Syndic lifecycle](systems/syndic-conversation-history/design.md).

Implement the closed opaque prepared command and package-owned submit/resume boundary for build
transfer, durable window, construction advance and terminal election. Capture the actual serialized
result, including exact replay and dynamically selected settlement. Known commits consume local
finalization once; ambiguous outcomes retain their exact owner and verify the selected referenced
closure under the shared 128-attempt/8,388,608-byte allowance. Preserve original and later failures,
historical committed classification, current continuation eligibility and explicit cleanup custody.
Committed cleanup is a later bounded resume; neither callback fragment replay nor the old looping
cleanup helper satisfies this boundary.

Verify real staged fresh and mixed edits over multiple windows, source-only progress, and more than
256 total fragments without operation-length-dependent verification. Exercise known commit and
noncommit, ExactOld/ExactNew, failed reconciliation retrigger, finalization and cleanup faults,
stale or substituted authority, byte-equal split publication, and terminal root/history/admission
reclamation. Assert charged read ceilings and exact retained or released custody. Obtain independent
semantic and package-required adversarial review before acceptance, then resume the preserved app
integration through this accepted API.

# Phase 329: Connect Production Marker Admission (pending)

Resume the preserved partial app implementation after the approved staged-build reconciliation
prerequisite is accepted. The [failure record](failures/composer-marker-admission.md) records
the exact boundary. Preserve the uncommitted app implementation and tests; do not accept this phase
or substitute status reads for required finalization.

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
