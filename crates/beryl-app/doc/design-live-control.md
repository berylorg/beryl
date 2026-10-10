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
- The production CAS-live service exposes opaque exact eligibility and consumer-owned feedback
  handles for these features. Service calls and storage reads execute off GPUI; presentation reads
  consume bounded observations. Feedback records are separate from operational work inventory and
  retain only the latest typed state and revision. The service retains weak associations so dropping
  the last consumer releases resolved presentation state without retaining an execution owner.
- At most 72 distinct live feedback records may be retained per service incarnation. Reserve a slot
  before GUI stop admission, reclaim expired associations before testing capacity, and report
  saturation without dispatch or eviction of a live record. This presentation limit does not replace
  execution queue, worker or stop-custody limits. Duplicate handles share one record. Service teardown
  resolves waiting handles without reviving them in a replacement service.
- The process-owned service's `exact_stop_worker` exposes a sendable `ExactStopWorker` for window
  consumers. It remains bound to that service's home and service generations, uses the same
  eligibility, request admission and sole-driver implementation as the service methods, and grants
  no shutdown or general service access. Holding it does not retain the home, connection registry,
  stop coordinator or published graph. Each worker call requires live command admission and exact
  current authority; retirement or missing ownership makes the capability inert. Window publication
  also binds it to the graph's weak restoration lifetime. GUI consumers revalidate authoritative
  selection before dispatch and before applying an asynchronous result. An in-flight admitted stop
  retains the existing execution custody independently of window presentation or capability drop.
  Publication loss may reject entry or invalidate an eligibility observation, but cannot replace
  feedback returned after request processing begins with a rejection. Preserve that opaque handle
  and fence its visible application separately; publication loss grants no retry or successor access.
- The same weak worker supplies a bounded selected-thread status observation containing the exact
  typed last parent state, a comparable opaque operation origin when known, and exact stop
  eligibility. Storage and live-owner observations remain off GPUI. A coherent observation uses
  accepted typed turn, turn-state, input-gate and active-target facts; drift or unavailable authority
  fails closed rather than combining facts from successive operations. Ordinary `working` requires
  exact live authority; durable compaction state may precede availability of stop eligibility.
  Gate-selected compaction remains visibly `compacting` through final settlement, including when
  its provider turn already has a proven terminal outcome. That terminal fact independently makes
  its exact popup anchor inactive; a workflow label cannot keep terminal stop controls available.
  After compaction settlement, the ordinary committed parent owns the last-turn readout again.
- Operation origins are service-owned observations, not command capabilities or visible backend
  identifiers. Their equality includes the home/service incarnation, thread and exact ordinary or
  provider operation. An origin remains stable through stop admission and feedback convergence,
  changes for a successor, and grants no admission, join or retry authority. The service associates
  each feedback handle with its originating operation so consumers can compare it with the current
  observation without reconstructing a target or proof.
- A window retains only its latest status observation and bounded exact request feedback. It
  revalidates selection, publication and service identity before applying worker results. Popup
  feedback requires the same still-active operation origin; terminal state, replacement, drift or
  authority loss closes that anchor while retaining required feedback for the Notifications
  contributor. Disposal releases presentation ownership without changing execution. View counts
  remain unknown unless their separate exact history and viewport facts are available; status
  observation never loads history or submits backend work to fill them.
- Each window awaits at most one status read at a time and shares a limit of 72 across exact feedback
  handles and opaque weak acknowledgement/refusal records, including its current popup and pending
  Notifications handoffs. Retention exhaustion
  disables new request admission before any stop effect; required waiting or resolved feedback is
  not evicted to make room. This presentation budget grants no additional service or execution
  capacity. A disposed view releases its observation task and presentation handles without
  cancelling an admitted request.
- The existing single status-observation loop also reads the latest bounded feedback revisions,
  including when no thread is selected. Notification acknowledgement releases only an exact resolved
  handle, clears cached eligibility, and preserves a bounded opaque-origin volatile refusal until
  a positively observed different service incarnation or same-thread operation successor makes it
  dispensable. Absent publication or same-origin inactivity alone does not release that refusal.
  Expired weak acknowledgement records without a required refusal can be reclaimed. No separate polling backlog,
  execution owner, retry capability or durable acknowledgement follows from this presentation state.

