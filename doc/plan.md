# Scope

The Operator's immediate direction is to keep GUI thread switching clean without overengineering.
Preserve existing execution ownership and behavior. Identify demonstrated non-GUI execution flaws
and suggest architecturally clean corrections separately; do not use them to expand this GUI change.
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

# Phase 327: Preserve Recovery Work Across GUI Thread Switching (finished)

Accepted the GUI-only recovery-prompt switching correction: exact draft flush and failed-switch
retention, process-route preservation, current-route rediscovery, and autosave suspension/restoration.
Ten recovery tests, both app checks and independent semantic review passed. The seven ordinary
GUI failures reproduce without these GUI changes and remain the next verification boundary.
See [evidence and limits](failures/native-lineage-view-lifetime.md).

# Phase 328: Verify Ordinary Composer Switching (wip)

Classify the seven ordinary GUI failures reproduced with and without the preceding GUI change.
Compare their flush, capture, marker and disposal drivers with the current typed contracts and
passing mounted-switch tests. Correct only demonstrated stale test-driving mechanics within this
verification boundary; retain their actual behavior assertions. Do not weaken expectations, add
arbitrary waits, or infer a production defect from an undriven test operation.

If diagnosis proves another GUI production defect, report its exact cause and establish the narrow
implementation boundary before changing production. Non-GUI execution flaws remain findings and
clean solution proposals, as the Operator requested. Verify corrected tests with focused nextest
using documented serial GUI settings and then the three affected GUI targets; independently review
any changed drivers against their retained assertions. Preserve all deferred provider changes.

Current milestone: read-only diagnosis is starting. The broader run had 27 passes and seven failures;
the seven failures reproduced with the five GUI production files at HEAD, after which exact current
bytes were restored and verified. Full shell claim/session/transcript activation remains an existing
rework integration limit, not authority to expand this phase into bootstrap or whole-shell work.

# Phase 324: Own Scheduled Execution Sessions In The Process (pending)

Deferred independent scheduler composition work. The uncommitted provider has no production GUI
switching caller and is not a prerequisite for Phase 327. Preserve its unaccepted source separately;
do not include it in the GUI phase's commit or infer it is required merely to detach a view.
Its concurrency test exposed [existing healthy-conflict fatalization](failures/cas-phase13-global-revision-publication.md).
Report the narrow existing-scheduler correction for that flaw; no execution repair or storage
redesign is authorized by the current GUI slice. Re-establish readiness before resuming this phase.

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
