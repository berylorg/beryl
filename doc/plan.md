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

# Phase 727: Establish Mounted Clipboard Admission Readiness (finished)

Accepted the Operator-approved [authority resolution](audits/composer-marker-feedback/clipboard-readiness.md#subsequent-authority-resolution)
with independent architecture review and documentation checks. Native acknowledgement/read bounds,
compact private-source expiry, foreign allocation, atomic origin admission fencing, captured paste
and streaming sidecar ownership are defined. Missing production APIs and actual clipboard workflow
qualification remain the implementation boundaries below. No native Beryl was launched.

# Phase 728: Implement Checked Native Clipboard Acquisition And Acknowledgement (wip)

Implement the owning GPUI checked Windows read/write boundary for the composer consumer. Preserve
caller byte ceilings before allocation, complete text/metadata acknowledgement, exact snapshot
sequence and typed unsupported/native failure. Qualify native ownership and format publication in
an isolated harness, injected write/read/close failures, exact-fit/one-over and malformed inputs;
independently review unsafe memory, complete acknowledgement and bounded release. Publish the
accepted fork revision and qualify Beryl's canonical dependency composition before consuming it.

The native implementation is owned in the GPUI fork under its root plan; Beryl remains the
consumer/integration owner. Use the existing LLVM, one-job, no-normal-debug and nonincremental
verification envelope. Do not launch native Beryl. Preserve unrelated source and widget-spec work.

Resolved on 2026-10-04: independent review found that Windows native image allocation may
exceed encoded payload length. The attempted exact-size requirement refuses supported images;
the Operator approved the [private companion correction](failures/checked-clipboard-image-representation.md).
Implement the fork-owned contract and qualify padded/nonaligned payloads, malformed companion,
complete companion publication and allocation limits. Preserve the unaccepted fork work;
do not publish its source revision or update Beryl dependency pins. The separate
[test preservation correction](failures/clipboard-qualification-preservation.md) is independently
accepted and included in the focused verification below. Native qualification and canonical
dependency composition remain required before acceptance and publication.

Current milestone: the approved image correction passed independent review and focused
deterministic verification. The earlier `OpenClipboard` access denial cleared during read-only diagnosis.
See [qualification evidence and resume command](audits/composer-marker-feedback/checked-native-clipboard.md).
The Operator actively uses the clipboard; no further qualification may acquire or modify it.
The shared test was replaced with a create-only private window station/desktop. Independent review,
locked metadata, focused Cargo check and all 20 current deterministic cases pass; analyzer restart succeeded.
Native execution remains blocked on named-station creation permission in the unelevated agent
process. The reviewed command in the evidence runs only the private case from an Administrator
terminal, with no clipboard preparation. Complete that native gate before publishing source or
aligning canonical dependency pins. No Beryl GUI was launched.

# Phase 729: Implement Eligible Private Composer Copy And Cut (pending)

Mount the one process-owned compact private source through bounded streamed provenance and checked
native copy/cut. Authenticate local/foreign candidate and exact cut sources through Syndic-derived
assignment; qualify token publication/promotion, expiry, clipboard failure, no deletion before
acknowledgement, exact one-step cut history and capacity/stale/disposal release. Complete independent
semantic review. This boundary does not accept mounted paste merely by exposing source selectors.

# Phase 730: Mount Captured Atomic Composer Paste (pending)

Consume paste and clipboard-limit events with exact Notifications feedback and captured pending
ownership. Implement bounded text/image/private acquisition, streaming sidecar admission and one
general host mutation with evidence/replay, cancellation and settlement. Qualify actual mounted
paste, local/foreign identity, stale selection, ordinary refusal and ambiguity, one-step undo/redo
and resource release; independently review the complete consequential boundary.

# Phase 731: Qualify Large-Draft Clipboard Refusal Preservation (pending)

Qualify actual mounted cut, later size/capacity/storage-refused paste and usable cut undo on large
drafts, with exact nonresident marker/selection/history preservation, eligibility and bounded
repeated-operation/disposal custody. Credit prior direct-marker evidence only within its unchanged
scope; accept the complete tracker item only after the clipboard workflow and independent review pass.
