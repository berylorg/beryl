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

# Phase 506: Implement The Bounded Pre-Inventory Observation Slot (finished)

Accepted connection-only assembly binding and one bounded open-or-sealed slot, with route bytes
and entries charged to the same limits. Ready handoff preserves qualified loss attribution;
pre-inventory eviction records sticky connection loss. Release, malformed input, exact limits
and generation isolation passed 36 focused tests and independent review. Generation-bound service
ownership and production capture remain below.

# Phase 500: Implement Failed-Store Ingress Mode And Retirement Cut (wip)

Implement nonwaiting durable-to-passive ingestion and the driver's failed-gate passive polling using
the accepted receive/cancellation primitives. Reuse bounded assembly and retention; one slot holds
an open or sealed unqualified observation while inventory is pending. Verify exact failure fencing,
reconciliation before acknowledgement, whole-observation loss across failure, slot eviction/gaps,
no effects or retries, and cancellation/join without provider completion. Use generation-bound
inventory outcomes and independently review custody before ordinary service mounting.
The service inventory publisher and shared-retention lifetime are mounted by phase 479. Preserve
explicit unavailable capture until that composition is present; helper tests alone do not accept
ordinary outage capture. The [driver-cycle diagnosis](failures/outage-ingress-readiness.md#driver-polling-blocks-the-ingester-only-transition)
remains evidence for the required nonwaiting protocol, not authority for an ingester inventory wait.

Blocked at readiness: a delayed steering echo requires the backend to obtain a replay source from
the ingester and verify content against failed-home reads. Its source-or-error interface cannot
consume a well-formed echo passively; rejection aborts decoding. Root inspection and independent
review confirmed the [passive steering seam](failures/outage-ingress-readiness.md#delayed-steering-echo-still-requires-healthy-replay).
Per repository instructions, stop before implementing a workaround. Define an explicit passive
correlation/loss outcome in the backend and app authorities, preserving healthy verification and
outstanding request outcomes, then replan its prerequisite before this transition. CAS stays unchanged.

# Phase 479: Connect Outage Capture To Failed-Service Retirement (pending)

Connect accepted outage-mode ingestion to ordinary store failure, admission fencing and failed
service disposal. Verify exact active-target custody, failure and overflow, cancellation, and no
buffer/connection transfer to replacement. The former producer-ordering blocker is resolved by
the explicit buffering exception; assembly and ingress-mode acceptance remain prerequisites.
Historical evidence remains in [outage ingress readiness](failures/outage-ingress-readiness.md).

# Phase 480: Specify Fresh Same-Home Recovery Composition (pending)

Resolve the concrete component boundaries for the backend-runtime system's ordered fresh-service
recovery protocol, using accepted candidate convergence and service ownership. Derive bounded
implementation phases for disposal, same-home reopening, supervisor attachment and publication
custody before branch services. Complete-stack publication and product mounting remain separate.

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

# Phase 423: Publish The Complete Initial App Service Graph (pending)

After every required service factory is independently accepted, compose and publish the complete
private graph with the same home generation, then release ordinary workers. Verify last-constructor
failure, cancellation, startup convergence and publication rejection with full cleanup ownership.
This integration cannot absorb missing service implementations or accept restored GUI visibility.
Theme preparation and candidate managed-session configuration are accepted.
Finish the remaining graph-factory inventory before activating this phase.

Prerequisite gap identified on 2026-09-16: the required durable-job coordinator is not implemented. The current
ordinary tool dispatcher explicitly refuses branch resolution; typed durable-job records and
read-only process-work inventory do not implement handoff recovery or execution. The rework gate
now admits non-GUI recovery prerequisites before branch handoff and complete graph publication,
separately from product mounting. Finish those service gates before activating this phase; do not
publish a partial graph or substitute an inert service. See
[factory readiness evidence](failures/target-bootstrap-composition.md#durable-job-factory-readiness).

# Phase 415: Specify Restore-Set Startup Composition (pending)

Resolve exact restoration custody, complete-set first visibility, threadless empty-session startup,
placement and native process-lifetime composition in owning authority, using the accepted session,
window and graceful-shutdown components. Derive bounded implementation phases before wiring the
ordinary executable. The [bootstrap readiness evidence](failures/target-bootstrap-composition.md)
identifies the remaining gaps without authorizing alternate startup behavior.

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
