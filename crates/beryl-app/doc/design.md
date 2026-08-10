# Goals

Own Beryl's GPUI desktop-shell package boundary and compose user-visible features over typed Beryl-home, Syndic, transcript, and Codex App Server services.

Keep independent main-window presentation responsive while process-wide services coordinate exact shared storage health, runtime readiness, catalog state, thread claims, and live execution facts.

## Non-goals

- Owning process entry, CLI bootstrap, or selection of the configured Beryl home.
- Owning the physical Fjall database, home lock, Syndic record schemas, CAS protocol transport, backend process construction, or transcript-provider storage.
- Repeating feature-level product behavior, GUI composition, or system-level consistency policy inside this package boundary.
- Exposing raw Fjall handles, Syndic storage encodings, backend JSON, CAS historical transcripts, or GPUI-owned state across public package APIs.
- Retaining workspace, workspace-member, semantic-graph, checklist, graph-upkeep, checklist-bound decision, recursive column-selector, pending-new-thread, or compatibility-adapter models.

# Decisions

## Authority And Dependencies

- Root `doc/design.md` and the linked feature, system, GUI, and package documents own behavior consumed by this crate.
- This crate is the Beryl package that depends on `gpui` and assembles the OS windows and slots declared by `doc/gui/integration.md`.
- It consumes pure shared values from `beryl-model`, typed home/domain services from `beryl-home-store` and domain packages, normalized backend services from `beryl-backend`, and the transcript host/provider boundaries from the transcript-presentation and Syndic systems.
- Its direct `sha2` dependency computes the exact SHA-256 identity of the canonical serialized
  conversation-tool registry. The digest is durable profile correlation, not an authorization or
  secrecy primitive.
- Product policy remains in feature docs, cross-package durability and execution policy remains in system docs, and stored representation remains in the owning storage package.
- App-local orchestration types may correlate those boundaries, but they must not become a second durable authority or a compatibility facade over removed models.

## Process Shell Services

- One process-shell service graph is created after the configured Beryl home opens successfully.
- Shared services include typed Beryl-home domains, store-health publication, the runtime/root
  registry, managed runtime supervisors, the revision-bound paged catalog, lineage, and activity
  services, cursor-paged status metadata, settings and theme repositories, durable
  orchestration-job coordinators, and worker pools with explicit count and concurrency limits.
- The projection-connection service owns one configured count-bounded worker pool. Each admitted
  connection acquires its driver-and-ingester permit pair atomically before either worker starts,
  so concurrent candidates cannot each strand one permit. Construction failure, retirement, and
  shutdown release the pair; this local concurrency pool is not process-memory authority.
- Shared services publish versioned facts and command outcomes. They do not retain references to individual GPUI views or mutate window-local presentation directly.
- A busy-home startup creates only the compact busy-home window declared by GUI integration and does not create the ordinary process-shell service graph.
- An unreadable startup store creates no ordinary main conversation window from unvalidated state.

## Main Window Ownership

- Each main conversation window owns one `WindowId`, one exact selected-thread claim, fixed-capacity
  compact navigation rings, focus, flyouts, transient menus, selection, editor projection,
  transcript host, activity/status presentation, notices, and window geometry.
- Construction checks the stable process main-window count before allocating the controller or OS
  window. The restore-set cardinality is owned by the main-windows feature contract; this package
  cannot create an overflow controller or hidden deferred window.
- No main-window controller or GPUI entity is shared between windows.
- Shared facts such as store failure, runtime readiness, catalog revisions, and thread occupancy are independently projected into every affected window.
- Window-local commands carry exact window, thread, draft, revision, runtime, root, CAS, turn, or job identities required by their owning system. A stale result is rejected instead of being applied to the currently visible window by coincidence.
- Ordinary close, dedicated Exit, unexpected termination, restoration, virtual-desktop placement, and additional-window acquisition delegate durable state changes to the main-window and Beryl-home contracts.
- Settings and busy-home windows are separate top-level controllers and never receive main-window thread claims or session-restore records.

## Startup And Progressive Readiness

- Before ordinary windows become visible, this crate consumes the validated minimal session discovery result and each restored window's validated selected-thread, current-draft identity, visible editor range, and compact editor-frontier seed. It never preloads a complete draft. An empty restore set first produces either one atomic most-recent-runtime/root empty-thread acquisition or the one permitted zero-runtime threadless seed.
- The first visible ordinary surface is the final main conversation shell. There is no app-local loading-window state machine.
- Catalog, selected transcript, and runtime/CAS readiness are independent versioned inputs. Their feature-owned controls become available independently without rebuilding the outer shell.
- The initial zero-runtime shell is the only threadless main-window state. Once a runtime exists, window creation and activation require an exact claimed Syndic thread before publication.
- Existing coherent title, draft, transcript, focus, and viewport state remains published while a later activation is prepared. One activation transaction replaces them only after the target claim, draft, transcript seed, and revisions are ready.

## Beryl-Home Integration

- This crate never opens Fjall or reads raw keyspaces. It receives typed domain handles and repository services from the Beryl-home boundary.
- Draft flush, input admission, thread/draft creation, runtime/root creation, claims, Syndic
  generated-title, usage, and automatic branch-discussion archive mutation, session update, settings
  update, asset reference, CAS-binding transition, and handoff-job transition use typed revision-
  checked commands.
- Correctness-sensitive success is not published until the home-store command reports the required durability barrier complete.
- Cancellation may withdraw queued draft persistence only before writer admission. An admitted save is drained; a surfaced post-admission storage or persistence failure suspends its exact draft binding until same-home verification or recovery and coherent current-draft reconciliation establish the durable revision and sealed content reference.
- Store-health transitions invalidate outstanding mutation authority by revision. Persistent failure preserves existing window controllers and their last coherent in-memory presentation while feature-owned gates reject further store-dependent work.
- Reopen rebinds services only to the same validated home generation; it never creates a substitute home, imports old state, or treats cached presentation as durable proof.

## Catalog, Threads, And Drafts

- The process holds compact catalog revisions, active query cursors, and bounded shared result pages. Exact search, scope, and ordering indexes remain durable rather than becoming one complete resident catalog clone.
- The non-GUI catalog projection coordinator prepares one explicitly named Syndic thread at a
  stable Beryl-home revision. It consumes either the exact current compact Syndic summary or a
  prepared source-fenced replacement, joins it with exact runtime/root and present-or-absent claim
  sources, and returns one all-or-nothing home command. Unchanged sources participate through
  validation-only contributions; a required Syndic-summary replacement and the Beryl catalog row
  publish in that same command. Exact agreement is a no-op and a missing Syndic thread is an
  explicit outcome.
- The coordinator maps only source-owned visible facts into the rebuildable row. It does not
  choose title precedence, inspect thread history, normalize search keys independently, or retain a
  second thread-metadata authority. Durable scheduling, readiness, paging, caches, activation, and
  GUI refresh remain outside this boundary.
- Main windows derive filtered recent-first row identities from revision-bound query pages. GPUI creates only the visible fixed-height rows and bounded overscan required by the conversation-thread GUI contract.
- Catalog construction and refresh never enumerate CAS threads, read CAS historical transcripts, or use backend names and working directories as Beryl metadata.
- Thread creation, pristine-thread reuse, current-thread no-op, first-runtime creation, additional-window acquisition, selection, and release use atomic home-store/Syndic claim operations rather than app-local check-then-act logic.
- The selected composer is a range-backed editor projection over the exact durable current draft. It
  retains only configured visible and overscan pages, bounded edit/IME state, compact positions and
  markers, and a bounded undo frontier. Dirty revision tracking, timed autosave, flush barriers,
  activation, replacement-edit intent, and acceptance call typed Syndic/home services rather than
  persisting or reconstructing a window-owned whole-draft buffer.
- Ordinary draft seeds and persistence requests carry no selected-path parent. Idle acceptance
  names the expected thread, draft, gate, and content authority, and Syndic selects the current
  thread tail atomically. Background accepted-input promotion never changes the resident editor,
  draft record, draft revision, undo state, or draft asset owner.
- A draft save fences the exact current draft identity/revision rather than an older selected-path
  thread revision. The app persistence binding and request carry no thread revision, so a completed
  tail advance that preserves the draft does not invalidate an in-flight save. A genuine
  serialized conflict reconciles against exact draft state and retries the retained edit; it does
  not report changed immutable draft shape or discard the edit.
- Draft persistence retains exact binding, edit, request, and timer generations around one in-flight save. A stale completion cannot clean a later edit; a lifecycle flush drains the in-flight save and, when necessary, chains the latest edit only after exact receipt or recovered-state reconciliation.
- A save executor prepares the exact chunk manifest away from GPUI, resumes or creates its content-addressed building object, appends only bounded chunk batches, and finally publishes the sealed content reference with the draft revision. Intermediate durable chunks do not change the visible or durable current draft.
- Draft-save publication consumes one opaque executor-issued completion bound to the full request identity, including home generation, thread, draft, expected revision, sealed content reference and proof, timestamp, and scheduling generations. A caller-constructed status value or another service's numerically similar generations cannot publish success.
- Input admission streams each draft marker through exact Beryl-state asset metadata into one
  unpublished paged reference set. After seal, one home command validates its source identity,
  count, frontier and digest, rebinds the compact draft/admitted owner heads, and publishes the
  matching Syndic admission. Marker-free admission carries no synthetic empty set; one typed
  validation-only Asset participant proves both source and destination heads absent on the same
  serialized writer snapshot as Syndic publication.
- Image-label readiness retains only an exact Syndic thread frontier/revision and bounded current
  draft marker facts. Allocation and same-label proof use point reads and bounded lineage queries;
  the app never synchronizes a complete historical label cache.
- Replacement-edit start validates the target content and sealed reference-set digest, retains the
  historical submitted owner head, and publishes a copy-on-write current-draft head over the same
  immutable set in the Syndic edit command. One Asset mutation participant asserts the unchanged
  historical head and creates the draft head; it does not duplicate Asset-domain command roles.
  Cancellation keeps that head because it clears only replacement intent.
- A surfaced post-admission failure retains the editor and reconciles the permanent Syndic
  admission receipt before publishing acceptance or rejection. Exact means the receipt matches
  every caller discriminator, including the source thread/draft/gate revisions, sealed asset
  proof, and caller-named replacement draft, while permanent accepted order and route-leaf
  identity agree. Because that receipt is published only by the opaque app-owned `SyncAll`
  command, it certifies the command's atomic compact asset-owner transition without requiring
  either owner head to retain its immediate post-command state. Current route, gate, leaf
  lifecycle, replacement draft, and later promoted asset owner may all be valid descendants. The
  app never enumerates every marker merely to treat a partial match or blind mutation replay as
  success.
- An in-flight save or reconciliation rearms autosave only while its captured timer generation remains current. A newer committed autosave-setting publication retains its interval, record revision, generation, and publication-time deadline anchor.
- Thread activation keeps the prior coherent selection until the target is ready and publishes title, lineage, draft, composer-history scope, transcript seed, runtime/root memory, and claim transition together.
- Lineage and activity snapshots are query generations with bounded resident pages, not complete
  ancestor or process-session collections. Composer history uses one fixed-capacity process pool of
  compact sealed Syndic input references and recalls content through range-backed copy-on-write
  drafts rather than copied payloads.
- The application exposes no manual title, pin, archive, delete, runtime/root removal, or thread-rebind command path.

## Runtime And Live Execution Orchestration

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while availability and errors are projected per window.
- This crate requests normalized backend sessions and capabilities through `beryl-backend`; it does not construct commands, parse JSON-RPC, inspect backend storage, or own CAS schema compatibility.
- Accepted input and exact selected-path proof enter the CAS-live Syndic system, which owns binding validity, CAS-native lineage precedence, one-time recovered-history injection, live capture, same-thread operation gates, and durable incomplete-turn outcomes.
- Every non-GPUI projection coordinator is bound to one exact healthy Beryl-home generation. Its
  operation enters the process-wide flight registry keyed by Beryl-home id, healthy generation, and
  Syndic thread, so separately constructed coordinators cannot duplicate same-thread remote work.
  Different homes, generations, and threads remain independent, and no registry mutex is held
  during storage or backend work.
- One process-owned connection service creates each candidate foreground backend client with
  immutable full-profile intent, its connection, runtime and managed-process generations, and
  configured parser, page, queue, and concurrency limits before initialize reads its first byte. It binds that
  candidate to one exact `Arc<HomeStore>`, Beryl-home identity, healthy home generation, and
  registered Syndic storage before compatibility probing. Successful initialize and probing admit
  the same exact candidate; a report, store, session identity, or generation supplied later by
  another caller cannot promote a request-only or detached client or authorize recovery injection
  and provider staging.
- The shared worker configuration requires at least the two permits owned by one admitted
  connection, one long-lived scheduled-ordinary permit, and one protected steering-critical
  permit. Permits have closed connection, scheduled-ordinary, and steering-critical roles.
  Atomic connection-pair and scheduled-ordinary admission leave one permit available for steering
  unless a steering-critical worker already owns that progress capacity under the same accounting
  lock; a scheduled ordinary worker never satisfies the reserve. Steering-critical work may
  consume the final free permit, and every permit remains owned until its actual worker returns.
- Home shutdown fences scheduler and provider issuance, retires and joins every broker and backend
  connection bound to that home, joins accepted-input scheduler workers, and then invokes provider
  shutdown to release every idle or returned admitted-session checkout. Only after those store
  references are released may the owning service use `Arc::try_unwrap` and call `HomeStore::close`.
  A live connection or process-shell checkout can neither outlast explicit close nor silently
  degrade it into drop-only unlocking.