## Compaction And Continuation

- The published service exposes a sendable weak context worker using the same home/service and
  restoration-publication lifetime fences as `ExactStopWorker`. It grants only bounded context
  status, opaque manual-compaction eligibility and request/feedback operations. It retains no
  home, connection registry, coordinator, GUI entity or published graph. Every call requires live
  command admission; missing ownership and retirement make it inert. Context observations are
  included in the coherent selected-status read rather than a second window polling loop.
- Eligibility captures the system-owned original selection/claim, projection and idle-gate
  authority. The service prepares off GPUI and passes that opaque authority through the existing
  coordinator to the actual atomic admission boundary. Its final election rechecks all captured
  revisions and accepted-work precedence. Runtime Ready or a still-visible thread identifier is
  insufficient. Publication loss before request entry rejects entry; after request processing
  begins it cannot discard returned exact feedback. Consumers fence visible application separately.
- Before request admission, reserve a consumer-owned feedback record from the existing shared
  72-record service-incarnation presentation budget. Manual compaction and exact stop share that
  limit; duplicate handles share a record and saturation rejects before any effect. Each record
  contains only bounded opaque origin, latest typed result and revision, not a coordinator or
  execution owner. The weak service association does not keep a consumerless record alive.
- Install the exact settlement destination before durable admission can dispatch. A live consumer
  can receive late settlement after the local operation is removed. Timer expiry and exact
  terminal publication serialize on the same record; terminal settlement wins and can never be
  overwritten by StillRunning. The admitted operation snapshots validated applied timeout settings
  once, retains the original deadline and never starts a fresh deadline for duplicate observation.
  Polling reads current revisions; it creates no timers, result backlog or additional dispatch.
- Feedback distinguishes request pending, rejected, waiting for an admitted exact operation,
  StillRunning, succeeded, failed, interrupted and lost/unknown authority. A proven nondispatch
  result is a failure, not success. Indeterminate admission cannot be reported as proven rejection.
  Only system-qualified exact successful settlement produces succeeded. Service teardown settles
  waiting consumers to lost/unknown authority; no old record regains authority after replacement.
- A feedback association starts with a bounded opaque request identity and original selected
  authority; durable admission adds the exact operation origin without changing its request
  identity. Rejected requests therefore require no invented operation. Each window's request
  admission order supplies the Notifications-owned shared stop/compaction FIFO selector; polling
  and late result arrival do not change that order. Indeterminate admission remains pending until
  exact convergence or retirement, rather than releasing required feedback early.
- A window shares its existing 72-record budget across stop and manual-compaction feedback,
  acknowledgements/refusals and pending Notifications handoffs. It retains the latest observation
  and exact result handles, including after thread deselection. Selection or anchor loss moves
  required feedback to the feature's Notifications contribution; it never applies a result to a
  successor or cancels the command. Acknowledgement releases resolved presentation ownership.
  Waiting and StillRunning records remain live until exact outcome or retirement; they cannot be
  evicted to admit another command. Disposal releases presentation without execution custody.
- Verification covers real ordered usage/quota ingress, numeric and sparse-window invalidation,
  exact interest replacement, selection and session ABA, and retirement without history work.
  Manual acceptance additionally exercises selection/claim and idle-gate drift at admission,
  accepted-input precedence, original-driver-only dispatch, draft preservation, occupied shared
  presentation capacity, timeout/terminal races, late settlement after local removal and window/
  service teardown. Existing coordinator tests do not by themselves accept the mounted worker.

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
- Provider observation continues while that exact compaction is stopping. The app preserves the
  operation and provider-turn checks and delegates stopping-gate authentication to storage; status,
  marker and terminal events may converge the existing stop without admitting another dispatch.
- An exact stop barrier keeps waiting while the same provider-operation turn remains gate-selected
  as compacting, including terminal publication followed by pending final settlement. A different
  gate target or ended gate ownership retains the existing convergence classification. The barrier
  does not create a second interrupt or treat provider acknowledgement as completion.
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
