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

# Phase 377: Serialize Mutation Observation With In-Memory Election (finished)

Accepted the weak home mutation observer and exact interval tokens. Mutation entry invalidates
prior reads; final in-memory election excludes mutation entry, and settlement wakes only after
command custody and locks release. Replacement, last-owner drop and generation disposal revoke
old tokens. The [early-return custody correction](failures/home-writer-early-settlement.md) is accepted.

Independent semantic review, 11 focused tests within 111 writer/proof/recovery regressions,
home-store/app production compilation, formatting and diff checks passed. All eight guarded jobs
and temporary directories were reclaimed. Scheduler adoption follows below.

# Phase 374: Recheck Idle Sessions On Ownership Release (wip)

The mutation-observation prerequisite has passed acceptance. Integrate its exact token
around required-work observation and final retirement election, and its writer-settlement wake
through the service-owned maintenance signal. Keep the previously failing real composer-admission
race as an acceptance requirement, including both orderings and exact original registration.
Existing idle-maintenance source and tests remain uncommitted and unaccepted until this boundary.

Feed bounded idle observations and conditional retirement through a distinct coalesced scheduler
maintenance wake. Relevant final view, preparation and live/control cleanup releases request another
inspection after their state becomes observable. Preserve dispatch/retry lane masks, exact runtime
binding, busy/stale refusal and generation fences; add no polling worker or retry loop.

The Operator-authorized [response-custody correction](failures/process-idle-response-custody.md)
has passed both prerequisite boundaries. Integrate the maintenance callback into the existing
process provider and scheduler signal, using non-owning work sources and the configured runtime
owner. Keep the full binding in the existing bounded interest entries and serialize the final
view check with acquisition. Separate in-memory retirement election from signaling and disposal.

Wire available-session, preparation, view, response and live/control release cuts after fact
publication. Verify idle disposal without later requests, required/loaded/checked-out survival,
same-binding view preservation and reacquisition races, retained completed response records,
unwritten final release, stop/compaction cleanup, stale sources and service-generation loss.
Prove maintenance-only wakes leave dispatch and retry masks closed. Run focused managed-runtime
and scheduler integration tests, required work/control regressions, production compilation and
independent semantic review. Production composition and graceful shutdown remain below.

# Phase 325: Own Running Work Independently Of Views (pending)

Compose process-owned execution interest across direct submission, scheduled input, compaction,
continuation and terminal-history work. Keep exact request routing and background attention under
their owning policies. Consume the accepted work inventory for shutdown and Running threads
without mounting a GUI per thread. Verify view-interest release and immediate
reattachment preserve the same live execution and capture, and all terminal/replacement paths
release required resources. Split any independently missing composition prerequisite before
activation; production runtime/provider composition remains explicit rather than inferred.

Composition resumes after the separate terminal-disposal, runtime-demand and idle-retirement
prerequisites and shared observation/idle-wake integration above pass acceptance.

# Phase 326: Coordinate Process-Wide Graceful Shutdown (pending)

Implement one admission fence and exact all-work convergence boundary shared by final-window close
and explicit Exit. Preserve accepted queues, prevent successor dispatch, retain exact pending and
noninterruptible targets through terminal history or authority-loss convergence, and return to
coherent windows on failure. Confirmation, final-window designation, durable restore mode and OS
close integration remain their subsequent rework acceptance boundary.
