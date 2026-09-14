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
