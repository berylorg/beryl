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