- Ordinary shutdown and exact typed persistent-failure observation elect one owner under the
  master command gate. The failed-health observation, gate epoch, and shutdown/failure election
  share one mutex. A scoped permit may commit only one short in-memory authority transition while
  holding that mutex; therefore either ordinary retirement linearizes first or failure invalidates
  it first, with no sampled-current gap. Connection authority is acquired before this master gate,
  and neither lock covers backend I/O, worker joins, or storage work. Ordinary shutdown may execute
  the preceding destructive sequence only when it wins. When failure wins, consuming service
  close joins the stable cut and returns one non-cloneable retained handoff instead of requesting
  worker shutdown, retiring a connection, stopping a provider, unwrapping the home, or calling
  `HomeStore::close`.
- Router admission, dispatch authorization, publication, finish, and abandonment mutations hold
  the router lane and commit through the exact operation-scoped command permit. Long-lived and
  destructor-owned router capabilities instead settle once under that lane and the master gate:
  ordinary-first performs bounded in-memory cleanup, while failure-first leaves or transfers the
  exact capability for freeze. A router waiter releases every drain-counted permit before waiting
  and observes the failure fence on a bounded wake path, so failure freeze never depends on a
  waiter whose command it is itself draining.
- The retained handoff owns the exact `Arc<HomeStore>`, home/service/failure generations, frozen
  stop and router evidence, bounded per-target diagnostic results, strong connection authority,
  and bounded pre-activation loaded projections surrendered by admitted workers. A pre-activation
  projection or ordinary same-native quarantine anchor owns one non-cloneable surrender child
  derived from its exact worker admission. The child and worker share one counted unit, so there is
  no separately acquirable residency pool. Activation releases the child only after a router has
  accepted the target; returning a target to pre-activation derives a fresh child from the still-
  admitted worker before router removal. Same-service quarantine transfer moves the child, while
  cross-service transfer receives the replacement worker's child first.
- Consuming a finished retained handoff checks its service out of the pointer-identical filled
  escrow slot while keeping that identity reservation continuously occupied, cancels and joins the
  old accepted-input scheduler, and seals one non-cloneable recovery inventory. Scheduler exit
  retains a typed distinction between the exact failed home generation rejecting old work, which is
  expected cut quiescence, and unrelated invariant failure, poisoning, or panic, which leaves the
  inventory non-promotable. The scheduler runtime remains owned outside scheduler-main unwind
  containment; a panic
  opens any owner-held worker launch gate, records sticky local failure, cancels both worker
  families, and joins every runtime-owned child before the parent unwind reaches its joiner. Child
  handle and kind metadata occupy one preallocated runtime record, and normal joins remove only the
  one handle being joined, so unwind cannot detach an unvisited worker. Each admitted command
  derives that distinction from its exact gate status; it never resamples mutable home health to
  reconstruct the cause, and an observed local
  failure or poisoned scheduler boundary dominates exit classification on either side of the
  persistent-failure election without displacing cut ownership. Inventory conversion reconciles
  that gate again after scheduler join so a later pre-seal fatality cannot be hidden by an earlier
  cut-correlated exit. A poisoned
  owner may be recovered only far enough to retain and join what it owns, never to certify the
  inventory as promotable. The inventory
  preserves every complete worker-surrendered pre-activation projection and every cut-identified
  barrier needed to dispose or adopt that bounded set. Inventory drop returns those owners to the
  same escrow through bounded in-memory work unless a later consuming stage disarms it. A late
  publication remains owned and makes the inventory non-promotable. Only a stable inventory may be
  consumed into the grouped local quarantine. Checkout validates the finished cut, sealed counts,
  zero late publication, and one available stage under the coordinator mutex before draining every
  retained collection. Preflight then requires pointer-exact agreement among cut, retained-service,
  and current connection sets; complete loaded-registry sibling coverage; an aggregate read-only
  match against every router's complete frozen-or-spent target-guard set; and exact connection-gate
  barrier counts with no live promotion or cleanup owner. The last check is separate because
  connection barriers have no loaded-registry token, so registry topology alone cannot prove that
  every private failure-retained barrier has a matching drained owner.
- Quarantine groups first by stable connection and exact loaded-thread/session identity, then
  compares the complete home, Syndic owner, binding revision, execution binding, CAS thread, loaded
  generation, and lineage witness. Exact-equal wrappers form one group with every distinct lease
  token and admission hold; a disagreement fails the whole group. Before registry mutation, every
  retained connection atomically exchanges its complete failure-retained promotion and cleanup set
  for one non-cloneable quarantine connection owner under that connection gate. Connections with
  zero barriers receive the same hold. An `Arc<ProjectionConnection>` proves identity but does not
  prevent retirement, while this hold prevents elected retirement from invalidating loaded tokens
  until quarantine adoption or local disposition. One registry-lock commit then keeps exactly the
  candidate tokens and locally removes every audited noncandidate token. Target guards settle only
  after that commit, rechecking the same aggregate batch before their consuming take.
- Success installs one opaque, non-executable authority with bounded content-free metadata. A
  preflight mismatch installs the complete drain as inert authority; a later poison, race, or
  disposition mismatch installs normalized but non-promotable ownership with all candidates,
  pending dispositions, completed-disposition counts, and the retained service still reachable.
  A publication crossing checkout is moved from the coordinator side collections into a separate
  inert installed authority before installation returns. Any publication after installation routes
  directly into another inert authority and conflicts the stage, so no late owner remains outside
  quarantine ownership. Success and owning error values expose the same bounded quarantine
  metadata while retaining all handles and tokens privately.
  Neither boundary selects one projection, reads the recovered store, issues a backend request or
  unsubscribe, mutates durable state, publishes a service, performs generation rebind, or reopens
  the old master gate. An incomplete cut remains inert and cannot become recovery authority.
- A normalized registry owner disarms only after the one-lock batch commit proves its exact token
  removed. If an inert local owner is later dropped, destructor settlement recovers a poisoned
  registry guard, revokes only that globally unique primary token wherever it resides, rebuilds
  connection-authority counts from surviving entries and reservations, and then releases its
  worker admission hold. Poison recovery may dispose local authority but never certifies a
  quarantine or re-enables ordinary registry operations.
- A loaded projection or same-native reacquisition anchor settles its complete wrapper and raw
  connection authority on one exact side of the master gate. Failure-first retains the wrapper's
  home, binding, execution, CAS-lineage, Syndic, and lease facts together; ordinary-first consumes
  only the raw authority. An under-gate retention callback validates identity without probing or
  reacquiring that gate.
- Live targets do not consume worker-capacity residency units. Each mounted router admits at most
  64 exact targets, while mounted connections remain bounded by their admitted connection-worker
  pairs. Failure therefore retains worker-derived surrender children plus router-bounded targets without a
  second capacity decision. The lowest loaded-registry lease layer can surrender an exact token
  even when no public wrapper exists. The service reserves its unique failure-escrow identity
  before mounting the cut; failure fills that exact cell, while constructor unwind or ordinary
  close releases only the pointer-identical empty reservation. Retention has no overflow leak path, public store command
  surface, or later quarantine, connection-epoch adoption, verification, reopen, retry, old-service
  retirement, or new-service publication behavior.
- A scheduled-promotion reservation that encounters persistent failure becomes an exact consuming
  failure-retained token keyed by the cut identity. The old command permit is released, but the
  connection retirement barrier remains owned until cut-identified recovery consumes that same
  token; a boolean marker or reconstructed reservation cannot authorize recovery.
- A cleanup owner crossing the same cut likewise becomes one exact consuming cut-identity token;
  no anonymous cleanup count may survive. Promotion release, cleanup completion, and their unwind
  drops commit the ordinary transition under the master gate or transfer the still-live authority
  to the cut, so failure-first cannot erase either barrier.
- Retired connection authority is not proof that its detachable runtime has released its retained
  home reference. The service retains every live attached connection shell until shutdown first
  takes and joins its store-bearing broker and driver, records bounded diagnostics, and leaves the
  shell detached. Ordinary reaping takes a registry snapshot, releases the registry, proves the
  exact core ordinary-retired and both workers finished, performs the joins, then reacquires the
  registry only to remove that pointer-identical detached shell. Admission runs this reaper before
  reserving another connection-worker pair, and consuming ordinary close runs it before draining
  the registry. It never calls a connection, driver, hub, or loaded-registry boundary while the
  service registry is held. A stale session therefore cannot hide an internal home owner from
  explicit close, while an already-reaped ordinary shell cannot become a spurious second shutdown
  failure.
- A connection-local one-way ordinary-shutdown settlement serializes worker checkout, joins,
  diagnostics publication, and hub detachment. One caller executes teardown; concurrent and later
  callers replay its clean-or-failed classification. The ordinary reaper enters the same settlement
  before interpreting an absent worker handle, so it cannot mistake another caller's checked-out
  join for completed teardown.
- Driver and ingester permits both follow actual worker return without forcing a join from Drop. The
  driver moves its permit through the stable adoption slot. The ingester shares a separate terminal
  disposition with the epoch's sticky ordinary-versus-exact-failure election: ordinary retirement
  arms release before cancellation, so terminal or already-terminal settlement drops the permit;
  exact failure arms cut-identified retention, so adoption join receives the same permit. Unknown,
  poison, and mismatch retain conservatively. Ordinary join therefore owns terminal proof only,
  while adoption join owns terminal proof plus the exact retained admission.
- Implicit service, connection, worker, lease, stop-owner, and router-capability drop is a bounded
  in-memory boundary. It may close admission, preserve conservative orphan evidence, request
  cancellation or retirement, wake a worker, and detach a handle; it cannot wait, join, call a
  provider shutdown hook, perform backend or durable-store I/O, or close the home. The consuming
  explicit service lifecycle owns those operations. Persistent-failure service drop fills the
  pre-reserved escrow without waiting, and ordinary last-owner settlement propagates its exact
  nonblocking retirement signal after every authority lock is released.
- This package implements the retained-connection adoption protocol owned by the CAS-live system as
  one stable backend core plus one replaceable service epoch. The stable core owns connection,
  runtime, managed-process, transport, sole stream driver, pre-reserved adoption-control slot,
  backend-bound forwarding hub, loaded registry, loaded session, lease identities, and the
  connection-scoped process fact and its final retirement authority. That fact is registered once;
  an epoch router can observe it and retire its own targets but cannot retire the stable connection
  when the old router is replaced or dropped.
- The replaceable epoch owns the home and Syndic handles, master command authorizer, router and
  forwarding endpoint, ordered broker and ingester, stop and compaction coordinators, scheduler,
  persistent-failure context, and every service-generation worker admission, including the driver,
  ingester, scheduled work, and pre-activation surrender children.
- A replacement app service exists first as a non-cloneable unpublished typestate behind the
  startup publication fence. It proves the pointer-identical retained `HomeStore` and canonical
  home identity, the same home id, a strictly newer healthy home generation, a strictly newer
  freshly allocated service generation, exact reacquired handles, an open new master gate, an empty
  connection registry, no prior attachment, adoption, worker start, or scheduler dequeue, and the
  complete replacement epoch topology. All execution remains behind that fence. Published services
  or independently cloned service handles cannot authorize adoption. The unpublished constructor
  uses a distinct dormant startup state and performs no recovered-pending read, Syndic revision
  read, durable convergence, or scheduler pass; closing worker startup after an ordinary
  constructor would not satisfy that boundary.
- Same-generation verification succeeds before persistent-failure election and keeps the current
  app service. It never creates the replacement typestate. Installed promotable quarantine proves
  the old generation is `failed`, so replacement construction requires the strictly newer healthy
  generation returned by exact same-home forced recovery.
- Exact current-home `verifying` observations are a nonterminal accepted-input scheduler pause, not
  a local scheduler failure. The paused path signals or joins the process supervisor, retains or
  safely restarts bounded scan work, and neither closes the master gate nor polls. Healthy
  verification completion must match both pointer-current home and service generations before the
  slot publishes `VerifiedCurrent` to the exact provider-waiter flight, retains its completed-flight
  snapshot, and then wakes that service with the dedicated same-generation-verified scheduler signal. The signal
  resumes all applicable lanes without enabling an unrelated active-steering retry; stale or failed
  verification cannot wake a replacement epoch. Nested replay, projection, admission, and
  publication errors preserve the exact typed health-gate cause through worker settlement; generic
  invariant strings and later mutable-health samples are not substitutes for that provenance.
- Provider staging and frame-build committers also join the sole supervisor-owned verification.
  Their exact service notification provides a monotonic multi-waiter completion epoch rather than
  the scheduler's consumable wake. Each committer atomically registers its exact home, home
  generation, and service generation, signals or joins, and waits without polling or holding a gate
  or process-slot lock. The supervisor publishes verified-current, failed, stale, or shutdown to
  every waiter before a failure cut drains live-command permits. Verified-current resumes the
  existing exact batch or build point-read reconciliation; all other outcomes become typed authority
  loss. Exact pre-command `verifying` joins before dispatch; if the reconciliation read observes a
  new `verifying` epoch, the committer joins again and repeats that same exact read without dropping
  its frontier. A provider worker never invokes home verification or changes durable identity to
  escape an ambiguous command.
- Persistent-failure notification elects the exact cut before publishing failed-or-stale provider
  completion, and that publication precedes both cut-worker signalling and live-command permit
  drain. Shutdown or unavailable provider completion similarly precedes ordinary or terminal
  service drain. Provider completion never consumes or substitutes for the scheduler resume wake.
- Adoption consumes that replacement typestate and the complete promotable quarantine. Its sorted
  pointer-identical connection owners must equal the closed old service's retained registry exactly,
  including retained connections with no candidates. Duplicate, missing, extra, retired, foreign,
  or late owners reject the whole set. The zero-connection case is valid only when both the registry
  and quarantine are exactly empty and still consumes both inputs. After commit, the complete
  connection set stays immutable through candidate reauthentication, seal, and final publication;
  no retired or unauthenticatable member may be pruned, including a zero-candidate member.
