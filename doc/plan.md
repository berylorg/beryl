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

# Phase 392: Fence Direct Submission Acceptance (finished)

Accepted original-epoch submission capture and counted final acceptance through durable result or
reconciliation installation. Independent review found no blocker. The 25 existing exact-submission
cases, five new shutdown-admission cases and shared-gate regression coverage passed; the production
library passed Cargo check. Draft preservation, stale callback rejection, cancellation and admitted
reconciliation remain distinct from scheduler dispatch and full shutdown completion below.

# Phase 393: Fence Accepted Input Promotion (wip)

Integrate process admission with the existing promotion reservation and retained reconciliation;
preserve accepted queue identity when the fence wins and settle a promotion that won first.

# Phase 394: Fence Provider Execution Dispatch (pending)

Revalidate the original queued execution epoch at pending-start, steering and compaction dispatch.
Retain typed exact nondispatch proof and separate uncertain dispatch from preservation eligibility.
Verify the shared router cut and stale queued callbacks without fencing terminal capture or stop.

# Phase 395: Fence Compaction And Continuation Admission (pending)

Integrate the shared gate with compaction admission and same-thread continuation settlement.
Cancel volatile intent at its exact cut, preserve already admitted pending identity and content,
and verify reconciliation custody and coherent reopening. Independently review each integrated
execution cut before process-wide coordinator composition.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Compose the accepted admission fence and exact all-work convergence boundary shared by final-window
close and explicit Exit. Preserve accepted queues and proven-undispatched pending turns, prevent
successor dispatch, retain dispatched or uncertain noninterruptible targets through terminal history
or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.

The [pending-turn correction](failures/process-shutdown-pending-turn.md) records the authorized
completion distinction. Verify durable pending preservation separately from uncertain dispatch,
including preparation cleanup, reconciliation failure and later recovery; no coarse work snapshot
or absent provider identity proves either completion outcome.

# Phase 382: Mount Crash Reporting At Process Entry (pending)

After target executable bootstrap exists, connect the accepted reporter and GUI boundaries before
ordinary storage/work startup. Verify reserved reporter mode cannot enter normal bootstrap,
ordinary startup installs fatal handling first, normal exit leaves no reporter, and a real
isolated application panic terminates its process while the report remains usable. No helper-only
or library-only evidence accepts this production mount; the current bootstrap removal gap remains
explicit until its owning rework checkpoint closes.
