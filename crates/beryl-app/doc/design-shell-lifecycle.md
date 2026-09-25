# Shell And Lifecycle

This supplement is normative only for its bounded beryl-app shell and lifecycle role and is
governed by [design.md](design.md). It does not independently declare engineering rigor.

## Process Service Graph And Windows

- One ordinary process service graph is constructed only after the configured Beryl home validates.
  A busy home creates only its compact surface, and an unreadable home creates no ordinary
  conversation window from unvalidated state.
- The graph composes typed home domains, health and readiness publishers, runtime/root services,
  bounded catalog and activity services, settings and theme services, durable-job coordinators,
  and explicitly bounded worker pools. Shared versioned facts never retain or mutate GPUI views.
- The graph also owns the bounded execution-session registry and shared shutdown coordinator
  defined by the [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md).
  These services retain exact execution custody without window handles and publish bounded
  running-thread and needs-attention pages to independent window subscribers.
- The validated home service graph constructs exactly one marker-seal service for each home
  generation with immutable limits. Windows and publication flights receive clones sharing its
  flight registry and capacity. Construction is restricted to home composition; consumers do not
  rediscover the service through a process-global registry. Graph retirement freezes admission and
  completes the existing disposal and generation-retirement protocol before replacement; dropping
  shared handles alone is not that protocol.
- `beryl-app` is the sole GPUI shell-composition boundary described by
  [GUI integration](../../../doc/gui/integration.md). Feature controllers mount only into declared
  windows and slots.
- Each main window owns one controller, `WindowId` and bounded transient interaction state.
  Runtime-backed windows additionally own their exact selected-thread claim, bounded navigation,
  composer host, transcript host and presentation projections. The sole zero-runtime initial
  shell retains its exact threadless session member without those selected-thread resources.
  No main-window controller or GPUI entity is shared between windows.
- Selection and nonfinal close release only view/editor/subscription ownership after the required
  flush and durable claim/session transaction. They do not dispose an execution session or cancel
  its accepted queue, pending request, compaction, or continuation. Attaching a running thread
  obtains presentation subscriptions and GUI occupancy without waiting for execution checkout.
- Construction checks the stable process main-window count before allocating the controller or OS
  window. The [main-windows feature](../../../doc/features/main-windows/design.md) owns capacity and
  visible creation, close, Exit, and restore behavior. One admitted construction may retain its
  exact unpublished OS window and controller only through bounded preparation to first publication
  or prepublication abandonment; the app creates no overflow, parked, or deferred hidden
  controller.
- Settings, busy-home, and home-failure windows are distinct top-level controllers and never receive
  main-window claims or restore records.

## Initial Service Preparation And Publication

- The app accepts the registration-complete private home candidate, complete Beryl-state and
  Syndic handles, process admission and validated immutable service configuration. It prepares one
  owned graph containing execution/session and runtime-interest services, scheduler and tool
  routing, shutdown, bounded catalog/activity/attention services, settings/theme services,
  durable-job coordination, marker sealing and their bounded worker custody.
- Preparation constructs services against exact candidate identity without claiming healthy-store
  admission. Constructors may validate candidate handle provenance and configuration; persisted
  recovery reads and commands go only through the explicit candidate recovery access. No
  constructor starts session discovery or an ordinary work loop.
- The accepted sequential CAS-live startup recovery completes before publication. Required worker
  creation and attachment also complete behind the startup fence before publication; workers
  retain their exact shutdown/join custody while waiting. A construction error or cancellation
  closes their admission and joins them before the candidate can be discarded.
- Private CAS preparation configures runtime interest and the exact attached execution-session
  owner through candidate runtime-record and Asset-revision validation. It preserves ordinary
  policy, capacity, token-directory and owner checks while leaving ordinary admission closed.
  Configuration only retains immutable preparation context and notifies the fenced scheduler;
  it starts no runtime or session discovery. Failure consumes private service custody and joins
  workers before candidate disposal.
- Candidate terminal-history convergence uses the ordinary item-freeze/finalize, item-projection,
  selected-transcript and gate-release algorithm through borrowed candidate access. Exact bounded
  metadata/page reads retain their surrounding confirmation checks, and current-domain commands
  retain their exact record fences, receipts and failure outcomes. An indeterminate command installs
  its existing reconciliation custody and aborts convergence; it never reports successful recovery.
  This candidate entry accepts no live terminal-completion publisher, creates no worker or external
  dispatch, and cannot release ordinary admission or publish the app graph.
