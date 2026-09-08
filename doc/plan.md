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

# Phase 339: Preserve Canonical Marker Admission Deletion (finished)

Accepted [canonical admission deletion](../crates/syndic-storage/doc/design-draft-storage.md#canonical-admission-deletion)
and its [complete bounds](../crates/syndic-storage/doc/design-schema-v7.md#canonical-admission-deletion-bounds).
Both tree kinds normalize underflow, preserve untouched subtrees and return exact deletion results.
Assignment authenticates the complete prior receipt transition before cleanup and shares one
pre-reserved ledger through public preparation, serialized work and first postcommit readiness.
The existing durable formats and captured reconciliation custody remain intact.

Independent readiness and implementation review passed. Isolated locked Syndic/app checks passed;
17 deletion and 15 writer tests passed, together with the other 124 selected admission/readiness
cases in the broader run. Evidence covers height 18, consecutive tall-tree cleanup, full fanout,
65,410 associations reaching 1,155 path references, real retained-receipt corruption, exact work
counters, cancellation and captured reconciliation. A stale cloned-advance expectation was
corrected to the documented revision conflict after an identical baseline failure was confirmed.

The [failure record](failures/syndic-marker-admission-deletion.md) retains the invalidated assumptions.
This accepts the deletion result and complete assignment boundary. The builder's duplicate
preparation still requires Phase 338's sealed-result reuse and one complete quantum ledger.

# Phase 338: Continue Bounded Marker Tree Effects (pending)

The approved resumable repair separates marker effects from ordinary range removal because they
have distinct continuation and acceptance rules. Existing marker removal changes sequence,
identity and order trees in one quantum; valid height-45 binary trees require 270 stored reads
across those three deletions before authentication or emissions. Ordinary range continuation alone
does not satisfy that boundary.

Establish owning authority for exact pending per-structure roots and closed effect substeps under
the existing operation's active-effect custody. Keep the coherent source roots selected until all
three changes finish; incomplete pending roots are not ordinary build roots. Preserve marker
identity/Asset/label facts, complete effect-chain advancement once, and authenticate each bounded
partial transition, replay and terminal cleanup. Verify forced continuation, restart, corruption,
and aggregate quantum charges before acceptance and staged-outcome completion. This phase is
pending design readiness and does not authorize implementation against unspecified cursor bytes.
Compose the canonical admission-deletion result in the separate Publishing command under the
captured Syndic revision and exact mutable head/capacity fences. Replace the current second
target-path preparation with sealed-result reuse and charge the complete builder invocation to
one ledger; the accepted standalone deletion and assignment proof cannot substitute for this work.

The [admission-deletion prerequisite](failures/syndic-marker-admission-deletion.md) is accepted.
Renew marker readiness against its canonical result and complete charged proof. The earlier
Publishing and insertion estimates remain provisional; owning V5 draft edits were restored and
no marker-continuation implementation began. The Operator approved subsequent continuation;
implementation still requires the closed cursor/program authority and independent readiness.

# Phase 336: Reconcile Durable Staged Build Outcomes (pending)

Preserved partial implementation waits for the marker-tree repair boundary above. Ordinary range
repair now passes the 257-fragment staged-operation reproduction. The earlier focused outcome run
left settlement replay (`EmptyContribution`), mixed-marker advancement (`InvalidGapWitness`) and a
one-window fixture expectation unresolved. Recheck those after marker continuation; dynamic
settlement, cleanup ambiguity, corruption, successor/collision and long-prefix bounds still need
complete runtime evidence and final review. No staged-outcome implementation is accepted or
committed. Preserve its source and tests and the long-operation requirement.

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
