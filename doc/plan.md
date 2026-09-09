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

# Phase 366: Dispose Permission Slots Before Ingester Completion (finished)

Accepted approval-slot disposal before caught-panic ingester completion and worker release.
Joined pending and reserved slot tests prove closure before later cancellation and capacity reuse.
Independent semantic review, production compilation and 49 selected stop, approval, worker and
terminal checks passed. Peak guarded job memory was 2.07 GiB; all children exited and temporary
directories were removed. The [custody evidence](failures/cas-stop-custody-worker-reservation.md#permission-disposal-prerequisite)
records the accepted prerequisite.

# Phase 363: Publish Revision-Bound Stop And Interruption Custody Facts (wip)

Expose bounded healthy-generation stop and permission-interruption facts from their existing
coordinator, broker and driver owners. Distinguish durable settlement, registry membership and
remaining driver custody; map removal or response completion alone does not prove work release.

- Page existing retained stop records without another historical catalog; distinguish abandoned
  records from live work. Observe removed-but-owned stop identities until exact settlement and
  release, including terminal consumption during backend interruption and nondispatch settlement.
- Preserve compact exact permission-obligation identity through preparation, broker installation,
  driver transfer, joined admission without a primary owner, invalidation and disposal. Reuse the
  existing app obligation custody; retain no backend payload or usable capability in observations.
- Derive removed-owner bookkeeping from the accepted stop reservation lifetime, covering one
  election-owning stop and one post-election driver tail per retained connection reservation.
  Permission observation still requires the driver-held and reserved/pending obligation intervals.
- Publish owner/home/service-bound revisions and count/byte-bounded pages; fence every exposed
  mutation and reject foreign/stale cursors, counter exhaustion and closed/unavailable generations.
  Queries never prune, admit, acknowledge, settle, dispatch or acquire operation authority.
- Verify exact handoff and terminal-before-driver-release intervals, joined permission handling,
  disposal, stable repeated reads, page limits and stale/foreign rejection; run focused stop and
  approval regressions, production compilation and independent semantic review with bounded cleanup.

Reservation-lifetime acceptance establishes the stop custody prerequisite. Include the driver's
post-settlement unbind interval as well as the primary owner itself. Do not omit tails, introduce
an unbounded observer catalog, or substitute an arbitrary observation cutoff for a derived bound.

Accepted permission-slot completion ordering establishes the worker-derived permission bound.
Stop and permission observation can now resume from the accepted custody prerequisites.

Existing volatile-stop fallback begins only after persistent failure closes the healthy generation;
these source pages reject that generation and do not claim to inventory post-failure cleanup.
Compaction/continuation observation and complete control-page composition remain separate boundaries.

# Phase 364: Publish Revision-Bound Compaction And Continuation Custody Facts (pending)

Expose compact exact facts through compaction queue/driver work, completion and target cleanup,
including the removed local operation before its driver releases command custody. Preserve
continuation identity and pending/cancelled state when the accepted intent moves out of the stop
registry into durable settlement and reconciliation. Derive bounds from all existing owner stages,
not queue/worker counts alone; observe without retaining capabilities or changing execution.

# Phase 361: Compose Revision-Bound Control Work Facts (pending)

Compose the accepted stop/interruption and compaction/continuation sources into bounded exact
control pages. Revalidate every contributing source and preserve handoff/settlement visibility
without creating another owner, admitting work or changing cancellation, response or shutdown
authority.

# Phase 358: Project Revision-Bound Process Work Inventory (pending)

Combine compact durable sources, exact process execution facts and the accepted attention pool into
bounded recent-first pages and a consistent logical thread count. Cover all declared work states,
deduplication, source drift, generation loss and observation without execution or acknowledgement
effects. Keep authority custody, view integration, picker mounting and shutdown admission separate.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Consume the accepted work inventory for shutdown and Running threads
without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Implement one admission fence and exact all-work convergence boundary shared by final-window close
and explicit Exit. Preserve accepted queues, prevent successor dispatch, retain exact pending and
noninterruptible targets through terminal history or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.
