# Scope

**IMPLEMENTATION HOLD — the entire current Beryl-home rework is blocked by Operator instruction
on 2026-09-17 until the Operator decides the CAS direction and explicitly releases the hold after
the affected design, rework tracker and implementation plan are reconciled.** This includes
non-CAS implementation, integration, mounting, cleanup and cutover work from that rework; earlier
continuous-implementation authorizations below do not override the hold. Research, evidence
collection and investigation-plan maintenance remain authorized. A generic request to continue
means continue the investigation, not resume the blocked implementation.

The active priority is the Operator's 2026-09-17 investigation of replacing CAS with a Beryl-owned
agent runtime over subscription-backed Responses. The research phases below gather decision evidence;
they do not authorize production replacement. Prior CAS implementation phases are suspended during
this investigation. Existing design remains the comparison baseline, including its CAS-only
boundary and delegated-runtime non-goals; proposed replacements must be identified explicitly
before later design changes and implementation planning. No new-thread handoff is needed.

Investigate personal ChatGPT Pro only, with direct OAuth preferred and an auth-only official-client
helper as a fallback to assess. Do not assume Platform API billing or feature availability applies
to the subscription endpoint. Preserve the Operator's central constraint: incremental parsing is
acceptable, but potentially large payloads must not need RAM retention or temporary disk spill
while waiting for routing/type metadata. Distinguish final owned storage from staging used to
wait for identity. If a required path inevitably violates that constraint, stop and explain the
concrete path to the Operator before designing a workaround.

Use the [initial assessment](memory/topic/responses-agent-runtime/beryl-feasibility.md) and its
pinned-source/live-probe notes as starting evidence. Preserve findings in focused memory notes;
keep sequencing here. For each material question, record the applicable Beryl requirement,
source identity/date, evidence strength (documented, source-derived, observed, inferred or unknown),
failure example, architectural consequence, alternatives and remaining verification. Public API
documentation, Codex subscription behavior and OpenCode behavior are distinct evidence surfaces.

Operator clarification: claims about live subscription-service behavior require direct requests
and inspection of direct responses, independent of CAS/Codex execution and decoders; mocks and
source inspection guide experiments but cannot substitute for that evidence. Use OpenCode as the
accepted practical precedent, without treating it as a service guarantee. Probe subscription
endpoints only, sequentially and with bounded cost; no Platform calls, scanning, fuzzing, stress
tests or access-control bypass. Stop on security challenges or denied access and respect rate
limits. Do not compete with the active client's refresh-token rotation.

Expand this plan when discovery reveals a new independent research boundary, before investigating
it; revise affected dependencies and keep only active/near-term phases detailed. Continue research
across accepted evidence phases without repeated permission requests. A phase may close with an
explicitly justified unknown and proposed handling; lack of a public guarantee alone is not a
technical blocker. Stop when remaining uncertainty cannot change a material architecture choice,
or is explicitly assigned to bounded implementation validation or Operator decision. Do not expand
into speculative feature parity outside Beryl's required envelope.

Prefer existing local evidence, pinned primary source and targeted probes. Every live experiment
must have an explicit question, bounded duration/output/concurrency, synthetic inputs, result
classification and cleanup. Read-only source comes before effectful probes. Do not install software,
revoke credentials, change subscription/account settings, or exercise external side effects merely
to test a hypothesis. Keep secrets and raw private traces out of durable notes. Any disposable
multi-step harness must follow the Rust-only automation policy. Avoid repetitive model prompts as
large-payload fixtures: the preceding attempt did not terminate its input as requested.

Research completion means a requirement-to-evidence coverage review, recommendations with concrete
ownership and lifecycle alternatives, a complete change/removal/new-work inventory, explicit risks
and unresolved decisions, and a proposed design-authority update map. Review the proposal against
contradictory evidence and failure scenarios before recommending adoption. These evidence gates
do not substitute for later implementation verification or change existing production guarantees.

The following earlier implementation context is retained for reconciliation after the investigation.

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

# Phase 490: Assess Storage And Product Integration Changes (finished)