- Fallible preflight reserves every replacement driver-and-ingester worker pair, router, forwarding
  endpoint, and fixed broker resource before changing a connection. For every exact old candidate
  hold, it also acquires one replacement scheduled-worker admission and converts it directly into a
  recovery surrender hold. The separately bounded old and replacement pools account for their own
  complete sets from preflight through commit; the successful result keeps the complete old set
  charged in its closed old-epoch attachment until explicit retirement. Preflight starts replacement
  ingesters behind a closed start barrier; every other replacement worker remains dormant behind the
  service startup fence. It orders each stable driver's one-shot command-frontier control through a
  pre-reserved capacity-one stable slot
  outside the old gate and epoch queue. The driver settles already-dispatched work without another
  backend request, explicitly rejects not-yet-dispatched old commands through the gate-close
  frontier with one typed cut-correlated nondispatch completion, lets each scheduled worker
  surrender without durable accepted-input mutation or retry, parks only after scheduler
  quiescence can join those workers, and explicitly joins every ownership-clean old ingester. No
  live old-epoch command crosses adoption. Each join returns the exact old ingester
  admission and terminal receipt into the attempt-owned old-epoch attachment; neither is dropped or
  released to the old pool. Later ordinary queue traffic cannot starve or overtake adoption, and the
  driver selects no new stream observation after it sees the control. Cancellation or detached
  thread drop is not a join proof. Commands carry their admitting epoch, and ordinary dequeue
  validates it under the stable adoption-slot execution guard. An unexpected mismatch receives the
  same typed nondispatch completion before provider work as a defensive invariant; it is never
  silently dropped or executed across epochs.
- The capacity-one control slot survives each successfully published app-service generation. Its
  cut-bound control message and park token are one-shot; publication leaves the slot empty and
  eligible only for a later strictly newer failure cut. An inert adoption failure permanently
  disables every touched core and cannot reuse the slot to retry that failed cut.
- Every driver cycle, including the one-time initial approval-interruption drain, begins under the
  stable adoption-slot execution guard. Hub or epoch coordination loss under that guard moves the
  backend session and worker admission into a stable non-executable quiesced state; it never falls
  through to transport shutdown. Exact-cut inert conversion changes empty, pending, parked,
  starting, or quiesced state into disabled state while retaining all admissions. Ordinary stop
  notification and implicit Drop cannot authorize shutdown from quiesced or disabled state. Only a
  typed proven ordinary-lifecycle exit or the sole explicit consuming inert disposition may shut
  down the backend, and the driver loop has no unconditional shutdown epilogue.
- Ordinary last-owner signaling does not reuse the quiesced or disabled adoption states and does not
  join from Drop. Its winning closure arms ingester-admission release before cancellation; a
  failure-winning closure instead arms exact-cut retention for adoption. A later admission or
  consuming service close snapshots registry membership, drops the registry guard, and reaps only a
  worker-finished core whose ordinary retirement already won; it removes that exact detached shell
  only after the join has completed. Admission performs this before connection-pair reservation.
  Persistent-failure and adoption-owned cores are ineligible.
- Preflight sends and awaits all driver park controls in ascending stable connection generation,
  joins all old ingesters in that same order, and only then acquires forwarding-hub epoch barriers
  in ascending stable connection generation. It next acquires the old and replacement service
  registries in ascending service generation. It never parks or joins while holding coordinator,
  router, connection-authority, loaded-registry, home, or Syndic locks, and never acquires a driver,
  hub, router, connection, or loaded-registry boundary while a service registry is held.
  Connection-owner and service-registry identity and topology are checked before parking and again
  at the fenced boundary without co-holding router and connection locks.
- The stable forwarding hub holds its epoch barrier from endpoint selection through synchronous
  acknowledgement. A selected `thread/closed` records the epoch router fence, releases that router,
  and invalidates exact stable connection and loaded-registry authority before releasing the outer
  barrier. An observation selected before adoption therefore settles completely against the closed
  old epoch; one selected afterward can reach only the replacement endpoint. Adoption preserves
  candidate owners regardless of token liveness; consuming candidate reauthentication performs
  the exact loaded-token authentication and rejects a candidate already revoked by closure.
- After every fallible check, start, park, join, and lock acquisition has succeeded, the fenced
  commit is infallible and ownership-only. It exchanges all epoch slots, worker admissions,
  candidate recovery holds, and service-registry memberships, then moves every ingester start token
  and driver park token into the success output. It opens no ingester and releases no driver. The
  commit performs no allocation, waiting, new lock acquisition, user callback, backend or storage
  work, or recoverable operation.
- Success returns one non-cloneable adopted-but-unpublished app-service authority owning the new
  service, every stable connection, every still-quarantined candidate owner and registry-token or
  local-disposition identity, and all closed old-service attachments needed for later retirement.
  Candidate execution stays unavailable through consuming reauthentication and final publication.
  Recovered-home or adopted-service drift, stable-core retirement or mismatch, service-membership
  loss, or unavailable shared registry authentication after commit returns one distinct terminal
  adopted-service owner for the complete fixed set. It cannot retry adoption, prune a connection,
  seal, or publish. Any pre-commit mismatch,
  late owner, duplicate use, poison, park or join failure, detected partial installation, or unwind
  consumes both inputs, every prepared resource, and every old or new attachment reached by the
  attempt into one inert non-executable owner with no retry, command, token, candidate, or
  publication surface. Before parking begins, preallocated attempt state provides an inert fallback
  for every exact core. Failure or unwind marks touched hub endpoints inert and touched drivers
  permanently parked or cancel-only before releasing any fence; no borrowed guard escapes, no Drop
  resumes a driver, and nothing reached by the attempt is released outside the inert authority or
  its pre-reserved escrow.
- Adoption's final ownership move locks the old-cut coordinator and late-owner escrow in that order
  and changes checkout to adopted only while both remain empty and pointer-exact. The success owner
  retains one non-cloneable adoption fence. After old publication sources retire, the retirement
  boundary consumes that fence under the same two locks into one retirement witness. A publisher
  that wins first
  prevents witness issue and leaves the fence owned by terminal failure; retirement that wins first
  enters a terminal adoption-retired stage whose escrow absorbs any later owner without changing
  executable state. The retirement boundary acquires no process-publication lock and the later
  publication commit must consume the witness with the converged adopted service.
- The adopted success authority also owns every new-ingester start token and stable-driver park
  token. Its new endpoint is installed but cannot acknowledge an operation, and no backend
  observation or command executes before publication. Only final recovery publication, after
  complete candidate-set convergence, old-epoch retirement, and startup convergence, may consume
  all activation tokens together. It locks every exact stable-connection authority and retirement
  gate in stable order and holds them continuously across final validation, process-service
  installation, and startup-gate opening. It arms every replacement ingester and stable driver
  against the same closed startup gate, then one short process publication commit atomically
  installs the current app service and opens that gate. Publication-first installs before releasing
  converged retirement retention; retirement-first returns the complete terminal adopted-service
  owner. A check-then-release-then-publish sequence is not authorizing, and wakes occur only after
  the publication and connection locks are released. The old-cut retirement witness is a separate
  required input to this same commit.
- The process publication slot issues only counted, non-cloneable scoped leases for its current
  service. Withdrawal removes the pointer-exact epoch and waits until each lease has first released
  its service `Arc` and only then decremented the count. Observing zero leases therefore authorizes
  exact consumption of the withdrawn service owner without polling or ownership retries.
- The process recovery supervisor owns the scheduled-ordinary provider factory across service
  generations. Stable admitted-session ownership remains in that factory; a service owns only one
  epoch-scoped provider view whose shutdown fences issuance and returns checkouts without retiring
  an adopted stable connection. Final process shutdown closes the factory after the current service
  is settled. The factory owner tracks issued views only through weak revocation controls and fences
  every still-reachable view before releasing the stable pool; it gains no service or connection
  ownership. Replacement construction always requests a fresh view and never reuses the failed
  service's provider object.
- Shutdown consumed during a forced-reopen delay uses a separate nonpublishing terminal disposition,
  not quarantine Drop and not adoption. It atomically checks out installed or conflicted old-cut
  authority into a terminal escrow, stops and settles every retained candidate, local disposition,
  connection barrier, stable driver, ingester, context worker, and old provider view outside the
  authority locks, then removes the exact service escrow only after no late owner remains. It keeps
  the exact opened home retained by the supervisor and exposes no replacement service or executable
  projection.
- Publication moves dormant accepted provenance into a bounded replacement-scheduler recovery lane
  before opening the shared startup gate. After the gate proves publication, that lane reconstructs
  the original registry token and worker admission, asks the current provider only for the remaining
  process-shell execution authority, and starts ordinary execution at complete input preparation.
  A provider decline restores the same dormant owner and hold for a later execution-ready wake. No
  ordinary projection acquisition or second lease token is permitted on this path.
- Adoption sends no recovered read, backend request, unsubscribe, durable mutation, candidate
  selection, generation rebind, service publication, history reconstruction, scheduler dequeue, or
  old-gate reopening. The inert owner's sole consuming explicit disposition can only cancel and join
  workers, revoke local authority, and release ownership outside every authority lock. Implicit drop
  uses only bounded cancellation, pre-reserved escrow, revocation, and handle detachment and never
  waits, joins, or performs backend or storage I/O.
- Terminal reauthentication disposition settles candidate and connection owners first, then
  requests and joins every unpublished replacement-service worker and shuts down its provider. It
  reports a typed shutdown failure only after attempting every join and releases without closing
  the retained same-home authority.
- The connection service rejects any candidate whose initialize proof opts out of foreground stream
  notifications. One bounded non-GPUI connection worker exclusively owns each validated client,
  serializes request and unsubscribe commands, and polls the stream only while idle. A second
  independently progressing connection-ordered ingester receives compact controls and approvals,
  dynamic-tool argument operations, and provider operations through the same capacity-one broker.
  After emitting one operation, the backend does
  not advance later parser input, refill the fixed parser window, or release the matching response
  before acknowledgement. The fixed window may contain bounded read-ahead.
- Worker commands receive only a request-capability view with the admitted lineage and unsubscribe
  operations. They cannot poll or drain the backend stream; event consumption remains structurally
  exclusive to the connection worker.
- An approval server request is a dedicated ordered-broker operation, never a generic compact
  control. The broker atomically validates and enqueues the non-cloneable request on its exact turn
  target before acknowledging it. Permission admission also installs its driver-owned interruption
  obligation before that acknowledgement; only then does the unblocked backend session perform the
  sole denial write and expose a later foreground response. Denial does not erase an already routed
  presentation event.
- The routed approval owns only bounded request, kind, thread, turn, and item identity plus one
  exact-session response state. The app does not receive command, cwd, reason, permission body, raw
  params, or a pretty-printed payload. Status and failure diagnostics derive from compact redacted
  facts, and interruption remains targeted by the exact routed turn identity.
- Successful approval acknowledgement means the request entered only that exact target queue and,
  for permission denial, the matching durable stop operation is already owned. Command-execution
  and file-change target-local failure may return the request for denial without another
  interruption. Permission target-local failure does so only when exact target closure proves that
  interruption is no longer required; otherwise it returns a fatal ownership result without
  exercising response authority. Missing exact targets, invalid routes, broker cancellation, and
  router-wide failures are likewise fatal; no approval is silently dropped or offered to another
  target.
- Before routing a permission request, the broker occupies its sole capacity-one interruption
  entry. Exact-route authorization admits or joins the durable stop operation with interrupting-
  approval cause, monotonically records that cause even on a join, and produces an obligation bound
  to that operation, connection, target registration, loaded generation, turn, item, attempt
  disposition, and timeout. The obligation is owned by the connection driver independently of the
  presentation receiver. It is installed before acknowledgement, and dropping the presentation
  target afterward cannot cancel the durable stop.
- Target polling is presentation-only and cannot execute an interruption. After a successful
  enclosing request or ordered stream poll, the sole connection driver drains the installed
  permission obligation through the stop coordinator before exposing that result or advancing
  later work. If the stop's sole attempt has not crossed a request byte, the driver dispatches it
  only after the denial. If the approval arrived interleaved with that exact attempt after a byte
  crossed, it joins the existing cut and the driver issues no second interruption. A target-local
  approval failure is handled as interleaved progress and settles the matching obligation without
  creating another stop. Interrupting-approval cause prohibits safe reopen: local nondispatch,
  pinned handler rejection, and authority-invalidating failure retire the connection through stop
  abandonment, while possible dispatch retains stopping until terminal or loss convergence.
- Command-execution and file-change denials already interrupt in the backend and never install or
  issue a second interruption. Permission denial alone requires the separate exact post-denial
  `turn/interrupt` owned by the durable stop operation.
- The worker carries the exact backend operation result separately from a subsequent observation
  routing failure. Whole-connection routing failure retires the connection and gates ordinary
  result publication; it does not erase dispatch evidence or turn an observed non-idempotent
  outcome into non-dispatch.
- Backend rejection and terminal text reaches the app only as the fixed bounded UTF-8 diagnostic
  projection, truncation fact, finite code, raw-data presence, and any closed method-specific
  verdict. The app never receives, serializes, clones, or reparses raw JSON-RPC `data` or a complete
  arbitrary backend message. Visible notices may further shorten that already bounded projection.
- One process stop coordinator is keyed by healthy Beryl-home generation and Syndic thread and
  participates in the same target-operation election as steering claim and terminal handoff. It
  accepts only an exact currently registered ordinary-turn or provider-operation target. Selected
  UI state, status text, and a process-local CAS id are never sufficient.
- The coordinator holds the target-operation fence from the pinned handler precheck through request
  disposition or terminal observation. No successor turn or compaction start may enter that loaded
  CAS thread during the cut. This caller-owned no-successor proof remains required for exact CAS
  0.146.0 unless retained release-scoped evidence proves one atomic targeted core interrupt after
  the app-server turn check.