- Candidate source-less terminal publication shares the ordinary stabilized turn-state, input-gate
  and history-summary frontier, checked next sequence and monotonic timestamp selection. It uses
  the existing exact live-source command through candidate access, preserving panic containment,
  command outcomes and installed indeterminate custody. It cannot recreate provider authority;
  source-less authority-loss eligibility remains enforced by the existing mutation contract.
- Candidate active-binding and stop-operation abandonment execute the same exact typed commands
  as ordinary recovery. They preserve checked binding-revision results, ordinary failure
  interpretation and installed indeterminate reconciliation custody. Abandonment grants no
  provider dispatch or ordinary admission authority.
- Candidate deferred-compaction convergence shares the ordinary restart classification and exact
  settlement or abandonment commands, including the consumed-operation fixed point and confirmation
  after a clean commit. It preserves command outcomes and indeterminate custody, creates no
  continuation intent and performs no provider dispatch.
- Only the complete private prepared graph can consume app publication. It publishes the same
  candidate generation and graph under one outer transition, then releases ordinary workers.
  Consumers receive a published graph or a typed failure, never a builder, partial handle tuple,
  candidate access, pending constructor or early healthy-store result.
- Exactly one marker-seal service is constructed from the prepared graph's home generation and
  immutable limits. Clones are distributed from that owner after publication, preserving existing
  flight capacity and retirement semantics without process-global service discovery.
- Settings/theme repository loading, compact session discovery and progressive window preparation
  begin only after the complete graph publishes. Their owning readiness and startup-failure gates
  still apply; publishing services does not certify any restore set or first visible window.
- Theme runtime preparation retains its exact candidate identity, immutable bounds and dormant
  typed subscription in a private app owner. The composing graph drops that owner, joining its
  watcher, before discarding the borrowed candidate. Loading accepts only the matching published
  generation and releases the already-created subscription; it uses the ordinary startup loader
  and retains its complete fallback and typed failure outcomes. No service factory independently
  publishes the home or makes a partial graph available to consumers.

## Prepared Native Placement Dependency

- Saved placement uses the outer window rectangle in platform logical screen coordinates. The
  app requires an explicit outer-coordinate mode from the owned GPUI boundary; ordinary client
  bounds keep their existing meaning. One creation rectangle carries the initial normal or
  maximized state. Outer mode requires explicit bounds and a prepared monitor, rejects a competing
  display index or fullscreen state, and never adds client-frame borders to saved outer geometry.
- Monitor discovery runs on a worker and visits one bounded snapshot at a time, without retaining
  an unbounded monitor list. A snapshot carries immutable native identity, device identity,
  physical monitor/work rectangles and scale. The app can retain the selected snapshot per
  admitted window; a snapshot is neither a native-window owner nor authority to switch desktops.
- Native construction validates the selected monitor's identity, geometry and DPI fallibly and
  creates the hidden window on that monitor before using its DPI. Disconnection or changed facts
  reject preparation without a panic or silent alternate-monitor substitution. The native owner
  disposes any partially constructed window on failure. No temporary visible window is permitted.
- Outer conversion uses checked finite positive geometry, physical screen coordinates and the
  selected monitor's scale. Windows placement workspace offsets account for top/left work-area
  exclusions; negative screen coordinates remain valid. The actual window DPI must agree with
  the prepared scale. Fixed normal/maximized state survives first publication without activation.
- Dependency acceptance requires focused conversion and invalid-input tests plus real Windows
  hidden normal/maximized construction, exact outer placement and nonactivation evidence.
  Prepared-monitor drift and construction failure must preserve disposal ownership. Independent
  review covers the native lifetime and coordinate boundary. Beryl-owned authority for this fork
  requirement remains here; no fork Markdown or document index is required.

## Hidden Native Operation Lifetime

- A Windows desktop worker receives a move-only operation token for one exact hidden, never
  published window. The native boundary admits at most one operation per window and rejects a
  visible, previously published, closing, destroyed or already leased window. Retaining a raw
  handle or a GUI entity alone is not sufficient. The token remains owned by the worker until
  its native calls finish or unwind; cancelling its caller cannot release that protection.
- While the operation is active, native destruction is deferred even if GPUI explicitly removes
  or drops its window wrapper. Completion releases the deferred destruction on the GUI thread;
  neither worker-side destruction nor a blocking GUI join is permitted. Each admitted window
  retains only one operation and one completion continuation, without a retry or waiter queue.
