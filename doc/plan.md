# Scope

Implement the Operator-approved background-thread behavior under the revised
[conversation-thread](features/conversation-threads/design.md),
[main-window](features/main-windows/design.md),
[CAS-live](systems/cas-live-syndic-transcript/design.md), and
[app package](../crates/beryl-app/doc/design.md) authority. The process owns execution; switching
views and closing nonfinal windows preserve it. Final-main-window close and explicit Exit use
confirmed process-wide graceful shutdown, with their distinct restore-set outcomes.

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

# Phase 324: Own Scheduled Execution Sessions In The Process (wip)

Implement the concrete process-owned `ScheduledOrdinaryExecutionProvider` for already-admitted
sessions under the app live-projection and CAS-live ownership contracts. Process composition
supplies typed request policy, assets and dynamic-tool authority; this phase does not launch
runtimes or mount windows. Reuse the existing non-cloneable execution lease and scheduler flight,
worker, connection and generation validation rather than duplicate them in another run object.

Reserve a bounded slot keyed to exact healthy home/service generation, thread and full execution
binding. Derive retained-slot capacity from configured worker capacity and per-connection worker
permits; count available, checked-out and retiring slots until definitive release. Decline missing,
busy, stale or full authority without consuming durable backlog or retaining an unbounded waiter.
Transfer each session once into the existing execution lease and return it only to its still-current
owner. Shutdown, retirement and generation loss fence issuance and return; release must not revive
a closed slot. Availability wakes the existing scheduler through a typed bounded notification.
The owner and checkout retain no GUI state or view-lifetime dependency; actual view detachment
and reattachment verification belongs to the following composition boundary.

Verify the production provider through the existing real scheduler: independent exact thread and
binding slots, durable promotion and terminal capture without a view, busy decline, return/wake
without duplicate dispatch, generation and asset rejection, service shutdown joining checked-out
work, late return after the provider-owner fence, saturation and repeated release with bounded
retained counts. Preserve
existing authority tests where they already prove the shared mechanism. Run focused nextest and
locked app checks, formatting and whitespace validation, then obtain independent semantic review
of authority transfer, return races, resource release and evidence before acceptance.

Current milestone: revised target authority is integrated; source readiness and lease boundaries
are reviewed. Implement the production checkout provider next.
The production provider seam and existing scheduler tests are identified; no implementation from
the superseded noninterruptible window-close phase is retained as pending work.

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
