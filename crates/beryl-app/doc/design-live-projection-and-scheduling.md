# Live Projection And Scheduling

This supplement is normative only for its bounded beryl-app live projection and scheduling role
and is governed by [design.md](design.md). It does not independently declare engineering rigor.

The [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md) owns projection,
lineage, dispatch, steering, scheduling, and recovery policy. This supplement defines app-owned
topology and typed execution surfaces.

## Runtime Interest And Connections

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while projected per
  window. Releasing one window preserves all other interest and required work; final release drains
  required work before orderly retirement.
- Admitted execution sessions hold required runtime interest separately from view interest.
  Process-owned work can preserve or acquire that exact demand without a mounted thread view;
  catalog queries and mere durable thread existence create none.
- Every validated connection has one non-GPUI driver, one ordered ingester, one non-cloneable sink,
  one bounded router, and one capacity-one broker with one acknowledgement slot and at most one
  current bounded operation. Only the driver polls and sends serialized provider requests.
- The service acquires the configured driver-and-ingester permit pair atomically before either
  starts. Construction failure, retirement, and shutdown release the pair.
- The runtime owner consumes completed connection retirement independently of later admission,
  capacity waiters, view activity, or explicit shutdown. Its bounded maintenance leaves active
  connections intact, uses exact runtime/process authority, and preserves the persistent-failure
  disposal fence. Contention defers inspection; poisoned cleanup ownership remains a typed failure
  while the consuming owner joins and releases the retained resources. Disposal-only recovery of
  poisoned locks never restores execution authority or reports clean completion.
- Clean ordinary connection disposal notifies the originating scheduler after driver and ingester
  joins and attachment detachment are observable. The existing idle-maintenance pass then reclaims
  the exact retired session slot and publishes execution readiness. Worker exit or an earlier
  retirement request alone is not this completion notification; progress cannot require a later
  diagnostic read or unrelated user action. This uses existing runtime maintenance and adds no
  polling worker or retry authority.
- Required runtime demand survives admitted-session handoff through loaded projections and exact
  request/worker cleanup. Its final release follows actual disposal, including driver retirement
  cleanup; it cannot depend on the runtime retirement that the same demand prevents.
- Workers receive only typed admitted capabilities. They cannot poll the stream, inspect backend
  storage, parse raw JSON, or retain GPUI, home-store, repository, or window handles.
- A connection retains at most 256 retired remote-thread lane fences. Overflow retires the
  connection rather than forgetting exclusion. Other queues, prefixes, registrations, and worker
  sets use configured finite capacities, not tuning values as semantic authority.

## Runtime Activity Enrollment

- The runtime-interest owner separates its opaque process-local readiness lifetime from the
  durable enrollment token specified by the backend-runtime system. An admitted producer carries
  exact home/service, runtime/process, thread/turn and lifetime authority; cloned readiness facts
  alone cannot enroll or publish. Storage token equality never substitutes for those live fences.
- The first producer reserves one enrollment slot through its bounded runtime owner. It
  prepares the typed Syndic enrollment against an exact home revision and publishes the token
  only from a proven committed result. Later producers reuse that token after authenticating their
  own source. The slot and its move-only witness survive acknowledgement loss until exact outcome
  classification; cancellation and retirement cannot fabricate noncommit or free unresolved
  custody. No runtime-wide list of participating threads is retained.
- One same-home process owner retains those slots independently of service generations, with
  capacity equal to the configured runtime limit and at most one reservation per runtime. The
  reservation retains the original typed witness before submission; an indeterminate result
  synchronously installs its home reconciliation and retains the returned exact handle before
  reporting, cancellation or retirement. Abandonment before submission and definitive outcomes
  release their reservation. Unresolved outcomes remain charged across service disposal and
  candidate failure, without retaining old services, sessions or execution capabilities.
- Natural-record enrollment inspection is not settlement of an installed home reconciliation.
  After uncertainty, the owner joins that exact registry outcome and its original enrollment
  witness before publishing the token. A matching derived head alone grants no dispatch,
  generation-replacement or durable-acknowledgement authority.
- Enrollment precedes current live projection mutation and preserves lazy CAS warm-up. Admission
  without a runtime remains canonical-only until an executor obtains the matching runtime and
  enrolls the source. An ended lifetime cannot publish Activity, but its already-owned capture and
  terminal custody still follows the canonical recovery path. Reader eligibility checks the live
  runtime lifetime and proven token as well as the revision-bound storage head; persisted rows or
  a reused numeric readiness counter alone cannot make a restarted runtime's collection current.