- Deliberate control, diagnostic control, healthy-home window close, and Beryl-owned interrupting
  approval admit or join one durable stop operation. Join monotonically adds the caller's closed
  cause to that record. The admission mutation atomically publishes the stopping gate and record,
  reroutes ready or retryable input to next-turn authority, and cancels any pending app-owned
  automatic lifecycle continuation. It preserves separately accepted input and never moves sent
  input back into the composer draft.
- A stop owner first reconciles the durable operation, claims one caller-generated attempt in
  Syndic, and only then hands the non-cloneable dispatch capability to the already authenticated
  foreground connection driver. The driver performs the sole `turn/interrupt`; the coordinator
  never creates a detached request-only connection, resumes a replacement session, or sends from a
  shell-owned worker.
- Stop admission, join, durable claim, and `begin_dispatch` revalidate their exact live-command
  generation while holding the same coordinator-state mutex used by persistent-failure freeze. A
  command that establishes that mutex fence first may finish its already-admitted transition; a
  failure that establishes it first rejects the stale command as home-authority loss. Sampling the
  gate before acquiring the mutex is not dispatch authority.
- The coordinator allocates distinct stop-operation and attempt nonces as 128-bit values from the
  OS cryptographic random source. Durable operation identity combines the Syndic thread with its
  nonce, and the attempt nonce is scoped to that operation. It never derives either from CAS ids or
  describes them as backend idempotency keys.
- Matching response acceptance leaves the durable operation stopping until exact terminal
  observation. Only a local outcome proven before every request byte may invoke the atomic safe-
  reopen mutation while the same target remains exact and interrupting-approval cause is absent. A
  pinned handler rejection proves that no core interrupt was enqueued but not that the target
  remains current; absent an already observed matching terminal, the coordinator retires the
  projection and converges through stop abandonment. Local nondispatch with interrupting-approval
  cause also abandons rather than waiving the post-denial stop. Possible dispatch leaves the
  claimed operation in place and prohibits another primary request. Connection retirement or
  process restart converges through stop abandonment and source-less incomplete history, not
  resend.
- Terminal observation, steering claim, and stop admission are serialized by exact target
  authority. Terminal consumes a matching stop record into ordinary history finalization; an
  earlier steering attempt settles before stop may win; and once stop wins no later steering work
  can claim its selected generation. Input accepted during the cut remains ordered next-turn work.
- Stop progress remains a working turn with popup-owned feedback until exact terminal or
  authority-loss convergence. A healthy-home window close keeps its claim and waits for that
  convergence. Store failure first closes new live-command authorization and may then perform only
  the separate one-shot in-memory exact best-effort interrupt defined by the Beryl-home system when
  retained evidence proves no earlier primary interruption may have crossed. Its fixed
  failure-generation guard prevents duplicates; it cannot manufacture durable success or release
  the claim.
- Hard escalation attaches to the same primary operation after durable admission. When escalation
  attaches, the app freezes one at-most-64-entry deduplicated process-local snapshot of normalized
  handles bound to the exact CAS thread, turn, loaded generation, and provider item. Later activity
  cannot refresh it, restart cannot recover it, and overflow or unsupported handle kinds become
  checked omitted-active counts plus closed limitation flags rather than unbounded storage.
- Exact CAS 0.146.0 contributes no individual turn-process entry unless retained release-scoped
  evidence proves an ABA-safe identity. Standalone
  `command/exec/terminate` belongs to an unrelated namespace, and
  `thread/backgroundTerminals/terminate` can match only a reusable numeric id rather than the
  frozen provider item. The app records a closed identity-unsafe unsupported limitation and never
  attempts to repair the ABA hazard with a prior list read.
- At most one pinned thread-cleanup entry names the exact loaded CAS thread. It is an explicitly
  coarse `thread/backgroundTerminals/clean` request admitted only when normalized selected-
  operation activity shows at least one still-active turn command at attachment time. It always
  runs after individually addressable frozen targets and reports request acceptance without
  claiming per-process or cleanup completion.
- A matching cleanup response releases the hard-run hold only under the same loaded-session and
  no-successor proof. Pinned core ordering then processes the already enqueued cleanup before any
  later Beryl operation submitted on that session. Session loss supplies no cleanup-completion
  fact; it retires old authority instead of transferring the ordering proof.
- A pinned coarse-cleanup JSON-RPC error after capability admission and local thread validation
  retires the parent projection and marks every later frozen target unavailable. The app never
  parses error text to infer that the error is target-local, and it never downgrades an exact parent
  terminal already published while that request was pending.
- One process-local escalation slot is keyed by the durable stop-operation identity. Duplicate hard
  callers join its running or finished bounded result and never refresh or repeat the snapshot.
- The first attachment may linearize before the primary response or after a confirmed
  `RequestAccepted` response while the same exact foreground connection remains authoritative.
  Confirmed acceptance has a distinct process-local state from completion unknown: both forbid a
  second primary interruption, but only confirmed acceptance can mint the sole late hard-run
  continuation.
- That slot is retained only while the stop remains current or its terminal successor owns the
  finalization-release hold. It is released after the bounded result reaches all already joined
  callers and status-operation feedback and the successor no longer needs the hold. A stale later
  caller observes the consumed stop identity and cannot rebuild the escalation, so retained slots
  are bounded by live foreground stop registrations rather than historical stops.
- Attachment reserves one non-cloneable continuation on the exact foreground driver plus one
  finalization-release hold keyed by the stop identity. A continuation attached before settlement
  inherits the original router election across primary settlement, runs inline after the authorized
  primary outcome, retains that election through fresh backend no-successor authorization, and
  releases it before waiting for cleanup response. A continuation attached after confirmed
  acceptance is queued on that same driver, acquires a fresh exact router election, freshly binds
  and authorizes the exact target, and runs without another `turn/interrupt`; no detached task
  reacquires the session.
- Exact CAS 0.146.0 contributes no eligible child or subagent interruption handle unless retained
  release-scoped evidence proves an atomic targeted child primitive whose successors Beryl can
  fence. It likewise contributes no individual turn-process handle unless exact-release evidence
  proves an ABA-safe identity. The snapshot
  records those closed unsupported limitations while retaining the independently supported coarse
  thread-cleanup target.
- The hard runner waits for matching primary response acceptance or local proven nondispatch, then
  attempts each still-authorized exact frozen target once in normalized provider-observation order
  with stable kind and handle tie-breaks and retains a separate result; coarse thread cleanup is
  last. A pinned handler rejection invalidates parent-target authority; completion unknown retires
  the foreground session and app-side router/registry authority unconditionally, independent of the
  enclosed error's generic transport classification. Either outcome marks all unattempted frozen
  targets unavailable and performs no escalation request. The runner never guesses from command
  text, process enumeration, working directory, names, or historical scans. Provider-operation hard
  escalation has no pinned child, command, or coarse-cleanup target; any future additional target
  requires its own exact release-proven primitive.
- Proven primary nondispatch keeps the stop cut closed while that bounded run proceeds and invokes
  target-kind-specific safe reopening only afterward if the blocked operation remains exact and interrupting-approval cause is
  absent. Matching primary acceptance, possible dispatch, or interrupting-approval cause never
  reopens because hard targets finished. Process loss discards the snapshot without replaying
  either primary or hard requests; a still-current stop abandons, while an already published
  terminal successor continues its ordinary or provider-operation finalization.
- The hard runner submits through the same exact foreground driver and loaded connection
  generation as the primary operation. It never constructs a backend connector, resumes a thread,
  or opens a detached request-only session for escalation.
- An interleaved terminal remains ordered provider ingress and may durably consume the stop into
  ordinary `FinalizingHistory` or the dedicated compaction terminal successor while a hard-target
  response is pending. The process-local hold authenticates the terminal successor's stop identity
  and withholds only finalization release to idle and accepted-next promotion until the hard runner
  settles. Process loss drops that non-durable hold and snapshot; startup finishes the durable fixed
  point without replaying hard work.

### Context Compaction

- One process compaction coordinator is keyed by healthy Beryl-home generation and Syndic thread
  and participates in the existing target-operation election. It admits manual compaction only
  from exact selected-thread authority and automatic compaction only from the exact terminal
  lifecycle intent; UI state or a process-local CAS id is never sufficient.
- The coordinator allocates distinct operation and attempt nonces from the OS cryptographic random
  source, retypes the operation payload as the provider-operation turn id, and derives the snapshot
  id with the system-owned domain-separated hash over the complete admission target. It asks
  Syndic to validate and atomically create that authority, snapshots the feature-owned completion
  timeout, claims the sole request attempt, and passes its non-cloneable dispatch capability to the
  authenticated foreground driver. It owns no detached connector or shell worker.
- The driver issues exact `thread/compact/start` while holding the no-successor election. Its empty
  acknowledgement is reconciled through the backend non-idempotent outcome family independently
  from provider ingress. Provider progress continues through the capacity-one ordered broker,
  whose target registration accepts the exact compaction CAS turn and item lifecycle without
  entering the ordinary activation FIFO or active-steering route.
- The target registration first accepts only the exact thread-scoped active status expected before
  turn publication. Ordered `turn/started` publishes the CAS turn into the provider-operation
  snapshot before a stop target becomes eligible. `ContextCompaction` item kind is known at
  provider begin and selects a bounded resident exact-schema marker parser rather than unpublished
  Syndic observation staging. The authenticated seal derives the canonical item identity from the
  exact router permit and publishes only the dedicated marker event with the provider-observed
  timestamp. Matching terminal controls commit through dedicated Syndic compaction mutations.
  Wrong identity, a second turn, unsupported lifecycle, broker loss, or target closure retires
  exact authority and converges the operation incomplete.
- Request acceptance starts one process-local completion deadline only while exact provider
  terminal has not already settled the operation. A late matching acknowledgement reconciles as a
  no-op only after Syndic authenticates that consumed successor against its independent exact
  settlement receipt, the consumed operation's full receipt commitment, and the concrete
  settlement-specific durable successor after any later gate progress. A late same-attempt completion-
  unknown result preserves the terminal-chosen binding and lifecycle but retires the foreground
  connection. Expiry resolves only that waiter with bounded still-running feedback; the
  coordinator and target registration remain live, provider ingress continues, and no durable
  state changes. Duplicate manual callers join or observe the existing operation rather than
  refreshing the deadline or dispatching again.
- Local proven nondispatch safely consumes the operation while preserving the exact valid binding.
  Pinned rejection and completion unknown while the operation remains live and unterminated retire
  the connection and binding and invoke source-less provider-operation convergence. Matching
  marker-then-successful-terminal runs bounded item finalization and exact settlement while
  preserving the binding. Interrupted terminal consumes interruption and preserves the binding
  only with exact ordered idle-status proof; failed, system-error, marker-incomplete, and idle-
  unproven interrupted outcomes retire it.
- Before CAS turn publication, stop lookup returns exact non-interruptible compaction state. After
  publication, the stop coordinator uses the provider-operation target and the same foreground
  driver. Transition to stopping retains the compaction coordinator only as blocked-operation
  correlation; terminal or authority loss consumes the paired records exactly once. Hard stop has
  no additional pinned compaction target.
- Local proven nondispatch of that primary interruption invokes the provider-operation safe-reopen
  mutation after any hard run: it restores the same live compaction record and compacting gate,
  retains `NextTurn(Compaction)` input, and creates no steering generation or new compact-start.
  Pinned rejection, possible dispatch with later authority loss, process restart, or terminal
  settlement never uses that reopen. Storage retains and authenticates the exact source and
  immediate successor compaction revisions for safe reopen, matching terminal, and abandonment;
  app coordination never infers post-stop ancestry from a revision floor.
- A `phase_continue` intent is process-local and keyed to its yielding turn. Terminal-history
  completion invokes automatic compaction admission only if no accepted-next work already exists.
  Existing work cancels the intent and wakes the accepted scheduler. Admission supplies the exact
  healthy-home identity to the durable operation. A successful compaction performs the storage-
  owned serialized user-work-versus-continuation settlement, whose API accepts no replacement
  home identity.
- The healthy-home window-close path cancels that process-local intent through the coordinator
  before stop admission or finalization waiting, including after parent terminal and before CAS has
  published a compaction turn. If compaction already exists, close handles the provider operation
  separately while its eventual settlement has no continuation authority. Another surviving
  window or process service cannot reattach the cancelled intent.
- After successful compaction and while the intent remains exact, the coordinator derives the turn
  and canonical-item identities, stages the fixed ownerless content-addressed candidate with an
  empty asset proof, revalidates that close or stop did not cancel the intent, and supplies those
  facts to atomic settlement. Storage independently verifies the identity domain from the durable
  operation's admission home identity. If user work wins, it consumes the intent and publishes
  accepted-next readiness; if continuation wins, it hands the resulting pending turn to the
  protected ordinary execution lane. Ambiguous settlement is point-reconciled across operation,
  turn, item, and content; no second automatic turn is created.
- Fixed-content staging reconciles by its derived content identity and exact sealed frontier. A
  definitive preparation failure consumes the intent, reports bounded continuation failure, and
  invokes successful compaction settlement without continuation so accepted input can proceed.
  Beryl-home failure follows the home barrier instead of fabricating that settlement.
- Stop, exact compaction failure, non-success terminal, and authority loss consume automatic
  continuation intent but never accepted user input. Completion timeout alone does not. Process
  restart drops every lifecycle intent and therefore may settle or recover compaction but never
  reconstruct the automatic message.
- A target-local streamed-observation or compact-control routing failure revokes only that exact
  target and gates the matching
  command result with the target identity and close reason. It does not falsely report connection
  retirement or let another target's ordinary request success escape the ordering boundary.
