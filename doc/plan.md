# Scope

Production marker admission and canonical widget publication have passed acceptance with the LLVM, one-job,
no-normal-debug and nonincremental settings under the root
[technology decisions](design.md#implementation-technology). The Operator authorizes pushing projects and owned forks as needed for
this work, including the accepted widget revision and Beryl changes. Preserve required
checks and independent review while using conditional delegation and bounded evidence.

Keep GUI thread switching clean without overengineering. The two authorized non-GUI corrections,
marker admission and healthy scheduler-conflict handling, are accepted. The Operator's instruction to
continue implementation resumes process-provider composition and the remaining phases below.
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

The Operator selected [fatal crash reporting](features/crash-reporting/design.md) with a separate
reporter process and immediate failed-application termination, explicitly requiring restrained
complexity. The [system boundary](systems/crash-reporting/design.md) supersedes the proposed
in-process panic-recovery correction. Establish its bounded components before returning to CAS
regression reconciliation; final production mounting remains dependent on executable bootstrap.

# Phase 387: Bound Window Creation Stack Layout (finished)

Creation advances through borrowed-self out-of-line handlers without new allocation or public API
changes. All 12 creation regressions and locked production compilation pass; independent semantic
review confirms transition, yield and custody preservation. The outer frame falls from about
1.44 MiB to 56,832 bytes; measured reconciliation-entry depth is about 586 KiB. The
[stack-layout record](failures/syndic-composer-mutation-stack-layout.md#creation-and-candidate-transfer-recurrence)
preserves evidence and measurement limits.

# Phase 388: Bound Candidate Transfer Stack Layout (wip)

Preserve the [Syndic draft contract](../crates/syndic-storage/doc/design-draft-storage.md) while
reducing measured transfer preparation and owned result pressure with borrowing, out-of-line
helpers and justified bounded boxing. Verify the ordinary-stack mounted scale witness, focused
staging/transfer and reconciliation semantics, production compilation and renewed frame attribution;
independently review durable reads, reservations, writes and exact error/custody behavior.

# Phase 384: Hand Exact Submission Acceptance To Process Execution (pending)

The Operator authorizes the correction identified by the
[submission handoff failure](failures/process-submission-execution-handoff.md). The
[composer package contract](../crates/beryl-app/doc/design-catalog-and-composer.md#history-publication-and-submission)
owns exact acceptance notification; existing process-generation ownership and scheduler lane gates
remain controlling. Add the bounded generation-scoped handoff and wire production submission
settlement before successor-editor activation, including unmounted settlement.

Verify committed, reconciled and already-accepted success, and absence of notification for
unresolved, cancelled and proven noncommitted outcomes. Verify wrong-home/generation rejection,
retired-service isolation and coalesced duplicate notification under existing dispatch gates.
Use the managed-runtime reproduction without a synthetic execution wake to prove direct and
accepted-input dispatch. Retain focused composer and scheduler regression evidence and obtain
independent semantic review of generation fencing, lane selection and notification placement.
Broader execution-lifetime composition remains the next separate boundary.

Implementation and initial semantic review are complete but acceptance is blocked. The
25 exact-submission cases now pass after phase 385; managed direct and accepted-successor dispatch
pass after phase 386. All 40 selected mounted/initial composer cases and directly affected target
compilation pass. The final creation/scale fixture run passes four cases and aborts ten with
ordinary-debug stack overflows. CDB attributes about 1.44 MiB to production
`MainWindowCreation::advance` before nested reconciliation; the scale witness separately overflows
in candidate-transfer preparation. The [stack-layout failure record](failures/syndic-composer-mutation-stack-layout.md#creation-and-candidate-transfer-recurrence)
preserves attribution and limits. The Operator authorized phases 387 and 388 before resuming this
acceptance; no larger-stack run counts as acceptance. Keep the composition fixtures
with this handoff evidence; broader terminal-history, compaction and shutdown acceptance remain
separate.

The package-wide test compilation attempt encounters pre-existing stale image-metadata arguments
and moved storage handles in unrelated composer/seal fixtures. It is not passing evidence and
does not expand this handoff into reconciliation of every deferred GUI/storage fixture.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Consume the accepted work inventory for shutdown and Running threads
without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

The terminal-disposal, runtime-demand, idle-retirement and regression-evidence prerequisites are
accepted. Resume composition from the existing process session provider, configured managed-runtime
preparation, shared work sources and control coordinators; verify their joined execution lifetime
without inferring complete composition from isolated component tests.

Resume after the separately authorized submission handoff correction. The managed-runtime
composition tests preserve the original reproduction; their later detachment, capture and
successor assertions remain unaccepted until exercised successfully. Joined terminal-history,
lifecycle-compaction and compaction-cleanup-to-successor evidence remains required.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Implement one admission fence and exact all-work convergence boundary shared by final-window close
and explicit Exit. Preserve accepted queues, prevent successor dispatch, retain exact pending and
noninterruptible targets through terminal history or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.

# Phase 382: Mount Crash Reporting At Process Entry (pending)

After target executable bootstrap exists, connect the accepted reporter and GUI boundaries before
ordinary storage/work startup. Verify reserved reporter mode cannot enter normal bootstrap,
ordinary startup installs fatal handling first, normal exit leaves no reporter, and a real
isolated application panic terminates its process while the report remains usable. No helper-only
or library-only evidence accepts this production mount; the current bootstrap removal gap remains
explicit until its owning rework checkpoint closes.
