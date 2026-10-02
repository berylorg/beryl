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

# Phase 705: Establish Failed-Home Resident Recovery Custody (finished)

Failed-home resident custody, fresh candidate publication with supplied real typed proofs and
protected same-editor/history reconstruction passed independent review, focused regressions and
isolated canonical checks. See [acceptance evidence](failures/ordinary-close-recovery.md#failed-resident-capability-acceptance).
Fresh marker preparation remains phase 706; ordinary command integration remains phase 595.

# Phase 706: Prepare Marker Evidence Through Recovery Candidate Access (wip)

Complete the candidate marker-preparation prerequisite identified by the bounded phase 705 audit.
The [image-assets system](systems/image-assets/design.md#reference-and-label-evidence),
[Syndic candidate evidence contract](../crates/syndic-storage/doc/design-draft-storage.md#candidate-marker-evidence-preparation),
[State Asset contract](../crates/beryl-state/doc/design-assets.md) and
[app composer authority](../crates/beryl-app/doc/design-catalog-and-composer.md) control this boundary.
Phase 705's save primitive consumes supplied typed proofs; it does not establish this preparation.

- Add candidate counterparts sharing Syndic's authenticated seal begin/advance/status/terminal
  preparation and State's original-staging-authority Building/completion reads. Preserve existing
  atomic contributions, schema, revision fences, bounded pages and exact terminal semantics.
- Retain unfinished process seal flight authority and independent outcomes before old graph
  retirement; drive fresh or resumed candidate preparation without old service capabilities or
  another flight-capacity owner. Refusal and uncertainty preserve custody and block publication.
- Verify real marker-changing resident preparation/save, original-flight recovery, missing or
  substituted authority, commit/noncommit/uncertain cuts, cancellation/supersession, stale delivery
  and resource limits. Require independent persistence/lifecycle review and canonical checks.

Current milestone: implement candidate Asset/marker preparation and retained process flight custody.

No schema or publication-algorithm blocker was found. Current ordinary-only seal/read access and
non-driving-flight disposal cannot substitute for this capability. Phase 595 consumes it after
acceptance; its ordinary command mounting remains separate.

# Phase 595: Compose Ordinary Window Close And Application Exit (pending)

Mount enabled ordinary Exit and native close commands through the persistent running owner under
[main-window authority](features/main-windows/design.md) and the
[app lifecycle](../crates/beryl-app/doc/design-shell-lifecycle.md#window-detachment-and-process-shutdown).
The accepted coordinator, exact native confirmation, serialized window admission, resident draft
flush, session publication and interrupted-Exit recovery remain reusable. Phase 703 supplies the
final-teardown consumer and blocked-shutdown outcome; this phase owns ordinary command routing.

Preserve nonfinal close as draft/session settlement followed by view and claim release, without
stopping background work. Serialize final-window designation with construction and other closes.
Final ordinary close uses the all-work barrier but records an empty restore set; dedicated Exit
preserves the complete layout. Both require exact native/auxiliary settlement before normal quit,
and retain the accepted recoverable-before-teardown versus blocked-after-teardown outcomes.

Resolve current feature gates before exposing enabled controls, keep native default destruction
vetoed until owned cleanup succeeds, and require fresh activation after cancellation or failure.
Derive focused integration cases and independently review the complete ordinary command boundary
before acceptance. Executable entry composition remains phase 596.

Next milestone: verify the connected native close callback and Exit toolbar through production
running-owner construction, including windows created after startup. Implementation also connects
exact restored-claim rebinding after interrupted close and retains newly created shells in the
complete process-owned set. Completion requires actual command delivery, exact draft and session
settlement, and owned native removal; helper-only admission does not accept this mount.

- Mount current availability before enabling Exit; retain one command consumer across cancelled
  or coherently failed attempts, with fresh activation and duplicate coalescing.
- Serialize nonfinal close, final-window designation and construction. Verify nonfinal close
  preserves background work, failures preserve the resident and claim, and final close publishes
  an empty restore set while dedicated Exit preserves all placements.
- Exercise native default-close veto, confirmation cancellation and duplicate delivery, feature
  gates, durable failure, exact native settlement and successful termination through focused
  integration tests. Run applicable app checks and canonical dependency validation, then obtain
  independent lifecycle/persistence review before acceptance.

Consume phase 704's exact persistence boundary for reported-failed ordinary closes. The Operator
approved this recovery direction on 2026-10-02; the former readiness blocker is recorded in
[the evidence](failures/ordinary-close-recovery.md). Implementation is connecting the ordinary
command boundary to retained running-owner custody and the accepted exact recovery transition.

Current checkpoint: production routing and recovery are implemented but not accepted. Eight app
session cases and focused mounted close/Exit, confirmed durable-work shutdown, small-draft
cancellation and unavailable-origin cases pass. New Window now publishes on the unchanged WinRT
stack after bounded creation-frame corrections. A broader run passed 19 of 28 cases, including all
ten shared activation cases. Corrected nonfinal resident fencing and healthy-conflict notice
admission then passed both focused cases, preserving background work and permitting fresh close.

Created-window close/Exit and the corrected nonfinal toolbar presentation now pass. Thirteen of
21 mounted cases have passing per-case evidence; seven recovery cases and genuine dirty editing
remain unresolved. Five existing dispatch regressions pass, but the real enabled-editor dirty case
still overflows the worker stack. No final combined run or phase acceptance is claimed.

Prerequisite identified on 2026-10-02: independent readiness review confirmed that surviving residents cannot enter
ordinary close admission after home failure, and existing retirement/reconstruction accepts only
a proven clean checkpoint. It cannot represent a failed dirty flush. The proposed separate
[failed-home resident recovery prerequisite](failures/ordinary-close-recovery.md#failed-home-resident-recovery-readiness)
was approved by the Operator and is now phase 705 under the updated app/storage contracts.
Preserve current source/test progress and existing clean validators. After that prerequisite and
the dirty worker-stack correction, run combined native/session/affected regressions and final
independent/canonical acceptance checks. Native verification retains an external per-case deadline.

# Phase 596: Reconstruct The Executable Bootstrap (pending)

Connect normalized fixed-home configuration, complete private domain registration, immutable
service/window inputs and required startup/running ownership at the sole executable entry.
Preserve the explicit zero-window GPUI lifetime, diagnostic-target protocol, required unavailable
feature states and fatal-hook ordering; remove the intentional placeholder only at this boundary.
Phase 382 owns production crash-reporter mounting and its isolated process evidence.

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
