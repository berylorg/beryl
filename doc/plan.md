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

# Phase 350: Preserve Distinct Proofs At Lifecycle Compaction Handoff (finished)

Accepted like-for-like lineage validation at lifecycle compaction admission, preserving independent
current-prefix and revision checks in storage. Real ordinary-terminal-to-compaction verification
passed with unchanged lineage and history; stale candidate admission proved no second operation.
Storage compaction passed 45/45 and app terminal/compaction regression passed 35/35. Independent
semantic review, formatting and diff checks passed; all test processes were reaped. The pending
timeout changes were preserved separately during prerequisite verification.

# Phase 349: Resolve Timeout Policy At Compaction Admission (pending)

Resolve the latest valid applied timeout when manual or lifecycle compaction is admitted, preserving
that value for the admitted operation and typed settings failure. Verify settings changes during a
preceding ordinary turn affect only subsequently admitted compaction, under the status-line contract.

Authority readiness and independent review passed against status-line, Settings startup availability,
typed settings reads and the app lifecycle contracts. Carry a narrow applied-settings capability to
admission; resolve whole seconds in `1..=86400`, distinguish absent and rejected saved values using
the default of 180 seconds, and preserve typed schema/home/read failure before admission. Retain
one resolved timeout per operation, including callers joining it. Fixed caller policy remains
available. Verify manual and lifecycle admission, mid-turn Apply, retained deadlines, invalid-value
feedback and settings failures with focused integration tests and independent completion review.

Blocked during real lifecycle verification: ordinary terminal publication advances the durable
represented prefix, but the returned projection retains its establishment lineage. Lifecycle
compaction compares those different proofs and returns `AuthorityMismatch` before timeout
resolution. Independent review confirmed this existing production contradiction; a focused probe
observed an empty establishment tail versus the completed turn's represented tail. See the
[binding-prefix failure record](failures/syndic-phase9-binding-prefix.md). The Operator authorized
the separate Phase 350 prerequisite; resume this timeout phase after its acceptance.

The timeout implementation remains unaccepted and uncommitted. Production checking passed; the
corrected real manual-admission/retained-deadline test and settings scalar matrix passed 2/2.
Lifecycle timing and typed-settings-failure acceptance remain blocked by the earlier proof check.

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
