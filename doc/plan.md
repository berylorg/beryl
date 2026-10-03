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
On 2026-09-26 the Operator reaffirmed that lock-order corrections and other bounded technical
prerequisites must proceed autonomously; only architectural or product-scope issues warrant a pause.

Continue in the current conversation thread; do not request new-thread handoffs.

At every phase boundary, explain any blocker directly to the Operator and suggest concrete next
steps. Evidence links supplement that explanation rather than replacing it.

Apply the [simplification audit](audits/code-simplification/report.md) selectively within each
owning acceptance boundary. It is evidence, not authority or a second plan; preserve its baseline
estimates and record actual dispositions only for accepted selected findings. Keep bounded supporting simplifications within their behavioral acceptance boundary; separate
distinct outcomes or material scope growth before work begins. Keep
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

# Phase 712: Expose Generation-Bound Exact Stop Worker Access (finished)

Accepted the weak, generation-bound worker capability through published window services, sharing
the service's sole stop implementation and preserving feedback after request processing begins.
Canonical metadata, combined app/executable checks, 81 passing cases and independent semantic
review passed; see [acceptance evidence](failures/executable-bootstrap.md#exact-stop-worker-access-readiness).

# Phase 711: Mount Exact Status Stop Controls (wip)

Connect the accepted exact eligibility and request feedback to the production main-window
turn segment and operations popup under the Status Line contract. Preserve selection, home
and service fences, worker-only service access, feedback-only disabled controls and exact
popup eligibility. Qualify actual mounted activation and lifecycle transitions before the
Notifications fallback boundary.

Use [exact stop controls](features/status-line/design.md#turn-state-view-count-and-stop-controls)
and the [status GUI mount](features/status-line/gui.md). Revalidate authoritative selection and
service ownership before applying asynchronous results. Verify duplicate activation, stale
responses after switching or retirement, durable nondispatch with fresh eligibility, volatile
nondispatch without retry, terminal popup closure and disposal without altering execution.
Reuse the accepted service tests; require mounted lifecycle evidence and independent review.

Active milestone: replace the production main-window host's empty status-line slot with the
selected-operation turn segment and its anchored single-command popup. Consume the accepted
service capability off the GUI thread and qualify the mounted path; legacy shell controls do not
establish this acceptance. Other status feature operations remain deferred. Preserve the current
bundled widget's pointer-only segment interaction and the menu's own focus and dismissal contract.

Readiness finding on 2026-10-03: the accepted exact-stop methods require the process-owned service, but
production window inputs expose no sendable worker capability for those methods. Calling them
from GPUI violates the worker-only service contract. Phase 712 accepted the Operator-authorized
worker access prerequisite; consume it through published window services for this mount. See
[readiness evidence](failures/executable-bootstrap.md#exact-stop-worker-access-readiness).

# Phase 709: Mount Exact Stop-Feedback Notices (pending)

Complete the next bounded Notifications contributor from the active shell checkpoint. Follow
[exact stop feedback](features/notifications/design.md#exact-stop-feedback-notices),
[status-line feedback](features/status-line/design.md#turn-state-view-count-and-stop-controls), and
[exact soft stop](systems/cas-live-syndic-transcript/design.md#exact-soft-stop).
Connect the prerequisite exact feedback projection to the sole notice arbiter when its popup anchor
cannot safely retain progress or outcome. Preserve opaque request identity, priority, bounded
updates, persistent waiting state and resolved dismissal without issuing another interruption.

Verify popup/notice eligibility transitions, repeated updates, stale request or selection changes,
durable and volatile nondispatch, interrupted completion without error payload, and authority
loss. Notices must not invent retry eligibility, terminal completion or a durable operation;
dismissal must not mutate thread or execution state. Qualify the real mounted contributor with
focused lifecycle tests and independent semantic review, reusing accepted stop and arbiter
evidence. Keep audio and other unavailable-feature notices separate.

Depends on phases 710 and 711. The Operator authorized their separate prerequisite boundaries
after the 2026-10-03 readiness finding; notice-only integration remains insufficient.
