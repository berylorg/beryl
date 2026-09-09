# Live Control

This supplement is normative only for its bounded beryl-app live-control role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

The [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md) owns interruption,
compaction, continuation, ordering, recovery, and authority-loss policy. This supplement owns only
the app coordinator, execution-driver, adapter, and custody surfaces.

## Approval Routing

- Approval crosses the per-connection ordered broker as a dedicated bounded operation, authorized
  and enqueued against one exact target before acknowledgement. It is never offered elsewhere or
  converted to generic control.
- The app receives only bounded approval kind, request, thread, turn, item, route, and response
  state. Command, cwd, reason, permission body, raw parameters, backend JSON, and pretty payloads
  do not cross this boundary.
- Exact request routes and installed denial/stop obligations are owned by the process execution
  session within existing broker/request bounds. Window detachment does not discard or retarget
  them and cannot change the existing approval policy. No interactive response capability follows
  from a display fact. Authority loss and generation retirement use ordinary ordered invalidation.
- Retained request state is ephemeral, uses no durable payload record or GUI handle, and contributes
  to process work and shutdown facts while unsettled. No request capability survives restart.
- Command-execution and file-change denials use provider-owned interruption and cause no second
  interrupt. Permission denial installs the driver-owned exact stop obligation before broker
  acknowledgement; denial then precedes the sole driver's required `turn/interrupt`.
- Presentation loss cannot cancel an installed obligation. The driver drains it through exact stop
  before exposing the ordered result or advancing later work.

## Exact Stop

- One process coordinator keyed by healthy home generation and Syndic thread admits or joins only
  an exact registered ordinary or provider-operation target and holds the operation fence through
  dispatch classification or terminal observation.
- Distinct stop-operation and attempt identities are 128-bit values from the OS cryptographic
  random source. They are not CAS-derived or backend idempotency keys.
- Admission atomically publishes the stopping gate and causes, preserves accepted input as
  next-turn work, and cancels automatic continuation for the exact target. Only its authenticated
  foreground driver receives the non-cloneable capability and sends the sole interruption.
- Typed pre-writer failure or proven `NotCommitted` may authorize only the system's narrow
  single-use volatile interruption on the same target and driver. Possible durable authority,
  indeterminate outcome, drift, driver loss, or retirement cannot become volatile authority.
- Response is not terminal evidence. Nondispatch, possible dispatch, terminal, target loss,
  restart, and interrupting-approval cause use system convergence without resend or guessed reopen.
- Visible controls and feedback remain in the
  [status-line](../../../doc/features/status-line/design.md),
  [notifications](../../../doc/features/notifications/design.md), and
  [main-windows](../../../doc/features/main-windows/design.md) features.

## Compaction And Continuation

- One process compaction coordinator keyed by healthy home generation and Syndic thread participates
  in the same target-operation election and admits only exact typed authority.
- Distinct compaction-operation and attempt identities are 128-bit OS-cryptographic-random values.
  The coordinator retypes the operation identity as the provider-operation turn identity and
  derives the snapshot identity with the system-owned domain-separated hash over the complete
  admission target. The non-cloneable dispatch capability goes only to the authenticated
  foreground driver; no detached connector exists.
- A request deadline bounds caller waiting but changes no durable or capture state. Duplicate
  callers join or observe; they do not refresh the deadline or dispatch again.
- Stop of compaction uses its exact provider-operation target and same driver. Nondispatch reopen,
  possible dispatch, loss, terminal, and restart retain system-owned custody and never create a
  replacement compaction.
- One process-local continuation intent is keyed to the yielding turn. Stop admission, admitted
  process shutdown, non-success terminal, compaction failure, authority loss, or process loss
  consumes it without consuming accepted input. Thread switching and nonfinal close preserve it.
- After exact successful compaction, the app requests Syndic's atomic fixed-content publication and
  passes only its sealed result to atomic user-work-versus-continuation settlement. The app owns no
  intermediate content build or unsealed manifest read. Ambiguity retains the ordinary
  reconciliation custody; no second automatic turn is created. A durable pending continuation
  recovers only as ordinary work.

## Custody And Release

- Approval, stop, compaction, and continuation state uses explicit count, byte, and concurrency
  bounds. Each non-cloneable capability has one coordinator or driver owner.
- An admitted primary stop retains the exact connection's existing two-worker reservation through
  caller custody, queue handoff, driver dispatch, settlement and backend unbind or disposal. The
  reservation is shared with its already-admitted workers; it consumes no additional capacity and
  creates no new capacity pool. Joined callers obtain no primary custody reservation.
- Target loss, local stop removal, connection retirement and worker exit do not release that
  reservation while primary stop custody or its driver cleanup remains. Worker joins need not wait
  for a suspended pre-handoff caller; the capacity remains unavailable for replacement admission
  until the original custody releases it. Rejected handoff, cancellation and unwinding release the
  retained reservation after their exact stop/election disposal. Idle observation retains none.
- Reservation retention does not authorize dispatch after loss, alter stop election or loss
  convergence, or create an interruption retry. Existing generation and command fences remain the
  authority boundary. Verify the reservation through a paused admitted caller, loss-driven local
  removal, ordinary retirement and attempted replacement admission, and through driver cleanup.
- Cancellation, denial, local failure, broker or connection loss, terminal, reconciliation,
  supersession, generation loss, and disposal move or release exact obligations, capabilities,
  permits, waiters, and operation custody at their typed cut.
- Selected UI state, status text, guessed process ids, coarse activity, and displayed CAS ids are
  never mutation authority.
