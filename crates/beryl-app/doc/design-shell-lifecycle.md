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
- The process service owner retains one main-window registry across initial-service retries,
  sharing the original process admission gate. Its published-window factory accepts only immutable
  request, activation, operation-identity and editor-configuration sources. It derives home/domain
  references, marker sealing, turn-start requirements and submission authority from the complete
  published graph; callers cannot substitute those services. Acquisition and creation share the
  same exact home-reference object.
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

- Releasing startup interaction never authorizes native default destruction. Main-window native
  close callbacks retain the window until the process owner has completed its close obligations.
  Before ordinary command routing is installed, native close remains vetoed; afterward it submits
  the exact close intent and still vetoes default destruction. Successful owned cleanup removes
  the window explicitly through its retained native destruction authority.

- Successful startup transfers one move-only running Exit consumer alongside the complete graph
  and published windows. Existing startup command producers remain valid for deferred Exit events,
  including after removal of the old startup surface. One pending bit and one exact active request
  coalesce repeated delivery without a queue. Consuming an intent grants no shutdown authority.
  Duplicates during that request cannot become a later attempt; ending the exact request after
  cancellation or failure requires a fresh activation for another attempt. A foreign or stale
  completion cannot end a successor request. Wake delivery occurs outside command-state borrows.

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
  Normal quit waits for confirmation settlement as well as other native cleanup.

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
- A failed barrier before final teardown releases interaction gates from the retained coherent state without restoring
  cancelled continuations or repeating possible dispatch. Closing a settings or auxiliary window
  never becomes the final-main-window execution barrier.
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