- A native close request during this hidden operation latches terminal close intent without
  invoking ordinary close or destroying the root. That intent remains queryable after worker
  completion and prevents publication or another operation. Completion does not replay ordinary
  close: the app first extracts and disposes the original typed shell custody. Explicit wrapper
  removal is a separate destruction request and completes native destruction after the operation.
- Publication and activation cannot expose or focus the window while leased or after terminal
  close intent. Native destruction is idempotent for the exact owned HWND; a later wrapper drop
  must not issue a second destroy against a recycled handle.
- The app retains the full shell and original acquired, restored or threadless cleanup custody
  until the worker and GUI completion settle. Cancellation fences publication and then follows
  the original disposal path. It is not a desktop fallback or permission to abandon a pending
  native call. A pending call retains one bounded operation until completion.
- This boundary requires a running GUI event loop. The process startup/shutdown owner must drain
  native operations and their deferred GUI disposal before ordinary application quit or retry.
  Fatal process termination remains governed by the separate crash contract. A token does not
  promise cleanup after the GUI executor has stopped.
- Acceptance requires real Windows evidence for worker-held lifetime across explicit removal,
  hidden close intent, publication/activation fencing, worker release/unwind and exactly-once
  native disposal, plus independent lifecycle review. Native lease acceptance does not accept the
  desktop COM worker, app cleanup integration or process quit barrier.

## Window Placement Preparation

- Worker placement preparation binds one exact window identity and its immutable saved placement
  to one chosen monitor snapshot and resolved outer logical rectangle. It creates no HWND and
  changes no durable placement. The native consumer must match the window and saved facts before
  construction; another window's preparation is not interchangeable.
- Monitor selection visits current monitors once and retains only the best candidate. Prefer an
  exact saved monitor identity. Otherwise choose greatest intersection with the saved logical
  rectangle, then shortest squared distance between rectangle centers; use monitor identity as a
  deterministic tie-breaker. Windows monitor identities use the GPUI snapshot UUID's canonical
  string form. No match depends on monitor enumeration order.
- For an exact identity whose work area moved, translate the saved rectangle by the displacement
  from the saved work-area origin to the current origin. Otherwise retain the saved origin before
  clamping. Preserve the saved size whenever it fits; cap oversized dimensions to the chosen work
  area and clamp the origin so the resolved rectangle is inside that area. This is best-effort
  restore geometry; the shell's normal minimum-size policy still applies at native construction.
- Use finite checked geometry throughout, including extreme persisted integer coordinates and
  dimensions. Invalid monitor facts or no usable monitor fail preparation explicitly. Normal or
  maximized state and the optional saved virtual-desktop identity pass through unchanged; geometry
  fallback cannot silently substitute a desktop or rewrite the saved record.
- Verification covers unchanged geometry, moved or missing saved monitors, changed work areas,
  oversized/offscreen windows, negative origins, fractional scale-derived work areas, deterministic
  ties, extreme saved values, no-monitor failure and exact prepared-window binding. Windows worker
  discovery must compose directly with the accepted bounded GPUI monitor visitor.
- On Windows, restored and threadless startup shell construction requires that prepared placement.
  The host checks exact window/saved-fact binding before allocating a controller or native window,
  then supplies the resolved outer rectangle, monitor snapshot and fixed normal/maximized state
  to GPUI. A before-construction failure returns the original selected-editor custody through its
  existing typed failure boundary; threadless failure releases its transient reservation only.
- Ordinary newly acquired windows may use platform-default placement. Startup's acquired
  empty-session replacement instead supplies prepared placement through the same host boundary;
  the complete startup-set owner must require preparation for every member. Geometry construction
  alone never certifies saved-desktop restoration or complete-set publication.

## Startup And Activation

- Startup accepts only the validated minimal session plus each restored window's selected thread,
  current draft, visible range, compact editor frontier, transcript seed, and placement facts. It
  retains bounded seeds and never preloads a complete draft or history.
- Later activation prepares one revision-consistent target bundle containing claim, durable draft
  selector, fresh candidate-session head, composer and transcript seeds, title, lineage,
  runtime/root, and window-local projection facts.
- Each window owns at most one unpublished target composer. The prior coherent selection remains
  authoritative until promotion fences and flushes it and publishes the complete target bundle.