The reviewed [package/storage/product inventory](memory/topic/responses-agent-runtime/storage-and-product-changes.md)
preserves durable admission/history and distinguishes CAS removals from new context/effect custody.
Digest-addressed sidecars need a separate final-resource decision; canonical text already uses
bounded indexed records. Existing-home conversion and executable integration are not proven.
Reviewed preceding evidence covers [streaming](memory/topic/responses-agent-runtime/ordering-and-bounded-streaming.md),
[execution](memory/topic/responses-agent-runtime/durable-execution-and-recovery.md),
[context](memory/topic/responses-agent-runtime/context-and-compaction.md),
[tools](memory/topic/responses-agent-runtime/local-tool-host-options.md),
[integrations](memory/topic/responses-agent-runtime/configuration-and-integrations.md) and
[scheduling](memory/topic/responses-agent-runtime/scheduling-and-branches.md).
Review corrected uncertain-delivery terminalization and scoped the sidecar constraint precisely.
These are accepted research reports, not adopted design or production verification.

# Phase 491: Assess Operational And Maintenance Requirements (wip)

Acceptance boundary: an operations/supportability assessment. Cover Rust HTTP/auth dependencies,
proxy/TLS/network behavior, credential protection, diagnostics/redaction, account/quota errors,
model/service drift, dependency licensing, packaging/updates and supported operating systems.
Identify useful content-free observability, capability checks and maintenance ownership without
turning hypothetical risks into new product requirements. Compare the cost of direct ownership
with keeping a narrowly scoped external auth helper.

# Phase 492: Design The Decisive Verification And Quality Experiments (wip)

Acceptance boundary: an evidence-gap-driven experiment specification and results for reasonably
bounded probes needed before architecture selection. Include coding-agent task quality, tool
correctness, long conversations/compaction, concurrency, memory growth, cancellation, failure cuts
and replay where earlier phases expose material uncertainty. Separate synthetic decoder evidence
from live service behavior and end-to-end quality. Set explicit budgets/stop conditions and reuse
earlier evidence; defer expensive nondiscriminating measurements with reasons. Add prerequisite
research phases if experiment design reveals a new material dependency.

# Phase 493: Synthesize Architecture Alternatives And Replacement Scope (pending)

Acceptance boundary: a recommendation comparing feasible ownership boundaries, dependency reuse,
direct transport and any justified auth-helper option against the requirement inventory. Provide
candidate component/dataflow/state-machine descriptions, the actual simplifications and new work,
critical dependencies, scope choices, maintenance burden and evidence-based effort ranges where
possible. Map proposed changes to owning root/feature/system/package docs and the active rework.
Describe a clean replacement and existing-data strategy; do not prescribe compatibility layers or
convert proposals into approved design through this research phase.

# Phase 494: Review Coverage And Present The Architecture Decision Package (pending)

Acceptance boundary: a reviewed decision package that traces every required capability to evidence,
a recommendation or an explicit unresolved choice. Challenge unsupported equivalences, omitted
CAS behavior, unsafe replay, hidden buffering and overclaimed resource bounds. Record blockers,
acceptable bounded failures, deferred validation and confidence separately. Present concrete
Operator decisions and the proposed design-update sequence. Only after those choices are accepted
and controlling design is reconciled may replacement implementation phases be derived; old CAS
phases must then be retired or reconciled rather than resumed automatically.

# Phase 479: Connect Outage Capture To Failed-Service Retirement (pending)

Route ordinary durable-store failures through the accepted outage buffer and admission fence,
preserving exact active-target custody and bounded teardown without transferring buffered facts
or process-local authority to replacement. Verify failure, overflow and retirement cuts before
accepting this non-GUI integration.

Blocked on 2026-09-17: the accepted buffer requires complete route-qualified facts, while the
provider stream supplies fragments before its route and the ingester terminates on persistent
store failure. No outage normalization/capture owner bridges those boundaries. This cannot be
completed as integration of accepted components alone. Specify bounded unpublished observation
custody, missing-route loss attribution and capture shutdown before deriving its prerequisite
implementation phase. Preserve the durable admission fence and reconciliation custody. See
[outage ingress readiness](failures/outage-ingress-readiness.md).

Operator-directed investigation on 2026-09-17 found the installed CAS is now 0.154.0 and its
matching release source still serializes lifecycle item content before thread/turn IDs. Decoder
reordering alone cannot provide lossless route-first capture without retaining preceding content.
Implementation remains stopped as requested. See the
[current wire-order evidence](memory/github.com/openai/codex/commit/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/lifecycle-route-wire-order.md).
The earlier scratch-owner suggestion is not approved; resolve the producer ordering constraint
before prescribing another buffer.

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