- Each connection worker accepts at most 64 queued compact commands and uses a 20-millisecond idle
  poll. A quiet poll advances content-free
  diagnostics but keeps the worker and every target active.
- Each connection owns one non-cloneable ordered sink and one capacity-one broker tied to the exact
  home, connection, and managed-process generations. The broker owns one preallocated
  acknowledgement slot, one current bounded fragment, and at most one provider observation or
  dynamic-tool argument request in building, sealed, or consuming state. It transfers provider
  pages directly into unpublished Syndic staging, transfers dynamic arguments into the selected
  feature sink, and returns the same sole page only after final acceptance. Page or queue
  unavailability applies backpressure or closes the exact connection with typed incomplete state; receiver abandonment,
  identity conflict, or retirement cannot redirect an observation to another home, thread, or
  generation.
- Approval broker, target-queue, and permission-obligation retention uses fixed count capacities.
  The capacity-one obligation entry remains occupied when discarded wire fields are oversized, and
  every drain, target-local failure, or whole-connection close releases it. The app never measures
  discarded payload length or reconstructs diagnostic JSON.
- The app starts that broker before binding its sink. Backend binding submits the bounded compact
  prefix retained by initialization or compatibility probing and waits for every acknowledgement
  before it may succeed, so the first bound poll cannot overtake older controls. Permission
  approval is ineligible for that unbound prefix because no durable stop owner exists; observing
  one retires the candidate session. Every eligible prefix entry remains in the fixed-capacity
  prefix from decode through acknowledgement.
- A terminal broker result atomically installs its ownership-preserving reply and closes later
  admission at the acknowledgement slot before publishing connection cancellation. The blocked
  submitter receives that exact reply even though the slot is closed, while no subsequent
  operation can enter the retired ingester.
- A fail-closed HomeStore writer panic during the complete live-source publication transaction,
  including pending-turn activation and later source-event publication, is contained while the
  ingester still owns the exact broker operation. Typed failed
  health first elects the master cut, the nested source-publication permit settles on the failure
  side, and the ingester releases both its nested and outer drain-counted commands before installing
  the terminal acknowledgement. The failure worker still drains every admitted command and has no
  provider-worker exemption or timeout.
- A provisional ordinary-turn target carries one immutable pending activation authority naming its
  exact Syndic thread and turn, post-activation binding and gate revisions, pre-activation turn-state
  revision, execution snapshot, observation time, and the loaded projection's current healthy home
  generation. Routed `turn/started` acquires one
  non-cloneable permit under that target's exact registration, owner, and loaded generation, then
  releases the router lock before publishing the active CAS identity and `TurnActivated`. The
  compact start never enters the generic target FIFO; the permit reports completion only after both
  facts are exact and durable.
- Ambiguous activation publication verifies only the same retained home generation, then
  classifies the exact active-identity or source-event command. Exact state continues, absent or
  colliding state fails the exact target closed, and no later provider operation can acquire its
  route. Both checked submitted-user controls cross this broker boundary before a streamed
  `turn/start` exact response can become visible. The response path accepts that result only through
  a broker-issued proof that activation is already durable; it performs no publication or
  reconciliation. A later matching routed start is idempotent and a different identity fails
  closed.
- Target-close cleanup retains the broker-bound CAS turn even when the compact start was never
  exposed. It classifies the exact absent, active-only, or activated durable frontier before
  abandoning the binding and emitting a source-less incomplete terminal event; only a target that
  never bound a CAS turn may use the pre-publication frontier.
- Whole-connection cancellation signals the broker before waiting for the serialized connection
  runtime. A permission request paused before ordered delivery is ownership-preservingly returned
  as fatal so the backend retires without denial; no durable stop exists yet. If cancellation wins
  after target enqueue but before stop ownership commits, the occupied entry becomes closing and
  returns the same fatal ownership result. If stop ownership committed first, the durable operation
  survives presentation cancellation and connection retirement converges it through abandonment.
  An empty timed receive is not authority to close the acknowledgement slot while a raced submit
  can still arrive, and an already queued presentation fact does not keep the whole connection
  alive.
- One provider `begin` snapshots the current healthy home generation and reacquired Syndic handle,
  then allocates one caller-owned durable 128-bit observation identity from the OS cryptographic
  random source. Every later control and fragment remains fenced to that exact authority, and seal
  may publish only when the target permit names the same generation. Ambiguous staging never
  rotates the identity. The broker classifies each exact offered batch against a point read:
  `Next` succeeds, `Expected` retries the same operation, and `Conflict` fails closed. It does not
  invoke generation-changing home recovery from a live connection.
- When pinned wire order presents size-unbounded item fields before trailing `threadId` or `turnId`,
  the broker carries connection-scoped unattached authority until the backend decoder validates the
  route. It then binds the sealed staging handle to the exact target or abandons it without
  publication. The app never constructs a normalized item solely to discover its route.
- Once an abnormal target close retires a remote thread lane, that CAS thread cannot register a
  replacement target on the same connection because wire events contain no local loaded-session
  generation. Each connection remembers at most 256 such lanes; needing another fence retires the
  connection instead of forgetting an older fence. A later proven-terminal reuse path must retain
  the same loaded authority and establish its own ordered handoff rather than clearing this fence.
- No target retains a queue of normalized provider events, generic compact controls, provider
  receipts, sealed handles, or an approximate parsed-byte allowance. Provider seal acquires one non-cloneable publication permit
  under the router's target gate, releases the router lock, and finishes or fails before broker
  acknowledgement. A close that wins first abandons the unpublished handle; a permit that wins
  delays final target removal until it reports. Valid compact controls without a target are counted
  but not retained, while a provider observation without a permit is abandoned and malformed route
  identities retire the whole connection rather than being guessed, rewritten, or redirected. The
  invalid split compact-control/provider ordering is recorded in
  `doc/failures/cas-phase13-split-provider-control-ordering.md`.
- The stable connection forwarding sink consumes compact ordered thread close before replaceable
  service-epoch broker cancellation, records the router-lane fence, and then revokes exact loaded
  authority under the connection gate. The replaceable ordered ingester applies the remaining
  closed compact-control union directly to each final owner. Turn activation, checked-user
  lifecycle, terminal publication, account facts, token usage, and bounded agent metadata never
  enter a generic target FIFO. A dedicated typed handoff that must survive acknowledgement is
  bounded to its protocol maximum until consumption.
  Delayed steering correlation uses that rule through its own Started-to-Completed tracker: one
  selected lifecycle and at most the two ordered checked results may exist, so both matching
  lifecycle events can precede the exact response. Ordered polling pauses while either result
  awaits its connection-owned consumer. Consuming checked `Completed` does not reset the tracker:
  it remains terminal until the later exact delivery-disposition owner explicitly releases it.
  Target queues retain only feature-owned approvals and dynamic-tool calls. The router has no
  `RoutedLiveEvent`, recursive retained-byte estimate, queued-byte counter, or independent byte cap.
- The production seal consumer replays the exact route-bound observation through bounded point
  reads and resolves the current durable item lifecycle. A legally admissible observation stages
  the exact provider frame without a normalized in-memory item and admits its source, canonical,
  lifecycle, activity, and projection effects. A structurally valid exact-route observation that
  conflicts with that lifecycle instead publishes the storage-owned compact
  provider-observation issue event under the same permit; it does not close the target, overwrite
  the canonical item, or retain a caller-local mismatch flag. Either result is durable before seal
  acknowledgement. Legacy materialized item lifecycle and delta controls are rejected from generic
  routing and cannot act as a second publisher.
- Exact account snapshots and bounded connection-lifecycle facts publish through one shared
  projection keyed by runtime id and managed-process generation. Every live connection to that
  process observes the same latest account fact with its exact source connection generation;
  per-turn consumers cannot steal or consume those shared facts.
- The service issues non-cloneable per-thread subscription leases from one process-wide generation
  allocator and retains only live lease entries. A recovered binding is usable or forkable only
  through one exact currently admitted lease. Its injection generation remains establishment
  provenance and its managed-process component remains an authority boundary. Ordinary lease or
  process loss does not authorize recovered resume. A second connection's registry observation is
  never equivalent authority.
- Connection-wide retirement and the retired-check-plus-registry-acquisition section linearize
  through one bounded in-memory gate. An acquisition that linearizes first is removed by retirement;
  an acquisition that linearizes later is rejected. The gate covers no backend or storage work, and
  a retired connection cannot leave or insert a loaded-thread entry.
- Consuming release revokes the local lease before connection-scoped unsubscribe. Every response
  or ambiguous failure remains locally non-authorizing, and connection/process loss revokes all
  matching live leases in a bounded walk without waiting for delayed CAS unload notification.
- Dropping a lease never performs backend I/O. It revokes the exact local token immediately; if it
  was the last token, the connection is retired so forgotten cleanup cannot block GPUI or leave
  reusable authority. The explicit consuming release is the non-GPUI path that performs bounded
  unsubscribe.
- Native continuation, resume, inclusive fork, and fresh lineage are selected from bounded Syndic
  proofs before recovery. A nonempty earlier prefix creates a distinct fork through its exact
  terminal CAS turn; an empty prefix starts fresh. The coordinator never dispatches in-place
  rollback. One-time injection starts its own fresh empty thread inside the coordinator, so a
  caller cannot supply a merely fresh fork result as an injection target.
- Recovery preparation retains only Syndic's compact revision-bound preflight proof. After the app
  establishes one fixed 65,536-byte recovery page and both capacity-one broker rings before it
  creates the exact fresh idle target. It then opens the corresponding opaque Syndic cursor and
  transfers that sole page through the storage and connection workers one request at a time. The app
  never assembles a recovery item sequence or backend batch.
- Every brokered page uses the same fixed page handle filled by Syndic and moved into the backend
  source event without another text copy. It repeats one domain-separated source identity derived
  from the exact home, thread, selected path, represented prefix, source revision, totals, and
  sequence digest. Cancellation, broker unavailability, revision drift, dependency read failure,
  and invalid durable source remain distinct typed failures; both sides always receive a terminal
  result and no home or router lock is held across the rendezvous. Content-free weak diagnostics
  expose page and ring capacity/current/high-water facts, logical progress, waits, and complete
  release without retaining page contents.
- The coordinator retires a native source only from an authoritative source-loss verdict. A
  source-preserving or unclassified resume/fork rejection retains the binding and enters bounded,
  coalesced retry against the same source; retry exhaustion returns an exact revision-bound
  recovery-decision capability and never silently selects injection. Explicit retry keeps the
  source, while explicit recover bypasses it for one fresh injected target projection. Recovery
  retires the source only when it is the target thread's own binding; another thread's fork source
  remains unchanged. After target retirement, bounded replanning must observe precisely the single
  binding revision produced by that retirement; a concurrent later mutation makes the consumed
  decision stale rather than widening its authority.
- Durable Syndic projection requests accept persistent CAS thread options only. Ephemeral
  maintenance work uses a separate backend workflow and never receives a durable Syndic binding.
- A native resume publishes establishment provenance for the exact durable source prefix it
  loaded. A later selected-path revision with the same stable tail and digest belongs to the new
  binding's represented-prefix fact, not to a fabricated newer CAS establishment event.
- Cancellation is checked at safe boundaries before remote dispatch. A synchronous request that
  has already been sent is drained and classified; its result is durably published or abandoned
  before cancellation can become caller-visible.
- A remote projection becomes caller-usable only after exact durable binding publication. Every
  failed or ambiguous fresh target is forgotten from the loaded registry and retained as stale
  provenance when its identity can still be committed; Beryl neither deletes it nor retries
  injection against it.
- Recovered-injection completion authority uses the app's local Unix wall-clock observation taken
  immediately after exact CAS success. A clock before the epoch or outside the durable range fails
  explicitly and abandons the target; the earlier request timestamp is never relabeled as
  injection completion.
- A surfaced publication ambiguity never returns a projection capability. If same-home
  verification later reveals a whole recovered binding whose process-local loaded generation was
  forgotten, the coordinator establishes another fresh projection from a complete eligible Syndic
  prefix. An ambiguous or uncommitted injection target remains non-authorizing provenance and is
  never promoted by resume or reinjection.
- Until the branch-discussion checkpoint supplies its separate selected-context projection proof,
  this coordinator rejects a context-bearing pending turn explicitly. It never establishes a CAS
  projection that omits the required selected assistant passage or conflates it with recovered
  history.
- App-local active-turn presentation keeps only the bounded exact identities and state needed for controls and routing. It is not selected transcript history or CAS-binding authority.
- Active-turn orchestration retries only work proven not dispatched or exactly rejected. A possibly
  dispatched start or steering request with no authoritative response is durably converged through
  the CAS-live system as incomplete or delivery-unknown, retires the unprovable projection, and is
  never replayed automatically. Relaunch and fresh projection recovery restore readiness without
  starting a replacement model turn.
- Cancelling an exact not-started activation does not imply that its loaded projection survived.
  The orchestration result distinguishes a retained exact projection from a still-pending turn
  whose connection authority must be reacquired. Matching start evidence followed by target loss
  remains completion-unknown and cannot be downgraded to an identity conflict or retry proof.
- Ordinary execution prepares the pending turn's one sealed user item as a count/digest-bound
  replayable descriptor source backed by bounded Syndic reads. Marker-free content yields one
  maximal logical text descriptor. Marker-bearing content merge-joins paged markers with sealed
  asset-reference authority and yields one verified `localImage` descriptor at a time, while
  repeated labels remain text-only references. A preparation failure before backend dispatch
  returns the same loaded projection with its exact pending turn, binding, input gate, and healthy
  home generation unchanged.