## Managed Session Preparation Outcomes

- Managed preparation retains typed target lookup outcomes. Confirmed missing runtime/root
  records are distinguished from changing sources, read failure, missing service configuration
  and binding mismatch. Only exact stable target absence or an existing runtime owner's confirmed
  failure supplies handoff failure evidence; a generic not-ready result supplies none.
- A generated pending parent reserves its existing handoff settlement slot while the scheduled
  same-thread acquisition owns preparation. Confirmed launch failure maps to runtime unavailable;
  confirmed foreground connection, initialization, release admission or connection loss maps to
  CAS unavailable. Stable absence of the exact configured root maps to root unavailable.
  Other runtime-owner failures use runtime unavailable without inferring a root cause from text.
  Confirmed failure is captured before settlement cancellation or retirement checks and survives
  through the accepted proof custody. Successful preparation and temporary contention release
  the reservation without pausing the job. Ordinary input retains its existing preparation path.
  Possible parent dispatch is never classified by this pre-activation boundary.
- An already retryable generated job may prepare its exact runtime under the existing runtime
  recovery authority, without constructing another handoff failure event. Runtime recovery keeps
  the job paused; ordinary dispatch still requires explicit handoff retry of the same input.
- Session publication and connector acquisition preserve confirmed failure of the exact runtime
  interest as a typed outcome under the existing runtime lock. Retirement, service closure and
  readiness mismatch remain non-failure outcomes. Consuming an interest cannot erase a confirmed
  failure between readiness and foreground-session publication.

## Passive Receive And Failure Retirement

- The sole connection driver owns receive serialization and exact attachment/worker lifetime.
  Reading and parsing the stream grants no durable or effect authority and holds no drain-counted
  live-command permit. Each durable ingester operation obtains its own durable admission; driver work after
  a poll, including approval handling and quiet-state publication, obtains fresh admission before
  using healthy-service APIs. A failure between receipt and that admission prevents those effects.
- Outbound commands retain their existing admission, dispatch evidence, response correlation and
  exact completion custody. Failure does not release an outstanding non-idempotent request as if it
  never dispatched. Notifications interleaved with its response use the same nonwaiting passive
  ingestion contract, so request settlement can release its original permit without awaiting the
  coordinator's inventory. No request is replayed or newly authorized by passive receipt.
- After exact persistent failure, the driver rejects queued ordinary commands and may continue
  passive polling on the original connection while the failed service remains alive. It serializes
  only already-authorized failure interruptions with that polling; receipt cannot authorize an
  approval response, tool execution, continuation, new turn, or durable publication. Ordinary
  shutdown and explicit cancellation instead stop polling. No wait for a terminal provider event
  extends failed-service retirement. A passive transport or schema failure directly signals exact
  ingress cancellation and stops the driver; it cannot depend on healthy-command admission to
  perform retirement.
- Each connection retains an exact non-authorizing ingress-cancellation handle established with its
  original broker. Retirement signals it before acquiring the forwarding attachment lock or joining
  the driver. The handle cancels/wakes that consumer only; it cannot submit work, access the backend,
  publish to storage, or target a replacement. Acknowledgement closure still follows return of the
  current operation and required reconciliation handoff. Driver and ingester joins precede attachment
  disposal; transient bytes and cancellation handles never transfer to a replacement service.

## Selected Runtime Retry Ownership

- The app service owns one explicit selected-binding recovery operation; the window owns only its
  presentation and cancellation interest. Admission authenticates the healthy home/service
  generation, exact State window claim and selection revision, durable execution binding,
  configured runtime/root, current thread source and represented prefix, and the original runtime
  failure attempt. It uses the existing command/publication fences; a notice record or a cloned
  failure snapshot grants no recovery capability. Validation and backend work execute off GPUI.
- The runtime owner elects launch Retry for the exact failed runtime attempt only after its prior
  cleanup completes. Concurrent windows share that election, but success for one selected thread
  does not certify another thread or root. If that exact launch has already succeeded while the
  selected condition still lacks usability proof, a later explicit Retry prepares the selected
  projection on the current matching runtime; it does not reauthorize the ended failed launch
  attempt or restart a healthy runtime. A newer runtime failure requires its own current failure
  authority. Each selected-binding operation retains at most one
  current attempt and one bounded terminal result, charged to existing worker, connection and
  projection-flight capacities. Contention supplies an unavailable result or an existing bounded
  capacity wake, never an extra waiter queue, timer retry or automatic second attempt.
