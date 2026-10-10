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

Operator selected best-effort minimized GUI startup in the existing Windows session on
2026-10-10. Request minimized startup where practical, preserving native activation and focus
assertions; no further launch or isolation framework is required.

Continue in the current conversation thread; do not request new-thread handoffs.

On 2026-10-08 the Operator explicitly instructed autonomous continuation through technical
prerequisites, stopping only for a blocker requiring product-level attention. This direction
releases the New Thread mounting pause below; derive and accept its missing capability before
ordinary confirmation mounting, without substituting a multi-command creation/activation flow.

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

# Phase 746: Define Context Observation And Manual Compaction Mounting Contracts (finished)

Accepted bounded context provenance/domains/continuity and weak manual admission/late feedback
contracts through independent semantic review. Notifications owns protected shared feedback FIFO;
pinned quota remains unavailable without a qualified model association. Documentation-only checks
and index reconciliation passed. See the [closure record](audits/context-compaction-mount-readiness.md#contract-closure).

# Phase 747: Mount Exact Selected-Thread Context Status (wip)

Implement and mount the qualified bounded usage/quota observation path into the existing status
cell. Accept authentic same-thread values, exact optional quota matches, Unknown/omission and
selection/publication/retirement fencing through real ingress and native mounting evidence.
Status reads must not start turns or mutate conversation history.

The production entry is Backend's ordered provider classifier/sink through the registered
foreground projection and coherent selected-status worker to the native context cell. Implement
fixed-size exact usage and unavailable quota observations under the owning
[Backend domains](../crates/beryl-backend/doc/design-provider-stream.md#context-observation-domains),
[CAS-live continuity/election](systems/cas-live-syndic-transcript/design.md#context-compaction) and
[App weak-worker contract](../crates/beryl-app/doc/design-live-control.md#compaction-and-continuation).
The pinned producer has no qualified model/metering association: coincident strings and labels
must not make quota available. Preserve the 300/10,080-minute supported domains without adding an
unqualified account read. Mount context observation only; manual command effects remain in 748.

Verify malformed counters and missing/nonpositive windows, ordered replacement and sparse quota
invalidation, multi-projection interest disagreement, authenticated ordinary revision continuity,
binding/session replacement, stale selection/publication and retirement. Exercise real ingress and
native cell rendering plus no history/start effect; focused Cargo checks and behavior tests use
the accepted root build policy and sole root Cargo permit. Preserve model-mount evidence and
best-effort minimized native launches. Completion requires independent review of the whole mounted
boundary, rather than acceptance of a parser or presentation helper alone.

Initial milestone: owning contracts are accepted; production source edits have not started.
No unresolved product decision blocks this phase. Phase-746 changed no source or manifest and
started no Cargo or GUI process. Root owns shared service/authority integration and Cargo.

# Phase 748: Mount Exact Manual Context Compaction (pending)

Mount the bounded Compact menu through the qualified opaque capability and existing coordinator.
Accept idle-only atomic admission, preserved drafts and queued-work precedence, actual sole-driver
dispatch, bounded rejection/uncertainty feedback, timeout with exact late settlement, selection
replacement and teardown through the complete production path.