- Cancellation, preparation failure, source drift, stale completion, rejection, supersession, or
  disposal releases the unpublished target and preserves the complete prior bundle. An unmutated
  fresh target is abandoned only through the typed Syndic boundary; a mutated or ambiguous target
  stays in its ordinary operation or reconciliation custody.
- Startup and activation presentation follow the
  [conversation-threads feature](../../../doc/features/conversation-threads/design.md); this
  package owns only typed preparation, fencing, and publication.

## Restore-Set Startup Ownership

- One process-owned startup attempt retains the complete published service graph and at most the
  session's 256 exact main-window members. It discovers the minimal session only after service
  publication, loads Settings/theme through their startup boundaries, and selects one complete
  restored set or the feature-defined empty-session replacement. It does not accumulate an
  unbounded queue of restore preparations or discover additional windows from catalog history.
- Restored-window custody is distinct from newly acquired-window custody. It binds the exact
  attempt, home and service generation, session revision, window record, paired restoring claim,
  selected thread, durable draft and prepared first-presentable editor. It grants no pristine
  thread substitution or session-record deletion. Cancellation or preparation failure releases
  transient editors, view interest, subscriptions and hidden native windows while preserving the
  durable restore set. Begin-restore and exact claim activation use their typed revision-checked
  session commands; a partial command sequence is reconciled through original custody, never
  undone by a guessed compensating command.
- If a committed startup command still owns unavailable local-finalization custody, the
  coordinator returns an explicit retained outcome identifying the begin-restore, initialization
  or window-claim operation and exposes its original receipt and failure for diagnosis. The same
  move-only attempt keeps the capability and cannot report prepared or disposed, silently retry
  a new command, or authorize a conflicting Retry/Exit. The process owner retains that outcome
  through its separate shutdown/recovery boundary.
- An empty restore set with runtimes uses the accepted exact runtime/root selection and
  claim-or-create acquisition. Zero-runtime startup instead prepares the sole threadless shell,
  including when a validated session header already exists with no windows. That shell owns no
  thread claim, selected composer or fabricated transcript seed. Empty-header initialization must
  be revision-checked and cannot overwrite nonempty, changed or runtime-backed session state.
- All required shell identities, placement decisions, appearance and first-presentable state are
  prepared and checked while hidden. One bounded startup-set owner retains the exact native and
  controller handles through publication. A missing member, stale attempt, changed generation,
  closed native window or failed preparation rejects the entire set before any native exposure.
  Revalidation cannot silently drop a member, select a replacement thread or create an overflow
  shell. Final GUI-side admission uses only prepared facts and short identity checks; storage,
  native placement discovery and editor loading stay on their accepted worker boundaries.
- Native publication follows the main-windows feature's explicit failure exception. The set stays
  interaction-gated until all of its exact native publication calls succeed; individual success
  does not transfer a member to ordinary window-close ownership. Reentrant callbacks cannot admit
  an editor mutation, another startup attempt or an ordinary-close session deletion during this
  transition. Whole-set success transfers the members to the ordinary process window owner and
  releases interaction. It does not wait for simultaneous compositor painting.
- Once native publication has started, failure or cancellation before whole-set success keeps one disposal owner for every
  attempted member, including possibly visible windows. It closes the entire native set, joins
  transient work and preserves durable restore records and unresolved command custody before
  presenting startup failure. This disposal is not ordinary window close or prepublication
  acquisition abandonment and cannot remove a restored session member. No native batch or hide
  operation is treated as evidence that an earlier exposure never happened.
- Before native publication starts, a genuinely newly acquired runtime-backed fallback uses the
  existing exact prepublication acquisition-abandonment protocol when cancelled or rejected.
  Original restored records are never eligible for that protocol. Once native publication starts,
  the attempted set instead preserves its durable records through native-failure disposal; a
  successfully or possibly exposed member cannot be reclassified as never visible to delete it.
- The process lifetime owns startup-attempt identity, failure-surface commands and retained
  enrollment, nondispatch and home-reconciliation custody outside each attempted graph. Retry
  admits only one new attempt for the same configured home after prior native/transient disposal
  and required graph shutdown settle. It rereads and validates the durable session; it does not
  reuse failed prepared shells or an old graph's authority. A failure after service publication
  uses the graph's shutdown/disposal boundary. Unsettled custody retains its exact owner and
  blocks successful shutdown or a conflicting reopen; it is not cleared to enable Retry or Exit.