- Retry establishes the same configured runtime and root through exact release admission, obtains
  a full foreground connection and an exclusive loaded projection through the existing session
  and projection owners, and keeps the same exact thread source. The command cannot select Syndic
  recovery in place of native lineage. Projection establishment may perform only the source-owned
  operations already allowed by the CAS-live system; it starts no model turn, admits no input and
  sends no steering fragment. Ordinary activation and history browsing remain backend-free.
- A successful result is a bounded, non-authorizing usability observation issued by the service
  only after the exact projection has been established and its real lease retained by the existing
  process session owner. It correlates the home/service, runtime/process, foreground connection,
  loaded-session and projection generations, thread, full execution binding, exact source,
  represented prefix and tool profile with the admitted recovery attempt. Runtime Ready, session
  registration, checkout, a missing failure or a connection initialize response alone cannot issue
  it. A result never contains a cloneable execution lease or lets the window dispatch backend work.
- Publication revalidates the original selection and graph lifetime, current durable source/prefix
  and the still-live projection authority under the existing nonblocking publication fences.
  Source drift, retirement, connection loss, replacement, contention or missing proof refuses
  success publication. The normal command owners still revalidate authority at each later use;
  recovery success does not promise every command is enabled despite independent turn, repair,
  approval or capacity gates. Other windows need their own exact usability observation.
- Cancellation, selection loss, window closure, Exit and home/service replacement revoke result
  publication and settle the operation through its original owners. Window interest cannot retire
  a shared runtime or an admitted execution session. Already-dispatched source operations retain
  their exact outcome and reconciliation custody until settlement; cancellation cannot certify
  nondispatch or authorize replay. A failed home transfers to the established home-recovery path
  and cannot keep this healthy-home operation current.
- Runtime Retry neither retires nor replaces a healthy home or its service graph. It leaves draft,
  history and delivery-unknown facts intact. Generated jobs paused for runtime failure still require
  their existing explicit handoff retry; establishing usability does not unpause them. Distinct
  already-admitted input can proceed only through its existing scheduler and delivery eligibility.

## Projection Authority And Leases

- A projection is usable only with exact home, service, runtime, managed-process, connection,
  loaded-session, Syndic thread, selected prefix, binding, and tool-profile authority. Equal ids or
  another connection's observation never substitute.
- Per-thread subscription leases come from one process-wide generation allocator. Each binding use
  requires one current admitted lease; release revokes locally before bounded unsubscribe. Drop
  performs no backend I/O and cannot leave reusable authority.
- Execution subscriptions and GUI observation subscriptions have distinct owners. The process
  registry retains the actual admitted loaded-session lease or transfers it exclusively into an
  execution checkout; removing the selected view drops only its observation subscription.
  Retaining an `Arc` after the authority-owning lease is dropped is not execution ownership.
- The process session-checkout provider accepts already-admitted sessions scoped to its exact
  healthy home/service generation, Syndic thread, and full runtime/root execution binding before
  projection establishment. Managed-process, connection, loaded-session, projection-binding, and
  immutable tool-profile authority is required when established and before the corresponding use;
  checkout alone grants no projection or dispatch authority. Process composition supplies typed
  request policy, asset preparation, and dynamic-tool authority without window discovery.
- The checkout provider transfers each session into the existing non-cloneable execution lease
  and matching scheduler flight; it creates no parallel execution lease or second run owner.
  The wider process registry composes this custody with other execution paths and work inventory.
  One session is either available or checked out, never usable by both. Return settles exact
  custody and emits the typed execution/capacity wake plus an idle-maintenance recheck; it cannot revive a retired
  slot or cross a service-generation boundary.
- Registry admission reserves one slot before insertion from the system-defined
  `worker_capacity / CONNECTION_WORKER_PERMITS` bound, sharing it across available, checked-out,
  and retiring slots without consuming the protected steering reserve. A slot is released only
  after exact return/retirement settlement, not merely when its connection workers end.
  Durable routes own waiting work, and saturation adds no resident waiter list. A checked-out
  session retains its worker and same-thread flight through exact terminal disposition. Once idle
  with no required work or view interest, the owner releases session resources through retirement;
  disposal revokes, joins, and reclaims all exact entries before generation loss completes.
