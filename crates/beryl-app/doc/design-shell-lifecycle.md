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

## Runtime And Root Admission Contribution

The selected-path service runs filesystem/backend validation on tracked workers and retains an
exact home-generation and invoking-window source. Canonical duplicates resolve before a second
managed runtime launch. Its process selection lease excludes overlapping selection, acquisition
and close through validation, durable reconciliation and coherent publication. A failed validation
with unsettled helper/process cleanup retains that exact owner and lease in the service; cancellation
or dropping the returned error cannot admit another probe. Retirement cancels and joins active
validation and settles retained cleanup before reporting completion.

For the first runtime, the service composes the admitted registry/home-root facts with Syndic
thread/draft creation, initial catalog claim and replacement of the sole exact threadless window
claim/session in one revision-checked HomeCommand. Later runtime/root admission preserves selection.
The committed capability retains publication custody; indeterminate outcomes retain the original
operation's targeted reconciliation. Exact old restores the initiating command, exact new permits
coherent publication, and collision/successor becomes terminal Unavailable with retained intent.
The New Thread controller owns visible pending, cancellation, error and Unavailable presentation.

## Running-Thread Selection Contribution

The published graph exposes a narrow weak process-work reader with revision-bound searchable pages,
exact logical positions and separate total and attention counts. A shell retains bounded pages and
its own request identities; it owns no execution session or aggregate process catalog. Source scans
and transcript preparation run on tracked workers. Close suspends these reads and awaits their
actual resource release before retiring Home access; cancellation preserves the last coherent view.

One process selection lease authenticates the invoking published window and exact published member
set against window acquisition and close admission. It remains retained through indeterminate claim
replacement and final view publication. The app joins State's opaque Session/Catalog replacement and
Syndic's fixed summary pair with runtime/root validation, using one participant per domain. Existing
window reveal revalidates the exact active claim and surviving shell without creating a window.

After coherent attachment, the selected shell retains the source-owned Session window record,
composer selection and existing native reservation. Settled startup construction custody releases
its duplicate composer service reference; later selection does not retain an obsolete acquisition
as current authority. Lifecycle attention routing uses those cached coherent view identities and
acknowledges only the exact displayed token after successful activation or notice dismissal.

## Startup Failure Presentation

`startup_surface::StartupSurface::open_busy` and `open_failure` mount the dedicated windows from
the [Beryl-home GUI contract](../../../doc/features/beryl-home/gui.md) without a home or service
graph. They return a typed GPUI window handle or an explicit native-open error. `open_failure`
borrows diagnostic text and retains only the feature-capped value in one read-only text input;
text bindings and built-in appearance are available before ordinary application composition.

The required callback receives `StartupSurfaceEvent::Retry(StartupAttempt)` or `Exit` after local
admission and outside the surface borrow. `StartupAttempt` identifies the exact surface and attempt;
`complete_failure` rejects any nonmatching or exited attempt and replaces the same detail input
without undo history. `request_retry` and `request_exit` share pointer and keyboard admission.
Native close and the busy countdown request Exit without removing the window or quitting GPUI.
The process owner retains cleanup custody and removes the surface only through its own lifecycle.

`attempt` snapshots the current surface/sequence identity, including initial startup. Each Retry
advances that identity. `block_cleanup` accepts only the matching snapshot, including after Retry
completion or an Exit intent; a later Retry invalidates every earlier snapshot. It retains bounded diagnostics, irreversibly closes
Retry admission and exposes the feature's explicit Quit Anyway command. Deferred events revalidate
their surface before delivery. The opaque Quit Anyway request is created only by explicit blocked
surface activation; consuming it terminates the current process directly on Windows without
unwinding, GUI shutdown, storage calls or panic-report publication. It grants no graceful disposal
proof. Native close always remains an orderly Exit request. Process-entry mounting remains owned
by the executable composition root.

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
- Failed initial service preparation has an explicit consuming disposal result. It joins prepared
  components before closing the original candidate, and retains an unsuccessful home-close result
  in the process owner while returning the original preparation failure to its caller. Publication rejection and
  cancellation use this same boundary. A graph whose worker-start fence could not be released
  similarly joins its unstarted services before explicitly closing its published home. Destructor
  cleanup alone does not prove home closure or authorize another attempt. An incoming candidate
  rejected before service-attempt admission is returned unchanged to its caller; it cannot replace
  existing graph or failed-close custody.
- Private CAS preparation configures runtime interest and the exact attached execution-session
  owner through candidate runtime-domain and Asset-revision validation. It preserves ordinary
  policy, capacity, token-directory and owner checks while leaving ordinary admission closed.
  Configuration only retains immutable preparation context and notifies the fenced scheduler;
  it starts no runtime or session discovery. Failure consumes private service custody and joins
  workers before candidate disposal.
- Managed-session configuration retains one immutable admitted token-directory root. It resolves
  the runtime-native token path from the exact current runtime at launch, without a startup-only
  list of runtime identities or retained references to an earlier home generation.
  It also retains the composition-supplied optional immutable WSL supervisor descriptor through
  initial preparation and recovery; runtime records cannot substitute another artifact.
- Failed launch or retirement retains the exact runtime's disposal-only owner. Explicit retry
  settles that owner before a successor launch, and repeated shutdown cannot discard a failed
  entry. Failed consuming service close retains the original service and partial graph privately;
  that graph is no longer published for ordinary access. Normal close and recovery retry the same
  cleanup custody and fence graph/home replacement until disposal is complete.
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
- The process service owner retains one main-window registry across initial-service retries,
  sharing the original process admission gate. Its published-window factory accepts only immutable
  request, activation, operation-identity and editor-configuration sources. It derives home/domain
  references, marker sealing, turn-start requirements and submission authority from the complete
  published graph; callers cannot substitute those services. Acquisition and creation share the
  same exact home-reference object.
- Immutable creation request sources resolve remembered runtime/root identities on the worker
  using borrowed typed references from the exact current window-service graph. They do not capture
  an earlier graph's home or State handles. Reusing a source after same-home recovery therefore
  resolves against the replacement graph and retains the ordinary generation and admission fences.
- The GUI-facing factory performs no storage reads and returns a worker-transferable bundle.
  Worker consumption creates the generation-bound restoration attempt and restore set, validating
  the graph lifetime before reads and before returning work. Creation admission validates exact
  source identity, healthy original home authority and submission binding before reserving a window.
  An old reference remains invalid after Retry even when its numeric generation is retained.
  Retirement during preparation uses the existing typed cleanup path and cannot discard acquired
  claims or prepared editor custody. No GUI entity or constructed widget configuration crosses
  the worker boundary.

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

## Native Exit Geometry Capture

- Exit obtains normal outer screen geometry, normal/maximized restore state and current monitor
  identity/work area/scale from a fallible owned GPUI window operation on the GUI executor. The
  bounded capture reads the exact published, unremoved native window without transferring a raw
  handle or lifetime to a worker. It does not enumerate monitors, mutate placement or activate a
  window. Hidden, removed, closing, destroyed and fullscreen windows are rejected.
- Native normal placement is converted from Windows workspace coordinates to physical screen
  coordinates with checked top/left work-area offsets, including negative coordinates and tool
  windows. A minimized window retains its normal rectangle and pre-minimize maximized restore
  intent. The returned monitor facts must agree with current window DPI; native failures and
  changed monitor facts are errors, never zero/default geometry or a replacement monitor.
- Capture returns physical geometry with the exact monitor scale; the app owns conversion into
  its durable logical placement model. Virtual-desktop observation and complete-set publication
  remain separate obligations. This snapshot grants neither destruction nor shutdown authority.
- Acceptance requires round-trip conversion and overflow rejection, real published normal,
  maximized and minimized-window capture without activation, rejection before publication and
  after removal, and independent review of coordinate and native lifetime handling.

## Durable Exit Geometry Conversion

- The app converts captured physical normal outer bounds and monitor work area using the exact
  captured monitor scale. The durable model stores integer logical origins and dimensions; round
  each component to nearest integer, with halfway values away from zero. Preserve negative origins.
  Reject nonfinite values, nonpositive dimensions, rounded zero dimensions and values outside the
  durable integer ranges before casting. Do not clamp, substitute geometry or mutate native state.
- Preserve normal/maximized restore intent and the captured monitor UUID in canonical string form.
  A separately observed optional virtual-desktop identity is passed through unchanged; this pure
  conversion neither observes desktops nor treats an absent identity as a newly observed desktop.
  The caller remains responsible for binding both observations to the exact original window.
- Conversion is fallible and all-or-nothing, with no durable write, shutdown transition or cleanup
  authority. Verify fractional scales, negative origins, rounding boundaries and range rejection,
  plus native-capture conversion and preservation of monitor, display state and desktop facts.

## Published Native Observation Lifetime

- A desktop observation worker consumes a distinct move-only lease for one exact published,
  visible, unremoved Windows window. Admission rejects a hidden, unpublished, closing, destroyed,
  fullscreen or already leased window, and a window with an active native confirmation. Hidden
  startup-operation admission keeps its separate single-use contract.
- The lease protects the original native instance until the worker completes or unwinds. Explicit
  removal and an admitted ordinary close defer native destruction to GUI settlement. Ordinary close
  callbacks still decide admission; an active observation does not grant close permission or latch
  hidden-startup close intent. No raw-handle probe substitutes for this owned lifetime.
- One operation and one GUI completion continuation may be retained per window. Abandoning the
  completion observer cannot release the worker's lease. Completion reports native destruction or
  failure; a surviving window may admit a later observation after exact GUI settlement, allowing a
  later Exit attempt. No waiter queue, retry or worker-side destruction is introduced.
- The worker uses the handle only for desktop observation and retains its lease through COM
  teardown. The owning Exit attempt must retain shell custody and drain GUI settlement before
  recovery, final teardown or normal quit. This primitive grants no desktop, durable-session or
  shutdown-completion authority.
- Acceptance requires native evidence for admission refusal, duplicate exclusion, sequential reuse,
  close veto and allowed close, explicit removal, observer abandonment, worker unwind and exactly-once
  destruction on the GUI executor, plus independent lifecycle review.

## Published Desktop Observation

- The app worker consumes the published native lease and reads only that window's desktop with
  the public `IVirtualDesktopManager::GetWindowDesktopId` operation. It initializes a worker COM
  apartment and releases the manager and its own apartment reference before releasing the lease,
  including on failure or unwind. Existing compatible COM initialization remains balanced.
- Return the exact canonical `VirtualDesktopId` on success. Initialization, manager creation and
  query failures retain their distinct stage and HRESULT. Do not turn failure into absent or saved
  desktop identity, retry, enumerate desktops, move a window or switch the active desktop.
- This synchronous worker entry point schedules no work itself. Its caller owns off-GUI execution,
  exact window/result association and the separate GUI lease-settlement receipt. A returned desktop
  result alone grants no recovery, publication, destruction or quit authority.
- Verify real published-window identity against an independent native read, unchanged activation
  and placement, fresh and existing compatible apartments, incompatible-apartment failure, balanced
  COM lifetime and sequential lease reuse. Review native and COM ownership independently.

## Retained Exit Placement Capture

- After work and resident drafts are ready, the original Application Exit attempt admits one
  placement capture of its complete published main-window set. It retains the original window
  identities, native handles through owned leases, shells and interaction gates. Duplicate capture
  refuses; final ordinary close does not capture an Exit restore set.
- Capture each window's geometry and admit its desktop lease in the same GUI update, validating
  the original window identity. Desktop reads run off the GUI executor. Process the bounded set
  sequentially, retaining at most one worker and native settlement continuation at a time. Every
  admitted lease is drained even after another capture fails; no failed member is retried or
  replaced and no partial placement set is published.
