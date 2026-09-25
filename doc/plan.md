# Scope

Operator selected continued use of unmodified CAS on 2026-09-23, authorized necessary buffering
for its payload-before-route ordering, and requested resumption and completion of the Beryl-home
rework. The whole-rework implementation hold is released by that explicit direction, subject to
ordinary phase design-readiness and acceptance gates. CAS replacement is deferred until this
rework is fully finished; the completed [research decision package](memory/topic/responses-agent-runtime/architecture-decision.md)
is retained as evidence, not current implementation scope. Its proposed subscription runtime,
tool host, model, authentication, recovery and integration selections do not change the CAS target.

The [root design](design.md), [bounded-resource system](systems/bounded-resource-dataflow/design.md),
[CAS-live system](systems/cas-live-syndic-transcript/design.md#durable-store-outage-buffer) and
[app capture contract](../crates/beryl-app/doc/design-live-capture.md) now permit necessary typed
pre-route retention. Healthy storage keeps bounded-page Syndic staging; outage capture uses
bounded unpublished assembly and existing qualified retention, with explicit loss attribution,
closed durable admission and full disposal before replacement. No CAS producer change or further
replacement-runtime investigation is a prerequisite. The existing release admission remains in
force; the 0.154.0 ordering investigation does not upgrade the pinned 0.146.0 provider contract.

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

Operator clarified that bounded technical prerequisites are to be planned and implemented
autonomously. Escalate only a material architectural change, impending scope explosion, significant
risk, or a concrete decision needing Operator input; a discovered prerequisite alone is not a stop.

Continue in the current conversation thread; do not request new-thread handoffs.

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

The Operator authorizes defining and reconstructing target executable bootstrap. Establish the
private home candidate and its typed consumers, explicit recovery access and prepared service
composition before restore-set and native process-entry integration. Preserve each separate
acceptance boundary and the intentional removal gaps; complete registration alone does not accept
the service graph or visible startup.

# Phase 465: Implement The Private Exact Terminal Repair Adapter (pending)

Only after new exact evidence proves the route and its implementation prerequisites are ready, implement the
bounded one-request adapter with exact target/capability admission, streaming closed-item output
and typed failure. Verify no successor race, cursor follow, retry, alternate history route or
partial publication; keep durable dispatch custody and runtime mounting separate. Remaining
recovery and branch components continue to feed bounded phases from the rework tracker before 423.

Blocked on 2026-09-16: exact source does not satisfy the complete semantic item and identity proof
for Beryl's supported thread population. The existing design requires unavailable repair. Resume
only after separately authorized prerequisites prove the route; do not substitute a history
fallback or silently select a new history mode. The Operator approved proceeding with unavailable
repair and explicit-incomplete recovery; this conditional boundary does not block phases 467–470.

# Phase 566: Coordinate The Bounded Complete Restore Set (finished)

Accepted complete bounded worker preparation, exact empty-session branches and original-command
settlement with typed retained outcomes for unavailable local finalization. All 13 new cases and
74 affected regressions, app checks and independent review pass;
[evidence](failures/target-bootstrap-composition.md#complete-restore-set-coordination).

# Phase 571: Apply Prepared Native Window Placement (wip)

Implement bounded startup placement for the prepared shell set: reachable saved geometry and
monitor selection, saved-desktop restoration through the documented Windows boundary and the
approved current-desktop fallback. Preserve hidden preparation, exact native identity and failure
disposal before complete-set publication. Verify changed topology, missing desktop and failed
desktop restoration without automatic desktop switching or changing another member's placement.

Resolve saved outer logical geometry and monitor/work-area hints into bounded prepared facts off
the GUI thread. Preserve fixed initial normal/maximized state through hidden construction. Apply
the saved virtual-desktop identity through documented Windows integration while retaining exact
native ownership; missing, removed or unavailable desktop restoration uses the current desktop.
Verify reachability after topology/scale changes, extreme saved coordinates, hidden-state retention,
identity-safe failure disposal and independent per-window fallback. Keep complete-set interaction
gating/publication and process startup ownership in their following phases. Require native-boundary
evidence, affected shell regressions and independent review before acceptance.

Readiness finding on 2026-09-25: the owned GPUI fork interprets creation bounds as client
geometry and expands the native frame; Beryl saves outer logical geometry. Its Windows monitor
lookup also uses a transient enumeration index with an unchecked failure, and initial HWND DPI
can belong to the default monitor. Before placement integration, define a bounded fork prerequisite
for explicit outer-coordinate bounds and fallible prepared monitor identity/DPI validation. Keep
one rectangle, preserve existing client-coordinate semantics, and verify negative coordinates,
mixed DPI, taskbar workspace offsets, stale monitors and actual hidden normal/maximized windows.
Desktop application must retain the exact hidden native window until its COM worker finishes;
cancellation cannot release or recycle that HWND while desktop movement is in flight.

Operator clarified on 2026-09-25 that zed-fork does not need rag-rat because Beryl-owned
Markdown is not authored there. Keep this prerequisite's design and planning authority in Beryl;
do not create a fork documentation/indexing prerequisite. The unnecessary local fork setup was
removed. Its `MissingModel` report referred to an unregistered model in the new index, not missing
shared model files. This tooling issue does not block native placement work.

# Phase 567: Publish And Dispose The Complete Native Startup Set (pending)

Keep every prepared native member interaction-gated until whole-set publication succeeds; verify
reentrancy and later native failure dispose the entire attempted set while preserving durable
restore records under the approved transient-exposure exception.

# Phase 568: Compose The Native Startup Attempt Owner (pending)

Connect accepted service-graph, restore-set and native-publication components to one process
lifetime owner with serialized same-home Retry/Exit, exact failure surfaces and retained shutdown
custody. Preserve the separate process-entry fatal-hook mounting gate.

# Phase 382: Mount Crash Reporting At Process Entry (pending)

After target executable bootstrap exists, connect the accepted reporter and GUI boundaries before
ordinary storage/work startup. Verify reserved reporter mode cannot enter normal bootstrap,
ordinary startup installs fatal handling first, normal exit leaves no reporter, and a real
isolated application panic terminates its process while the report remains usable. No helper-only
or library-only evidence accepts this production mount; the current bootstrap removal gap remains
explicit until its owning rework checkpoint closes.

Blocked on 2026-09-15: `crates/beryl/src/main.rs` remains an intentional compile-error placeholder
for the later target bootstrap checkpoint. There is no ordinary executable composition root at
which to mount the accepted reporter and GUI services. Resume after that bootstrap boundary is
specified and reconstructed; do not substitute helper-only evidence or invent an alternate entry.