- Accepted-input replay uses the same final source engine but derives its identity, content, sealed
  reference proof, and `AcceptedInput` asset owner from one exact immutable Syndic record. Its
  compact factory rechecks the record, owner head, content, cancellation, and Beryl-home generation
  and creates fresh non-cloneable cursors with equal count/digest headers and no shared pass state.
  The pure correlation codec accepts only `beryl.accepted-input.v1:` plus the input identity's 32
  lowercase hexadecimal digits. Ordered delayed-lifecycle ingress now resolves that identity
  through one fixed-work delivering-route read, acquires a strict already-active router permit,
  compares the provider lifecycle against one fresh replay pass, rechecks the exact durable route,
  and publishes each result into one bounded two-result ordered app handoff at the permit's
  target-stable linearization point. Successful checking does not change accepted-input delivery
  state. A correlation,
  lifecycle, source, or route invariant failure retires the unprovable production target and lets
  the existing loss publisher perform its atomic active-abandonment disposition; the ingester does
  not keep a dead projection current or write a separate accepted-route outcome. The permit cannot
  activate a pending turn or publish a provider/source fact.
- Outbound active-steering delivery is one synchronous non-GPUI
  `ProjectionConnectionService` operation beneath one process-owned level-triggered accepted-input
  scheduler. Its steering scan reads bounded revision-bound durable ready-source pages and supplies
  one exact live target and accepted input identity per acquired steering-critical permit.
- Accepted-input publication crosses the public app boundary only as an opaque prepared admission
  consumed by its owning `ProjectionConnectionService`; callers cannot extract and execute the raw
  cross-domain command. The immutable accepted-input record carries the complete original
  admission intent, so exact reconciliation compares that durable receipt plus permanent order and
  route-leaf identity and remains valid after later route or gate descendants. Exact accepted
  publication then wakes the scheduler. An unresolved read or a successful command without the
  exact receipt fails the service closed; correctness does not depend on a global app publication
  mutex spanning every route publisher.
- Accepted-ready and next-turn publication, exact target readiness, worker and same-thread-flight
  release, durable terminal or projection-loss publication, and runtime/session readiness coalesce
  scheduler wake state. Steering and next-turn work share this signal and the worker pool's one
  release waiter while retaining separate compact scan cursors and eligibility facts. Durable
  routes, not process-local input ids or waiters, own the backlog. A pass that cannot reserve the
  applicable worker role stops without durable mutation; a later pass resumes from durable
  authority without accumulating resident delivery work.
- Scheduled-worker completion wakes only bounded join and disposition bookkeeping. It opens a new
  next-turn pass when the completed worker reports typed immediate continuation; a parked
  authority-unavailable, ambiguous-`Prior`, cancellation, or retirement-lost disposition does not
  self-retry or transfer its released permit through the recovered-pending lane and back into
  another accepted-next attempt. Otherwise only fresh durable, execution, flight, cancellation,
  recovery, or an actually awaited worker-capacity release opens the next bounded pass.
- The coalesced signal retains lane-specific wake facts. An ordinary-only wake cannot mount a
  speculative steering scan, and the provisional steering-reserve permit used before source
  discovery cannot satisfy scheduled-ordinary demand when that empty scan releases it. Only a
  connection permit or a steering permit committed to a spawned worker is external scheduled
  capacity authority. One physical release that satisfies both lane waiters publishes both facts
  atomically.
- Active accepted-next workers are indexed by Syndic thread. A redundant scan that reaches its
  own worker's thread stops without arming the same-thread flight waiter, so mandatory lease
  release cannot turn a parked disposition into an indirect self-retry. The worker pool's single
  coalesced release waiter retains separate steering and scheduled-ordinary demand facts. Any
  permit release may satisfy steering demand, but the scheduled worker's own permit release cannot
  satisfy scheduled-ordinary demand; only an external connection release, a steering permit
  committed to an actual worker, or that worker's typed continuation disposition can reopen the
  capacity path.
- Replacement of the healthy Beryl-home generation invalidates the whole old-generation service,
  including scheduler domain handles, connections, flights, and scheduled-execution leases. That
  scheduler fails closed after joining its workers and leaves unpromoted durable work for explicit
  new-generation recovery; readiness cannot turn the old service into new-generation authority.
- Service construction owns one exclusive synchronous restart-recovery interval before the service
  becomes available for connection or accepted-input admission. It walks bounded input-gate pages
  directly from Syndic and retains only the current page, cursor, and one stabilized case. This
  work runs off the GPUI thread. No scheduler lane starts and no new thread or input can enter the
  opened home through the service until recovery hands off.
- The app asks Syndic to classify each source row rather than assembling storage invariants from
  unrelated point reads. An active case supplies the exact durable activation, CAS thread, loaded
  generation, and route authority needed by the existing generic active-abandonment publication.
  A post-abandonment case supplies only the authenticated source-less terminal target. A safe
  pending case is left durable for scheduled ordinary execution. A compacting case supplies only
  its closed storage-owned admitted, possible-dispatch, exact-terminal, or paired-stopping recovery
  authority, and any inconsistent case closes service construction.
- Restart loss convergence reuses the same reconciled generic abandonment and source-less
  incomplete publication as live target loss, but requires no process-local router or loaded
  session authority. Terminal publication atomically leaves
  `FinalizingHistory(turn)` as the durable non-idle input-gate source. Recovery runs the ordinary
  bounded terminal-history convergence over already captured evidence and issues its exact gate-
  completion command before scheduler handoff; this may finalize completed items but cannot
  synthesize a missing CAS item or make an unresolved frontier eligible. A restart that finds an
  existing finalizing gate resumes convergence without publishing another terminal event. The
  active binding itself is possible-dispatch proof. Absence of an active CAS-turn record, response,
  lifecycle, or process-local dispatch flag never makes an activated `turn/start` replayable.
- When startup convergence reaches the end of the gate scan with no active, post-abandonment, or
  finalizing-history obligation left unsettled, one one-way handoff opens steering,
  recovered-pending, and accepted-next scheduling together and emits the existing `Recovery` wake.
  Preexisting retryable steering receives exactly one recovery retry pass. The immutable
  construction-time next-turn block is removed. Once construction succeeds, content-free
  diagnostics expose the completed startup page, case, active-convergence, terminal-convergence,
  pending, and compaction-convergence counts plus the one-way handoff fact. A startup read,
  classification, or publication failure is returned as the typed construction error before a
  service or scheduler becomes observable.
- Startup consumes admitted compaction locally without dispatch and preserves its valid binding.
  A claimed operation with durable proven nondispatch finishes that same safe consumption without
  reusing the attempt. A claim with no disposition, acceptance, or completion unknown is possible
  dispatch: startup retires the old projection, publishes provider-operation authority loss,
  finalizes only captured evidence, and never repeats compact-start. Pinned rejection also retires
  the target despite proving no core admission. Marker-then-successful-terminal resumes bounded
  success finalization even without a recorded acknowledgement; every other terminal successor
  follows the storage-owned idle-proof or failure disposition.
- A compacting operation paired with stopping is recovered by the stop abandonment boundary, not
  by a second compaction classifier or interrupt. All compaction recovery paths preserve accepted
  input, publish readiness only after the exact fixed point, and omit unadmitted lifecycle
  continuation because that intent was process-local. When the consumed compaction witness already
  names a committed Beryl-origin continuation, startup hands that existing `PendingTurn` to
  ordinary recovered-pending scheduling; it does not omit, reconstruct, or duplicate the durable
  turn. Repeated restart remains idempotent through the consumed compaction witness.
- The scheduler distinguishes ordinary admitted passes from retry-eligibility passes. A durable
  `Retryable` leaf proves only safe non-dispatch; worker or connection-attempt release does not make
  it eligible again by itself. Only an explicit fresh cancellation lifecycle, recovery authority,
  or a future explicitly transient deadline may open one bounded retry pass. Ordinary
  accepted-ready and target-readiness wakes schedule admitted work without clearing a parked retry
  gate. The scheduler retains compact revision-bound cursors plus one process-global retry-pass
  state and no input id, payload, waiter, retry set, or per-input timer.
- A capped process-global retry deadline may be armed only by a closed app-owned disposition that
  explicitly proves both non-dispatch and transient failure. The current backend and replay error
  taxonomies expose no such disposition, so the mounted scheduler has no timer-driven production
  retry path. Adding one later requires an explicit typed classification and corresponding package
  and system authority; elapsed time or the broad `Retryable` lifecycle is never that proof.
- The accepted-input scheduler's ordered next-turn scan acquires one long-lived
  scheduled-ordinary permit before reading a revision-bound next-source page. It retains at most
  one candidate per permit and never builds a resident backlog. After global discovery names a
  thread, it acquires one same-thread flight and keeps that flight through candidate validation,
  execution admission, promotion reconciliation, projection establishment, and ordinary dispatch.
- At service construction, any already-present next-source record fences the next-turn lane for
  restart recovery. More generally, every scheduler lane remains behind the startup recovery fence
  until all stale active authority has converged. Same-process publication cannot consume newer
  work ahead of a durable predecessor; ordinary execution-readiness or worker-release wakes do not
  bypass the fence.
- After handoff, one revision-bound recovered-pending scan walks `PendingTurn` gate rows whose
  blocking turn is the current incomplete committed tail, with no selected active route,
  source-free pending state, and a non-active binding. It reserves the same scheduled-ordinary
  worker role, acquires the same per-thread flight, obtains the same exact service lease, and
  enters the existing pending-turn projection and `turn/start` path. It does not consume an
  accepted-next candidate or publish promotion. Its own completed or work-neutral mutation may
  invalidate the page revision after the physical cursor has advanced; the current sweep rebinds
  that floor and continues instead of retrying an already-attempted source.
- Pending-turn projection may inject the immediate predecessor only through storage's explicit
  authority-lost tail-context proof. The app sends that bounded exact durable sequence as context,
  waits for injection completion, and then starts only the already pending successor. It never
  relabels the predecessor complete, replays its prior `turn/start`, or substitutes a shorter
  prefix when storage rejects the context. Cancellation, exact connection retirement, or expected
  shutdown and home-generation drift parks the worker without claiming settlement. Every other
  projection-acquisition refusal, including structural recovery-proof rejection, fails the
  accepted-input scheduler closed before any successor backend request and is never reported as
  successful worker completion.
- The recovered-pending scan is opened by recovery and may be resumed by later execution,
  worker-capacity, or flight readiness. A parked provider-unavailable result advances past that
  thread for the current pass so unrelated recovered turns can progress, and only a fresh external
  readiness wake starts another complete scan. Its own worker completion or permit release cannot
  create a self-retry loop. Every owner capable of making a previously scanned thread newly
  eligible must publish `ExecutionReady`; that wake discards the current floor and starts from the
  beginning. A wake already consumed reset before the read, while one arriving during a stale read
  remains pending for the next scheduler loop, so cursor rebinding cannot hide newly eligible
  earlier work. An occupied worker or flight retains only a compact cursor and waits for its typed
  external release.
- The process shell exposes one synchronous no-backlog scheduled-ordinary execution provider.
  Before promotion it issues one non-cloneable lease bound to the exact healthy home generation,
  Syndic thread, admitted projection session and runtime generation, late-bound model, reasoning,
  developer-instruction, and turn-start policy, exact `AssetState`, feature-owned ordinary dynamic
  tool handlers, and the already reserved long-lived worker permit. Unavailable or changed
  authority declines the lease without retaining the candidate or mutating Syndic. The provider's
  provisional issued result becomes usable only after the service reconfirms its current healthy
  home generation, proves the supplied session is one still-attached connection in that service's
  registry with matching runtime and managed-process generation, and validates the typed
  `AssetState` handle against the owned home. Foreign, retired, detached, or stale authority is
  rejected before promotion or CAS work. Immediately before the durable command, the scheduler
  acquires a non-cloneable reservation from that exact connection while serialized with the
  service's accepting/connection registry and the connection-retirement gate. Whichever of
  reservation, shutdown, or retirement linearizes first controls the result. A winning reservation
  carries no payload and holds no mutex during storage. The scheduler revalidates exact home
  identity, healthy generation, and Syndic authority after acquisition, but does not let a later
  service-acceptance fence revoke the winning reservation. Later retirement fences new work but
  defers final invalidation and detachment until promotion reconciliation releases the reservation.
  It is released before projection establishment or CAS work. The provider's required shutdown boundary
  fences further issuance and releases retained session checkouts after scheduler workers join and
  before the service closes its home.
- For the earliest effective leased candidate the scheduler supplies fresh turn and item
  identities to one cross-domain home command that atomically publishes Syndic promotion and swaps
  the compact accepted-input asset owner to the submitted item. Marker-free input uses the matching
  validation-only asset assertion. A known command conflict is ordinary drift and releases the
  lease; an ambiguous commit or persistence result first reconciles the immutable promotion
  witness and exact successor identities across compatible monotonic descendants. A later accepted
  admission against the promoted pending gate does not erase that proof, and correctness does not
  depend on serializing accepted admission behind the same-thread flight or an app mutex.
  Cross-domain reconciliation brackets the selective Syndic result with matching observations of
  only the accepted-input and submitted-item asset heads. A relevant owner change requests another
  attempt; unrelated thread or domain commits do not.
- `Prior`, `Exact`, or `Collision` promotion reconciliation completes before any projection or CAS
  work. Only `Exact` enters the existing ordinary pending-turn execution path under the retained
  flight and lease; `Prior` stops without dispatch, while collision or unresolved state fails the
  scheduler closed. The input is not replayed through active steering, assigned another
  accepted-input correlation, or represented as an app-local queue. Worker exhaustion, occupied
  flight, execution-authority unavailability, page drift, or a competing user submission causes no
  promotion or CAS request.
- Accepted admission may advance the current thread revision after a projection observed the same
  selected tail and digest. Native reuse, pending-turn preflight, binding activation, activation
  reconciliation, and reopen validation all accept only that monotonic compatible descendant.
  Activation advances the represented-prefix source revision while retaining the exact original
  CAS establishment lineage; an older revision, changed tail, or changed digest remains a
  collision.