- Retain the complete converted result or original capture failure in the attempt. Worker unwind
  is a capture failure and still requires its exact GUI settlement. Missing settlement keeps
  recovery fenced; desktop failure alone does not lose settlement custody. Draft release and
  coordinator recovery refuse until the capture has drained all admitted native operations.
- A retained GUI continuation owns the running owner through one completion callback outside
  owner borrows. Caller abandonment cannot cancel that custody. Results are available only for
  that original work-ready attempt while drafts remain ready; release invalidates their use.
  Capture completion grants no durable publication, window destruction or quit authority.
- Verify real native capture and exact identity association, duplicate/refused admission, delayed
  worker ownership and recovery exclusion, failure/unwind with settlement, and original-attempt
  recovery after drainage. Independently review lifecycle and partial-failure handling.

## Exit Placement Preparation Policy

- Placement preparation validates the exact active Exit request and original work-ready Application
  Exit attempt, then schedules its retained complete-set capture. Ready delivery preserves the
  request, attempt, drafts and interaction gates for durable session obligations.
- A terminal capture failure uses the exact draft recovery handoff only after native settlement.
  Deliver both the original capture failure and the separate recovery result or error. Missing
  settlement refuses recovery and retains custody; recovery cancellation cannot replace the cause.
- Scheduling refusal returns the original request without callback or recovery. Accepted scheduling
  retains the owner and request through exactly one terminal callback outside owner borrows, even
  when the caller abandons its handle. Duplicate capture remains refused without a retry.
- This policy neither completes the command nor publishes session state, destroys windows or
  authorizes quit. Verify readiness/request refusal, exact result retention, capture failure with
  successful recovery, failed release preserving both causes and custody, and caller abandonment.
  Independently review the lifecycle and recovery boundary.

## Exit Session Command Preparation

- A worker prepares one session command from the retained complete Exit placement set and the
  original home's typed session handle. Preparation performs no write and grants no readiness,
  recovery, disposal or quit authority. The running owner remains responsible for admitting this
  work only after exact work, draft and placement readiness.
- Before reading durable state, reject empty, duplicate or over-capacity captured sets. Read the
  home revision, session domain revision and minimal session snapshot, requiring Running intent
  and exact identity equality with the captured set. Bind each placement by window identity to
  that snapshot's record revision; input order has no significance.
- Recheck the home revision after the bounded reads. Drift, absent session, read failure or set
  mismatch returns an error without preparing a replacement from newer facts. Build one
  HomeCommand using the original home/domain/session/window revisions and the complete typed
  Exit contribution. The serialized writer retains responsibility for rejecting later drift.
- The move-only command is returned to its caller without execution, retry or retained background
  work. Its later executor must preserve ordinary command outcomes and reconciliation custody;
  command preparation alone cannot complete the running Exit attempt.
- Verify threadless and multi-window identity binding, shuffled order, capacity, rejected incomplete
  or foreign sets, absent/already-exiting state, no preparation writes and writer rejection after
  durable revision drift. Independently review this persistence preparation boundary.

## Exit Session Execution Outcome Custody

- A worker prepares and executes the complete Exit session command once. Preparation refusal
  performs no write. Definitive noncommit retains its typed evidence; durable commit retains its
  receipt, optional later failure and optional local-finalization capability without translating
  a later failure into noncommit or discarding its capability.
- An indeterminate result synchronously installs its sole reconciliation custody before returning
  an app-owned pending outcome. That outcome retains the original command failure and exact
  registry handle. Abandoning the returned value leaves the installed home-store gate intact.
- An explicit worker reconciliation pass consumes that pending owner and uses only its original
  handle. ExactOld proves nonpublication; ExactNew returns the original durable receipt. Both
  preserve the original failure. A failed pass returns the same pending owner with the separate
  reconciliation failure. Collision and the unsupported successor classification remain blocked;
  neither is publication or recovery evidence. No pass executes a replacement command, clears a
  gate, retries automatically or switches home identity. Same-home recovery owns any required
  health restoration and exact-handle retrigger before a later pass can observe its result.
- This worker boundary grants no running-attempt readiness, draft release, service reopening,
  native disposal or quit authority. Its caller must retain the outcome under the original attempt
  and separately compose those transitions. Verify complete-set commit, definitive refusal,
  postcommit failure preservation, installed ambiguity, failed/foreign reconciliation retaining
  custody, exact-new resolution and abandonment. Independently review outcome and custody mapping.

## Retained Exit Session Execution

- The original work-ready Application Exit attempt admits one session worker only after complete
  draft and placement readiness. Admission reserves its result slot before moving the complete
  process service owner to the worker. Final ordinary close, duplicate admission and unavailable
  services refuse without scheduling or callback.
- The worker uses that original graph's home and typed session handle and the retained placement
  set. It executes once and returns the complete service owner with the exact preparation or command
  outcome. Worker unwind returns the service owner but remains an unproven publication outcome;
  it never implies noncommit. No replacement write or automatic reconciliation is scheduled.
- A strong GUI continuation retains the running owner through service return and result retention,
  then notifies once outside owner borrows. Abandoning the caller cannot discard worker custody.
  The result remains owned by the original attempt; notification alone consumes no receipt,
  local-finalization capability or reconciliation handle and grants no session readiness.
- Once admitted, session custody fences draft release and coordinator recovery until a separate
  outcome policy proves their prerequisites. This fence includes settled but unconsumed results
  and unwind. Shells, claims, drafts, work readiness and interaction gates remain retained. This
  boundary grants no disposal or quit authority.
- Verify refused and duplicate admission, delayed off-GUI execution, caller abandonment, exact GUI
  delivery and service return, commit and unwind custody, and recovery exclusion before and after
  settlement. Independently review the lifecycle and persistence boundary.

## Retained Exit Session Reconciliation

- An explicit reconciliation pass admits only the original attempt's indeterminate session
  outcome or its retained failed reconciliation. It transfers that exact reconciliation owner
  and the complete original process service owner to one worker, reserving the attempt's slot
  first. Missing readiness, unavailable services, concurrent passes and terminal outcomes refuse
  without consuming custody or invoking completion.
- The worker uses the original graph's home and the retained handle. It preserves the complete
  typed reconciliation result, including original command failure, exact receipt, pending owner
  and separate pass failure. It never publishes a replacement command or retriggers recovery;
  same-home recovery remains responsible for health restoration and an exact-handle retry.
- A strong GUI continuation returns services and retains the result in the original attempt before
  invoking one completion outside owner borrows. Caller abandonment does not cancel the pass.
  Worker unwind returns services and retains an unproven outcome; installed home-store custody
  remains authoritative, and no ordinary retry is inferred from unwind.
- Every result, including ExactOld and ExactNew, keeps the existing draft and coordinator recovery
  fences until a separate outcome policy consumes its proof. Reconciliation delivery alone grants
  no session readiness, gate release, window disposal or quit authority.
- Verify invalid and duplicate admission, delayed worker ownership, caller abandonment, exact GUI
  delivery, retained failed passes, explicit same-handle resolution, terminal refusal and unwind.
  Independently review the original-attempt ownership and persistent outcome boundary.

## Retained Exit Session Readiness

- Session readiness is derived from the original attempt's retained result while its complete
  services, work, drafts and placement set remain ready. A clean committed result, or exact-new
  reconciliation, must supply a receipt accepted by that current healthy home's typed session
  handle and affecting the session domain. Foreign, stale, unavailable or unaffected-domain
  receipts refuse readiness. This inspection performs no durable read or write.
- Preparation failure, definitive noncommit, pending execution or reconciliation, exact-old,
  collision, unsupported successor and unwind cannot establish readiness. A commit with a later
  failure or local-finalization capability remains blocked for separate failure handling; neither
  its durable receipt nor that capability silently converts the failed attempt into success.
- Inspection borrows and preserves the exact outcome, original failures, receipt and capabilities.
  Readiness is not cached across service transfers or home availability changes. It releases no
  draft, coordinator or interaction fence and grants no window destruction or process quit.
  The later final-teardown policy must compose this proof with its remaining obligations.
- Verify clean and reconciled readiness, current-home/session receipt provenance, unavailable
  health, all non-ready outcome classes, retained failure/capability custody, and native original-
  attempt readiness with unchanged recovery fences. Independently review this persistence boundary.

## Exact-Request Exit Session Publication

- The Exit publication handoff validates the active request and its original work-ready Application
  Exit intent before admitting the retained session worker. Draft and complete placement readiness
  remain required by that worker. Refusal returns the original request without callback or write.
- Accepted scheduling retains the exact request until service return and outcome retention, then
  delivers once on the GUI executor outside owner borrows. Delivery derives readiness through the
  retained outcome's current home/session check; it never caches readiness or consumes its proof.
- Failed readiness delivers a diagnostic while the original typed outcome, receipt, capability or
  reconciliation custody remains in the attempt. It starts no reconciliation, replacement write,
  draft release or coordinator recovery. The caller must separately compose failure recovery;
  delivery alone cannot finish the command, release interaction gates, dispose windows or quit.
- Verify exact request identity through success and failure, refused early and duplicate scheduling,
  caller abandonment, complete service return, durable commit, definitive noncommit, postcommit
  failure and indeterminate custody with unchanged fences. Independently review this handoff.

## Interrupted Exit Recovery Ownership

