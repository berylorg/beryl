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

# Phase 380: Deliver Fatal Reports Across Process Exit (finished)

Accepted the bounded fatal-reporting authority and app-owned Windows channel with aborting hook.
Focused production/fixture checks and all 17 record/process tests passed; independent semantic
review accepted handle/mapping ownership, hook bounds and exact timeout-child cleanup. Production
adds four focused source files, 445 nonblank lines, within existing packages and dependencies.
The [containment lesson](failures/crash-report-test-containment.md) records direct nextest invocation.
GUI presentation and ordinary executable bootstrap remain separate acceptance boundaries.

# Phase 381: Present The Isolated Crash Report (wip)

Implement the declared report-only GPUI surface with its bounded preview, Copy feedback, keyboard
focus and terminal Exit/close behavior. Verify only the two commands exist, text remains inert,
clipboard export is explicit and presentation opens no home, settings, backend or recovery path.
Exercise the accepted report handoff with an isolated GUI fixture and independently review the
presentation and sensitive-data boundary.

# Phase 378: Reconcile Existing CAS Regression Evidence (pending)

Restore the bounded regression evidence identified in the committed-baseline comparison before
resuming process composition. Derive fixture and assertion corrections from the existing composer
marker contract, exact service/control ownership and terminal-failure authority; source and tests
remain governed by the app package and CAS-live system design. Diagnose the four recorded failure
families, preserve the behavior each test must distinguish, and repair obsolete test setup without
weakening protected outcomes. A required production correction needs its own ready plan boundary.

Verify the 18-case baseline selection and affected fixture consumers, then the app lifecycle/control
suite; document any independently remaining failure instead of treating a partial run as green.
Use focused independent review for changes to fault-injection expectations or custody assertions.

Paused after the [activation-panic diagnosis](failures/cas-regression-baseline.md#reconciliation-blocker):
the Operator replaced the proposed in-process correction with independent fatal reporting.
Reconcile panic cases against that policy without treating library-unwind tests as proof of
application recovery; retain the ordinary returned-error assertions.
The current test corrections remain uncommitted and this phase remains unaccepted: the latest
19-case selection has nine passes and ten failures, including unresolved image and compaction
fixture setup. Independent review accepts only the two terminal-storage assertion corrections.

The [fatal reporting assessment](failures/cas-regression-baseline.md#fatal-panic-reporting-direction)
preserves the rejected same-process assumption and the selected replacement.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Consume the accepted work inventory for shutdown and Running threads
without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

The terminal-disposal, runtime-demand and idle-retirement prerequisites are accepted. Composition
resumes after the separate regression-evidence reconciliation above.

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