- The operation acquires one app-owned fixed worker permit before reading or claiming a ready
  route. If no permit is available, or the exact connection already owns a steering attempt, the
  operation returns typed transient saturation without mutating durable route state, spawning,
  buffering, or retaining delivery work. It then acquires one connection-wide, non-cloneable
  steering attempt before the Ready-to-Delivering transition, reopens that exact delivering route,
  and keeps the attempt through terminal durable disposition.
- Claim, retry, completion, structured rejection, and active-abandonment requests carry stable
  input/leaf, target, binding, and disposition identity rather than treating an earlier shared gate
  or route-head revision as exclusive delivery ownership. The serialized Syndic mutation validates
  that identity against current compatible authority and records the actual gate and route head it
  consumes. Admission of another input to the same steering generation is therefore a legal
  descendant before claim or while a request is in flight; it cannot fail the scheduler, erase a
  known provider result, or fabricate delivery-unknown. Reconciliation consumes the immutable
  successor witness against monotonic compatible descendants rather than waiting for two identical
  shared-authority snapshots.
- A claimed delivery arms the exact checked lifecycle owner before replay preparation,
  creates one fresh accepted-input replay pass and canonical V1 correlation, and dispatches only
  through the specialized streamed `turn/steer` request authorized for that exact active target.
  Its bounded tracker can retain both ordered lifecycle results when they precede the response.
  Exact success waits for matching Started then Completed lifecycle proof. Durable completion,
  retry, or structured steering rejection commits before the lifecycle owner and steering attempt
  are released.
- Proven pre-dispatch failure first returns the exact route to safe retryable storage while the
  target remains current, then supplies one closed scheduler disposition. Cancellation parks until
  an explicit fresh cancellation-lifecycle or recovery wake. Every other current target-current
  source, authority, validation, serialization, lifecycle-arm, or command-authorization failure
  first commits that safe durable disposition and then fails the exact target closed through atomic
  active abandonment, so the proven-undispatched fragment becomes projection-lost next-turn work
  rather than delivery-unknown. Failure of that convergence fails the scheduler service closed.
  These paths never arm repeated attempts. A connection-invalidating pre-dispatch failure uses the
  same target-loss boundary rather than becoming scheduler-visible retry work. The connection
  driver seals the exact armed no-lifecycle branch from its proven-nondispatch result before it
  requests broker cancellation, so the delivery owner can consume that proof after connection
  closure without reclassifying it as `BrokerClosed`. A closed
  active-turn-not-steerable verdict records structured
  rejection while preserving the active target. Any other exact rejection uses named
  exact-rejection abandonment, while possible dispatch, lifecycle failure, route loss, or
  unprovable disposition uses generic abandonment and delivery-unknown authority. Target loss
  fences new command authorization and atomically receives the in-flight attempt only while no
  exact delivery disposition has committed. Once exact success, retry, or structured rejection is
  durable, the delivery owner releases and finishes the attempt without replacing that disposition
  from later mutable target state. The atomic finish reports any deferred loss, which then uses
  ordinary loss convergence against the new durable state. Loss publication uses the same
  storage-owned current-authority rule, so compatible sibling admission between target-loss proof
  and abandonment is included atomically rather than surfaced as a publication collision.
  A disposition that removed the last live steering route may preserve an independently proven
  terminal source outcome. Exact Retry instead retains live steering, so deferred loss converges
  projection loss and cannot fabricate a terminal receipt. Steering cleanup never overwrites a
  terminal outcome the durable gate could actually publish.
- Active-steering loss ends after it atomically publishes or observes target loss and invalidates
  the shared live target. It does not run terminal-history convergence: the ordinary capture owner
  wakes through that invalidation and performs convergence while still holding the same-thread
  flight. If that owner is lost, startup resumes the durable `FinalizingHistory` obligation.
- After successful preparation, execution activates the exact durable binding and consumes the
  loaded projection into one non-cloneable live target. A routed matching start event is itself
  identity proof; a response-derived turn id must confirm the target before live activation is
  claimed.
- The app's capacity-one text broker exposes one compact source id and immutable source proof, then
  validates every pass, descriptor, and absolute page request against that retained authority. Its
  single maximal text descriptor maps source-relative coordinates directly to the sealed Syndic
  content range. A caller-chosen proof alone never selects a backing range.
- The connection ingester is the sole live-path publisher of source-producing compact controls.
  Checked submitted-user start and completion never enter the target FIFO, and status-only
  `turn/completed` exposes only one bounded proven-terminal outcome after durable publication. The
  ordinary caller holds control and handoff state but no source sequence, turn-state revision,
  input-gate revision, or publication timestamp. After abnormal target loss it converges from an
  exact durable frontier only after any in-flight publication permit resolves.
- Each activation or source permit checks its target-captured home generation and reacquires the
  current typed Syndic handle before publication. Connection-admission generation and storage
  handles are not publication authority after same-home recovery publication and executable
  projection reconstruction.
- Live capture exhaustively maps the closed normalized item union into borrowed typed provider views;
  it never reduces a public item to generic text or a fieldless activity marker. It streams one
  bounded provider-field fragment at a time into Syndic's item-owned `ProviderItemV1` content,
  answers dynamic tools through the exact target, and preserves every published prefix on loss.
- Exhaustive capture begins after backend ingress exclusions. A standalone image-generation view
  contains `savedPath` and non-binary lifecycle metadata but never the upstream base64 `result`;
  app orchestration cannot persist, inspect, decode, or recover from that discarded field.
- The submitted `UserMessage` lifecycle is correlated against the exact already durable input rather
  than duplicated; its provider metadata and checked content reference remain typed. Completion-only
  variants such as pinned `SubAgentActivity` are admitted only through their explicit typed path and
  retain their complete public payload through bounded durable chunks, never a whole app-owned
  value. Unknown, malformed, or unsupported history-relevant items
  preserve the exact provider terminal fact but prevent a history-complete publication through a
  typed incomplete reason.
- MCP and dynamic-tool structured values cross the app boundary through the closed typed storage
  algebra, not raw JSON, opaque bytes, or a generic catch-all. Provider completion is reconciled
  field-by-field against bounded durable reads. Non-narrative final fields remain exact. Completed
  `AgentMessage` and `Plan` narrative must equal the prior live append sequence byte-for-byte; equal
  ranges may be reused, while disagreement records typed history incompleteness and never appends or
  selects replacement text.
- Exact-route duplicate start, repeated or reversed completion, item-kind conflict, and other closed
  lifecycle conflicts are history-producing observations. The ordered consumer asks storage to
  derive and publish a compact issue reference from the sealed observation; it neither abandons the
  evidence nor converts the conflict into a fatal routing failure. Missing, malformed, mismatched,
  cancelled, or retired routing remains fatal or unpublished according to the routing contract and
  cannot create an issue event.
- Ordered terminal capture publishes the exact normalized provider outcome and the independent typed
  history-incomplete reason together before acknowledgement, then exposes the proven durable result
  to ordinary execution. It never converts provider `Complete` into local `Incomplete` merely
  because canonical history remains unresolved. A durable provider-observation issue selects
  `CompletionMismatch` for normal provider terminal publication.
- Exact CAS 0.146.0 interrupted terminal capture selects `ForcedAbortOrderingUnproven` unless
  retained release-scoped regression proof establishes a
  no-later-item barrier. A later same-target source event fails that connection after the terminal
  cut and cannot reopen source admission or revise the durable terminal.
- Every item delta carries its expected normalized kind. Capture validates the exact CAS item and
  kind plus its closed field identity, element ordinal, and protocol index before durable mutation,
  so an agent, plan, command, file, reasoning, or tool delta cannot append to another variant or
  field.
- Proven terminal handoff first closes canonical source admission, advances binding authority, and
  atomically leaves the exact input gate in `FinalizingHistory(turn)`. While retaining the same-
  thread flight, it resumably builds and finalizes each immediately eligible visible item
  projection and the selected transcript from already durable canonical history. An exact final
  storage command proves the durable convergence fixed point and only then releases the current
  compatible finalizing gate to idle. Queued admissions may advance draft, broad-thread, summary,
  and gate accounting without invalidating a transcript build for the unchanged selected tail and
  digest; completion and ambiguous-result reconciliation preserve those admitted descendants.
  Worker completion cannot expose accepted-next work before that release.
- Live capture retains only one bounded pending provider-field fragment. It resolves item identity,
  lifecycle, kind, typed frame frontier, and exact logical-text prefix from Syndic's
  record-stabilized CAS-item/canonical-item/content reads; active-item and completed-item maps are
  forbidden. Pinned `turn/completed` is a status-only fence, not a full-item snapshot: capture flushes
  the pending fragment and audits every already admitted durable item through fixed-size cursor pages
  before allowing `TurnEnded`. Every completed item must name an exact sealed final typed frame whose
  kind, structure, field ranges, and content frontier are complete. Capture neither invents
  terminal item backfill nor treats idle state as completion proof.
- Foreground stream loss retires the exact target and converges the retained prefix as incomplete;
  app orchestration never reconnects or resumes a replacement connection into that same capture.
  CAS history reads and a later terminal notification are not notification replay or repair. Loss
  keeps its primary terminal reason while storage retains any earlier provider-observation issue as
  a separate durable fact.
- A completion/live narrative mismatch likewise makes the current capture permanently
  history-incomplete. The app continues routing the already active turn until terminal convergence
  or authority loss and starts no later turn on that loaded session. At terminal convergence it
  consumes all old execution leases but holds the old remote subscription as a non-execution anchor,
  opens a fresh connection to the same managed process, and installs one non-execution replacement
  reservation before resuming the same CAS thread. Connection-scoped `thread/closed` revokes the
  anchor, reservation, or transferred replacement lease even when no live turn target exists.
  Transfer consumes the exact anchor and reservation atomically under both connections' retirement
  gates; exact same id and idle state establish a new loaded-thread generation before the old anchor
  is released. A reserved replacement connection cannot admit unrelated loaded-thread work. If the
  process or old anchor is lost first, the thread becomes unavailable; if only the fresh replacement
  or its reservation is lost, the old anchor remains eligible for Retry through another fresh
  connection. The app neither cold-resumes recovered lineage nor injects an incomplete Syndic
  prefix. The separate authority-lost tail-context path for an already pending successor is not
  recovered-lineage rotation: it establishes a fresh projection from storage-proven exact context,
  leaves the predecessor incomplete, and starts no replacement for that predecessor.
- Ordinary-turn publication and convergence use writer-admitted single-domain commands carrying
  exact logical record fences. Preflight and convergence reread only their exact thread/item/build
  anchors, so activity on another Syndic thread cannot conflict or starve the current operation.
- Ordinary submitted-input execution never assembles the sealed composer content into one app
  `String` or moves it through the connection command queue. An app-owned bounded broker serves
  exact immutable Syndic text pages to the sole connection worker while keeping HomeStore,
  cancellation, and durable-revision authority outside the backend package.
- Complete input preparation borrows the exact loaded projection and observes one explicit
  projection-cancellation token before activation. A pre-activation preparation failure returns
  that same still-live projection with its typed error; it never drops the last lease and then
  reacquires or reconstructs native context. The projection is consumed only after preparation has
  succeeded and execution is ready to create the live target.
- A returned projection remains directly retryable only while its Beryl-home generation stays
  healthy and current. If a structural preparation error closes that generation, this package
  keeps every worker-bounded old wrapper non-executable in the cut-identified recovery quarantine;
  it does not choose one candidate. A wrapper remains unusable until its stable connection has
  completed service-epoch adoption.
- After adoption, the adopted unpublished service's owning ledger exposes one explicit consuming
  reauthentication for each quarantined candidate. It performs no backend request and changes no
  durable record. It rereads and stabilizes the exact pending ordinary turn and full valid binding,
  confirms the same stable connection and registry lease before and after those reads, reconfirms
  the recovered home generation, and only then moves the capability into dormant accepted
  provenance.
- Accepted provenance is non-executable and preserves only the recovered home/service identity,
  exact CAS connection, managed-process and loaded-thread generation, CAS thread, Syndic owner,
  execution binding, binding revision, lineage proof, lease token, durable witness, and replacement
  hold. Executable projection reconstruction occurs only after final recovery publication. The next
  execution then performs complete preparation again; no asset, sidecar, path, marker, source-page,
  or cancellation evidence crosses the home-generation boundary. Candidate-local durable mismatch,
  unstable revision, or token revocation returns the original capability for retry or explicit
  cleanup. Recovered-home or service drift, stable-core retirement or mismatch, service-membership
  loss, or unavailable shared registry authentication terminalizes the whole adoption instead of
  becoming a candidate retry.
- The adopted app-service authority keeps one exact ledger for the complete candidate set. Every
  rejection returns with its replacement recovery hold to that ledger; retry remains available only
  before service publication and blocks publication while outstanding. The ledger seals only after
  every original candidate is either accepted into the dormant recovered-candidate inventory with
  its exact replacement hold or explicitly disposed. Disposition confirms or performs local
  registry revocation, consumes the lease and quarantine capability, and returns the hold to the
  replacement pool without backend or durable work. Sealing atomically transfers every exact
  connection-quarantine owner, including a zero-candidate connection's owner, into private
  candidate-set-converged retirement retention while authenticating the complete accepted-token
  set. That retention exposes no connection operation and is released only by final publication or
  converged-authority disposal. Only the resulting non-cloneable candidate-set-converged authority
  can enter service publication; no rejection, retry, disposition, rejected lease, rejected
  registry token, connection-owner capability, or rejected hold may cross that boundary. Each
  accepted entry's stable lease and registry token remain in the sealed dormant inventory.
