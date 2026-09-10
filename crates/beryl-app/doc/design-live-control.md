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
- A retained response observation contributes request-handling work only while no successful
  response write is recorded and at least one response capability remains. Written responses and
  unwritten observations with no remaining capability create no response obligation, even when
  their inspection records remain present. Live target/handler ownership, permission-stop custody,
  and connection or worker cleanup remain independently required through their actual release.
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
- Manual compaction reserves one of 72 process-local operation slots before installing local operation
  state or publishing durable admission. This custody budget derives from the 64 queued operations
  and eight execution workers; those queue and worker limits continue to apply independently.
  Exhaustion rejects admission before durable mutation. Joining an existing operation consumes no
  additional slot.
- Accepting `PhaseContinue` reserves from that same budget before creating accepted intent or
  attention state. Exhaustion leaves the yield unaccepted. Other lifecycle-yield outcomes retain
  their existing acceptance policy and do not consume this reservation.
- The exact yielding thread/turn shares its counted reservation with its later compaction; the
  transition never acquires a second slot. This share grants no dispatch authority and cannot bind
  another intent or compaction. Retain the reservation until both intent disposal and compaction
  command/target cleanup finish. Cancellation, registry removal and response completion do not
  release it while either owner remains. Verify direct-caller suspension and connection reuse,
  cancellation and removed cleanup, and admission/settlement handoff with the full budget occupied.
- A compaction reservation follows exact command custody through preparation, failed admission,
  queue handoff, execution, settlement and target cleanup. Local removal, response completion,
  connection retirement and worker reuse cannot release it while that custody remains. Final
  disposal of the last custody owner, including cancellation and unwind, releases the slot; result-only waiters retain none
  after command disposal. Verify paused failed admission and replacement pressure, queue/driver
  handoff and post-settlement target cleanup without changing exact dispatch or settlement rules.
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
- Ingester completion closes and disposes the approval-interruption slot before publishing its
  terminal receipt or releasing worker admission, including caught-panic exit. Prepared custody
  unwinds before that completion boundary; a joined obligation without a primary stop owner cannot
  remain pending after capacity becomes reusable. Verify pending joined and reserved-preparation
  failure paths independently of a later retirement caller's cancellation signal.
- Selected UI state, status text, guessed process ids, coarse activity, and displayed CAS ids are
  never mutation authority.