- Retirement linearizes with registry acquisition through one bounded in-memory gate containing no
  backend or storage work. Connection or process loss revokes matching leases and registrations.
- Conditional idle-session retirement names one exact available registration and preserves it when
  checkout, loaded projection, promotion or cleanup ownership prevents release. Successful election
  excludes new acquisition before signaling and disposal outside the ownership gates. These local
  checks do not establish complete process idleness: the process owner supplies required-work and
  view eligibility, and existing request/stop custody survives through joined cleanup.
- Process owners share the inventory's revision-checked work readers through non-owning source
  handles scoped to the exact home and service generation. A read may retain its sources only for
  the bounded observation; retaining the handle creates no home, session or runtime demand.
  Closed, unavailable, foreign or changing sources cannot establish idle eligibility.
- An idle-eligibility observation examines only the configured bounded admitted-session set,
  existing bounded local work and each named thread's point input gate. It does not enumerate
  durable backlog or prepare presentation metadata. Pending turns, preparation, execution,
  requests, stop, compaction, continuation and terminal cleanup retain required work; attention
  alone and unadmitted queued input do not retain an idle execution session. These shared read
  facts neither acquire execution nor replace the final conditional-retirement ownership checks.
- The process scheduler rechecks idle eligibility on a distinct coalesced maintenance wake.
  Final view-interest release, available-session publication or return, preparation settlement,
  response completion and relevant live/control cleanup releases notify after their facts become
  observable. Notifications retain only the existing generation-scoped wake owner. They add no
  polling worker, retry loop, dispatch authority or cross-lane retry eligibility.
  When execution readiness shares the batch, its existing authorized dispatch passes precede
  idle maintenance so a newly prepared session can fulfill the work that requested it.
- Idle retirement preserves the exact service, registration, runtime/process and full execution
  binding. One service-owned home mutation observer supplies non-owning work sources with the
  shared admission fence and directs writer-settlement wakes to the existing generation-scoped
  maintenance signal. Capture its token before reading required work and consume it at the final
  retirement cut; a busy or changed token preserves the session. The final absence-of-view check
  is serialized with view-interest acquisition through the existing runtime-interest owner;
  election contains no storage or backend work, and signals
  and resource disposal follow outside ownership gates. Required work, unavailable or changing
  observations, checkout, loaded leases, promotion and cleanup ownership all prevent retirement.
  A declined maintenance pass waits for a relevant later release rather than waking itself.
- Native continuation, resume, inclusive fork, fresh lineage, or one-time recovery injection is
  selected from bounded typed proofs. The app never dispatches rollback, summarizes a prefix,
  assembles recovery history, or silently selects injection after unclassified native failure.
- Recovery creates one exact 65,536-byte page and both capacity-one broker directions before its
  fresh target. The coordinator derives the system-defined domain-separated source identity from
  the exact home, thread, selected path, represented prefix, source revision, totals, and sequence
  digest, then transfers the sole page between typed storage and connection workers without
  assembling a recovery sequence or backend batch.
- Cancellation, broker unavailability, revision drift, dependency-read failure, and invalid
  durable source or proof remain distinct typed adapter failures. Both broker sides receive one
  terminal result, and every terminal cut releases the page, rings, target, and lease.
- Immediately after exact recovery-injection success, the app coordinator obtains the typed Unix
  completion timestamp used by durable publication. A clock before the Unix epoch, a value outside
  the durable range, or another typed clock observation or conversion failure abandons the fresh
  unpublished target and returns no capability or publication. The coordinator never substitutes
  the earlier request timestamp and does not retry injection merely to obtain another time.
- Failed or ambiguous fresh-target publication submits the exact opaque publication scope for
  targeted natural-record reconciliation and returns no projection capability. Only the system's
  `ExactNew` classification and reread of the named eligible binding may establish another fresh
  projection; uncommitted or stale target provenance remains non-authorizing and is never promoted
  by resume or reinjection.
- Caller usability begins only after exact durable binding publication.

## Failed Shutdown Admission Reopening