- A terminal transition demotes every accepted, rejected, or unprocessed entry that was not already
  disposed to one service-wide terminal reason. One non-retryable whole-attempt owner then retains
  all candidates, connection owners, replacement holds, adopted-service state, and old/new
  attachments; retry, per-candidate disposition, seal, and publication are unavailable. Explicit
  whole-attempt disposition settles every candidate token and replacement hold before releasing
  connection owners and explicitly disposing the inert unpublished service without closing the
  retained home. Zero-candidate sets use the same owner and ordering; implicit drop stays bounded
  and nonblocking.
- Input preparation retains only the compact header, source identity and proof, and cursor/pass
  state needed to replay the current maximal logical text or verified local-image descriptor.
  Physical Syndic chunking is an implementation detail of bounded range reads and does not create
  proportional descriptors or app-owned whole-input storage. The count/digest-bound source moves
  once into the backend and replays independently across the request plus both lifecycle echoes.
- The marker/asset merge join holds only the current marker and sealed reference, one verified
  sidecar, and one Host or WSL runtime path. It releases those authorities before cursor advance
  and never retains a materialized image list, label set, source map, path list, or handle
  collection.
- The same broker boundary services the backend's incremental live `UserMessage` verifier. Exact
  started and completed echoes are compared against the submitted Syndic text without retaining a
  second whole input. A source read, segmentation, lifecycle, or byte mismatch fails the exact
  target rather than flattening, regrouping, or accepting approximate evidence.
- One broker service loop belongs to one serialized start command. While the connection worker
  writes that request and verifies its two synchronous user-message echoes, the caller retains the
  borrowed home store and answers at most one bounded absolute-page request at a time. Every
  authorization, cancellation, source-failure, transport-failure, and normal-result path wakes
  both sides with a typed terminal outcome; neither side abandons a page rendezvous or holds a
  home, store, or router lock while waiting.
- Stop and hard stop follow the durable exact protocol above. Compaction, steering, queue delivery,
  replacement edit, and retry commands likewise name exact targets and flow through their owning
  operation gate. This crate never guesses a process, turn, parent, or lineage scope.
- Automatic thread-title maintenance uses a bounded background backend session and commits only a
  validated one-way Syndic thread-attribute mutation with the exact eligibility witness. It never
  occupies a selected foreground stream or exposes maintenance CAS threads as user threads.

## Branch Discussion And Durable Jobs

- `Discuss in new branch` consumes exact selection provenance from the transcript host and calls the atomic Syndic/home branch-creation command before activating the result.
- Branch creation performs no CAS request; its context-bearing draft envelope and parent-thread
  binding come from durable target-system records.
- Selected-discussion activation supplies the transcript host with the immutable context-owner descriptor and branch insertion parent; transcript residency reads the exact envelope and publishes the synthetic context group without creating a turn.
- Discussion-scoped resolution tool calls cross a bounded request/response bridge to the branch-handoff coordinator. Turn workers receive structured outcomes and never hold direct GPUI, store, repository, or main-window handles.
- Resolution admission, queued-input deferral, composer gating, parent ordering, retry, idempotency, recovery, and archive publication are projections of the durable handoff system, not app-local flags.
- One discussion revision publishes the composer-adjacent discussion-status strip and composer writable or inert state together so presentation cannot disagree about whether input is accepted.
- App-local presentation may invoke retry only for an already admitted exact job and may not synthesize a resolve, merge, archive, parent, or replacement destination.

## Image Assets

- Image paste, preview, clipboard reconstruction, label allocation, submission preparation, transcript resolution, and unavailable-state presentation consume the image feature and system boundaries.
- The app package owns GPUI editor atoms, marker menus, preview-window state, and correlation of asynchronous preparation results to exact draft and asset revisions.
- Content addressing, bytes, durable references, Host/WSL projection, and cleanup policy remain outside this crate's presentation state.
- Image preparation and decode run away from the GPUI thread under deterministic item and byte budgets. Stale completions cannot update another draft, marker, preview, runtime, or transcript row.
- Generated-output admission reads only the exact normalized `savedPath` through its retained
  runtime/process/session provenance. It never asks the backend for the discarded base64 result or
  reconstructs it from CAS history. Missing, unreadable, changed, or unsupported output leaves the
  generated-media resource unavailable or pending according to the image-asset system contract.

## Transcript Host And Rendering

- Main-window transcript code interacts with one transcript host through the shell boundary defined by `doc/systems/transcript-presentation/shell-boundary.md`.
- The app package does not call `syndic-storage` directly, retain full-history clones, derive transcript narrative from CAS reads, or let renderer callbacks initiate authoritative history transitions.
- Activation seeds, provider responses, live events, renderer demand, selection, quote, context-menu targets, media commands, and scroll commands retain stable Syndic provenance and presentation revisions.
- The exact routed live event path supplies arrived bounded fragments of transcript-visible text
  deltas to the transcript host independently from durable Syndic coalescing. The host publishes all
  available fragments to the next GUI frame in parent-delta order without a character-reveal timer
  and relinquishes its transient suffix only after exact durable-prefix reconciliation; it never
  retains a second whole response.
- Anchor-relative chunk loading, resident-data release, row virtualization, huge-turn streaming, media admission, nested scrolling, selection pins, and measurement caches remain bounded by the transcript-presentation contracts.
- Render, prepaint, deferred-frame, scrollbar, and status paths consume prepared snapshots and facts; they do not parse full Markdown, scan resident history, decode media, query storage, or call the backend.

## Activity, Status, Notices, And Notifications

- Activity is a revision-bound paged presentation query over already admitted Syndic provider
  lifecycle records plus bounded GUI-derived facts. Status remains a statically bounded projection
  over final compact controls, Beryl metadata, and transcript-host facts defined by its feature
  contract. Model discovery follows backend cursor pages and never aggregates all model pages into
  an app cache.
- Activity metadata resolution receives only the bounded selected facts from a metadata-only
  `thread/read` response. The app never receives a backend `ThreadSummary`, history collection,
  source tree, preview, cwd, or backend thread name for row decoration.
- App projections accept only facts from their final compact or paged owner. No mounted or detached
  shell adapter consumes a backend `ToolActivityEvent` aggregate or consumes or reconstructs the
  removed `TurnStreamEvent`, materialized `ThreadItem`, `ThreadSummary`, or collection-backed
  `UserInput` graph.
- Each status rate-limit query supplies its bounded exact active-model interest to the backend
  scanner and retains only the one bounded matching bucket or an ambiguity/unavailable fact. The
  process projection never owns a complete backend bucket map or unmatched bucket collection.
- Activity-panel presentation is never durable conversation authority. Exact supported command or
  file output may separately enter Syndic as provider-sourced canonical operational history, but
  it remains outside transcript narrative and is not reconstructed from presentation rows.
- Status chrome does not estimate token usage, context limits, rate limits, turn counts, or active targets. Unknown exact facts remain unknown.
- Surface notices are bounded window-local queues. Dismissal changes presentation only and cannot acknowledge, retry, repair, or mutate the underlying failure by itself.
- End-turn sounds and operator-attention signals consume exact foreground-turn and lifecycle eligibility; maintenance, metadata, restore, catalog, compaction-continuation, and background work do not masquerade as ordinary user-turn completion.

## Settings And Themes

- The settings window consumes the app-neutral settings-window package through Beryl adapters and mounts the sections defined by the settings and theming feature docs.
- Scalar settings drafts validate and commit through typed Beryl-home settings commands. Unapplied drafts remain window-local and never mutate active state.
- Theme documents, role resolution, preview, install, update, activation, and editor presentation use
  the theming feature boundary and theme repository. The app holds only bounded manifest pages and
  resolved finite-schema theme state; source validation and mutation stream through range-backed
  repository services. The app package owns only GPUI integration, cache invalidation, and bounded
  UI bridges.
- Theme and settings operations cannot reach Syndic thread properties or history, Beryl runtime/root
  state outside their declared setting, backend-owned Codex configuration, or unrelated Beryl-home
  domains.
- No graph, checklist, workspace-member, or removed-surface settings adapter remains in this package.

## Dynamic Tools And Lifecycle Yield

- Every persistent Beryl conversation lineage starts with one canonical versioned,
  deterministically ordered conversation-tool registry. Native continuation, resume, and fork are
  eligible only when their binding proves that same registry profile; Beryl never silently varies
  tool definitions by thread kind or later reconstructs an otherwise native lineage merely to add
  a feature tool.
- Registry membership advertises stable capabilities and does not grant mutation authority.
  The generic broker authorizes the exact CAS thread, turn, call, target registration, and loaded
  generation before argument ingress. Feature-owned handlers receive only the validated Syndic
  thread and turn plus their non-cloneable typed request, then authorize feature state and durable
  revisions; a registered tool invoked outside its feature scope rejects without mutation.
- The canonical registry binds each installed tool to one feature-owned incremental argument
  admission contract. After the backend validates the pinned compact envelope, the selected sink
  consumes argument structure and admitted scalar fragments, enforces the tool's product schema
  before allocating bounded typed fields, and seals one non-cloneable feature request.
- Generic connection routing, command queues, target routing, and outstanding-response state retain
  only compact identities and one shared response authority. They never clone a complete tool
  request, retain `serde_json::Value`, or validate `maxLength` and `maxItems` only after allocation.
- Unknown tools, reordered pinned discriminants, schema violations, cancellation, target loss, and
  handler failure produce one exact response or connection failure according to dispatch state.
  They do not create a raw JSON spool, generic argument tree, or second response owner.
- Specifically, a valid routed envelope seals an unknown-tool or product-schema failure as one
  bounded typed rejection for the exact target to answer once. A request observed before registry
  binding, or one with reordered, duplicate, missing, or late-mutated envelope identity, retires
  the connection before any feature request exists.
- Tool workers never retain `ShellView`, GPUI handles, window controllers, raw repositories, or storage mutation handles.
- The mounted lifecycle-yield and branch-resolution tools each keep their feature-owned schema and
  authorization. Future tool families, including any later theme or settings tools, must do the
  same; a generic tool bridge does not combine their permissions.
- Tool responses report durable admission, rejection, deferral, conflict, or bounded failure accurately; request acceptance is not turn completion or downstream job completion.
- Secret-like values and unbounded content are rejected or redacted before diagnostic retention.

## Diagnostics And Isolated Child Control

- Supervisor diagnostics expose bounded content-free process, memory, renderer, retained-state, settings-window, transcript-frame, media, and catalog summaries through explicit snapshot builders.
- Diagnostic reads never require loading nonresident conversation history, rendering hidden rows, scanning full catalogs on the GPUI thread, or querying CAS history.
- Isolated-child controls dispatch through the same exact Beryl-home window, thread, composer, stop, popup, scroll, and activation command paths used by direct interaction.
- Child control requests use exact child-known ids and expected state, reject ambiguity or stale targets, and never mutate private state behind those command paths.
- Diagnostic retention is bounded by record count and bytes and excludes transcript text, draft text, titles, root paths, search text, credentials, capability tokens, raw tool payloads, and other user content unless a separately authorized diagnostic contract explicitly requires a redacted value.

## Concurrency And Responsiveness

- The GPUI thread performs no blocking filesystem, Fjall, process, transport, protocol, history, Markdown, image, persistence, or model work.
- Background work is keyed by exact durable identity plus revision or cancellation generation. Completion applies only when every target fact still matches.
- Correctness-sensitive commands use revision checks and short typed commits, not long-lived locks across external work or await points.
- Same-process reacquisition locks its two connection-authority gates in ascending connection
  generation order, checks the old and replacement router lanes, and enters the global loaded-thread
  registry only after both sides admit the transfer. Backend and storage work never run under those
  gates. A connection driver records `thread/closed` in its router lane before acquiring authority,
  making close versus transfer a linearized race rather than a timing-dependent observation.
- Worker pools, channels, retry sets, title jobs, tool requests, backend notifications, resident
  activity pages, notices, catalog projections, transcript caches, media, and diagnostic rings have
  deterministic count and byte bounds.
- Foreground projection configuration fixes its nonzero pre-bind compact-control capacity and
  worker-pool limit before connection admission. Provider and recovery brokers allocate only their
  final fixed pages and rings; compact messages carry no allocation capability or per-message
  residency reservation.
- Submitted-input broker channels have fixed capacity and carry at most one bounded source request
  or page per direction. Closing, cancellation, store failure, target retirement, and transport
  failure wake both sides with a typed result; no caller or connection worker may remain blocked on
  an abandoned page request.
- Recovery-source broker channels likewise have capacity one in each direction and carry
  only compact control plus the sole fixed page. The channels and page are established before
  fresh-target creation and release completely on success, source failure, closure, connection
  retirement, or shutdown.
- Foreground turn streaming, visible transcript demand, draft persistence, and exact user commands take priority over speculative preload, title generation, metadata decoration, catalog maintenance, and diagnostic work.
- Rendering exhaustive thread/root/runtime, lineage, activity, and backend-model collections uses
  bounded resident query pages, stable fixed-stride virtualization, stable identities, focus and
  tooltip preservation, and content-free diagnostics.
- Quiet backend streams are not failures, and ordinary bounded request timeouts do not impose an inactivity timeout on live turns.

## Dependency Boundary

- This crate may depend on `gpui`, Beryl widget/adaptor packages, `beryl-model`, `beryl-home-store`, Beryl metadata-domain packages, `syndic-storage` only through higher-level Syndic service boundaries, `beryl-backend`, and transcript-host/provider packages as allowed by their own designs.
- Renderer-facing modules must not depend directly on `syndic-storage`, `beryl-home-store`, `beryl-backend`, Fjall, or raw app-server protocol types.
- Storage, backend, and provider workers return typed bounded results that contain no GPUI entities.
- Cycles are prevented by keeping pure identities in `beryl-model`, physical-store ownership in `beryl-home-store`, Syndic records in `syndic-storage`, backend protocol integration in `beryl-backend`, and shell composition in this crate.