- The running owner latches a reported session-publication failure as cancellation of the exact
  Exit request. The process-owned recovery supervisor preserves its bounded immutable source and
  result evidence through graph retirement under the
  [interrupted-Exit recovery contract](../../../doc/systems/backend-runtime/design.md#interrupted-exit-during-same-home-recovery).
  Original service handles, receipts and close tickets cannot authorize replacement-generation
  work. No retained evidence object itself releases draft, work or interaction fences.
- Session command preparation retains the complete expected source and resulting session/window
  revisions and placements before execution. Known commit remains known after a later failure;
  pending registry custody, original failure and any local-finalization capability remain owned
  until their applicable settlement or retirement. New-generation validation uses fresh typed
  handles and exact immutable facts, never old receipt acceptance or a replacement Exit command.
- The app composes the separate session resume contribution and retains its own execution and
  reconciliation outcomes. It neither calls startup begin-restore on surviving windows nor infers
  Running from a healthy replacement graph alone. Service rebinding and resident draft/work
  settlement remain separate prerequisites to atomic coherent reopening of the cancelled request.
- Selected-shell adoption validates the exact gated root, mount, resident and retained draft against
  authenticated candidate window facts before invoking resident adoption. Success synchronously
  installs the fixed-size window record and fresh selection, transfers the existing native-window
  reservation and renews that draft's close ticket. Refusal preserves shell and draft custody;
  preparation follows its existing cancellation/cleanup contract. The recovered shell cannot enter
  startup publication or disposal, and old-generation appearance prevents interaction release.
  Threadless bindings, appearance/notice replacement and process recovery settlement are separate
  prerequisites; this shell adapter alone grants no publication or reopening authority.
- Retained shutdown drafts route shell adoption through the existing exact window entry. The
  aggregate must be prepared, idle and unreleased, and its resident and close ticket must match
  before adoption. Success keeps the renewed ticket in that entry and clears cached readiness;
  other windows may still adopt before fresh draft readiness is established. This does not settle
  work or complete the cancelled Exit request.
- Running-owner attachment routes its exact ready preparation flight through that retained draft
  entry. Success renews the captured resident ticket synchronously with shell adoption, returns
  candidate/session custody once and consumes the flight. Missing or busy drafts, stale request
  identity and unready or cancelled preparation refuse attachment without discarding custody.
- Threadless recovery authenticates the exact surviving window in the replacement generation of
  the same home through fresh candidate handles. The session must retain its sole threadless
  member, without a selected thread, remembered target, fallback, reverse claim or configured
  runtime. Bounded reads must observe one unchanged home revision. The worker retains only the
  fixed-size window record and home/generation identity; explicit revalidation rejects changed
  facts. This proof neither begins startup restoration nor grants shell attachment, publication
  or interaction release by itself.
- Threadless shell adoption requires the exact gated retained draft and retired threadless
  construction with no composer. It consumes authenticated fixed-size facts for the same window
  and home in a replacement generation, transferring the existing native reservation. Refusal
  preserves both inputs. The recovered shell remains threadless, excludes startup paths and
  requires fresh appearance before interaction release. The process recovery owner must still
  revalidate candidate facts and settle the cancelled request before publication and reopening.
- Retained shutdown drafts also route threadless adoption through the exact existing window entry
  while prepared, idle and unreleased. Refusal preserves facts and cached readiness; successful
  shell adoption consumes the facts and clears readiness. This route retains the original draft
  and adds no worker or service custody.
- Running-owner threadless attachment requires the exact active cancelled request, successful
  graph retirement and available original session custody, without a resident preparation,
  settlement or outstanding resident frame. Authenticated facts must match the supplied candidate's
  home and generation. It routes through the retained drafts without consuming candidate/session
  custody or performing storage work on the GUI thread. Refusal preserves all inputs; success only
  consumes the fixed facts and clears draft readiness. Fresh candidate-state revalidation, session
  settlement and appearance/notice replacement still precede publication and coherent reopening.
- Fresh appearance registration requires an attached recovered shell, its shutdown fence and
  published native-release custody, with the old appearance publication target retired. The fresh
  owner must match the attached same-home replacement generation. Existing registration admission
  and final validation precede one GUI commit that replaces shell/editor appearance, notice arbiter,
  widget, ingress lifetime, subscription and native-release ownership. Refusal preserves bindings;
  success retains shutdown fencing and inert notice commands. An adopted recovery-fenced editor
  accepts appearance only after predecessor snapshot custody is consumed and fresh service/close
  custody is installed. Later theme updates use ordinary exact-home validation. This registration
  adds no worker or retained recovery queue and does not settle the cancelled request.
- Successful resident adoption releases its exact invalidated predecessor widget protection before
  discarding the predecessor snapshot. This consumes obsolete protection custody only; the widget
  remains disabled and the resident remains recovery-fenced until process recovery settles.
- A surviving published shell may retain its native handle in the process restore set or already
  have transferred release ownership to its root. Recovery uses the same registration validation
  for both custody forms. A retained handle replaces its appearance owner only after registration
  succeeds and installs fresh root-release ownership. Settled startup destruction enrollment and
  its exact receipt remain retained; active or incomplete disposal refuses binding. Later handle
  release, where allowed, also uses the fresh owner. Refusal preserves custody,
  and neither form releases shutdown interaction or grants whole-graph publication.
- Running-owner appearance binding requires the exact active cancelled request, successful graph
  retirement and available original session custody, with no preparation, settlement or outstanding
  resident frame. The fresh appearance target must match the candidate identity. An idle, prepared,
  unreleased draft entry and retained published shell must both identify the exact native window.
  Binding preserves candidate/session custody, draft readiness and process publication ownership;
  it installs only the shell's fresh appearance/notice ownership and keeps interaction fenced.
- A returned successful candidate settlement may be revalidated through the same single worker
  slot under the exact cancelled request and successful retirement. Fresh reads must prove either
  the original noncommit Running state or the exact committed resume result. This pass performs no
  command execution or reconciliation. Pending work excludes another pass; refusal and worker
  return preserve original outcome and candidate custody, including after a stale request or
  unwind. Failed validation remains retained and fenced. Revalidation alone grants no publication,
  draft/work release or interaction reopening.
- Complete recovered shell-binding validation reads the existing published set and retained drafts
  under the exact cancelled request, successful retirement and returned successful session custody.
  Every published shell must have exactly one idle prepared draft, recovered candidate identity,
  fresh appearance/notice ownership and native release custody. Selected shells must retain the
  exact renewed resident and mount close ticket; threadless shells must have neither composer nor
  composer draft. Validation retains all gates and custody and does not cache success. It performs
  no storage reads and cannot substitute for fresh durable validation or draft/work settlement.
- An adopted resident's renewed service close ticket may be released while its local recovery and
  shutdown fences remain installed. Admission requires the exact recovered resident/mount binding,
  no predecessor snapshot, pending resident mutation or close worker, and no fresh flush or disposal.
  Use nonblocking service access and retain exact completion in the mount's existing release slot;
  busy access remains pending without another worker. Repeated completion preserves the same local
  ticket and fences. This settles only fresh service close custody, performs no durable write and
  grants no graph publication, ordinary close, disposal or interaction release authority.
- The running owner routes recovered draft release under the same exact cancelled request and
  successful retained candidate settlement as complete binding validation. Validate every published
  shell and its unique retained draft before releasing any renewed service ticket. Each selected
  shell uses the existing close-release boundary; a validated threadless shell is already settled.
  Pending or failed passes retain every draft and any completed mount evidence for a later explicit
  pass. No aggregate readiness/release flag changes, worker, collection or storage operation is
  introduced. Completion settles service close custody only; work settlement, graph publication
  and coherent interaction release remain separate prerequisites.
- Verify reported-failure cancellation through late exact-new resolution, source evidence across
  replacement, refused stale or unproven state, resume outcome custody and unchanged native windows
  and claims. Independently review lifecycle and persistence composition.

## Ordinary Running-Home Recovery Ownership

- The running process owner admits an ordinary returned home failure independently of Exit or
  native-close requests. The existing process supervisor owns the exact configured home, failed
  home/service generation, unique attempt identity, retry deadline and publication slot under
  [same-home composition](../../../doc/systems/backend-runtime/design.md#same-home-recovery-composition).
  A runtime failure in a Healthy home does not admit this route. Duplicate notices for the owned
  failed generation coalesce; old-generation notices and stale attempt completions cannot replace
  current custody. No synthetic Exit, close, startup restoration or session-resume command is used.
- Before graph retirement, the owner fences home-dependent command admission and captures every
  surviving published window in its bounded process window set. Capture records the exact native
  identity and reservation, shell/root/mount/resident identity, session/window and paired-claim
  facts, selection, placement and last coherent presentation. Selected residents use failed-home
  move-only capture, including unsaved live edits and original publication/reconciliation custody;
  threadless residents retain their exact construction and sole-member session facts. Native
  windows, focus, position, size and desktop placement remain preserved. This capture uses a
  recovery fence, not healthy close admission or a fictitious successful draft flush.
- The preserved set and admission fence belong to the process owner across bounded attempts.
  Each attempt owns at most one worker flight, and each resident owns at most one preparation
  flight. Worker delivery returns candidate, service and resident/outcome custody even when stale,
  cancelled or unwound. A refused or incomplete capture, admitted mutation, unsettled native
  operation, pending resident work or incomplete old-service retirement blocks replacement.
  Abandoned GUI delivery drains candidate resources through their owning cleanup boundary.
- The unchanged-session route has no session mutation to undo. Fresh typed candidate State access
  validates the captured complete Running session, exact membership/window revisions and placements,
  selected-thread bindings and paired Active claims under one unchanged home revision. Threadless
  membership retains the existing no-selection/no-fallback contract. Changed membership, claims,
  selection or unsupported successor refuses this route; absence or a healthy graph is not proof.
  Reads and outcome reconciliation run off the GUI thread. Actual admitted command outcomes settle
  through their original typed custody before any dependent qualification; terminal uncertainty
  keeps the affected request unavailable and cannot be converted into noncommit or replay authority.
- An already admitted Exit or close retains its distinct request, original command outcome,
  reconciliation and native-settlement custody. It uses the corresponding interrupted lifecycle
  recovery contract where settlement requires a resume or membership restoration. The ordinary
  attempt never adopts its facts as unchanged Running evidence. The process owner serializes
  lifecycle admission with capture and publication: a request admitted first must settle or transfer
  to its exact interrupted route before ordinary capture; a request arriving after fencing cannot
  mutate the preserved set or start native destruction. A shutdown racing publication follows the
  system cancellation-or-new-graph-close rule and cannot consume unpublished resident custody.
- Successful complete retirement precedes reopening. Fresh candidate work validates session and
  claims, settles retained process enrollment/nondispatch and original resident outcomes, and uses
  the complete graph preparation and convergence contracts. No old service capability transfers.
  Failed candidate validation or construction disposes and joins candidate work, retaining actual
  writes and failed-home lock/reconciliation custody for the same supervisor's bounded retry.
- Selected attachment follows the checked preserved-resident adoption contract in
  [composer authority](design-catalog-and-composer.md), keyed by ordinary attempt identity instead
  of a cancelled request. It renews the exact resident binding and service close ticket while
  retaining local recovery fencing. Threadless attachment authenticates the same surviving window
  and replacement generation and transfers its existing native reservation without a composer.
  Neither attachment enters startup publication, creates a native window or grants destruction.
- Every surviving shell requires fresh candidate-generation appearance, notice, subscription and
  native-release ownership. Registration uses the existing preserved-shell validation and one GUI
  commit; all fallible preparation precedes adoption. Complete binding validation matches each
  captured native window exactly once to its fresh shell and selected resident or threadless facts.
  Refusal preserves custody and fencing, including after partial multiwindow attachment.
- Before whole-graph publication, fresh candidate revalidation proves the unchanged Running session
  and claims; complete graph/supervisor attachment, resident draft/work settlement, renewed service
  close-custody settlement and complete shell bindings must all belong to this exact attempt and
  candidate. The process publication slot serializes the existing atomic whole-stack transition.
  No fallible constructor or attachment follows storage publication. Coherent interaction reopening
  releases only settled recovery fences; independently terminal unavailable requests retain their
  intent, evidence, duplicate suppression and explanation. Affected turns resolve as repaired or
  explicitly incomplete before successor admission. Recovery never automatically repeats input,
  Exit, close or an uncertain mutation.
- Verify the actual running-owner failure entry without an Exit request through complete old-worker
  disposal, same locked-home candidate, fresh graph and preserved-window attachment, publication and
  interaction reopening. Cover selected and threadless sets, unsaved edits/history/selection and
  placement, duplicate/stale notices and deliveries, candidate failure/cancellation, partial binding,
  retained original outcome/proof custody, and close/Exit admission and publication races. Independently
  review source correspondence, persistence, worker cleanup and lifecycle ownership; helper or widget
  evidence alone does not establish this production route.

## Nonfinal Native Close Recovery

- Before native destruction admission, the running owner may retain a distinct exact pre-native
  cancellation proof with the request identity, gated shell/resident custody and original removal
  outcome. The proof requires this request never to have admitted native destruction, no other
  destruction operation to be pending, and the same published, unretired shell to remain owned.
  An absent attempt, handle or error alone is insufficient. Serialize dependent window/claim
  commands while this proof is live; background thread execution continues.
- If an uncertain removal leaves the original home Healthy, reconcile its retained outcome through
  ordinary admission before attempting failed-home retirement. Under the pre-native proof, proven
  noncommit validates the exact original state without an inverse write; proven commit uses the
  existing ordinary State restoration route and retains separate restoration outcome custody.
  Reconcile and validate a committed restoration rather than repeating it. Pending uncertainty or
  conflicting facts retain fenced custody. This proof cannot authorize native destruction or be
  substituted for GPUI settlement after native admission.
- Authenticated pre-native restoration rebinds the same editor and renewed claim before coherent
  gate release and completion of the cancelled request. Preserve the original healthy graph,
  history, selection and placement. If the home fails, transfer the retained original/restoration
  outcomes and resident custody through the established failed-home route. Never manufacture home
  failure, start destruction to obtain a survival proof, or replay the cancelled close.
- Verify pre-native removal noncommit, committed removal, indeterminate reconciliation, restoration
  uncertainty/conflict, repeated or stale completion, same resident and background-work continuity,
  failed-home crossover and fresh-close success. Independently review the proof's admission and
  consumption together with the persistence and interaction-release boundary.
- The running owner retains the exact cancelled close request, immutable removal evidence and
  original outcome, native reservation and recoverable shell/resident custody through native
  destruction settlement. Draft and session settlement still precede destruction. Native cleanup
  admission requires quiescent detached presentation; nonfinal detachment transfers recoverable
  construction custody instead of irreversibly disposing it. Final teardown keeps its separate
  irreversible contract. No GUI callback waits for storage or worker completion.
- Native observer admission precedes durable removal where possible. A failed native operation
  permits restoration only after exact settlement proves the original window survives and no
  pending destruction can later consume it. Handle absence or an error string is not proof.
  Proven destruction releases retained custody once; uncertainty keeps custody and dependent
  actions fenced. Failure never starts another native destruction operation automatically.
- Consume GPUI's owned recoverable native-destruction attempt, which retains the exact window
  slot/root/wrapper until settlement. Its settled surviving-failure result supplies native survival
  proof; the irreversible `remove_window` path and its observation receipt cannot supply that proof.
- While the original home remains Healthy, use State's authenticated ordinary restoration route
  under the same generation. Proven removal noncommit requires exact original-state validation
  without an inverse write. Proven commit requires the exact State restoration contribution, with
  separate outcome/reconciliation custody. Validate a committed restoration rather than repeating
  it. Preserve serialization of window creation, claims, selection and competing close/Exit
  commands while this attempt can restore membership; background thread execution continues.
- Restore the same protected editor and shell using authenticated restored window/claim facts.
  Renew affected claim capabilities rather than reviving pre-removal revisions. Unchanged healthy
  graph resources need no replacement. Preserve native identity, resident history, caret, directed
  selection, inline gaps, scroll and placement. Complete durable settlement and exact shell/resident
  binding precede atomic gate release and completion of the cancelled request. Fresh activation is
  required for another close.
- If storage fails during restoration, transfer immutable original and restoration outcomes plus
  retained resident custody to existing failed-home recovery. Reconcile the restoration before any
  new inverse write, retire old-generation resources through their established boundaries, and
  bind through the replacement graph. Never mark a Healthy home Failed merely to enter recovery.
  Terminal uncertainty or conflicting facts retain fenced custody and the reported failure.
- Verify native preflight refusal, settled destruction failure with exact surviving identity,
  successful destruction, delayed/duplicate/stale native completion, healthy restoration noncommit,
  commit and uncertainty, conflicting claims, and storage failure during restoration. Mounted
  evidence must prove unchanged editor/history, renewed claim, background-work continuity and
  fresh-close success. Independently review persistence and native lifecycle composition.

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

## Native Close Admission During Reentrancy

- A registered close callback must execute successfully and explicitly allow close before the
  native boundary treats it as permission. Failure to access the app or exact window, including
  temporary reentrant borrowing during publication, denies that request. It does not queue or
  replay close, install a permanent veto, or change the default for a window without a callback.
- A later ordinary request can execute the same callback after the current update returns.
  Startup's callback remains a side-effect-free admission check while interaction is gated;
  native wrapper removal for failed-startup disposal is a separate owned operation.
- Qualify a synchronous close during a borrowed native window update, unchanged pending destruction
  receipt, subsequent successful close, explicit veto and absent-callback default. Require semantic
  review of the dependency change; complete-set command and editor gating remain separate.

## Native Window Destruction Receipt

- Startup may register one move-only destruction receipt for each exact live Windows native
  window before handing it to native-set ownership. Registration is single-use and rejects a
  destroyed window. It neither requests close nor changes desktop-operation admission.
- The receipt reports terminal native destruction of that original window instance, including
  destruction deferred by an outstanding desktop lease. Removing the GPUI wrapper, requesting
  destruction, entering `WM_DESTROY`, or observing a reusable numeric HWND is not completion.
- Native failure or lost completion authority is an error, never successful disposal. The startup
  owner retains original typed cleanup custody and blocks failure completion or replacement while
  native disposal remains unproven. No automatic native retry or alternate-handle probe is added.
- Dropping the receipt does not cancel native disposal or retain a native window. Storage remains
  one completion channel per admitted window, without waiter lists or global orphan queues.
  Native destruction and completion delivery stay on the running GUI executor; process shutdown
  must drain them before stopping that executor.
- Acceptance requires real native hidden and published removal, deferred removal while leased,
  duplicate admission refusal, observer abandonment and exactly-once terminal completion, with
  independent lifecycle review. Startup-set transfer and process quit draining remain separate.

## Windows Desktop Worker

- Desktop preparation is one synchronous worker operation consuming the exact hidden native lease
  and an optional saved `VirtualDesktopId`. It performs no GUI work, durable write, desktop
  enumeration, active-desktop switch or visible-window workaround. The app associates its result
  with the original shell flight and waits for GUI lease settlement before any publication.
- The identity's 16 bytes encode the GUID's canonical numeric value in big-endian order, not the
  native in-memory structure layout. Capture and restoration use the same explicit conversion.
- Without a saved identity, leave Windows' first-show desktop assignment untouched and report
  the current-desktop default. With a saved identity, initialize COM on the worker, create the
  documented `IVirtualDesktopManager` and attempt exactly one `MoveWindowToDesktop` for the leased
  HWND. Interface destruction and balanced successful COM uninitialization precede lease release;
  no COM interface crosses threads or outlives its apartment.
- Report accepted saved assignment separately from current-desktop default. COM initialization,
  manager creation or movement failure selects that best-effort default and records only a bounded
  stage and native error code. Do not invent a current-desktop GUID, use private interfaces, retry
  the move or rewrite saved placement. The default outcome names the selected fallback, not a
  claim that a hidden window's eventual desktop has already been observed.
- Do not gate the operation on hidden `GetWindowDesktopId` or current-desktop queries: those
  observations do not establish a pending assignment before first show. Accepted movement remains
  best-effort under the feature's topology/desktop-change policy. Cancellation or native close is
  handled by the owning flight after worker completion and is never classified as desktop fallback.
- Verification covers exact GUID conversion, absent identity without a desktop effect, accepted
  movement and rejected movement on owned native windows, COM initialization failure with balanced
  apartment lifetime, hidden-state retention and original native lease release. Qualify alternate-
  desktop retention and default fallback through first nonactivating publication; state any absent
  environment coverage. Require independent semantic review of identity, COM lifetime and outcome
  mapping. Full shell custody and startup-set publication retain their separate acceptance gates.

## Shell Desktop Placement Flight

- An admitted flight consumes one unpublished main-window shell and derives the saved desktop
  from that shell's retained controller. Admission failure returns the original shell and starts
  no worker. Cancellation already requested at admission performs no desktop effect.
- One detached GUI continuation owns the full shell and a required completion callback through
  both the desktop worker and native lease settlement. The callback retains its startup owner
  strongly and transfers the original shell into publication or typed-cleanup custody. An optional
  observer or result channel is never the owner of that custody. Cancellation signals intent; it
  does not cancel the continuation or abandon the worker.
- Once enrolled, the shell retains pending, ready or terminal rejected placement admission plus
  its cancellation signal. Publication rejects pending/rejected admission and cancellation even
  after successful completion delivery. Native close/window loss rejects the flight separately
  from desktop fallback. A successful result exposes the accepted/default desktop outcome; the
  later startup-set owner requires actual successful flight admission for every member.
- Never-enrolled ordinary shell behavior is unchanged. Desktop readiness does not replace existing
  selected-editor, source-generation or first-presentable checks. Complete-set publication and its
  final cancellation fence remain separate from per-member desktop completion.
- Settlement failure retains the original shell, prohibits publication and cannot certify native
  disposal. Its ordinary prepublication cleanup methods must not release typed custody while native
  settlement remains unproven. Cancellation, native close and proven native loss after successful
  settlement may instead enter the existing acquired/restored/threadless cleanup boundaries.
- The GUI executor remains live until completion delivery and subsequent cleanup. Generic worker
  panic continues to use the fatal-panic contract. Verification covers admission failure, cancellation
  during work and after ready delivery, close/window loss, observer disposal and original selected
  and threadless cleanup, with real native integration and independent semantic review. No fake
  native lease or global abandoned-flight queue substitutes for this ownership.

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

- The process startup controller retains one fixed configured-home path and immutable service and
  window-source inputs. Its required composition-root opener receives that path and the current
  preparation cancellation, returning the complete private typed candidate, busy ownership, or an
  explicit failure with any original unsuccessful-close custody. Retry cannot replace the opener,
  path or unsettled service owner. Storage opening, Settings/theme loading, restore preparation and
  service disposal run on workers; GUI callbacks admit only bounded command intent.
- Native whole-set success invokes the required process-owner handoff immediately with the complete
  service graph, published set, appearance owner, sticky Exit intent and any remaining auxiliary
  startup-surface destruction custody. The old error surface stays present throughout pending Retry.
  After success its separate native destruction may settle under ordinary process ownership; neither
  wrapper removal nor transfer grants clean-quit proof. An admitted deferred Exit survives removal
  through the persistent command owner. Auxiliary disposal failure after that handoff cannot turn
  already-interactive main windows back into a failed startup set. Failure-surface allocation or
  destruction-authority failure returns explicit unavailable-presentation custody to the required
  process callback, without claiming disposal or authorizing ordinary quit.
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
- Consuming native startup returns the cancellation handle for that native operation. The process
  owner replaces its preparation-phase cancellation slot with this handle before admitting further
  cancellation. It both fences the operation and wakes its bounded readiness observer; a previously
  retained preparation token is not the native operation's cancellation API. Dropping the native
  handle or an external completion observer does not cancel the owned operation. Readiness observes
  the exact roots, selected composers, appearance and native closure without a polling queue.
  Final storage validation uses private read-only facts captured before preparations move into
  shells, with the original attempt retained. It provides a bounded consistency check followed by
  short GUI admission, not a storage lock across native publication.
- Each enrolled shell closes command, notice, editor and ordinary-close admission before exposure.
  Editor gating composes with lifecycle promotion and resume; background first-presentable loading
  remains allowed. Existing enabled/read-only controls must actually accept the disabled state.
  Gate refusal prevents publication and retains the shell for disposal. Denied native close while
  startup owns the shell is ignored, without ordinary-close work or replay.
- After complete native success, interaction release is one outer GUI update containing only
  widget/admission state changes. A widget may reject re-enabling under its existing capacity
  contract. The release pass then restores the gate on every attempted member before any native
  disposal call or return to event processing. No partly released set enters ordinary ownership.
  Native-set membership validation and final process-owner transfer remain the coordinator's role.
- Once native publication has started, failure or cancellation before whole-set success keeps one disposal owner for every
  attempted member, including possibly visible windows. It closes the entire native set, joins
  transient work and preserves durable restore records and unresolved command custody before
  presenting startup failure, except for the feature-defined blocked-cleanup surface when disposal
  itself cannot settle. That exception retains every unresolved owner and grants no Retry or
  orderly quit authority; only explicit Quit Anyway may terminate without disposal. This disposal
  is not ordinary window close or prepublication
  acquisition abandonment and cannot remove a restored session member. No native batch or hide
  operation is treated as evidence that an earlier exposure never happened.
- Before native publication starts, a genuinely newly acquired runtime-backed fallback uses the
  existing exact prepublication acquisition-abandonment protocol when cancelled or rejected.
  Original restored records are never eligible for that protocol. Once native publication starts,
  the attempted set instead preserves its durable records through native-failure disposal; a
  successfully or possibly exposed member cannot be reclassified as never visible to delete it.
- Acquired fallback transient retirement has a distinct move-only custody type with no acquisition
  abandonment operation. It settles only the original fresh editor candidate, retaining its exact
  reservation on cancellation, rejection or uncertain settlement. Proven retirement releases the
  transient reservation without deleting the acquired window, thread, draft or session record.
  This worker-side boundary does not certify native destruction or ordinary mutable-editor close;
  native-set ownership separately proves destruction and keeps interaction gated throughout.
- Startup disposal first fences and releases GUI editor work while its exact native window remains
  available to dispatch continuations. Each gated selected composer admits one event-driven release
  completion using the existing semantic-quiescence and widget-release protocol. Loading and an
  already admitted dispatch may settle; interaction cannot resume after release starts. No polling
  queue or replacement editor is introduced. Dropping the completion observer does not cancel
  release, and a failed or lost completion never proves release. The startup owner retains the
  shell on failure; successful GUI release alone grants no native or durable disposal authority.
  Startup has its own exact-selection slot release admission, excluding active selection,
  submission, ordinary disposal and native-lineage transitions. It accepts only the existing
  cancellation/release request remainder; it does not pretend an ordinary close or switch is active
  or weaken those operations' release admission checks.
- Native startup disposal retains the complete shell through editor release and exact native
  destruction. Only after both proofs may it transfer the original transient retirement custody
  to a worker. The complete-set owner supplies the sticky publication-started disposition for all
  members, including members not yet shown; a member's own publication flag cannot authorize
  acquisition abandonment after another member's native publication was attempted.
  Hidden enrollment stores the exact destruction receipt inside the shell before desktop placement
  or exposure. Enrolled shells reject early custody extraction and handle-only handoff. Before the
  first publication call, the set owner seals every member for record preservation; enrolled
  publication requires that irreversible seal. Successful set transfer keeps each complete shell
  and receipt with the process owner.
  Member disposal consumes the shell into one GUI continuation with a required completion callback.
  It waits for editor release, requests native removal and awaits the registered destruction proof.
  Unexpected native destruction before editor release, missing completion, removal failure or
  unresolved desktop custody retains the shell with an explicit failure. No retry, replacement
  receipt or implicit drop proves disposal. Successful destruction transfers restored or acquired
  cleanup custody with its original kind; only an unsealed acquired member exposes prepublication
  abandonment. A threadless member releases its reservation only after destruction proof.
- Startup construction registers native destruction ownership immediately after allocation, before
  fallible composer mounting or appearance registration. A failure before allocation returns the
  original prepared member. A failure after allocation returns the complete hidden native owner,
  its original controller/reservation and any mounted editor; requesting removal alone never
  releases them. Partial composer setup retains its created entity when a later subscription or
  autosave step fails. Cleanup skips editor release only when construction proves no editor was
  mounted; otherwise it uses the same gated release and native-destruction protocol. Receipt
  registration, gating or completion failure retains explicit custody and cannot authorize
  publication or replacement. The GUI executor remains live through terminal completion.
- The process lifetime owns startup-attempt identity, failure-surface commands and retained
  enrollment, nondispatch and home-reconciliation custody outside each attempted graph. Retry
  admits only one new attempt for the same configured home after prior native/transient disposal
  and required graph shutdown settle. It rereads and validates the durable session; it does not
  reuse failed prepared shells or an old graph's authority. A failure after service publication
  uses the graph's shutdown/disposal boundary. Unsettled custody retains its exact owner and
  blocks successful shutdown or a conflicting reopen; it is not cleared to enable Retry or Exit.
- Same-home startup Retry reuses the process-owned enrollment, nondispatch and admission authorities.
  It admits a fresh service generation only after the previous attempt's native/transient work,
  graph retirement and home closure have settled, using the exact process reopening fence. A
  retained close or reconciliation failure blocks reopening. This startup-attempt transition does
  not reopen ordinary mutable windows after a cancelled running-session shutdown.
  The service owner distinguishes initial admission, owned preparation, an installed graph, proven
  retirement and blocked retirement. Only initial or proven-retired authority may admit another
  candidate, and rejection returns that candidate unchanged. Proven retirement retains its exact
  process fence; Retry cannot replace a stale fence with a newly observed one. Reopening checks
  settled outer custody and admission reservations, reopens that exact gate, then constructs fresh
  services behind their separate worker-start fence. Fresh services must not capture execution
  permits while the process gate is fenced, and old permits/references remain invalid. Failed graph
  shutdown never creates retirement authority, including errors after graph consumption.
- After startup-native and transient ownership has settled, a published graph whose home is
  Failed uses failed-home retirement instead of the healthy-home graceful-shutdown barrier.
  The process owner refuses that boundary while main-window reservations or outer Activity and
  nondispatch custody remain. It fences process admission and invalidates restore references,
  requires exact CAS failed-home terminal retirement, then joins the remaining graph components
  and explicitly closes the original home. Rejected CAS retirement retains its service with the
  original graph; consumed retirement failure retains its original diagnostic custody. Home-close
  failure retains its open-home custody. Any failed proof leaves the attempt blocked, even if
  all service handles have been consumed. Only complete retirement and settled outer custody
  retain the exact process fence as authority for a fresh initial startup attempt. This boundary
  cannot retire an ordinary running window set or replace running-session same-home recovery.
  A joined handoff scanner's direct or page-read health-gate refusal for that exact Failed home
  generation is expected retirement evidence, not a join failure. Panic, command/reconciliation,
  settlement and unrelated read failures still block; no failed-home read is required to succeed.
- Startup verification includes failure of the last required preparation, stale first-member
  completion, native failure after an earlier member shows, reentrant close/Retry, cancellation,
  unchanged durable restore records after failed restoration, exact empty-header threadless
  initialization and preservation of unresolved custody. Full-set success is separate from later
  progressive catalog/transcript loading and runtime warm-up.

## Window Detachment And Process Shutdown

- Ordinary close retains State's immutable exact removal evidence and its command outcome with
  the same process-owned window custody before publishing session removal. Reported failure
  cancels that close. Same-home replacement implements the
  [interrupted-close protocol](../../../doc/systems/backend-runtime/design.md#interrupted-ordinary-close-during-same-home-recovery),
  settling proven noncommit or the separately reconciled exact restoration before fresh resident
  binding and interaction release. A committed removal cannot use the header-only interrupted-Exit
  resume operation. Renewed selected residents use the new claim revision while preserving native
  identity and content. Final ordinary close and nonfinal close share this session recovery rule;
  only final close owns the process-wide work barrier.

- Releasing startup interaction never authorizes native default destruction. Main-window native
  close callbacks retain the window until the process owner has completed its close obligations.
  Before ordinary command routing is installed, native close remains vetoed; afterward it submits
  the exact close intent and still vetoes default destruction. Successful owned cleanup removes
  the window explicitly through its retained native destruction authority.

- The resident composer has a separate shutdown mutation gate, controlled by the process owner.
  Setting or clearing that gate preserves the resident entity, selection, scroll and read-only
  interaction. It blocks new edits, rich paste, submission, cut and image-marker mutation while
  allowing copy and inspection. Already admitted edits continue settling through their existing
  custody. Startup and ordinary-close gates remain independent: releasing either cannot clear
  shutdown read-only state, and clearing shutdown cannot release another active gate. Pending
  editor promotion also preserves the shutdown gate. This local gate grants no shutdown admission,
  close, flush or service-reopening authority; process-wide installation and coherent release are
  separate owner obligations.

- The shell exposes a local shutdown interaction gate to the process owner. It forwards the gate
  to its exact resident composer and rejects New Window activation with the application-wide
  waiting reason. Threadless shells require no composer; a selected shell with missing controller
  or resident composer cannot report a successful transition. Failed installation retains the
  shell gate, and failed release cannot reopen the shell. Startup gating remains independent.
  This local adapter does not install a process-wide barrier or grant coherent reopening authority.

- The shell keeps the Exit toolbar position visible before ordinary Exit routing is mounted.
  Without that binding it is disabled with `Application Exit is not available.`; an independent
  startup gate instead explains `Beryl is preparing its windows.` The shutdown gate takes
  precedence, projecting the feature-owned `Exiting…` loading label and exact waiting tooltip.
  Removing that gate restores the remaining unavailable state, never implicit activation or
  command delivery. This presentation uses the existing shell gate and creates no new lifetime
  or admission authority. Enabled routing and terminal teardown presentation remain separate mounts.

- The running owner installs those local gates only after shutdown admission, for either final
  close or application Exit. It snapshots all published shell handles under a short owner borrow,
  then gates them on the GUI executor without holding that borrow. Every captured shell is visited
  even when an earlier transition fails. Missing windows and failed local transitions are explicit
  failures; installation never rolls back a successful gate, releases shutdown custody or treats
  an empty published set as success. Repeated installation is idempotent. This boundary grants no
  progress, teardown or reopening authority; coherent gate removal remains a separate obligation.

- The running owner may release the installed shell gates only while it retains an unconsumed
  coordinator failure result proving coherent reopening, with complete services returned and no
  shutdown intent. It snapshots the published handles under a short borrow, then validates the
  nonempty set and every native shell, controller and live resident composer before changing any
  gate. Validation and release run in one synchronous GUI update without yielding or holding the
  owner borrow. A failed validation leaves every gate unchanged; successful release preserves
  independent startup and ordinary-close gates. Release does not consume the progress result,
  complete a command, restore cancelled work or admit a successor attempt. Consumed or absent
  evidence, readiness and failure without proven reopening confer no release authority.

- Successful startup transfers one move-only running Exit consumer alongside the complete graph
  and published windows. Existing startup command producers remain valid for deferred Exit events,
  including after removal of the old startup surface. One pending bit and one exact active request
  coalesce repeated delivery without a queue. Consuming an intent grants no shutdown authority.
  Duplicates during that request cannot become a later attempt; ending the exact request after
  cancellation or failure requires a fresh activation for another attempt. A foreign or stale
  completion cannot end a successor request. Wake delivery occurs outside command-state borrows.
  The running owner may arm one detached GUI wait for that consumer. The wait retains the complete
  owner and releases every owner borrow between polls. A second pending wait is refused without
  replacing its callback. Delivery clears the wait slot before calling the required GUI callback
  with the move-only exact request, outside owner borrows; the callback may arm its successor.
  The active request continues coalescing duplicate activations until its exact explicit completion.
  Completion wakes a pending waiter only after both command-state and owner borrows have ended.
  Owner-level completion refuses while initial observation, native confirmation custody, any
  retained shutdown intent, or pending/unconsumed progress remains, or complete service custody
  has not returned. Refusal leaves the exact request active and emits no wake. An unadmitted
  intent must first be explicitly ended after its observation settles; an admitted intent must
  first obtain the coordinator's proven reopening. Completion itself releases none of that custody
  and does not validate native-window liveness, so loss of an invoking window does not prevent
  ending an otherwise coherently settled failure. A pending command waiter is allowed and receives
  its wake outside the owner borrow after successful exact completion.
  The policy consumer retains the request until cancellation or failure has coherently settled;
  dropping it does not complete it. Waiting or delivery alone selects no invoking window, observes
  no work, admits no barrier and grants no quit authority.

- Each main-window Exit producer carries its original window identity. The first pending
  activation retains that identity through delivery; duplicate activations from any window cannot
  replace it or queue a successor. A deferred startup Exit has no main-window origin and binds once
  to the first published main window in restore-set order. Resolution requires the exact active
  request from this owner and a live published controller for the selected identity. Once bound,
  loss of that window refuses resolution instead of selecting another window. Producer creation
  also validates its published member. These checks grant no close lease or execution fence;
  confirmation and admission must still validate their original window and work evidence.

- The running Exit channel shares independent availability gates across all producer clones.
  The process owner sets and clears unavailable-routing, Settings-reconciliation and home-store
  failure gates from their owning feature state. Home failure takes explanation precedence over
  Settings reconciliation, then unavailable routing. A gate blocks new running-stage activations,
  including retained startup producers, before changing pending origin or waking a consumer.
  Clearing one gate preserves the others and neither creates an intent nor wakes a waiter.
  Already accepted pending or active requests retain their original custody and completion rules;
  startup-stage cancellation and deferred intents accepted before handoff remain unchanged.
  This channel primitive begins with no feature gates and supplies no toolbar mounting or readiness
  authority. The shell remains independently unavailable until ordinary routing is mounted, and
  that mount must install current feature gates before exposing an enabled command. Availability
  projection reads the same shared gate state used by activation; service admission still revalidates
  current evidence. Gate changes alone never reopen services, release shutdown custody or quit.

  Running-owner construction binds the channel once to a non-owning service reference for its
  published home generation. Availability and each new activation read that home's current health;
  a non-healthy or different generation supplies the home-unavailable reason ahead of manual gates.
  This binding survives worker transfer of services and is shared by previously retained producers.
  Clearing a manual gate cannot override it. Recovery never revives producers from a retired
  generation; replacement services require their own channel. The reference grants no home-close
  authority. Reading health performs no storage I/O, and final admission still revalidates evidence.

  The running owner projects that shared disabled reason into every published shell, initially
  and through one owner-scoped GUI observation task. Each delayed pass reads only in-memory
  availability, snapshots the bounded published handles under a short owner borrow, then updates
  shells outside that borrow. Unchanged reasons do not notify or redraw. The task holds a weak
  owner reference and is cancelled when the owner is released. Missing native shells are skipped
  only for presentation; their absence supplies no destruction or shutdown evidence. The shell's
  admitted shutdown and startup explanations retain precedence. With no shared reason, an unbound
  toolbar still explains unavailable routing and stays disabled. Observation never activates Exit,
  consumes a request, installs feature gates or grants final teardown authority.

- Exit work classification consumes a settled initial observation result while retaining the exact
  active request with its caller. It revalidates the original published invoking window and refuses
  competing observation, confirmation or shutdown custody. Failed or cancelled collection never
  supplies idle evidence. Work-bearing evidence returns the original observation and invoking
  identity for native confirmation, without acquiring a lease or fencing execution. Idle evidence
  enters the existing atomic idle admission with application-Exit intent; stale evidence refuses
  without a fence. Every refreshed result is classified again. Classification neither completes
  the Exit request nor retries, opens a dialog, advances progress or grants quit authority.
  Close-confirmation preparation preserves typed runtime-validation failures separately from
  unavailable services, an already admitted shutdown and invalid window evidence. Idle admission
  carries that distinction through classification so later policy can recognize changed evidence
  without parsing displayed error text. Native confirmation setup may format the error for its
  existing failure-delivery boundary; preparation still acquires no lease or execution fence.

- Initial Exit observation takes custody of the move-only active request after resolving its
  original invoking window. It uses the existing single worker observation slot and returns the
  same request with the result to a required GUI callback, outside owner borrows. Refused request
  validation or scheduling returns the request to the caller and never invokes that callback.
  Cancellation and collection failure also return the original request, with no idle evidence.
  Pending work retains the complete owner and keeps duplicate activations coalesced. Completion
  releases the observation slot before callback delivery, permitting explicit successor scheduling.
  This boundary does not complete the request, classify evidence or admit shutdown; subsequent
  routing still revalidates the request, original invoking window and observed evidence.

- Exit work routing composes that classification with the owned native confirmation boundary in
  one GUI call. Idle admission returns an admitted outcome; work-bearing evidence opens the
  application-Exit confirmation for the exact invoking window and returns a confirming outcome.
  It releases the owner borrow before native setup. Native setup revalidates the original evidence;
  refusal neither opens a replacement dialog nor calls the completion callback. Only a scheduled
  confirmation invokes that callback, after native settlement, using the existing retained owner
  and exact confirmation custody. The caller keeps the active Exit request and explicitly consumes
  the confirmation result. Repeated routing while confirmation is retained refuses without
  replacing its callback; command duplicates still coalesce under the same active request.
  Routing does not complete requests, refresh observations, consume positive confirmation, advance
  shutdown progress or quit.

- Routed Exit confirmation retains the exact originating request identity with its native
  operation. Result consumption first validates the active request, original live invoking window,
  application-Exit intent and operation association. A stale, foreign or successor request cannot
  consume that result. Pending settlement returns pending without changing custody. Cancellation
  returns cancelled without a lease or fence; changed window evidence refuses confirmation.
  A valid positive result consumes the original snapshot into the existing retained confirmed
  intent and returns awaiting observation. Native failures preserve the existing proven-clean
  versus unresolved-cleanup distinction. This boundary neither completes the request nor schedules
  observation, shutdown progress or quit; the caller retains responsibility for coherent completion.

- Initial Exit routing connects request-retaining worker observation to the existing work route.
  Explicitly changed initial collection or idle-admission evidence schedules a fresh worker read
  after a 50 ms delay. The owner reserves that read before yielding and retains the original
  request and cancellation token. Every result is classified anew: newly observed work requires
  native confirmation and cannot inherit permission from an earlier idle result. Only the typed
  work-change errors eligible for confirmed refresh qualify; busy, unavailable, foreign, window
  and request failures remain terminal. Cancellation is checked again before classification.
  Each refreshed result validates the original invoking window; no replacement is selected.
  It returns the original request on scheduling refusal, with no completion callback. Once
  scheduled, it retains that request through observation and any native confirmation, delivering
  it exactly once with admission, cancelled confirmation, cancelled confirmed observation, or a
  typed routing failure on the GUI executor outside owner borrows. Native
  settlement first consumes the exact request-associated confirmation through the existing validated
  boundary. Missing settled evidence is an explicit failure, never cancellation or positive intent.
  Failed request/operation identity validation or unresolved native cleanup preserves the original
  confirmation custody; stale window snapshots and rejected confirmed intent retain their existing
  consumption semantics. A valid positive result schedules one confirmed worker observation at a
  time using the same cancellation token and retained request, including the changed-evidence
  refresh defined below. Its existing exact admission/discard boundary settles before delivery.
  Scheduling refusal, non-refreshable collection failure or cancelled confirmed observation
  retains the original unadmitted intent and lease for explicit completion; cancelled confirmation
  remains distinct because it creates no intent. Observation cancellation or failure never routes
  idle admission. Duplicate activations continue coalescing throughout this operation. Routing
  performs no other automatic retry, request completion, progress scheduling or quit; its caller still
  owns coherent completion of the request.

- Exit progress retains the exact active request through one worker-owned shutdown progress pass.
  Scheduling requires its original live invoking window and an admitted application-Exit intent
  for that window. Refusal returns the unchanged request without callback or service transfer.
  After that validation and before service transfer, each pass installs the accepted shell gates.
  Installation failure returns an explicit interaction error and retains admitted custody and
  every successfully installed gate. On returned coherent-reopening evidence, progress releases
  all shell gates before consuming that evidence. Failed release returns an explicit interaction
  error while retaining the settled result, gates and active command; it cannot complete the
  attempt or start another pass. Other results retain the gates. These transitions run outside
  owner borrows and preserve independent startup and ordinary-close gates.
  Scheduled progress returns the same request exactly once on the GUI executor, after complete
  service custody is restored and, except for failed gate release, the settled progress result is
  consumed, outside owner borrows.
  Waiting, readiness, failed progress with its exact reopening evidence, and service errors remain
  distinct. Missing settled evidence is an explicit error. Only the underlying proven-reopening
  boundary releases shutdown intent; delivery itself neither completes the Exit request nor
  schedules another pass, reopens services, tears down windows or grants quit authority.

- The Exit progress driver retains that same request and complete running owner until one
  non-waiting result. It schedules one pass at a time through the validated Exit progress boundary,
  consuming each result before another pass. Waiting yields through one bounded-delay GUI
  continuation; no owner borrow or worker remains held by that delay. Every successor revalidates
  the original request, invoking window and admitted intent. Cancellation is passed unchanged to
  the coordinator rather than abandoning progress. Readiness, either failure reopening outcome,
  service error or successor scheduling refusal ends the driver and returns the request exactly
  once outside owner borrows. Initial refusal returns it synchronously without notification.
  The driver never retries a failure, completes the command, releases retained intent, starts
  teardown or quits; those remain explicit policy actions on the returned outcome.

- The composed Exit attempt connects initial routing to the progress driver. Only an admitted
  routing result starts progress, using the same request and cancellation token. Cancelled
  confirmation, cancelled confirmed observation and routing failures return unchanged in meaning;
  they never start progress. Once initial observation is scheduled, even a refused driver start
  returns through the one required GUI completion callback with the original request. Initial
  observation refusal remains synchronous without callback. Completion runs outside owner borrows
  and preserves all existing intent, service and native-cleanup custody. This composition performs
  no automatic command completion, unadmitted-intent release, retry, presentation, teardown or quit.

- The running Exit policy wraps that composed attempt and reports its original result together
  with whether the exact command completed. Before delivering a cancelled or failed attempt, it
  ends only the settled unadmitted application-Exit intent returned by that attempt's confirmed
  observation, then uses guarded exact command completion. Request identity and original invoking
  identity must still match before releasing that intent. Other retained intents, pending native
  cleanup, observation, progress or missing service custody prevent completion. A progress failure
  completes only after the coordinator has proven reopening and released its intent. Ready and
  Waiting never complete the command. The policy delivers outside owner borrows and preserves the
  original result even when custody prevents completion; it never retries, destroys windows or
  grants quit authority. Initial scheduling refusal still returns the request synchronously for
  explicit guarded completion by its caller.

- One running Exit consumer activation composes the detached command wait with that attempt
  policy. It retains the complete owner, cancellation token and required completion callback
  before command delivery and through the attempt. A duplicate pending wait refuses synchronously
  without invoking or replacing callbacks. Once a command arrives, initial scheduling refusal
  also delivers its original request and typed error through that callback, after guarded exact
  completion; unresolved custody still prevents completion. Every delivered outcome runs outside
  owner borrows on the GUI executor. The callback may explicitly arm a successor wait, but no
  successor activation, retry, teardown or quit is implicit in this boundary.

- Before its completion callback, the running Exit consumer contributes initial observation,
  routing or progress-delivery errors to the original invoking surviving window's Notifications arbiter. The record
  is a dismissible commandless error with the feature-owned title and a byte-bounded diagnostic
  projection produced without first allocating the complete formatted error. Each delivered
  attempt contributes once with its own condition identity; duplicate activations contribute no
  additional record. Missing original windows have no replacement destination. Notice omission or
  refusal changes neither the original result nor command completion, custody or quit authority.
  Cancellation outcomes and successful admission contribute no failure record. This boundary
  does not report final-teardown outcomes, infer persistent backend/home conditions,
  select threads, move focus or arm successor waits.
  Progress-delivery errors include refused scheduling, invalid request/intent, unavailable settled
  results and service errors. Reporting preserves the original error and any unresolved admitted
  intent; it cannot prove reopening, release a gate or authorize command completion.

- A coordinator failure identifying an unproven execution or compaction contributes the same
  commandless Exit-failure notice to surviving published windows whose current resident composer
  selects that exact thread. Compaction attribution uses the thread embedded in its operation ID;
  reporting performs no storage lookup. If no surviving window views that thread, only the original
  invoking surviving window receives the report. Missing destinations are not replaced. Attribution
  is collected on the GUI executor before notice delivery, outside asynchronous work; delivery runs
  outside owner borrows. Both proven reopening and retained admission preserve their original
  result, completion and custody. Cancellation, Waiting and Ready produce no coordinator notice.
  Coordinator SourceUnavailable, StopFailed and CleanupFailed results carry no thread attribution;
  they report only to the original invoking surviving window through the same bounded commandless
  contribution. Their detail describes the failed coordinator step without claiming final teardown,
  successful reopening or persistent backend/home failure. No missing destination is replaced.

- The persistent running owner accepts that complete handoff without reconstructing services or
  extracting window handles. The composition root retains it for the ordinary process lifetime.
  It starts at most one GUI-executor cleanup task for the transferred startup surface. That task
  retains both the owner and the original surface through native settlement and exposes no
  cancellation handle. Pending cleanup blocks clean quit; failure retains the original surface
  custody and diagnostic in the running owner. Only successful destruction settles that auxiliary
  obligation. Main-window identity, interaction and pending Exit delivery remain unchanged while
  cleanup runs or fails. This ownership boundary grants no shutdown or startup-rollback authority.

- The running process window owner retains one native confirmation control and its completion,
  bound to the invoking window, shutdown intent and exact observation. Windows uses the owned
  GPUI native confirmation boundary, with feature-owned strings and Cancel-default behavior.
  Duplicate requests reveal that same operation. Cancellation restores the invoking window's
  retained logical focus when it still exists. Native creation or settlement failure grants no
  shutdown or immediate-termination authority. A positive result is usable only after native
  settlement and fresh validation of its exact process/window/work intent; it is not itself an
  execution fence. Ordinary work changes permit a fresh observation under the same confirmed
  shutdown intent without repeating confirmation, as required by the feature's admission policy.
  The Exit consumer refreshes explicitly changed work evidence through one worker read at a time,
  delayed by 50 milliseconds between passes. It reserves the next tagged observation before
  yielding, retaining the original confirmed intent and window lease throughout the delay.
  Cancellation settles that exact observation without admission. Only typed work-revision or
  mutation-interval changes permit refresh; busy, unavailable, foreign, stale-attempt and
  window-custody failures remain terminal delivery. Refresh does not reopen confirmation, replace
  window evidence, finish the command or begin shutdown progress before admission.
  Normal quit waits for confirmation settlement as well as other native cleanup.
  Native failure recovery uses the retained control's exact cleanup-settlement evidence. A failed
  operation whose native cleanup is proven complete may release its dialog slot, restore logical
  focus and end that attempt; another attempt requires a fresh explicit activation. Failure without
  that proof retains the original control and context and blocks clean quit. Neither an error
  string nor an absent native handle establishes cleanup, and cleanup evidence never confirms intent.
  After publishing the exact operation's settled result and restoring applicable focus, its
  retained GUI continuation invokes the required completion callback outside owner borrows.
  The callback may consume that result and explicitly schedule the next operation. An unconsumed
  result retains the original slot. Duplicate reveal requests neither replace the original
  callback nor receive another completion; scheduling refusal delivers no callback. The adapter
  does not consume results, retry, admit shutdown or select policy automatically.

- Before choosing confirmation or idle admission, the running owner schedules at most one initial
  work observation on a worker. A detached GUI continuation retains the complete owner until the
  result is delivered. Pending observation excludes another initial observation, confirmation and
  shutdown admission without fencing ordinary execution or window construction. Collection failure
  and cancellation yield no idle evidence; cancellation is checked again before GUI delivery.
  The observation slot is released before the required GUI callback, outside the owner borrow,
  allowing explicit successor scheduling or policy classification. Scheduling refusal delivers no
  callback. This boundary neither retries nor chooses a window or shutdown policy; the consumer
  still validates its invoking window and evidence through confirmation or idle admission.

- [Fatal crash reporting](../../../doc/systems/crash-reporting/design.md) bypasses this ordinary
  shutdown coordinator. Its separate process initializes only a report surface with fixed
  presentation resources, never the process service graph, persisted theme, home or backend.

- Ordinary close and application Exit are coordinated with process window construction through
  one bounded admission gate. The app revalidates whether a closing window is final before
  admitting shutdown or durably removing it; overlapping closes cannot each assume another
  window will survive. Confirmation carries exact attempt, window-set, and work revisions and
  owns no stop or mutation authority until the shared shutdown coordinator admits the barrier.
- Before confirmation, the process window registry may inspect an exact close snapshot and
  invoking member under its existing process/construction serialization. Inspection validates
  registry identity, membership revision, open process admission and absence of an admitted close
  owner, then reports whether that member is final. It installs no lease, fence or interaction gate;
  construction and execution remain available afterward. The result is point-in-time evidence,
  so later close admission still consumes and validates the original snapshot. Membership ABA,
  foreign snapshots, absent invoking members, competing close ownership and process shutdown
  refuse inspection without changing authority or reopening anything.
- Running shutdown validates the retained close lease inside the observed admission boundary,
  after acquiring unpublished process closing and before publishing its fence. The lease must
  belong to that exact process gate and retain its original close owner, membership revision and
  invoking member; final ordinary close additionally requires a single member. Registry validation
  is nonblocking and follows process-gate then registry lock order. Every membership change and
  close-owner release uses that same process gate, so its held closing guard protects the validated
  window evidence through fence publication. Refusal leaves the coordinator and execution authority
  unchanged. Work refresh retains the original lease and window evidence; it never substitutes a
  new window snapshot under an earlier confirmation. This check grants no confirmation authority.
- After a settled positive confirmation, the running owner consumes its original snapshot into
  one retained close lease and preserves the invoking member and ordinary-close versus Exit mode.
  Only that confirmed attempt may prepare one worker observation at a time. Completion carries
  exact attempt identity; stale or duplicate completion cannot admit a successor. Failed collection
  or refused admission retains the original attempt without a fence; another work observation may
  refresh it without another dialog. Ending an unadmitted attempt releases its lease only after
  its worker result has settled. Successful admission retains the lease and intent with the running
  owner through the separately owned shutdown lifecycle. A new confirmation cannot replace this
  custody, and neither an observation job nor confirmation alone starts shutdown progress.
  Cancellation may discard an exact settled worker result without admission, including successful
  collection delivered after cancellation. Discarding a stale result cannot settle another worker.
- The running owner schedules confirmed observation through one detached GUI continuation that
  retains the complete owner until worker collection and exact admission or discard settle. It
  checks cancellation again on the GUI executor before admission, including after a successful
  worker read. Cancellation settles that job without fencing and retains the unadmitted intent
  for explicit completion. The required completion callback runs once on the GUI executor, after
  releasing the owner borrow; it may end the attempt or request a fresh observation after refusal.
  No caller-held task handle controls this custody, and the adapter neither retries automatically
  nor starts shutdown progress. Collection errors retain the original intent and window lease.
- A no-work activation uses the same running owner and exact published invoking member, but may
  enter admission without native confirmation only with a successful observation reporting no
  process work. It acquires and validates original window custody through the same atomic observed
  handoff. Work-bearing evidence requires confirmation; stale, busy or unavailable evidence refuses
  without a fence and releases any unadmitted close lease. A fresh observation must be classified
  again, so work appearing during idle admission cannot inherit permission to shut down. Successful
  idle admission retains the original lease and close-versus-Exit intent in the same admitted owner
  slot as confirmed shutdown. Pending confirmation or another retained attempt excludes this path.
  This boundary does not collect work on the GUI executor, retry, progress shutdown or quit.
- Shutdown runtime revision validation uses nonblocking read-only checks of every nested source.
  Busy, poisoned, closed, exhausted or changed sources refuse validation without altering source
  custody, notification, execution authority or failure state. Complete facts and durable reads
  precede admission; this runtime-only check performs no home/storage I/O. A successful revision
  check is point-in-time evidence and does not retain locks or authorize a later publication.
  Atomic admission must hold its required guards through publication of the process fence.
- Fixed runtime sources provide opaque, read-only revision guards for scheduled sessions, stop
  state, compaction operations and work state, projection flights, loaded-thread membership and
  the master command gate. Acquisition is nonblocking and retains a constant number of locks;
  it performs no traversal, I/O, notification, recovery or mutation. Exact revisions and session
  ownership are checked while locked. Compaction control validation checks the exact shared stop
  coordinator and retains both its stop state and operations lock, including their poison state.
  Refusal releases all acquired guards without changing execution authority. Dropping successful
  guards only releases locks; guards expose no mutation or execution capability.
  Composition acquires sessions, shared stop/compaction controls, flights, loaded membership and
  finally the master command gate before entering connection-work election. Session validation
  checks the command gate briefly before its final retained acquisition. All acquisitions under
  process admission remain nonblocking; publication performs no callbacks into held sources.
- Process admission supplies a scoped unpublished closing guard only while its gate is open,
  reservations have settled and a successor epoch is available. Acquiring or dropping it changes
  neither the epoch nor execution authority. Its consuming publication operation installs the
  fence while the caller still holds subordinate validation guards. No callback-return gap may
  separate validation from publication. A published fence keeps prior execution permits stale
  even after a coherent reopening; failed acquisition or unpublished disposal leaves them valid.
- A shutdown work observation brackets its complete bounded runtime/durable read with the existing
  home mutation observer and shared connection-work boundary. It retains only weak interval proofs
  alongside its exact revision and work-presence result, and validates both intervals before return.
  Admission validates exact service/session provenance, acquires unpublished process closing and
  the fixed runtime guards in the order above, then elects the connection interval and coherent
  home interval (mutation, reconciliation, health). Fence publication occurs inside both elections.
  It performs no historical connection traversal or durable read under process admission. Busy,
  stale, foreign or unavailable evidence refuses without a fence; a caller must collect fresh
  evidence before retrying. This boundary admits the observed work set; native confirmation and
  final-window policy determine when that admission is authorized.
- That same observation supplies the shutdown confirmation's distinct running-thread count.
  Merge exact thread identities from non-idle durable gates, accepted-input indexes, live work
  facts and projection flights before counting; a thread present in several sources counts once.
  Attention alone and idle loaded/session/connection custody do not add a running thread. Cleanup
  custody may still require shutdown even when this count is zero. Read source pages with fixed
  bounds, reuse the bounded live-work facts, and retain only the scalar count after collection;
  do not read catalog metadata or accumulate durable thread identities. The count shares the
  observation's revision, interval, cancellation and failure checks and grants no admission.
- Running shutdown admission and coordinator installation form one service-owned handoff. The
  coordinator reserves its exclusive empty attempt slot, successor identity and bounded capture
  configuration before publishing the observed process fence. Refusal leaves execution authority
  and coordinator state unchanged. After fence publication, installation is infallible and retains
  that exact fence before returning the attempt identity; no fallible setup or caller-owned gap may
  lose it. This handoff issues no stop or durable command. Progress and coherent failure reopening
  remain owned by the existing shutdown coordinator.
- The process service owner observes and admits running shutdown through its installed graph's
  exact CAS service and execution sessions. An unavailable graph or already-owned shutdown refuses
  a new observation or admission. Successful handoff installs the returned attempt and invalidates
  the graph's restoration lifetime; refusal preserves both. Only the existing coordinator's proven
  coherent reopening clears that attempt and supplies a fresh restoration lifetime. Earlier window
  preparation references remain stale. No caller supplies a replacement service, session owner or
  process gate at this boundary.
- Preparing a shutdown observation job performs no storage read on the GUI executor. The move-only
  job captures weak work sources, the exact execution-session registry and a weak graph lifetime;
  it gathers the existing bounded observation on a worker without transferring the service graph.
  Graph retirement or shutdown invalidation rejects the job before or after collection. Cancellation
  and failed reads publish no idle result. Returned evidence still requires the graph-owned atomic
  admission path; neither job creation nor successful observation grants a process fence.
- The shutdown coordinator freezes all execution/successor cuts, joins exact process-owned work,
  preserves durable queue custody and proven-undispatched pending work under the CAS-live shutdown
  completion rules, and composes resident-preserving draft flush with typed session
  publication. It retains windows and claims until success, then joins service/runtime disposal
  before process exit. Explicit Exit and final ordinary close retain their distinct restore modes.
- An admitted running shutdown advances by one worker-owned progress pass at a time. The running
  owner transfers its complete service owner to that pass while retaining all windows, appearance,
  commands, auxiliary cleanup and original close lease on the GUI executor. The continuation
  retains the running owner and returns that same service owner before publishing the result;
  no replacement graph or concurrent service operation may enter during the transfer. Duplicate
  progress and new shutdown intent are refused until the prior result is consumed. Waiting, ready,
  errors and failure without proven reopening retain the admitted intent and lease. Only the
  coordinator's explicit coherent-reopening result releases them. Cancellation goes through that
  coordinator, never through dropping a worker or inferring completion from missing services.
  One pass neither retries automatically nor grants final teardown, window destruction or quit.
  Each admitted pass delivers one required completion callback on the GUI executor after restoring
  service custody and publishing its result, outside the running-owner borrow. The callback may
  consume the result and explicitly schedule a successor pass or handle coherent reopening; an
  unconsumed result continues to exclude new progress and intent. A refused scheduling request
  delivers no callback. Completion delivery retains the owner and is not controlled by a caller-held
  task handle. Delivery itself neither consumes the result nor advances shutdown policy.
- A failed barrier before final teardown releases interaction gates from the retained coherent state without restoring
  cancelled continuations or repeating possible dispatch. Closing a settings or auxiliary window
  never becomes the final-main-window execution barrier.
- The running owner retains work readiness on its original admitted shutdown attempt after a
  successful coordinator Ready result and complete service return. Consuming the delivery result
  does not erase that state. Work readiness preserves the invoking member, close-versus-Exit mode,
  lease and execution fence; it proves no draft, session, restore-set or native cleanup obligation.
  Further ordinary work polling is refused. Before final teardown, an explicitly cancelled progress
  pass may still ask the existing coordinator to reopen coherently after a recoverable obligation
  failure. Scheduling that pass clears readiness before service transfer; only another Ready
  result restores it. Waiting, error and failure without reopening never manufacture readiness.
  Coherent reopening releases the original attempt through the existing boundary. No ready state
  completes an Exit request, removes interaction gates, consumes services or grants quit authority.
- A shutdown draft adapter binds each preparation to the exact shell entity and resident composer
  close ticket. It requires the shell shutdown interaction gate and rejects startup custody.
  Selected shells retain their existing editor and close state through worker-backed flush polling;
  repeated preparation joins that same close ticket. A threadless shell supplies explicit no-draft
  evidence only while its controller remains threadless and has no composer. Polling revalidates
  shell and composer identity; missing or replaced custody is failure, never readiness. The caller
  retains this preparation, including on failure. This adapter grants no session publication,
  composer disposal, native destruction, gate release or process-quit authority.
- Recoverable shutdown draft release uses the retained exact shell/composer preparation. Release
  requested or pending is not completion. The resident mount records exact-ticket completion only
  after the service and editor close gates release and autosave resumption succeeds. A later close
  admission invalidates that evidence; missing, foreign or superseded tickets cannot prove release.
  The shell revalidates its identity and resident composer while requesting or polling release;
  threadless release requires the same explicit no-composer evidence. Errors retain the preparation
  and cannot authorize interaction-gate release. Draft release preserves independent shutdown and
  startup gates and grants no service reopening, session publication, disposal or quit authority.
- After retained work readiness and result consumption, the running attempt owns one draft
  obligation set captured from its published windows. Capture precedes shell updates; updates run
  outside the running-owner borrow. Every successful preparation remains retained even when another
  window refuses preparation. Failed preparation is not retried or replaced within that set.
  Polling visits every admitted preparation; only exact resident Ready or explicit threadless
  evidence contributes readiness. Missing custody, unsatisfied flush and disposal/release states
  cannot count as ready. Recovery irreversibly changes this set to release mode and visits every
  admitted preparation, including after partial failure. Only exact release completion of all
  admitted preparations permits a cancelled coordinator pass; an unprepared window has no draft
  gate to release. Until then, service transfer and attempt removal are refused. The original
  attempt, lease, window set and interaction gates remain retained throughout. Released drafts
  alone neither reopen services nor release interaction gates. This aggregation supplies no
  session durability, final teardown, native destruction or quit authority.
- Automatic draft progression retains the running owner and the exact captured obligation set
  until one GUI completion delivery. A single driver owns either preparation polling or release
  polling; overlapping drivers and manual draft operations refuse while it is active. Pending
  results schedule another bounded delayed GUI pass, never a tight loop. Each pass revalidates
  original attempt custody. Readiness, exact release or failure ends polling and releases driver
  exclusivity before delivering outside owner and obligation borrows. Refused scheduling delivers
  no callback. Dropping a caller-held handle cannot cancel this custody. Errors keep the original
  obligations and interaction gates. Release completion alone grants no coordinator recovery,
  session durability, native teardown or quit; the caller must compose those separate boundaries.
- Exit draft recovery binds the exact active Exit request to its original work-ready Application
  Exit attempt before admitting release. It retains that request and owner through automatic exact
  draft release, then drives an explicitly cancelled coordinator pass only after Released evidence.
  Release failure or refusal transfers no services and preserves the attempt and interaction gates.
  Coordinator progress uses the existing exact-request validation and proven-reopening gate release;
  neither release alone nor a failed recovery result completes the command. The caller receives
  one terminal callback outside owner borrows after successful scheduling, or its original request
  on synchronous refusal with no callback. This handoff does not initiate preparation, discard the
  original obligation failure, complete an Exit command, publish session state or authorize quit.
- Exit draft preparation validates the exact active request and original work-ready Application
  Exit attempt before scheduling the retained preparation driver. Ready delivery preserves the
  attempt, request and interaction gates for subsequent durable session obligations. A terminal
  preparation failure automatically uses the exact draft recovery handoff; its delivery retains
  both the original preparation failure and the separate coordinator recovery result or error.
  Recovery does not replace the original failure with cancellation. Scheduling refusal returns
  the original request without callback or recovery. Accepted scheduling delivers exactly once
  outside owner borrows, retaining the owner even if the caller drops it. This policy does not
  complete the command, publish session state, dispose residents or authorize quit; the attempt
  consumer must interpret the result and use the existing guarded completion boundary.
- The ordinary Exit attempt consumer follows work Ready with exact-request draft preparation,
  complete placement preparation and session publication in that order. It reports SessionReady
  only after publication delivers current receipt-backed readiness, retaining the command,
  original attempt, complete placements, drafts, session outcome and gates for final obligations.
  Preparation failures remain distinct typed errors containing the original cause and separate
  recovery evidence. Scheduling refusal remains a progress error. Session readiness failure is a
  distinct session-publication error whose diagnostic does not replace the retained typed outcome.
- All outcomes use guarded completion and the invoking-window notice boundary. Only proven settled
  custody permits command completion; SessionReady explicitly retains the command. Session failure
  does not start reconciliation or draft/coordinator recovery and cannot clear any fence. The
  separate same-home outcome policy owns recovery before enabled ordinary mounting.
  Delivery occurs once outside owner borrows, retaining the owner through accepted preparation and
  publication. Verify ordered successful publication, exact request and service return, noncommit,
  postcommit failure and indeterminate custody, commandless failure notices, refusal and duplicate
  exclusion, caller abandonment, and existing preparation recovery. Independently review lifecycle
  integration. This consumer boundary grants no disposal, native destruction or quit authority.
- Final service cleanup exposes a typed failure boundary: rejection performs no graph consumption
  in that call, while failure after consumption is irreversible for that attempt.
  The process window owner enters final teardown only after work and durable window obligations
  settle. Late failure preserves surviving resident presentation and exact remaining custody;
  it cannot reopen execution or use startup Retry. Service completion errors retain their original
  cause and phase, including errors after all component handles were consumed. Absence of handles
  is never success proof. Rejection describes the current call, never reopening authority; a later
  unavailable-call rejection cannot clear an already irreversible attempt. Native destruction after service retirement shares this late-failure
  policy. The process owner admits at most one exact explicit blocked-shutdown Quit Anyway request;
  ordinary close, cancellation and stale callbacks cannot mint that authority. Execution remains
  fenced until ordinary successful quit or explicit immediate process termination.
- Before the Exit consumer proceeds from draft readiness to placement and session publication,
  it prepares the complete detached read-only composer source set defined by the
  [composer adapter](design-catalog-and-composer.md#detached-read-only-composer).
  The running owner retains one process staging pool with the storage system's finite limits.
  Final admission requires complete exact source preparation and installation before consuming
  services. Surviving detached sources and local clipboard handling outlive home retirement until
  their native windows are destroyed or the process terminates. Threadless shells need no source.
  This terminal read-only phase cannot reopen services or enter interrupted-Exit recovery.

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

## Failed-Home Resident Recovery

- The running owner captures the complete preserved window set under its exact ordinary home
  attempt or cancelled lifecycle request, as defined by their separate ownership contracts,
  before allowing old graph retirement. Selected residents use the separate failed-generation
  capture and retirement boundary in [composer authority](design-catalog-and-composer.md);
  threadless residents retain their existing exact custody. Capture remains available after home
  failure and does not call ordinary healthy close admission or synthesize a successful flush.
- Each retained resident owns at most one capture, preparation and publication outcome flight.
  Input protection, native identity, claims and reservations remain owned while admitted work
  drains. Incomplete capture, work settlement or service-reference retirement blocks replacement.
  Nonfinal close still admits no all-work barrier; failed-home whole-graph recovery owns its
  separate process fencing and retirement.
- Fresh candidate work settles exact session membership/claim recovery before dependent resident
  qualification. It authenticates the captured candidate and original publication outcome,
  independently settles any necessary save, and returns explicit retained custody on failure,
  cancellation or stale delivery. Only complete fresh resident/session/graph bindings and draft
  and work settlement authorize the existing atomic reopening boundary.
- Normal healthy close, clean interrupted-Exit retirement, detached final-teardown sources and
  their readiness proofs retain their existing contracts. Failed-generation custody cannot
  authorize native destruction, service publication, automatic close retry or interaction release.

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

- After successful whole-service-graph retirement, the retained process owner may settle its
  Activity enrollment and parent nondispatch slots with fresh same-home replacement candidate
  access and typed State/Syndic participants. Refuse absent or incomplete retirement and old or
  foreign candidate identity before settlement. Run the existing enrollment settlement followed by
  nondispatch convergence, preserving typed failure/outcome custody and already completed slots on
  cancellation or failure. Success requires both retained owners to be empty. This worker-side
  contribution adds no queue or cached readiness, preserves native reservations and process fencing,
  and grants no graph publication or interaction-release authority.

- Interrupted Exit routes this contribution through its existing retained candidate worker slot
  after successful session settlement, with the exact active cancelled request and successful
  graph retirement. Original session and complete service custody must be available; resident
  preparation and outstanding resident frame work exclude admission. The worker reacquires fresh
  typed State/Syndic participants and returns the service owner, original session, candidate and
  typed outcome before GUI delivery, even when the request changes or work unwinds. Failure stays
  in the single slot and blocks another pass; diagnostic reporting borrows that outcome without
  consuming it. This contribution neither publishes a graph nor releases recovery fences.

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