- Startup verification includes failure of the last required preparation, stale first-member
  completion, native failure after an earlier member shows, reentrant close/Retry, cancellation,
  unchanged durable restore records after failed restoration, exact empty-header threadless
  initialization and preservation of unresolved custody. Full-set success is separate from later
  progressive catalog/transcript loading and runtime warm-up.

## Window Detachment And Process Shutdown

- [Fatal crash reporting](../../../doc/systems/crash-reporting/design.md) bypasses this ordinary
  shutdown coordinator. Its separate process initializes only a report surface with fixed
  presentation resources, never the process service graph, persisted theme, home or backend.

- Ordinary close and application Exit are coordinated with process window construction through
  one bounded admission gate. The app revalidates whether a closing window is final before
  admitting shutdown or durably removing it; overlapping closes cannot each assume another
  window will survive. Confirmation carries exact attempt, window-set, and work revisions and
  owns no stop or mutation authority until the shared shutdown coordinator admits the barrier.
- The shutdown coordinator freezes all execution/successor cuts, joins exact process-owned work,
  preserves durable queue custody and proven-undispatched pending work under the CAS-live shutdown
  completion rules, and composes resident-preserving draft flush with typed session
  publication. It retains windows and claims until success, then joins service/runtime disposal
  before process exit. Explicit Exit and final ordinary close retain their distinct restore modes.
- A failed barrier releases interaction gates from the retained coherent state without restoring
  cancelled continuations or repeating possible dispatch. Closing a settings or auxiliary window
  never becomes the final-main-window execution barrier.

## Prepublication Window Abandonment

- Acquisition, creation and initial-editor consumers retain typed home service references; the
  process owns the home lifecycle. Initial-editor admission uses the exact reference supplied by
  its acquisition service and preserves acquisition custody on a provenance mismatch. Retained
  consumers cannot keep a retired home open or adopt a replacement home generation.
- The app admits abandonment only for an exact acquired main window that has not been published
  visible. Once visibility is published, only the separately owned ordinary-close and Exit
  lifecycles apply.
- `WindowId` is the sole operation and bounded-flight identity. Private fixed operation facts bind
  the exact acquisition fingerprint; they authorize no substitute thread, draft, runtime, root,
  placement, disposition, or fresh operation identity.
- One shared process registry admits at most 256 window acquisition or abandonment flights and at
  most one flight per `WindowId`, with no resident wait queue. Definitive settlement releases the
  exact entry; indeterminate and pending reconciliation retain it.
- The app composes the typed session, catalog, durable-job, and Syndic participants into one
  `HomeCommand`. It neither inspects package-private records nor performs a compensating cleanup
  command. A reused pristine thread is validation-only; a created fallback contributes the typed
  authenticated pristine-deletion mutation.
- App abandonment and reconciliation custody is opaque, move-only, and retained across
  cancellation and acknowledgement loss until exact classification. `NotCommitted` returns the
  same abandonment custody, committed or exact-abandoned settles it, and indeterminate transfers it
  synchronously into the sole reconciliation owner. Pending reconciliation returns that same owner;
  exact-acquired returns retry custody; collision grants no cleanup authority.
- Natural reconciliation combines only bounded State and Syndic observations bracketed by equal
  home-revision reads and returns exact acquired, exact abandoned, or collision. Revision drift is
  retried within a fixed bound; exhaustion preserves custody. A fresh same-home service uses stable
  operation facts and fresh typed handles rather than an old-generation capability, and it never
  guesses completion from missing or partial state.

## Typed Home Integration

- Branch resolution commands compose the exact State admission with Syndic discussion-gate
  admission and parent-frontier validation. The app first resolves the request index and latest
  attempt through typed reads, then revalidates the same proposal at the writer. Its reconciliation
  closure covers the job, live/request/attempt/latest indexes and exact discussion gate; parent,
  binding and turn observations remain preconditions, never substitute outcome evidence.
- Parent input admission composes the generated Syndic input/turn/provenance closure with the
  exact job's `starting_parent` transition. Terminal failure composes that job transition with
  gate release; success composes job success, gate release and archive in one `SyncAll` command.
  The sole home reconciliation registry receives ambiguous outcomes before any response or wake.
  Participant APIs alone do not authorize a standalone gate, parent input or archive publication.

- The package never opens Fjall, reads raw keyspaces, or constructs storage encodings. It receives
  typed domain handles, repositories, revisions, and command outcomes.
