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

At every phase boundary, explain any blocker directly to the Operator and suggest concrete next
steps. Evidence links supplement that explanation rather than replacing it.

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

# Phase 342: Correct Terminal Marker Admission Cleanup (finished)

Accepted authenticated replay-target reclamation at writer handoff and ordinary terminalization,
with exact logical/physical accounting, shared bounded work and deletion reconciliation. All 49
focused storage cases and ten public app witnesses passed across the recorded runs. Isolated
locked production Syndic/app checks and independent semantic/adversarial review passed. The
[failure record](failures/draft-marker-replay-cleanup-accounting.md#accepted-writer-handoff-correction)
preserves the cause, correction and evidence. App presentation and mounted acceptance remain next.

# Phase 329: Connect Production Marker Admission (wip)

The staged-build reconciliation prerequisite is accepted. Renewed implementation readiness found
no missing public API or architectural prerequisite under the
[app composer contract](../crates/beryl-app/doc/design-catalog-and-composer.md#edit-marker-and-candidate-adaptation)
and [feature reconciliation rules](features/composer/design.md#durable-mutation-reconciliation).
The [failure record](failures/composer-marker-admission.md) preserves earlier app failures.
The [Syndic mapping prerequisite](failures/syndic-marker-build-frontier.md#accepted-v6-continuation)
is now accepted, as is the
[terminal cleanup correction](failures/draft-marker-replay-cleanup-accounting.md#accepted-writer-handoff-correction).
All ten public app evidence cases pass, including exact build cancellation, disposal and mismatched
replay cleanup. Preserve that verified composition while finishing the unaccepted app work.
Review found that typed admission refusals have no visible consumer; connect their distinct
messages to the existing bounded GUI presentation. Then complete the mounted cases, focused
regressions, production app check and final independent review. App acceptance remains outstanding.

Replace post-finish raw submission and terminal-status polling with opaque Syndic prepared commands
and retained outcome flights. Resume existing work before preparing another command, within the
host's transition budget. Preserve exact flight custody through current and detached execution,
cancellation and service disposal. Keep committed classification and original/later diagnostics
separate from cleanup or availability failures. Terminally unavailable requests remain unavailable
and cannot be revived through the marker-admission retry path.

Compose accepted widget evidence and Syndic fresh-asset readiness before production storage
MutationBegin. Preserve generic HomeStore proof composition, exact selection and operation,
bounded replay, cancellation and reconciliation. Remove the test-only admission restriction and
verify fresh insertion, replacement, moves, removal, and the three previously failing mounted GUI
cases without weakening their assertions. Broader process-provider work remains deferred.
Verify bounded evidence and staging agreement, stale operation/predecessor/generation refusal,
distinct size/capacity/storage failures, exact refusal cleanup, post-finish ambiguity, cancellation
with a retained flight, committed cleanup failure and detached/disposal drain. Credit the accepted
Syndic/HomeStore guarantees and verify the app's composition through real fault boundaries.
Run the production app check and focused serial composer/slot/lifecycle regressions, including
shared history support when changed. Obtain independent semantic and adversarial custody review
before acceptance and commit.
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