- The service reopens only its own current fenced process admission after all counted reservations
  return. It owns the master and process admission locks before calling the exact owned home store's
  `try_elect_coherent` with its expected home generation, following the
  [shutdown coordination boundary](../../../doc/systems/cas-live-syndic-transcript/design.md#application-shutdown-coordination).
- The callback changes only the already-owned process fence. Refusal preserves the fence and
  shutdown custody for later polling. Success keeps old execution permits stale and grants no
  dispatch, rollback or reconciliation authority by itself.

## Outbound Preparation

- Pending-turn and accepted-input execution share one bounded range-backed engine retaining compact
  source identity, immutable proof, count/digest header, and cursor/pass state. It never assembles
  composer content or an image list in app memory.
- The text broker has capacity one and answers one bounded absolute-page request at a time only
  through retained source authority. Marker/asset preparation retains one current marker, sealed
  reference, verified sidecar, and runtime path at a time.
- The accepted-input correlation codec accepts exactly `beryl.accepted-input.v1:` followed by the
  identity's 32 lowercase hexadecimal digits. No other prefix, case, width, or encoding is valid.
- Pre-activation preparation failure returns the same exact still-live projection. Consumption
  occurs only after preparation and execution readiness; retry remains valid only in the unchanged
  healthy generation.
- The connection ingester is the sole live publisher of source-producing compact controls. Results
  become visible only with the typed broker proof required by activation or lifecycle.

## Scheduling And Steering

- One process-owned level-triggered scheduler reads revision-bound durable pages for steering,
  accepted-next, and recovered-pending work. Durable routes own backlog; resident state is bounded
  to compact cursors, lane wake facts, one candidate per permit, and join/disposition facts.
- Eligibility and execution checkout are independent of GUI selection. Direct, accepted-next,
  pending, steering, compaction, and continuation paths all participate in the process shutdown
  admission fence before their irreversible dispatch/successor cut. A fence preserves durable
  candidates and captures already-winning flights for joined settlement; no shutdown pass starts
  queued successors to empty the backlog.
- Steering and ordinary lanes share a coalesced wake service and bounded worker pool while retaining
  distinct eligibility and permit authority. A wake or release cannot be retyped across lanes.
- Each ordinary candidate acquires its worker permit before claim and one exact same-thread flight
  through validation, promotion reconciliation, projection establishment, dispatch, and terminal
  disposition. Saturation mutates no route and creates no backlog.
- Direct ordinary execution obtains the same ordinary worker role from its exact connection's
  service pool, then wins shared process acquisition before creating its flight. It does not use
  a separate pool or add a capacity setting. Scheduled execution reuses its already admitted
  worker and acquisition. Refusal returns the original loaded projection without activating the
  turn. Direct return releases ordinary custody with the flight; any returned idle projection
  keeps only its existing generic cleanup custody. Scheduled cleanup retains its longer lease.
- An ordinary execution binds one terminal-completion publisher to its exact projection flight
  before binding activation. Revision-bound shutdown capture can retain an opaque observer of
  that slot. Only the exact terminal-history command path constructs its completion evidence;
  execution return, a stop response, or flight release cannot construct it. Observers name the
  original turn even when a successor has replaced the selected tail. They retain compact facts
  only and grant no projection, session, dispatch or cleanup authority. The service validates its
  home and service generations before accepting a captured observer for shutdown.
- The service provides an attempt-local execution capture bound derived from its ordinary worker
  capacity plus the existing compaction observation and custody bounds. It retains exact ordinary
  observers and authenticated admitted compaction identities across revision retries, scoped to
  the same fence and service. Generic shutdown pages fold their source pages into a bounded prefix
  without first accumulating every live thread. Capture exposes late admission winners separately
  from generic preparation and cleanup, permitting later active work to progress before global
  settlement. Provider receipt reads preserve their operation identity through successor admission.
- Shutdown connection reads and cleanup retain a bounded number of connection handles independently
  of the registry's size. Failed retirements can outlive active worker custody and cannot justify
  a worker-capacity bound on a complete connection snapshot. Traversal validates exact service
  membership revision through its final read; membership drift invalidates the traversal, including
  removal and reinsertion that restore the same apparent entries. Connection lifecycle calls run
  outside the registry lock. Failed-join evidence remains observable after detachment and cannot
  be dropped or treated as clean to satisfy a traversal bound.
- Each service generation owns one connection-work observation/election boundary. Its constant
  state records an exact owner identity, monotonic revision, active mutation count and terminal
  closure/unavailability. It retains no connection catalog, payload, execution capability or home.
  An observation weakly names that exact boundary and revision; it cannot pin a retired service or
  validate another generation. Mutation intervals invalidate earlier observations before changing
  covered state and remain active through the final local update. Nested/concurrent intervals are
  counted independently; no-op intervals may conservatively invalidate observations.
- Membership, connection authority/custody, retirement, attachment, router work and registered
  backend response facts participate in this boundary, including detached and workerless owners.
  Source construction and registration establish participation before the source becomes visible.
  Existing bounded traversal collects facts between observations; no full-registry guard vector
  or worker-capacity assumption about historical cleanup is needed.
- Observation and exact election acquire the boundary nonblockingly. Busy, foreign, stale,
  unavailable or closed evidence refuses without changing source state, notifications, custody or
  process execution authority. Election requires no active mutation and retains its mutex through
  the caller's bounded local publication. It must not acquire connection/source locks, perform I/O,
  wait, invoke source callbacks or reenter the boundary. Other publication guards are acquired
  nonblockingly; their lock order is explicit at composition. Publication after election returns
  is not covered by the proof.
- Mutation admission briefly synchronizes with election, then releases the boundary mutex before
  changing local state. Cleanup and existing state transitions are never suppressed by observation
  failure. Poison, counter/revision exhaustion or unwinding an unfinished change permanently make
  observation unavailable; they do not manufacture a clean source or cancel cleanup. Closing the
  boundary irrevocably rejects observations while already-owned cleanup may finish. Observers and
  mutation participants retain only the boundary's constant state, not its owning service graph.
- Consuming connection disposal continues releasing its retained scope after poisoned ownership,
  unavailable revisions or membership drift, and preserves a failed outcome. Its bounded traversal
  follows immutable connection identity so concurrent removal cannot skip another retained owner;
  a finite initial scope does not admit later connections as evidence of a complete original read.
  Only proven-clean connections may be removed, with the traversal accounting for its own exact
  removal separately from external membership changes. Opportunistic inspection defers on lock
  contention and rejects invalid ownership. Implicit shutdown only signals retirement and leaves
  registry custody available to the runtime owner that must join resources.
- Persistent-failure capture closes and drains command admission before traversing exact registry
  membership. Every surviving original router is frozen before any provider obligation is installed.
  A retained connection pins at least one surviving original driver or ingester worker admission;
  even partial custody remains charged until capture is released. This bounds retained connections
  by existing worker capacity and target batches by the existing per-router target capacity. No new
  quota is introduced. Original weak router and worker sources remain inspectable independently of
  forwarding-hub health; an expired original router contributes no target authority.
  Routers without worker custody are sealed and classified one at a time without retaining dispatch
  guards. Already-retired routers cannot authorize dispatch. Preserve existing target records and
  failure evidence, dispose retained terminal projections outside the router lock, and aggregate
  completed results as counters rather than retaining historical target identities. Invalid registry
  membership or inaccessible router state leaves the cut incomplete and prevents dispatch. Additional
  capture storage depends on worker and router-target capacities, not historical registry length.
- Promotion uses one exact service lease and one atomic typed home command for Syndic promotion and
  Asset-owner transition. `Prior`, `Exact`, collision, and unresolved outcomes settle before any
  provider work; only exact promotion proceeds.
- A connection-wide non-cloneable steering attempt is retained from Ready-to-Delivering through
  exact success, retry, structured rejection, delivery-unknown, or target loss. Possible dispatch
  is never automatically replayed.
- Completion opens another pass only through typed continuation or a fresh durable, execution,
  recovery, flight, cancellation, or relevant external-capacity wake. No timer retry, per-input
  retry set, payload waiter, or self-wake loop exists.

## Fresh-Service Establishment

- The unpublished fresh stack performs bounded durable pending, stop, compaction, terminal-history,
  and repair convergence off the GPUI thread before scheduler, projection acquisition, admission,
  or execution wakes open.
- Ordinary and candidate startup recovery resolve a durable repair-required gate through Syndic's
  exact incomplete-convergence command when the pinned repair source is unavailable. They retain
  the original target and request disposition, record `AuthorityLost`, and finish bounded canonical
  and transcript publication before releasing the gate. Command failure prevents recovery success;
  indeterminate outcomes install home reconciliation custody. Restart resumes from the durable
  repair or finalizing-history state without recreating backend request authority.
- A sealed recovery-ready handoff is consumed only after supervisor attachment and atomic whole-
  stack publication, opening fresh lanes and projection establishment from durable authority.
- Old-generation schedulers, connections, flights, leases, workers, projections, and route
  authority are joined and released. Durable work remains for the new service; readiness never
  turns the old service into replacement authority.
