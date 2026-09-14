# Process Shutdown Pending-Turn Completion

## Invalidated Assumption

Readiness review on 2026-09-13 found that the process-wide graceful-shutdown phase cannot yet
derive a complete acceptance rule for a durable pending turn whose provider command has not
dispatched. Reusing the existing exact stop barrier does not define completion for this case.
No shutdown implementation was attempted.

## Authority And Source Evidence

The [CAS-live shutdown contract](../systems/cas-live-syndic-transcript/design.md#application-shutdown-coordination)
requires the barrier to fence pending dispatch and preserve durably admitted pending turns, but
also requires every captured target to reach terminal-history or existing authority-loss
convergence. Its [startup contract](../systems/cas-live-syndic-transcript/design.md#startup-and-runtime-recovery)
allows proven-undispatched pending work to resume. The
[Exit contract](../features/main-windows/design.md#application-exit) says an already materialized
continuation must settle in the same barrier and requires all exact work's terminal-history
outcome. Neither authority explicitly classifies a preserved, proven-undispatched pending turn
as a completed shutdown obligation.

This is a reachable state: the accepted managed lifecycle-compaction test pauses after consumed
durable settlement with a `PendingTurn` gate and no successor dispatch. Its
[acceptance evidence](process-lifecycle-continuation-projection.md#accepted-continuation-correction)
also covers pending continuation recovery after storage reopen.

- [Stop admission](../../crates/syndic-storage/src/read/stop_admission.rs) classifies pending work
  as `StopAdmissionIneligibility::PendingTurn`, with no eligible exact provider stop target.
- [The existing stop barrier](../../crates/beryl-app/src/cas_projection/stop.rs), through
  `window_close_ineligible_status`, returns `Waiting` for that same pending turn.
- [Ordinary start settlement](../../crates/beryl-app/src/cas_projection/ordinary/execute/start.rs),
  through `finish_not_started`, cancels binding activation and returns `NotStarted`; it does not
  terminalize the durable pending turn.
- [Ordinary service shutdown](../../crates/beryl-app/src/cas_projection/service/shutdown.rs)
  closes admission and disposes services. It cannot substitute for a reversible graceful barrier
  that preserves capture and reconciliation until convergence.

Independent source review reached the same conclusion. The missing distinction might be intended
by the word "settle", but treating it as an exception would choose observable lifecycle and
recovery semantics without explicit authority. Keeping dispatch fenced while waiting for terminal
history provides no completion path for an untouched pending turn with no provider target.

## Authorized Course Correction

The Operator approved defining proven-undispatched durable pending work as safely preserved
at shutdown, subject to exact nondispatch proof and settlement of its admitted preparation or
binding obligations. Preserve its identity and content for ordinary later recovery. Continue to
require exact terminal-history or authority-loss convergence for dispatched or uncertain work;
do not infer nondispatch from missing provider identity or a coarse idle state.

The main-windows feature, CAS-live system and app lifecycle supplement now distinguish that outcome
from terminal-history or authority-loss convergence. Derive implementation and verification from
those authorities. Do not fabricate terminal history, dispatch
pending work merely to drain shutdown, or relabel it as an accepted queue item.

The independent dispatch inventory identified distinct cuts for direct durable submission,
accepted-input promotion, pending start, steering, compaction admission/claim/start and lifecycle
continuation settlement. Router dispatch elections already use live-command permits, but those
permits do not currently provide one shared reversible fence over all durable admission cuts.
Preserve the existing compaction settlement and local-mutation lock order when composing that
fence. [The root plan](../plan.md) separates this authority correction, shared admission fencing,
and the all-work shutdown coordinator into independently reviewed acceptance boundaries.

Independent semantic review and root authority/diff validation found no blocking gap in the
clarification. The documentation index is current and whitespace checks pass. This accepts the
completion contract only; exact proof construction, admission races, cleanup and recovery remain
implementation verification obligations.

## Bounded Provenance Blocker

Proof-readiness review found that the existing bounded durable reads cannot establish the approved
nondispatch distinction for every pending turn. A current `PendingTurn`, zero source events and a
`Valid` binding are insufficient even after joining the current execution flight:

- [Abandonment](../../crates/syndic-storage/src/mutation/binding/abandon.rs) can preserve a pending
  turn while publishing a stale binding and exact projection-loss route provenance.
- [Recovery](../../crates/beryl-app/src/cas_projection/execute/recovery.rs) can publish a valid
  replacement binding after replaying only that pending turn's parent. The
  [binding publication mutation](../../crates/syndic-storage/src/mutation/binding/transition.rs)
  permits this without changing the pending turn or its route.
- [Activation](../../crates/syndic-storage/src/mutation/binding/active.rs) does not require the
  pending gate's selected route to be absent. A later activation and exact
  [cancellation](../../crates/syndic-storage/src/mutation/binding/cancel.rs) can therefore clear
  that selected route. Proving the later attempt did not dispatch does not prove that an earlier
  attempt was undispatched.
- The [recovered-pending reader](../../crates/syndic-storage/src/read/delivery_recovery/pages.rs)
  checks current pending, source and binding facts after excluding selected routes; it has no
  earlier-attempt anchor. Turn topology and state records carry no binding-history or execution-
  snapshot anchor. Snapshots and active CAS-turn records are keyed by snapshot identity.
- The [flight registry](../../crates/beryl-app/src/cas_projection/service/flight_registry.rs)
  releases the flight key and wakes a waiter without retaining a completed dispatch proof. The
  [scheduler settlement](../../crates/beryl-app/src/cas_projection/accepted_input_scheduler/next_turn/worker/settlement.rs)
  result is generic and also discards projection-release errors; neither supplies missing proof
  for older completed flights.

Contiguous binding history can distinguish untouched work, exact cancelled `Active` to `Valid`
successors, and an earlier `Active` to `Stale` abandonment. The existing
[history validator](../../crates/syndic-storage/src/validation/bindings/history.rs) establishes those
transition distinctions. However, a complete per-thread history walk has no fixed bound. The
[storage package authority](../../crates/syndic-storage/doc/design.md#bounded-work-and-stable-identity)
restricts composite reads to bounded constituents and exhaustive enumeration to explicit validation,
scrub, background maintenance or corruption investigation; routine recovery follows only a bounded
closure around its natural anchor. Paging alone does not authorize a complete routine proof scan.

The Operator selected a clean bounded solution on 2026-09-14. The CAS-live system and storage
authorities now specify explicit per-turn dispatch provenance and a replacement turn-state record
encoding. Activation and exact cancellation maintain the anchor atomically; abandonment and
rebinding cannot erase uncertainty. Cancelled evidence follows the exact snapshot and immediate
binding successor, while live flight and cleanup obligations remain separate. The root plan
separates persistent provenance, its bounded read and shutdown composition. Implementation and
verification remain acceptance obligations; the design correction alone does not establish proof.

## Cleanup Custody Is Separate Evidence

Concrete settlement review found two ways that valid durable pending evidence could coexist with
unfinished cleanup. A failed unsubscribe released its cleanup owner before publishing connection
retirement; a retained session owner prevented automatic retirement in that interval. Separately,
an indeterminate cancellation could publish all pending records while retaining an installed
reconciliation scope in a healthy home. Neither a released flight nor healthy point reads closes
these gaps.

The correction retains exact flight custody through session and tool return, checks projection
owners and connection retirement, publishes failed-release retirement before ending cleanup
custody, and withholds settlement while the bounded home reconciliation registry remains occupied.
The guard retains its process fence and must be revalidated by its coordinator. Focused regressions
cover issued and parked authority return, a real failed unsubscribe with a retained session, and
indeterminate cancellation before and after exact reconciliation. These obligations supplement
the bounded durable proof; they do not change the authorized completion distinction.

## Acquisition Admission Gap

Readiness review of shutdown composition on 2026-09-14 invalidated the assumption that the accepted
dispatch fence and bounded work capture alone freeze the complete process work set. The existing
process fence deliberately leaves health-authorized commands open for stop, capture, repair and
cleanup, but new acquisition paths also use that authority without a process admission election:

- [Service admission](../../crates/beryl-app/src/cas_projection/service/admission.rs), through
  `prepare_session_admission_with_workers` and `finish_session_admission`, obtains a live-command
  permit, connects and initializes a backend, then registers its connection. The final
  `LiveCommandPermit::is_current` check validates service health and epoch, not the process fence.
  A same-home, same-service acquisition can therefore start after the fence.
- [Projection acquisition](../../crates/beryl-app/src/cas_projection/execute.rs), through
  `obtain_projection`, acquires the per-thread flight without a process admission check.
  [The flight registry](../../crates/beryl-app/src/cas_projection/service/flight_registry.rs)
  enforces only home/generation/thread exclusion. An already admitted session can create a new
  projection behind the fence, including provider `thread/start` in
  [fresh projection](../../crates/beryl-app/src/cas_projection/execute/fresh.rs).
- [Scheduled admission](../../crates/beryl-app/src/cas_projection/service/scheduling.rs) and
  [session preparation](../../crates/beryl-app/src/cas_projection/process_sessions/preparation.rs)
  similarly admit through flight and live-service checks without retaining an original process
  acquisition reservation through preparation settlement.

These are new acquisitions, not the already-admitted convergence that must remain available.
Revision validation can detect the resulting work change, but it cannot prevent admission after
the barrier's final validation. Retaining exact per-thread settlement guards also cannot exclude
new work on another thread or a new threadless connection.

The existing [CAS-live shutdown authority](../systems/cas-live-syndic-transcript/design.md#application-shutdown-coordination)
already requires atomic execution admission, captured winning work and joined custody. The clean
correction is a separately accepted acquisition-admission component using the shared process gate:
new acquisitions elect before effects, winning acquisitions retain counted custody through exact
publication or disposal, and later acquisitions are refused. Preserve terminal, repair,
reconciliation and cleanup authority behind the fence, as well as the separate turn-dispatch cuts.

The root plan places this prerequisite before coordinator composition. Required evidence covers
post-fence public connection/projection/preparation refusal, pre-fence acquisition racing shutdown,
publication and cleanup failure, and stale admission after coherent reopening. Source inspection
and independent semantic review established the gap. At diagnosis, no reproduction test or production
correction was attempted; the blocker was reported under the Operator's stop-on-technically-invalid-plan
instruction. The Operator then authorized the clean correction.

The accepted correction elects acquisition against the original live-command process epoch and
shares one counted reservation through nested session, runtime and projection preparation. The
reservation is not dispatch permission. Runtime readiness and connection/projection publication
release acquisition custody only after the work enters its existing retained owner; failed cleanup
keeps the reservation in the failed runtime entry. Provider cleanup and installed reconciliation
remain separate settlement obligations after a failed publication.

Acceptance on 2026-09-14 includes public post-fence connection/projection/preparation refusal,
winning admission and real managed-process preparation across fencing, cancellation, stale commands
after reopening, a real provider start followed by indeterminate publication and blocked unsubscribe,
and failed runtime disposal that prevents reopening. Independent semantic review, the production
library check and all 96 targeted regressions passed. Test corrections supplied a pending ordinary
turn for native projection, reconciled deliberately indeterminate publication before home close,
and retried bounded read-only inventory capture when concurrent retirement invalidated its revision.

## Terminal-Predecessor Completion Handoff

Coordinator review on 2026-09-14 invalidated composing shutdown solely from bounded live capture,
current-tail terminal evidence and pending preservation. The draft forgot exact captured turn
identities during progress, then accepted an idle row or its disappearance during a later sweep.
Retaining those identities exposes an existing completion-proof gap rather than fixing it alone.

- [Terminal convergence](../../crates/beryl-app/src/cas_projection/ordinary/converge/mod.rs)
  commits the Idle gate before returning the old projection. The existing
  [managed execution test](../../crates/beryl-app/tests/runtime_session_preparation/execution_lifetime.rs),
  `managed_execution_retains_session_through_terminal_history_without_a_view`, verifies that its
  `AfterGateRelease` pause still retains the checked-out session, running process and unfinished
  unsubscribe. Normal execution has already removed router registration at this point; exact
  execution, session and loaded-projection custody still exists.
- [Direct submission](../../crates/beryl-app/src/composer_host/submission/acceptance.rs) reserves
  process admission before its final durable command, independently of the old projection flight.
  A winner can publish after the fence. [Idle admission](../../crates/syndic-storage/src/mutation/admission/idle.rs)
  replaces the selected tail and transcript head with the new pending turn without requiring old
  app cleanup to have returned.
- [Terminal evidence](../../crates/syndic-storage/src/read/terminal_history.rs) requires the named
  turn to remain the current committed tail with an Idle gate and current finalized transcript.
  Once the new pending turn is published, it cannot authenticate the predecessor even after all
  old cleanup finishes. The new pending proof authenticates its own identity and nondispatch;
  it does not prove the predecessor's terminal-history fixed point.
- [Completion command dispatch](../../crates/beryl-app/src/cas_projection/ordinary/converge/command.rs)
  discards a successful commit receipt. Convergence returns `()`, and the ordinary terminal
  outcome retains projection and status without an immutable terminal-completion handoff.
  Stop-terminal observations are not terminal-history proof either.

The clean prerequisite is bounded exact completion evidence retained through execution cleanup
and successor admission, composed with live cleanup and reconciliation custody. Establish its
owning contract and acceptance boundary before resuming coordinator integration. Preserve the
separate pending and provider-operation outcomes; the existing authenticated
`compaction_recovery_read` settled case already supplies compaction receipt evidence.

Root inspection and independent semantic review confirmed the reachable ordering and missing
handoff. Two initial draft-coordinator tests passed pending preservation/join/reopening and idle
cancellation, but did not cover this gap; they do not establish coordinator acceptance. The
unaccepted source and tests were removed, preserving all accepted components and unrelated work.
The combined successor-admission race was established from source and the existing tested cleanup
pause; no new end-to-end reproduction or correction is claimed. The root plan records the blocker.

The Operator authorized the prerequisite, accepted on 2026-09-14. Each live ordinary execution
binds one fixed completion slot to its exact flight before activation. Only a committed terminal
history command publishes its receipt, including a commit followed by later failure; noncommit
and indeterminate outcomes publish nothing. Bounded revision-checked shutdown capture retains an
observer bound to home, home generation, service generation, thread and turn. It survives flight
release and successor admission without granting cleanup authority or retaining completed history
in the registry. Startup recovery remains sequential before service publication.

Independent semantic review, the production library check and 106 targeted regressions passed.
Real managed execution tests pause both before completion publication and after gate release,
publish a winning successor after the fence, and retain the predecessor's exact completion while
session/process cleanup remains outstanding. Evidence also covers authority-loss incomplete
history, command commit classifications, observer release, capture races and recovered-home and
service-generation rejection. Three regression assumptions were corrected: promotion tests wait
for exact reservation release rather than a worker counter; paused provider preparation retains
admission until cleanup; the native-lineage fixture permits bounded exact-thread unsubscribe and
resume retries before dispatch after a concurrent storage revision change. Both original retained
turns must still complete. The affected scheduler suites passed in full after these corrections.

## Bounded Shutdown Obligation Ownership

Readiness review on 2026-09-14 found that the accepted terminal handoff closes predecessor proof
loss, but does not by itself make bounded coordinator composition ready. A page-at-a-time approach
that retains exact observations until each page settles creates a progress dependency:

- [Thread settlement](../../crates/beryl-app/src/cas_projection/service/shutdown_settlement.rs)
  calls `validate_process_settlement_fence` before acquiring its thread guard. The
  [process fence](../../crates/beryl-app/src/process_admission.rs) requires the global admission
  count to be zero. An earlier page containing unresolved pending/preparation work cannot settle
  while a later page contains an admitted active execution that still needs shutdown's soft stop.
  Waiting for the first page prevents that later stop and the admission release it would enable.
- Retaining every unresolved page is not an established bounded alternative. The direct
  [ordinary execution entry](../../crates/beryl-app/src/cas_projection/ordinary/execute/start.rs)
  acquires its raw flight before preparation. The
  [flight registry](../../crates/beryl-app/src/cas_projection/service/flight_registry.rs) enforces
  same-thread exclusion without a global flight cardinality limit. Router or scheduled-worker
  capacities do not bound this separate direct path. Terminal conversion also removes router
  registration before history convergence releases the flight.
- The accepted [completion observer](../../crates/beryl-app/src/cas_projection/service/process_work/shutdown_capture/completion.rs)
  can discharge committed ordinary history independently of later cleanup. Pending preservation
  cannot use that receipt, and its current settlement boundary still requires admission and
  preparation closure. Forgetting an accepted pending identity or accepting current Idle state
  would reintroduce the original proof loss.

Root inspection and independent semantic review confirmed the dependency and absence of an
accepted composing API. This is source-backed readiness evidence; no new deadlock reproduction
or coordinator implementation is claimed. The proposed prerequisite is bounded exact obligation
ownership across progress passes using existing execution ownership, while durable backlog stays
paged. An alternative per-target settlement boundary would need exact admission closure against
late winning acquisition; deleting the global-count check is insufficient. Resolve this choice
in owning authority before implementation. No new quota, completed-history registry, or silent
weakening of pending preservation is authorized by this record. These prerequisites precede
coordinator composition.

The Operator authorized the bounded ownership prerequisites. Direct ordinary execution custody
was accepted on 2026-09-14: its exact connection supplies one existing service ordinary worker
permit before process admission and flight creation. The permit and counted reservation remain
through execution return; scheduled execution reuses its existing custody. This bounds ordinary
execution without charging returned idle projection cleanup or adding a quota. Capacity, fence
and cancellation refusal preserve the exact pending turn and loaded projection before activation.

Independent semantic review, normal and test-faults library checks and 124 targeted regressions
passed cumulatively. Evidence includes real minimum-capacity direct and scheduled execution,
pre-flight fencing, terminal handoff, permit reuse and unwind. Two continuation tests previously
expected connection invalidation to release every worker while the direct handler was still
paused. They now require the remaining ordinary permit, count the replacement connection
separately, and verify zero workers after handler return; continuation and attention guarantees
are unchanged.

Bounded exact execution capture was accepted on 2026-09-14. Each fenced attempt retains the
original service-bound ordinary observers and authenticated admitted compaction identities across
stale or failed refreshes. Distinct pre-fence attempts of the same pending turn retain distinct
observers; exact turn completion can authenticate their common durable outcome. Compaction uses
its existing exact settlement receipt after successor advancement and source release. Its retained
bound includes both 80 initial observation rows and 72 custody-owning admission winners that may
publish later, in addition to the service ordinary worker bound. An observed operation id without
durable admission remains preparation. Generic source pages stream into a bounded prefix.

Independent semantic review, normal and test-faults library checks and 130 targeted regressions
passed cumulatively. Evidence includes capture beyond 270 earlier generic preparation records,
retention after stale refresh and flight release, late ordinary slots, sequential attempts of one
pending turn, real managed predecessor/successor races, and compaction capture before and after
durable admission behind the same fence through authenticated settlement and cleanup. Pending,
continuation, permission, compaction and managed-session guards remain intact. Coordinator
composition and final-window integration remain unaccepted.

Connection inventory required a separate retention boundary. The
[admission path](../../crates/beryl-app/src/cas_projection/service/admission.rs) appends after its
convenience reaper; failed retirement polls remain registered. [Connection shutdown](../../crates/beryl-app/src/cas_projection/connection/lifecycle.rs)
can release both worker admissions and detach before recording a failed join. Consequently,
historical failed entries can exceed worker capacity. Neither a complete registry snapshot nor
filtering by an attachment flag establishes a worker-capacity bound.

Bounded coordinator reads alone were insufficient. Final service close joins the runtime owner,
whose retirement formerly collected every matching connection and invoked the full-registry
reaper. Ordinary final close invoked that reaper before its existing ownership transfer, and
implicit drop cloned the registry to signal retirement. A fail-fast visitor also could not replace
consuming disposal: poisoned or exhausted ownership must report failure while still releasing
retained resources. Implicit shutdown must leave custody for the runtime owner that joins them.

The failure cut exposed another instance of the same invalid assumption. Its former snapshot
selected connections that did not report detachment. A poisoned forwarding hub reports attached
even after its endpoint is gone; an endpoint can also remain present after both worker permits
return. Streaming dispatch during traversal would violate the independent requirement to freeze
all targets before issuing provider requests.

The Operator authorized bounded cut capture, accepted on 2026-09-14 as `19c9338e`. Its strict visitor
runs after command admission closes and drains. Each retained connection pins at least one original
driver or ingester admission, including partial pairs, through result completion. Existing worker
capacity bounds retained connections; the existing 64-target router limit bounds their batches.
Original weak router and worker sources remain reachable independently of forwarding-hub health.
Expired routers contribute no target authority; retired routers cannot dispatch. Zero-custody routers
seal without dispatch guards, dispose retained projections after unlocking, and reduce results to
counts. Every router freezes and final membership validation succeeds before obligation installation.

Independent semantic review, normal and test-faults library checks and 76 targeted regressions passed
on the isolated capture prerequisite. Tests retain nine failed historical connections with worker
capacity four and poisoned detached hubs, check the additional handle count, and preserve registry
evidence. Two-router tests prove no installation while the later router is blocked, and none after
membership drift. Other evidence covers poisoned router and registry ownership, exhausted revisions,
late prepared admission, partial worker release, and real zero-worker target projection disposal
with no retained guard. The terminal fixture requires closure without further provider commands.

The complete bounded shutdown traversal correction was then accepted on 2026-09-14. Shutdown capture,
per-turn settlement and cleanup polling use the strict visitor without registry locks across lifecycle
calls. Consuming disposal uses immutable identity progress and a finite initial upper identity,
continues after callback failure or invalid ownership, and retains a failed outcome. Only proven-clean
exact entries are removed, with own revision changes distinguished from external drift. Opportunistic
inspection defers contention and drift. Implicit shutdown signals every retained connection without
transferring registry custody; final close transfers its existing inventory and preserves invalid
ownership as failure after joining. Failed-join evidence remains observable after detachment.

Independent semantic review, normal and test-faults library checks and 96 targeted regressions passed
with coordinator composition excluded. Evidence includes bounded historical traversal, insertion,
removal and ABA, exact own-removal accounting, later cleanup after failed joins, poisoned and exhausted
ownership, implicit signaling, actual runtime disposal and isolation, preparation, successor handoff,
terminal capture, stop retention and reconciliation. Coordinator composition and final-window
integration remain separate and unaccepted.

## Failure Reopening Coherence

Independent coordinator review on 2026-09-14 invalidated composition of a healthy-home snapshot,
an empty `pending_reconciliations()` list and `reopen_process_admission(fence, true)` as a coherent
failure-reopening proof. The service's live command permit is nonexclusive. A concurrent already
authorized command can create reconciliation after inspection but before reopening, even with no
counted process execution admission. The current coordinator's tests pass only separated inspection
and recovery cases; 14 focused tests passing does not establish this atomic boundary.

The gap also exists before installation: [pending handles](../../crates/beryl-home-store/src/reconciliation/registry.rs)
omit `Reserved` scopes, while [indeterminate custody](../../crates/beryl-home-store/src/command/result.rs)
can still own its reserved slot after the writer's mutation interval ends. Installation transfers
that slot under the reconciliation registry lock independently of mutation observation. A fresh
mutation token and empty handle list therefore cannot prove absence of unresolved custody.

The [existing mutation election contract](../../crates/beryl-home-store/doc/design-atomic-commands.md#mutation-observation-and-in-memory-election)
also forbids acquiring ownership locks inside its callback; the current reopening path acquires
the master and process admission locks. Wrapping that call in `try_elect` is not an authorized fix.

The Operator authorized bounded coherent election and atomic app admission composition. The
home-store boundary was accepted on 2026-09-14: one direct in-memory election holds mutation,
reconciliation and health synchronization through the caller transition, refuses every nonvacant
scope and stale or unhealthy generation, and retains no inventory. Caller ownership precedes this
election; its callback cannot acquire ownership, perform storage work or resolve custody.

Normal and test-faults library checks, independent semantic review and 48 focused and affected
regressions passed. Evidence includes indeterminate custody before installation, drop installation,
collision retention, exact-new recovery, stale generation, poison, active mutation and competing
election. An already-admitted read pauses before confirmation, then attempts structural failure
while election holds health ownership; publication waits until the callback returns. Existing
mutation, reconciliation, recovery and maintenance health behavior remains intact.

App admission composition was then accepted on 2026-09-14. The service selects its exact owned home
and expected generation. Master and process ownership are acquired before home election; only the
successful callback opens the process flag. The production boolean-coherence entry is removed.
Refusal preserves the fence, and successful reopening does not revive pre-fence execution permits.

Independent semantic review, isolated normal and test-faults checks, and 54 focused and affected
tests passed. Tests cover counted admission return, stale and foreign fences, closed service,
active mutation followed by returned indeterminate custody, installation, exact-new resolution,
failed home health and concurrent live commands. An existing no-connection test now permits idle
workers to retire between count snapshots while still forbidding worker growth and provider contact.
Coordinator composition remains separate and unaccepted. Do not clear custody, weaken coherence
or add an inventory quota.

## Noninterruptible Compaction Progress

The initial diagnosis of runtime test
`shutdown_waits_for_noninterruptible_compaction_and_cancels_its_continuation` incorrectly located
the failing line in its first waiting loop. Nextest run
`25aafc9d-7258-461f-9e86-0db9c6ba4db4` failed in 11.918 seconds with
`StopCoordinationError::ConnectionUnavailable`, but the failure occurred in final convergence
after releasing compaction. The initial withheld-response wait had succeeded.

Source inspection and independent review confirm that existing durable stop admission returns
`Ineligible(Compacting)` until the CAS turn identity is known. The fixture subsequently emitted
`turn/started` but never answered `turn/interrupt`; it rejected every text request after terminal
emission. A valid later eligible stop could therefore time out or race terminal and be rejected.
The passing instrumented rerun `e2333c6e-af69-436a-8198-e9ce3b1e6a30` further demonstrates the
fixture's timing dependence, not acceptance of a production correction.

The correction adds a dedicated fixture mode that validates and acknowledges one exact compaction
interrupt, then withholds terminal completion until explicitly released. Verification must cover
initial noninterruptible waiting, later eligible stop, acknowledgement without completion, exact
terminal settlement, cancelled continuation and cleanup. No new production compaction-wait
boundary is justified by this failure. The separate concurrent stop-read retry remains applicable.
Confirm the exact failing stage before inferring a prerequisite gap from elapsed time alone.

The corrected fixture then exposed a separate production rejection in run
`3376a070-fb26-41ac-9e10-6e322b387a0c`: initial waiting, durable stop eligibility and interrupt
acknowledgement succeeded, but final convergence timed out. The exact compaction retained request
acceptance, its completed marker and `Stopping` state with no terminal observation; the stop
remained dispatch-claimed. The app's `publish_provider_event` rejected `Stopping` through its
`is_live` guard. The next idle-status event therefore failed before terminal publication.

Storage's `provider_event_operation` already authenticates `Stopping` against the exact gate and
provider target. The required correction permits that state locally in the app publisher while
preserving storage validation. Generic live-operation eligibility and barrier retention are not
the defect and must not be broadened or removed.

The component correction was accepted on 2026-09-14 after independent semantic review, normal
and test-faults library checks with coordinator composition excluded, and 59 compaction/stop
regressions. The new standalone test failed at status publication before the correction and passed
afterward. It verifies exact matching-terminal stop evidence, manual compaction success, original
committed tail, no restored continuation and refusal of events after authority consumption.
The real-runtime shutdown fixture remains separate integration evidence.

## Finalizing Compaction Classification

After the stopping-event publication correction, the integrated shutdown test on 2026-09-15
reached exact interrupt acknowledgement and terminal publication but failed during final
convergence. Focused run `e704569a-f48e-4f31-a683-cf8880f66e23` passed 17 of 18 tests. Diagnostic
run `f15f2aa0-4aeb-4683-814c-22a3cff6295a` identified
`Read(Invariant("delivery-recovery gate turn does not block its thread"))`; temporary instrumentation
was removed. This is distinct from the earlier fixture timeout and stopping-event rejection.

Provider terminal publication leaves the exact operation in `Finalizing`, its provider turn
terminal and its input gate compacting until named compaction settlement consumes that authority.
The generic delivery-recovery classifier invokes ordinary `validate_blocking_turn` for every
compacting gate, so stop admission and an already-retained stop barrier reject this valid
intermediate state. The named compaction recovery reader already represents it as pending success,
interrupted-with-idle-evidence or failure finalization; it is not settled ordinary history.

Independent review recommends a bounded compaction-specific classification that authenticates
the exact gate nonce, operation, provider turn, snapshot and terminal/turn-state agreement while
preserving deferred compaction ownership. Stop admission must remain ineligible until finalization
finishes. If the existing compacting-ineligible result is retained, the legacy stop barrier must
wait while that same target still owns the gate; its current unconditional convergence is too
early. Keep ordinary committed-tail and blocking-turn rules intact for ordinary operations.

Removing retained-barrier polling alone is insufficient: first stop selection can encounter the
same state, and skipping polls changes failure visibility when another primary stop later safely
reopens the same ordinary target. Do not retry invariant failures, discard captured obligations,
or treat terminal publication as completed compaction settlement. Implementation paused under the
Operator's invalid-plan rule pending the owning storage and app prerequisite corrections.

The Operator authorized those corrections. The bounded finalization classifier now authenticates
exact home, operation, gate, parentless provider turn, execution snapshot, binding, CAS turn and
terminal-state agreement; it preserves deferred recovery and compacting-ineligible stop admission.
The original focused regression failed before the change and passed afterward. Independent
semantic review and normal/test-faults package checks passed. The final fault-enabled compaction
and delivery-recovery cases passed in the broader 139-test selection before an unrelated stop
test interrupted that run. Acceptance remains uncommitted pending the distinct issue below;
the app's exact-target barrier correction has not yet been implemented.

Classifier acceptance completed on 2026-09-15 after the pending-continuation prerequisite was
committed separately. All 166 tests in the broader storage selection passed, including terminal
compaction outcomes with and without prior stops, exact-authority corruption, stale-source and
point-limit refusal. Normal/test-faults package checks and independent semantic review passed.
The correction changes no ordinary blocking-turn rules and consumes no operation authority.

The app barrier correction was accepted separately on 2026-09-15 after independent review,
isolated normal/test-faults app checks and 65 compaction/stop/broker regressions. Real-storage run
`354eb195-8ee0-41a7-b16c-3cae32daaaae` first reproduced premature `Converged` after terminal
publication while the operation remained finalizing. The corrected test requires repeated
`Waiting` until exact manual settlement releases the gate, then `Converged`. The classification
matrix preserves different-target convergence and ordinary safe-reopen failure visibility.
Process-wide shutdown remains a separate integration acceptance boundary.

## Pending Continuation Dispatch Provenance

Broader storage verification on 2026-09-15 exposed a separate inherited production inconsistency.
`pending_published_target_stops_and_consumes_a_matching_terminal_without_activation` fails its
first whole-home scrub with `compaction continuation settlement and successor disagree`.
Baseline run `d602d09d-50a7-495b-aff4-eb41b9484296` reproduced the failure with all finalization
classifier production changes excluded. Those changes were then restored and verified exactly.

The fixture uses production compaction settlement, binding activation and CAS-turn publication.
Binding activation advances dispatch provenance and the turn-state revision while leaving the
continuation pending until a provider activation event. The continuation validator still requires
every pending descendant to have revision `FIRST`, rejecting this valid state. Independent review
confirmed the production mismatch; changing the fixture or removing the scrub would hide it.

The recommended bounded correction is to recognize authenticated pending dispatch-provenance
descendants while retaining the exact initial continuation content and counters. Existing graph
dispatch validation authenticates activation and cancellation provenance; no schema change or
history scan appears necessary. This is separate from terminal-compaction classification and
requires the owning contract and plan to be reconciled before implementation. Work paused under
the Operator's invalid-plan rule; the failing test remains intact.

An earlier ordinary race-test assertion also depended on which validation layer first detected
concurrent mutation. It returned the required typed `ConcurrentChange` from gate/source
reconciliation instead of the stop-read layer. The test now requires that typed result and retains
diagnostics without fixing the internal layer name. Independent review accepted this test-only
repair; all 18 stop-admission-read cases passed in the subsequent run before the continuation scrub
failure. The unrun remainder of the stop-storage suite is not claimed as passing.

The Operator authorized the pending-descendant correction. On 2026-09-15 the implementation passed
independent semantic review and normal/test-faults package checks. A 166-test broader storage run
passed, including all previously unrun stop cases. The original pending stop/terminal test passed
as run `97631f6d-02e6-4ce6-977d-13bb98bca493`. Five targeted provenance tests passed after final
review refinements in run `846fe31d-bd8e-4f22-9d3e-3d834a4c97cb`.

Both validation and scoped consumed-successor reconciliation now recognize the provenance-based
pending revision distinction. Scoped reads authenticate historical activation or exact cancellation
records and decoded turn/state identities; initial content and capture counters remain fixed.
Corruption tests perform scoped authentication before whole-home scrub: a failed scrub changes
home health, so checking only a later read error would not prove the scoped validator ran. No
record encoding, history traversal or provider-event requirement was added. This prerequisite is
accepted; terminal-compaction classifier acceptance and app barrier composition resume separately.