- Service configuration supplies a nonzero minimum capture reserve to `beryl-home-store` and
  receives the home-store-owned opaque turn-start space requirement after dependency validation.
  The app neither knows nor recreates the dependency's fixed budget arithmetic.
- Direct submission and accepted-input promotion pass that same opaque requirement intact to the
  typed free-space query. Callers cannot provide a path-specific or pre-aggregated total.
- Correctness-sensitive success is published only after the typed home command's required durability
  barrier. Health transition invalidates mutation authority by generation while windows may retain
  only last coherent inert presentation.

## Same-Home Replacement Contribution

- The process root retains the bounded Activity enrollment owner across current and candidate
  service graphs. Disposing either graph, including consuming close, retirement failure and
  implicit drop, cannot dispose this outer custody. Fresh same-home candidate settlement joins
  each original home reconciliation handle with its typed Syndic witness before CAS-live startup
  convergence. Exact old or exact new agreement releases retired custody without publishing a
  token for the ended runtime lifetime; read failure, disagreement or unresolved submission
  retains the slot and blocks replacement. Final shutdown requires the owner to have no pending
  slots; otherwise it reports failure and retains the process owner. This owner is not another
  command-outcome registry and cannot authorize provider work or publish a partial graph.

- The process graph owns one durable branch-handoff coordinator with the lifecycle and bounded
  scan policy in the [handoff system](../../../doc/systems/branch-discussion-handoff/design.md).
  Its private preparation follows CAS-live candidate convergence and uses fresh typed State and
  Syndic participants. Ordinary job scanning and exact execution admission remain fenced until
  whole-graph publication. Graph retirement joins its scanning and admitted work; no branch worker,
  page, queue, request response owner or reconciliation descriptor transfers to a replacement.
  A narrow process-owned exception preserves bounded immutable parent nondispatch proofs and
  their existing settlement slots across home replacement. They retain no old service handle,
  session, execution capability or permit. Fresh candidate settlement of these proofs precedes
  CAS-live startup convergence; ordinary handoff scanning still follows it. Pending proofs block
  final shutdown readiness and successful pending preservation. If shutdown cannot settle them,
  it reports failed shutdown and retains custody rather than completing process exit.

- `beryl-app` contributes one complete unpublished app service graph to the process-wide same-home
  replacement. Before candidate construction, the old graph fences admission and disposes its
  connections, brokers, routers, schedulers, projections, leases, workers, custody, subscriptions,
  and service-local handles; none transfers.
- Candidate construction, durable convergence, and supervisor attachment remain unpublished. The
  app exposes the candidate only through outer atomic whole-stack publication.
- Failure before publication disposes the candidate and publishes no authority. After publication,
  schedulers and projection consumers establish fresh authority from durable typed facts under the
  new generation.
- CAS retirement consumes its service and returns only exact home/service/failure identity and
  bounded disposal evidence after its runtime, connection, broker, scheduler and compaction workers
  settle. It retires outage retention before joining ingress. When that service owns the failed
  home, retirement separates the owned home from the disposed service; a reference-only component
  cannot manufacture home ownership. Neither form proves retirement of other graph components.
- A retirement failure grants no reopening authority. Any owned failed home stays in explicit
  failure custody for terminal disposal, preserving the home-store reconciliation and lock rules.
  Successful CAS retirement alone cannot publish replacement services or restart ordinary work.
- Fresh CAS preparation accepts the owned reopening candidate, reacquired Syndic handles and the
  same immutable configuration inputs as initial preparation. It runs the shared sequential
  candidate convergence, constructs fresh generation-bound services and configures managed-session
  preparation behind the worker fence. The initial and recovery candidate capabilities are never
  converted into each other or exposed as early healthy stores.
- An unpublished replacement retains complete disposal ownership. Construction or convergence
  failure and cancellation join all prepared CAS workers before aborting its candidate. Candidate
  failure returns explicit failed-home custody to outer recovery; no ordinary store reference,
  prepared worker or outage payload crosses to the next attempt.
- The outer app graph requires every component's retirement and preparation before handing the
  complete candidate to the process supervisor. CAS-only disposal or preparation is not a complete
  graph receipt. The supervisor's single attempt, attachment, retry and atomic publication follow
  the [backend-runtime composition](../../../doc/systems/backend-runtime/design.md#same-home-recovery-composition).
