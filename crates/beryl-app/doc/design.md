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

- The [root design](../../../doc/design.md) and its linked feature, system, GUI, and package
  documents own behavior consumed by this crate.
- This crate is the Beryl package that depends on `gpui` and assembles controllers for the OS
  windows and slots declared by [GUI integration](../../../doc/gui/integration.md).
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

- Startup accepts the validated minimal-session result plus each restored window's validated selected-
  thread identity, current-draft identity, visible editor range, compact editor-frontier seed, and
  transcript seed. Controller construction retains only those bounded inputs and never preloads a
  complete draft. It reads only the durable current-draft selector and opens a fresh editor-
  candidate session; no unpublished session from a prior process is routine recovery authority.
- Later activation first flushes the prior candidate session, then accepts one revision-consistent
  bundle containing the target claim, durable draft selector, fresh candidate-session head,
  transcript seed, and window-local projection facts. It atomically replaces the controller's prior
  bundle or leaves the complete prior bundle and session frontier unchanged on cancellation,
  failure, conflict, or ambiguity.
- The [main-windows](../../../doc/features/main-windows/design.md) and
  [conversation-threads](../../../doc/features/conversation-threads/design.md) features own startup
  and activation behavior; [GUI integration](../../../doc/gui/integration.md) owns the window
  skeleton into which the resulting controllers render.

## Beryl-Home Integration

- This crate never opens Fjall or reads raw keyspaces. It receives typed domain handles and repository services from the Beryl-home boundary.
- This crate configures the home-store-owned typed nonzero minimum turn-capture reserve. Service
  construction obtains the direct and queued owner-derived durable-start footprints, asks
  `beryl-home-store` to validate them against its immutable
  `DURABLE_START_ADMISSION_BUDGET_BYTES = 268_435_456` policy and checked-add the reserve, then
  retains the resulting opaque requirement. Zero capture reserve, envelope drift, or overflow
  invalidates configuration before service publication and before any free-space query.
- Both direct submission and accepted-input promotion pass that same home-store-owned requirement
  intact to the home free-space query. This crate does not recreate package record arithmetic, define
  the fixed policy, or accept a caller's pre-aggregated total.
- Package tests prove the constant is exactly 256 MiB, both durable-start paths use the same opaque
  requirement, the current 1,328,763-byte shared owner-derived envelope fits beneath the constant,
  and zero capture reserve or checked-add overflow prevents service publication and performs no
  reserve query.
- Draft flush, input admission, thread/draft creation, runtime/root creation, claims, Syndic
  generated-title, usage, and automatic branch-discussion archive mutation, session update, settings
  update, asset reference, CAS-binding transition, and handoff-job transition use typed revision-
  checked commands.
- Correctness-sensitive success is not published until the home-store command reports the required durability barrier complete.
- Cancellation may win a draft edit's durable `Cancelled` settlement only before its candidate-
  adoption command is admitted. An admitted command is drained; an indeterminate adoption retains
  the exact operation and reconciliation custody until its immutable settlement proof and coherent
  candidate-session read establish the adopted root. An indeterminate autosave or flush instead
  reconciles its exact publication receipt against both current-draft and session heads.
- Store-health transitions invalidate outstanding mutation authority by revision. Persistent failure preserves existing window controllers and their last coherent in-memory presentation while feature-owned gates reject further store-dependent work.
- Reopen validates and preserves the same durable home identity, then rebinds services only to the
  newly published monotonic home generation; it never creates a substitute home, imports old state,
  or treats cached presentation as durable proof.

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
- The catalog adapter consumes revision-bound query pages and exposes stable row identities plus
  bounded row facts to the mounted
  [conversation-thread GUI](../../../doc/features/conversation-threads/gui.md). It does not define
  row realization, layout, focus, or overscan policy.
- Catalog construction and refresh never enumerate CAS threads, read CAS historical transcripts, or use backend names and working directories as Beryl metadata.
- Thread creation, pristine-thread reuse, current-thread no-op, first-runtime creation, additional-window acquisition, selection, and release use atomic home-store/Syndic claim operations rather than app-local check-then-act logic.
- The selected composer host opens one exact Syndic editor-candidate session from the durable
  current-draft selector, binds the session's newest candidate root, and exposes it to the
  range-backed text input as revision-bound bounded text and zero-width-marker page sources. The
  window retains only configured visible and overscan pages, bounded edit and IME state, compact
  positions, resident marker facts, and source and presentation generations; it never owns or
  reconstructs the complete draft.
- The composer host configures an explicit retained-memory budget and per-frame work budget for
  realization; neither is computed from raw viewport dimensions. It grants capacity first to
  caret/IME/directed-selection geometry, then the interaction or scroll anchor, then nearby content.
  Unrealized nominally visible regions collapse into bounded filler coverage and one typed capacity-
  saturated state rather than an unbounded demand queue. Logical scroll extent remains sourced from
  paged indexes, and filler interaction re-anchors and requests only bounded work. The OS shell and
  renderer, not this retained projection, reject or clamp an unrepresentable drawable surface or
  framebuffer.
- Compact editor restoration facts contain only the exact published combined-root binding and logical extent,
  caret and directed-selection positions, scroll anchor or continuation, durable edit-history
  frontier identity, and exact undo/redo availability. They contain no text or marker pages,
  complete marker collection, piece tree, layout state, or draft-sized inverse content.
- The host maps typing, paste, deletion, and marker commands to exact predecessor-candidate
  composite range replacements through the app-neutral text-input transaction-session protocol.
  Begin captures exact predecessor caret and directed selection. Bounded source and proposal pages
  carry canonical cumulative identity and explicit finish-input; inserted and moved markers use
  successor-relative coordinates and order. Each marker insert, remove, move, or same-id replacement
  is emitted as one complete proposal item on its natural widget/proposal page. It carries only the
  widget's accepted successor anchor, stable id, final label, same-anchor order key, checked charges,
  and complete predecessor occurrence proof when removal applies. It carries no caller-selected gap
  or immediate-neighbor witness; storage derives those facts from its current working roots after
  any removal. Alongside that bounded widget page, the host supplies exact insertion-time marker
  metadata keyed by the same stable object identity: the final label and the authenticated `AssetId`
  admitted for that marker. Translation requires a one-to-one match for every inserted marker and
  passes the resulting canonical `(marker id, label, AssetId)` association into Syndic. Move and
  same-id replacement preserve the existing authenticated `AssetId`; changing the referenced asset
  requires a new marker identity. This metadata is canonical draft state captured at insertion, not
  a caller-authored seal mapping or a late Asset lookup. The host does not force marker effects
  into the first page, pre-scan or reorder the widget stream, or retain earlier pages to complete a
  later effect. No partial replacement is exposed to the widget,
  session head, history frontier, or current-draft selector, and no whole-operation fragment vector
  or hardcoded cumulative page cap exists.
- Before translation or durable admission, the host validates one widget page against its exact
  binding, operation and lane frontier: expected cursor, ordinal and prior cumulative identity;
  canonical page and cumulative identities; checked page and cumulative totals; a nonempty set of
  at most 256 items; and at most 65,536 retained bytes. The operation high-water and lane frontier are
  qualified by the exact activation binding, candidate-session identity/generation, and predecessor
  base candidate/root/history revision. Any change to that tuple resets the high-water with the new
  operation; a completion from the old tuple is a stale terminal conflict even when operation,
  ordinal, or cursor bytes repeat after an ABA change. Each lane retains only fixed next-frontier
  state and the immediate-last `PageReceipt` identity. Exact byte-equal reuse of that immediate page
  returns replay without translation or storage work. Differing immediate reuse collides, and any
  older ordinal is obsolete and rejected. Invalid cursor, ordinal, prior identity, totals, item
  bytes, or canonical identity can therefore cause no staging effect.
- One newly accepted widget page is translated in isolation to one nonempty bounded boxed or slice-
  owned batch of existing Syndic staging pages. The mapping places each translated staging
  item in its own one-item physical page. Source items translate one-for-one. Proposal UTF-8 items
  use the minimum number of scalar-safe chunks under a 49,152-byte cap, one page per chunk: every
  non-final chunk ends at the greatest UTF-8 scalar boundary no later than 49,152 bytes from its
  start. A UTF-8 scalar occupies at most four bytes, so every non-final chunk contains at least
  49,149 bytes. Two non-final chunks would require at least 98,298 retained bytes, above the widget
  page's 65,536-byte aggregate ceiling. Across no more than 256 input items there can therefore be
  only one additional chunk in total beyond the one base chunk or page produced by each item, while
  every other supported item translates once; the exact maximum is 257 physical pages. The host
  retains exactly one current validated widget-page request and, while storage admission is in
  flight, one prepared atomic physical-page batch derived from that same request. It retains no
  earlier widget page, prior prepared command, proposal prefix, or whole-operation payload
  collection.
- The host passes that complete translation to `syndic-storage`'s single atomic physical-page batch
  preparation, contribution, and reconciliation boundary. `SourceSelected`, including the source
  classification after an indeterminate call, retains that exact validated request and the same
  prepared command byte for byte; it neither retranslates, advances the widget frontier, releases
  payload, nor accepts another page. `TargetSelected` advances the fixed widget frontier exactly
  once, installs the immediate-last receipt, and releases both the page payload and prepared batch
  only after every page, progress receipt, final staging head, and final candidate-session endpoint
  is proven. Partial occupancy, target disagreement, or collision makes the operation fail closed
  and cannot be treated as page replay. The one-
  physical-page small-keystroke path calls this same batch boundary with one element.
- Cancellation may discard the retained page and prepared command only while the exact command is
  still unadmitted and reconciliation selects its source closure with every target key absent. It
  produces no batch effect and releases that exact request only through the widget's terminal pre-
  admission cancellation path. Cancellation after an indeterminate command waits for exact source/
  target/partial batch reconciliation before terminal
  handling and cannot retract a selected target. `Indeterminate` remains custody, not a sixth
  widget outcome. After authenticated finish, the host supplies no prior widget page, proposal
  prefix, fragment bytes, or app-built reconstruction. It drives `syndic-storage` only by the exact
  `(draft, session, operation)` build endpoint while storage derives each next bounded staging
  window from durable custody. Each command consumes at most 256 physical pages/items under storage's
  521-read and 34,144,256-byte acquisition ceilings, independently admits at most 256 fragments and
  65,536 inserted UTF-8 bytes, and advances a nonempty source-only window even when it produces no
  fragment. Storage completes the copy-on-write piece-tree, marker-identity-index, and marker-order-
  commitment successors and
  exposes one atomic candidate-adoption command. The app retains only the current
  endpoint and terminal intent while that durable builder runs.
- Every admitted draft edit reaches exactly one durable `Committed`, `Rejected`, `Conflict`,
  `Cancelled`, or `Error` settlement. An indeterminate command remains reconciliation custody rather
  than a sixth result; the host preserves the last coherent projection and exact operation intent,
  suppresses dependent mutations, and accepts only the matching immutable settlement proof on
  replay. Only `Committed` means the host durably and exactly adopted the immutable successor into
  that editor candidate session and may return its root, extent, caret, and selection facts to the
  widget. It does not publish the current-draft selector. Every other settlement adopts none of that
  edit. Widget-page payloads were already released at their individual `TargetSelected` frontiers;
  terminal settlement releases only the fixed endpoint, intent, and settlement custody retained for
  the operation.
- Build, root, settlement, and indeterminate custody are all session-qualified. Abandoning a
  crashed session releases its app custody and cannot occupy or block the natural identities of the
  fresh session opened from the durable selector.
- Candidate-head drift in the ordinary single-owner widget slot is treated as a stale internal
  completion, not as evidence that an external writer changed the durable draft. An exact existing
  settlement is consumed; otherwise `Conflict` settles that widget transaction without adoption.
  The host retains only its bounded logical authored intent and may re-propose it against the newly
  authoritative candidate when exact position mapping is proven. If mapping or rebase cannot be
  proven, the editor becomes coherently unavailable instead of dropping or guessing content.
- The host coordinates but does not own undo/redo authority. Syndic's compact durable root-
  transition journal and frontier advance atomically with every adopted ordinary candidate and
  clear redo only in that committed command. Undo and redo authenticate exact retained lineage,
  then request the dedicated direct historical-root adoption that creates one new candidate
  generation, advances the durable frontier, and restores exact caret and directed selection.
  Rejection, conflict, cancellation, error, collision, late response, or indeterminate custody
  preserves both frontiers. The widget and window retain only current availability and one bounded
  operation cursor; neither retains inverse text, a marker registry, a root graph, or history-sized
  RAM. Autosave publishes the candidate with its matching frontier rather than synthesizing history.
- Ordinary draft seeds and persistence requests carry no selected-path parent. Idle acceptance
  names the expected thread, draft, gate, combined root, root-bound materialization, and asset proof,
  and Syndic selects the current thread tail atomically. Background accepted-input promotion never
  changes the resident editor, draft record, draft revision, undo state, or draft asset owner.
- Dirty autosave and timed or lifecycle flush barriers snapshot only an already adopted candidate
  and edit-history frontier. Before marker evidence work, the host asks Syndic for one opaque
  bounded publication-source capture that authenticates the exact candidate generation, root,
  immutable history frontier and ordinary or historical adoption closure, publication operation,
  durable selector base, and editor session. The capture contains no marker evidence, authorizes no
  write by itself, and remains the lane's sole source custody while any number of later candidates
  continue to adopt. The host then asks Syndic to compare the captured root's exact marker
  commitment with the prior published root. Equality reuses and validation-asserts the existing
  nonempty CurrentDraft Asset head and proof, or validates an absent head for marker-free state,
  without a marker scan. A changed commitment starts or resumes Syndic's bounded seal over the exact
  captured root. For a nonempty successor, it feeds each returned authenticated marker-id/label/
  asset page into Beryl-state's unpublished reference-set construction in the same `HomeCommand`
  that advances the Syndic seal cursor, while retaining only the current pages, cursors, and custody
  values; an empty successor stages no Asset set.
  It releases a page only after the exact Syndic seal frontier and Asset staging frontier agree;
  restart or an ambiguous page outcome replays/reconciles that immutable page from those durable
  frontiers rather than retaining or reconstructing an operation prefix.
- After a changed-marker seal completes, the host composes one `HomeCommand` with one Syndic
  mutation participant prepared from the opaque captured source and one Asset participant. Final
  preparation reauthenticates the source, its immutable adoption closure, and the current live
  session without reloading the captured state from the session's mutable frontier. A changed
  nonempty commitment also requires the
  completed Asset set and opaque proof; Syndic requires its `SequentialMarkerSummaryV1` and
  `OrderedMarkerAssetSummaryV1` to equal the seal proof's summaries, and Asset swaps the exact
  CurrentDraft head. Changed-to-empty instead has Syndic validate the completed seal against the
  exact root/commitment and require both exact empty summaries/removal branch; the one Asset
  contribution validates and removes the exact prior head, with no Asset proof or synthetic empty
  set. The host never constructs a commitment-to-summary mapping, adds a
  second Syndic validation participant, or creates a per-root Asset head. Publication fences the exact durable draft identity, selector revision/root, candidate-
  session lineage, and captured generation, carries no selected-path thread revision, performs no
  work proportional to unchanged draft length, and never builds `ComposerV1`.
- Edits may continue adopting newer candidates while one captured frontier publishes. Draft
  orchestration retains only the opaque bounded publication source plus exact session, candidate,
  timer, publication-request, and dirty generations around that work. The capture is released on
  every proven terminal disposition and is never reconstructed by walking candidate history. A
  completion clears only its captured generation and cannot clean
  a later candidate. The first clean-to-dirty adoption arms the applicable interval and later dirty
  edits do not debounce it. A successful dirty save rearms only if a dirty successor remains; a
  proven noncommit or recoverable nonterminal autosave failure rearms from its classification time,
  while a superseded timer or settings generation cannot rearm. A flush drains an admitted
  publication, reconciles an ambiguous writer outcome, and repeats from the newest eligible dirty
  frontier until published and candidate frontiers agree. A recoverable noncommit or nonterminal
  failure ends that flush unsatisfied without a retry loop; external durable-base conflict or
  terminal unavailability likewise leaves the barrier unsatisfied, and terminal unavailability
  does not rearm autosave.
- Published, exact-replay, and superseded callbacks converge through one revision-fenced bounded
  observation of the current durable selector and captured editor session. A changed Syndic
  revision retains the prepared lane for replay instead of classifying a mixed read. An exact cut
  or authenticated same-session published descendant installs the current coherent selector;
  another session's durable descendant is a durable-base conflict. No callback may reinstall its
  older selector or classify a compatible same-session descendant as collision.
- Each home service admits only a fixed nonzero number of marker-seal flights and bounded marker and
  Asset page custody. It coalesces or supersedes obsolete save demand instead of retaining an
  unbounded queue. Success, cancellation, failure, supersession, session disposal, home-generation
  loss, and service disposal release every flight, page, cursor, and custody value.
- Canonical `ComposerV1` materialization is submission-only orchestration and starts only after the
  required flush has selected one exact immutable candidate root as the current draft. The app
  starts or resumes the bounded Syndic materializer without changing the draft or session and
  accepts only its exact sealed root-bound mapping. A later candidate adoption or publication
  neither conflicts with nor rewrites that build or sealed result; it only makes the older result
  ineligible for an acceptance that names the newer current root.
- Submission materialization retains the content-bound `SealedContentMarkerSummary`, including its
  embedded `SequentialMarkerSummaryV1`. It may reuse or independently validate exact sequential
  evidence as required by admission, but it never treats a draft marker-tree commitment as either
  summary.
- Submission preparation streams authenticated marker-id/label/`AssetId` pages from that exact root
  into one unpublished paged reference set. The admitting home command independently validates the
  root-bound `ComposerV1` content identity/full digest and opaque draft-marker seal proof through
  Syndic, requires its embedded `SequentialMarkerSummaryV1` and the seal proof's ordered association
  summary to equal the sealed Asset proof's respective summaries, validates exact compact source and
  destination owner heads, then publishes Syndic acceptance and the asset-owner transition
  atomically. Marker-free admission carries no synthetic empty set; its validation-only
  Asset participant proves both heads absent on the same serialized writer snapshot.
- Submission admission validates the same clean editor session and exact published root used by the
  materialization. Only atomic accepted send-and-clear disposes that session and authorizes the app
  to clear the editor and atomically close its durable edit-history frontier. Rejection, conflict,
  cancellation, error, or ambiguous
  writer custody preserves the last coherent editor/session frontier until exact reconciliation.
- Ordinary thread switch, healthy window close, Exit, and submission dispose a session only after
  its flush succeeds. An unexpected process loss performs no recovery of unpublished candidates:
  fresh activation opens from the durable selector, so edits after the last completed autosave may
  be lost and old candidate sessions, roots, and owner-neutral asset sets remain unreachable for
  future garbage collection. Persistent external selector conflict makes the old editor coherently
  unavailable rather than rebasing it implicitly.
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
- Thread activation keeps the prior coherent selection until the target claim, exact combined-root
  binding, first required text and marker pages, and other activation facts are ready. It publishes
  title, lineage, composer source, compact editor facts, composer-history scope, transcript seed,
  runtime/root memory, and claim transition together; cancellation or failure leaves the prior
  bundle authoritative.
- Lineage and activity snapshots are query generations with bounded resident pages, not complete
  ancestor or process-session collections. Composer history uses one fixed-capacity process pool of
  compact sealed Syndic input references and recalls content through range-backed copy-on-write
  drafts rather than copied payloads.
- The package registers only command adapters supplied by the
  [conversation-threads feature](../../../doc/features/conversation-threads/design.md) and its
  service boundaries; it invents no additional thread or catalog mutation command surface.

## Runtime And Live Execution Orchestration

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while availability and errors are projected per window. Releasing one window's demand preserves shared runtime interest for every other window and required operation; after final interest and required work drain, this crate releases the managed runtime for orderly retirement.
- This crate requests exact-release-admitted backend sessions and pinned normalized operations
  through `beryl-backend`; it does not construct commands, parse JSON-RPC, inspect backend storage,
  own CAS schema compatibility, or consume a runtime-discovered capability surface.
- Accepted input and exact selected-path proof enter the CAS-live Syndic system. Normal transcript
  and history authority remains Syndic-backed; CAS historical reads are available only to the
  system's exact terminal-turn repair path.
- Every non-GPUI execution coordinator is bound to one exact healthy Beryl-home generation and
  Syndic thread. A store failure closes new durable command admission, retires the affected live
  service and its backend sessions, and preserves coherent window and editor state.
- Successful same-home recovery first closes and disposes the failed service, then constructs an
  unpublished fresh home/app/backend stack. Behind the outer publication fence it converges durable
  pending, stop, compaction, and repair obligations and attaches the supervisor. One atomic
  whole-stack publication makes that replacement observable; only afterward may its scheduler
  reacquire projections from durable Syndic binding authority. No live execution authority derived
  from the failed service becomes authority in the replacement service.
- Before starting a turn, app/home-store coordination performs the authoritative free-space
  admission check with the service's one composed requirement. A failed low-space check leaves the
  user's input and durable draft intact and starts no backend turn. `Sufficient` is an early guard
  only: it reserves no capacity and makes no promise against a later `ENOSPC`.
- The live-capture coordinator owns a fixed-capacity, priority-ordered outage buffer for normalized
  observations that arrive while durable capture is temporarily unavailable. It reserves capacity
  for terminal and lifecycle evidence ahead of ordinary text deltas, applies backpressure within
  Beryl-owned local limits, and never treats buffered observations as durable history.
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
  0.146.0 unless generated-schema and pinned-release source evidence establish one atomic targeted core interrupt after
  the app-server turn check.
- Deliberate control, diagnostic control, healthy-home window close, and Beryl-owned interrupting
  approval admit or join one durable stop operation. Join monotonically adds the caller's closed
  cause to that record. The admission mutation atomically publishes the stopping gate and record,
  reroutes ready or retryable input to next-turn authority, and cancels any pending app-owned
  automatic lifecycle continuation. It preserves separately accepted input and never moves sent
  input back into the composer draft.
- If that exact admission fails before reaching the writer or returns typed `NotCommitted` while
  the same target and driver remain valid, the stop coordinator consumes the resulting no-commit
  evidence to create the backend's narrow volatile pre-admission authorization before retirement
  begins. The authorization is bound to the same existing authenticated foreground target and its
  sole driver, is process-local and single-use, and cannot select a detached, replacement, resumed,
  request-only, or newly acquired session. Failure of any eligibility check creates no volatile
  interruption request.
- Creating or consuming that volatile authorization cancels the exact target turn's process-local
  automatic lifecycle continuation before the sole driver may dispatch interruption. Separately
  accepted input remains preserved and ordered.
- The volatile path has no durable stop operation or claim, join, retry, recovery, restart
  reconstruction, durable success, or terminal claim. It is unavailable after `Committed`,
  `Indeterminate`, cancellation after writer admission, possible durable authority, target drift,
  driver loss, or service retirement; those states cannot be converted into a volatile
  authorization.
- A stop owner first reconciles the durable operation, claims one caller-generated attempt in
  Syndic, and only then hands the non-cloneable dispatch capability to the already authenticated
  foreground connection driver. The driver performs the sole `turn/interrupt`; the coordinator
  never creates a detached request-only connection, resumes a replacement session, or sends from a
  shell-owned worker.
- Stop admission, join, durable claim, and `begin_dispatch` revalidate their exact live-command
  generation before dispatch. A stale command is rejected as home-authority loss.
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
- The stop adapter retains exact operation and convergence facts and routes typed updates to the
  feature projections that consume them. The [status-line](../../../doc/features/status-line/design.md),
  [notifications](../../../doc/features/notifications/design.md), and
  [main-windows](../../../doc/features/main-windows/design.md) features own visible feedback and
  close behavior; the [CAS-live system](../../../doc/systems/cas-live-syndic-transcript/design.md)
  owns interruption, convergence, and authority-loss policy.
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
  correlation; terminal or authority loss consumes the paired records exactly once.
- Local proven nondispatch of that primary interruption invokes the provider-operation safe-reopen
  mutation: it restores the same live compaction record and compacting gate,
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
  exact marker-free Asset-head absence contribution and no synthetic set or proof, revalidates that
  close or stop did not cancel the intent, and supplies those
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
  prefix retained during initialization and waits for every acknowledgement
  before it may succeed, so the first bound poll cannot overtake older controls. Permission
  approval is ineligible for that unbound prefix because no durable stop owner exists; observing
  one retires the candidate session. Every eligible prefix entry remains in the fixed-capacity
  prefix from decode through acknowledgement.
- A terminal broker result atomically installs its ownership-preserving reply and closes later
  admission at the acknowledgement slot before publishing connection cancellation. The blocked
  submitter receives that exact reply even though the slot is closed, while no subsequent
  operation can enter the retired ingester.
- The app-owned `Ingester` is the immediate custodian of every provider-staging
  `Indeterminate` outcome. Before it installs `BrokerReply`, closes `AckSlot`, releases
  `ActiveObservation`, or observes route cancellation or retirement, it synchronously and
  infallibly moves the outcome's sole opaque reconciliation descriptor together with its complete
  already-reserved registry capacity into the current home's `beryl-home-store` reconciliation-
  scope registry. The
  pre-writer reservation makes this transfer non-rejecting; no broker, acknowledgement, operation
  holder, connection, or service retains another copy.
- A consuming seal `Indeterminate` reaches the `Ingester` as one move-only seal custody guard. The
  guard privately retains the inert consumed stager with the sole home-store custody and exposes
  only terminal installation. The `Ingester` consumes it synchronously; installation moves home
  custody into the registry first and releases the stager second, before any permit settlement,
  reply construction, acknowledgement-slot closure, cancellation observation, or retirement. The
  guard exposes no stager, receipt, sealed handle, successor, retry, or publication authority.
  Unwind or ordinary guard destruction still performs the home custody field's fail-closed fallback
  installation before dropping the inert stager; it publishes no acknowledgement or successor.
- After that transfer, the terminal acknowledgement reports only the closed
  `not-applied/reconciling` disposition. It carries neither a receipt nor the descriptor. Registry
  handoff preserves custody and closes only the exact operation's publication scope; it starts no
  reread, retry, rollback, publication, or reconciliation execution.
- Registry handoff also ends any requirement to retain the old process-local provider stager.
  `ActiveObservation`, its stager, and their service-local holder may then be released or disposed.
  A still-current service may consume only the later typed natural-record classification exposed by
  `syndic-storage`; fresh-service recovery reads durable Syndic authority. Neither path adopts or
  reconstructs authority from the discarded object, and `Collision` yields no continuation.
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
- A connection forwarding sink, broker, router, loaded authority, and ordered ingester belong to one
  service generation and are retired together. No sink, broker, router lane, or loaded authority is
  transferred into a replacement service. Within the owning live service, the ordered ingester applies the remaining
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
- A surfaced publication ambiguity never returns a projection capability. If targeted natural-
  record reconciliation of that exact opaque binding-publication scope returns `ExactNew`, the
  coordinator rereads the named binding and establishes another fresh projection only when that
  binding proves a complete eligible Syndic prefix. An ambiguous or uncommitted injection target
  remains non-authorizing provenance and is never promoted by resume or reinjection.
- A context-bearing pending turn requires the branch-discussion system's exact selected-context
  projection proof. Without that proof the coordinator rejects the turn explicitly and never
  establishes a CAS projection that omits the selected assistant passage or conflates it with
  recovered history.
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
- Within the unpublished fresh stack, service construction owns one exclusive synchronous restart-
  recovery interval. It walks bounded input-gate pages directly from Syndic and retains only the
  current page, cursor, and one stabilized case. This work runs off the GPUI thread. No scheduler
  lane, projection acquisition, new thread, or input admission may start before both this recovery
  interval and the outer whole-stack publication fence complete.
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
  finalizing-history obligation left unsettled, it seals one recovery-ready handoff for the outer
  publication owner. While the stack remains unpublished, that handoff opens no scheduler lane,
  starts no projection acquisition, and emits no execution wake. After supervisor attachment and
  atomic whole-stack publication, consuming the handoff opens steering, recovered-pending, and
  accepted-next scheduling together and emits the existing `Recovery` wake. Preexisting retryable
  steering receives exactly one recovery retry pass. Content-free diagnostics expose the completed
  startup page, case, active-convergence, terminal-convergence, pending, and compaction-convergence
  counts plus publication and handoff facts. A startup read, classification, convergence,
  supervisor-attachment, or publication failure is returned before a service or scheduler becomes
  observable.
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
  it eligible again by itself. Only an explicit fresh cancellation lifecycle or recovery authority
  may open one bounded retry pass. Ordinary
  accepted-ready and target-readiness wakes schedule admitted work without clearing a parked retry
  gate. The scheduler retains compact revision-bound cursors plus one process-global retry-pass
  state and no input id, payload, waiter, retry set, or per-input timer.
- The mounted scheduler has no timer-driven retry path and owns no retry deadline. Elapsed time and
  the broad `Retryable` lifecycle never prove retry eligibility.
- The accepted-input scheduler's ordered next-turn scan acquires one long-lived
  scheduled-ordinary permit before reading a revision-bound next-source page. It retains at most
  one candidate per permit and never builds a resident backlog. After global discovery names a
  thread, it acquires one same-thread flight and keeps that flight through candidate validation,
  execution admission, promotion reconciliation, projection establishment, and ordinary dispatch.
- At service construction, any already-present next-source record fences the next-turn lane for
  restart recovery. More generally, every scheduler lane remains behind the startup recovery fence
  until all stale active authority has converged, and remains behind the outer publication fence
  until the complete fresh stack is observable. Same-process publication cannot consume newer work
  ahead of a durable predecessor; ordinary execution-readiness or worker-release wakes do not
  bypass either fence.
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
- Each published Beryl service owns one generated-media finalization coordinator keyed by durable
  resource identity, with at most one flight per identity and configured finite queue, worker, page,
  and byte capacities. Its lifetime outlives any window, transcript host, connection, item handler,
  or request waiter but not the service: retirement cancels and joins every flight and releases all
  coordinator capacity, while a replacement service constructs a fresh coordinator. Admissible
  reconstruction evidence and terminal convergence follow the
  [image-assets system](../../../doc/systems/image-assets/design.md).
- The submitted `UserMessage` lifecycle is correlated against the exact already durable input rather
  than duplicated; its provider metadata and checked content reference remain typed. Completion-only
  variants such as pinned `SubAgentActivity` are admitted only through their explicit typed path and
  retain their complete public payload through bounded durable chunks, never a whole app-owned
  value. Unknown, malformed, or unsupported history-relevant items
  preserve the exact provider terminal fact but prevent a history-complete publication through a
  typed incomplete reason.
- MCP and dynamic-tool structured values cross the app boundary through the closed typed storage
  algebra, not raw JSON, opaque bytes, or a generic catch-all. Provider completion is reconciled
  field-by-field against bounded durable reads. Non-narrative final fields remain exact. Any proven
  or conservatively suspected capture gap, including a completed `AgentMessage` or `Plan` narrative
  that disagrees with content received live, makes the whole turn repair-required; the live content
  is provisional rather than a prefix-equality constraint on repair.
- Exact-route duplicate start, repeated or reversed completion, item-kind conflict, and other closed
  lifecycle conflicts are history-producing observations. The ordered consumer asks storage to
  derive and publish a compact issue reference from the sealed observation; it neither abandons the
  evidence nor converts the conflict into a fatal routing failure. Missing, malformed, mismatched,
  cancelled, or retired routing remains fatal or unpublished according to the routing contract and
  cannot create an issue event.
- Ordered terminal capture publishes the exact normalized provider outcome and any independent typed
  repair-required reason together before acknowledgement, then exposes the proven durable result
  to ordinary execution. It never converts provider `Complete` into local `Incomplete` merely
  because canonical history remains unresolved. A durable provider-observation issue selects
  `CompletionMismatch` as the repair reason for normal provider terminal publication.
- Exact CAS 0.146.0 interrupted terminal capture selects `ForcedAbortOrderingUnproven` unless
  generated-schema and pinned-release source evidence establish a
  no-later-item barrier. A later same-target source event fails that connection after the terminal
  cut and cannot reopen source admission or revise the durable terminal.
- Every item delta carries its expected normalized kind. Capture validates the exact CAS item and
  kind plus its closed field identity, element ordinal, and protocol index before durable mutation,
  so an agent, plan, command, file, reasoning, or tool delta cannot append to another variant or
  field.
- Proven terminal handoff first closes canonical source admission and advances binding authority.
  When the exact terminal publication carries required capture-gap provenance and correlation, the
  same commit leaves the input gate in `RepairRequired(turn, Available)` and preserves the gap without
  starting ordinary terminal-history convergence. Otherwise it leaves the gate in
  `FinalizingHistory(turn)`. While retaining the same-thread flight for the finalizing branch, the
  app resumably builds and finalizes each immediately eligible visible item projection and the
  selected transcript from already durable canonical history. An exact final storage command proves
  the durable convergence fixed point and only then releases the current compatible finalizing gate
  to idle. Queued admissions may advance draft, broad-thread, summary, and gate accounting without
  invalidating a transcript build for the unchanged selected tail and digest; completion and
  ambiguous-result reconciliation preserve those admitted descendants. Worker completion cannot
  expose accepted-next work before the applicable repair or finalization gate is released.
- Live capture retains only one bounded pending provider-field fragment. It resolves item identity,
  lifecycle, kind, typed frame frontier, and exact logical-text prefix from Syndic's
  record-stabilized CAS-item/canonical-item/content reads; active-item and completed-item maps are
  forbidden. Pinned `turn/completed` is a status-only fence, not a full-item snapshot: capture flushes
  the pending fragment and audits every already admitted durable item through fixed-size cursor pages
  before allowing `TurnEnded`. Every completed item must name an exact sealed final typed frame whose
  kind, structure, field ranges, and content frontier are complete. Capture neither invents
  terminal item backfill nor treats idle state as completion proof.
- Foreground stream loss and every other proven or conservatively suspected capture gap retire the
  exact live target into repair-required state. If the turn later has exact terminal CAS authority,
  the coordinator first commits Syndic's target-scoped request claim, which atomically consumes the
  durable `Available` disposition under the same-thread no-successor fence. Only the proven committed
  successor yields the sole non-cloneable dispatch capability. `NotCommitted`, unresolved
  `Indeterminate`, `ExactOld`, and `Collision` yield no capability; only `ExactOld` may authorize the
  same claim command once. Process or service loss after claim consumption never recreates it.
- The coordinator consumes that capability into one bounded backend repair attempt and submits the
  result, including exact source identity, consumed claim provenance, and repair provenance, to
  Syndic's terminal-turn repair staging. A consumed claim with no complete durable staged response
  converges incomplete on recovery without another backend request.
- One repair attempt sends exactly one descending latest-turn request with `limit=1` and
  `itemsView=full`. It never follows a cursor, requests an adjacent turn, invokes item-history or
  whole-thread `thread/read` fallback, enumerates CAS history, or becomes a normal transcript read
  path.
- Only one exact matching terminal turn with complete full-item semantics is eligible for Syndic
  repair. Bounded cursor metadata for older turns is consumed and discarded without traversal and
  does not make that exact target incomplete. The complete terminal snapshot may replace the whole
  provisional live turn; missing or additional turn results, failure, ambiguity, nonterminal
  history, or identity or completeness mismatch resolves the turn as incomplete.
- This package's CAS-live repair coordinator owns the bounded whole-turn response, item/content page,
  and generated-media preparation sequence. For each repair `savedPath`, it drives authenticated
  sidecar admission and one bounded cross-domain stage command that records matching inert Beryl
  repair-asset evidence and a noncanonical Syndic media witness. No window, backend adapter, image
  worker, or transcript host owns that lifetime or publishes an asset.
- After every required stage is durable, the coordinator assembles one final `HomeCommand` from the
  `beryl-state` asset-publication participant and `syndic-storage` whole-snapshot selection
  participant. Success publishes both domains and enters `FinalizingHistory` atomically. Failure or
  incomplete convergence publishes neither; staged sidecars and records remain inert. A replacement
  service may finish an already complete durable candidate, but it never adopts a process-local
  response or rereads CAS or a recreated runtime path.
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
  healthy and current. Store recovery never revives it; later execution reacquires authority from
  the fresh service and performs complete preparation again.
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
- Soft stop follows the durable exact protocol above. Compaction, steering, queue delivery,
  replacement edit, and retry commands likewise name exact targets and flow through their owning
  operation gate. This crate never guesses a process, turn, parent, or lineage scope.
- Automatic thread-title maintenance uses a bounded background backend session and commits only a
  validated one-way Syndic thread-attribute mutation with the exact eligibility witness. It never
  occupies a selected foreground stream or exposes maintenance CAS threads as user threads.

## Fresh-Service Recovery Publication

- This package's process-wide home-recovery supervisor owns the sole outer unpublished-stack
  publication slot for the process home. It serializes recovery attempts and retains that slot from
  failed-service disposal through candidate construction, durable convergence, supervisor
  attachment, and exactly one atomic publication or complete candidate disposal. No inner service,
  backend connection, scheduler, or window may publish a candidate independently.
- Same-home recovery first fences new commands and joins, closes, and disposes the failed app
  service, backend connections, drivers, brokers, routers, schedulers, workers, projections, loaded
  sessions, leases, and request authorities. Disposal completes before construction of the
  replacement stack begins; none of that authority crosses the recovery cut.
- Recovery constructs the newer home generation, typed Beryl and Syndic handles, app/backend
  services, connections, schedulers, and presentation publishers as one unpublished private stack.
  No window, command path, scheduler lane, or projection consumer may observe or use a partial
  candidate.
- Behind the outer publication fence, the candidate converges every durable pending-turn, stop,
  compaction, terminal-history, and repair obligation to the fixed point required by its owning
  system. This convergence creates no early scheduler or projection authority.
- Only after convergence succeeds does recovery attach the supervisor to the complete candidate.
  One atomic whole-stack publication then makes the new home/app/backend service generation
  observable together; physical reopen, service construction, convergence, or supervisor
  attachment alone is not publication.
- Scheduler lanes and projection establishment remain closed through that publication cut. After
  publication, the fresh scheduler consumes the sealed recovery-ready handoff and reacquires each
  required CAS projection only from durable Syndic binding authority under the new service.
- Failure before whole-stack publication disposes the unpublished candidate before the recovery
  supervisor releases its publication slot and publishes none of the candidate's handles, services,
  scheduler state, or projection state. It never restores authority to the disposed service or
  falls back to a partially recovered stack.

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

- Image adapters own GPUI-local atom bindings and correlate bounded asynchronous preparation
  results to exact draft, asset, and presentation revisions. Stale results are rejected rather than
  rebound to another local projection.
- The [image-assets feature](../../../doc/features/image-assets/design.md) owns visible workflows and
  the [image-assets system](../../../doc/systems/image-assets/design.md) owns content, durable
  references, runtime projection, decode, and recovery. This package consumes their typed commands,
  facts, and bounded results without becoming another byte or asset authority.

## Transcript Host And Rendering

- Main-window transcript code interacts with one transcript host through the
  [transcript shell boundary](../../../doc/systems/transcript-presentation/shell-boundary.md).
- The app package does not call `syndic-storage` directly, retain full-history clones, derive transcript narrative from CAS reads, or let renderer callbacks initiate authoritative history transitions.
- The adapter accepts activation seeds, bounded live fragments, renderer demand facts, and narrow
  commands with stable Syndic provenance and presentation revisions. It forwards arrived live
  fragments in source order without retaining a second whole response and exposes only prepared
  host snapshots to renderer-facing code.
- Residency, durable-prefix reconciliation, scrolling, selection, rendering, and media policy remain
  behind the [transcript-presentation system](../../../doc/systems/transcript-presentation/design.md)
  and the [transcript GUI](../../../doc/features/transcript/gui.md). Renderer-facing paths neither
  query storage nor call the backend.

## Activity, Status, Notices, And Notifications

- Activity adapters expose revision-bound query pages over admitted lifecycle records and bounded
  app-local facts; status adapters expose statically bounded facts from their owning services.
  Visible content and GUI composition are owned by the
  [activity-panel](../../../doc/features/activity-panel/design.md) and
  [status-line](../../../doc/features/status-line/design.md) features and their linked GUI documents.
- The process-wide runtime orchestrator creates one opaque `runtime activity period` identity when a
  managed runtime generation becomes continuously usable and supplies it to activity admission and
  query adapters. Thread switches, turn completion, and later turns retain that identity. Process
  restart, managed-runtime teardown or replacement, and same-home service replacement end it; late
  results from the ended identity are rejected rather than attached to a replacement period.
- The app activity projection incrementally scans bounded raw-command fragments only through the
  first nonempty command display projection without materializing the complete command. It
  accumulates file, addition, and deletion totals in checked fixed-width counters only from explicit
  normalized `fileChange` records. It derives a completed child handoff's exact byte frontier from
  retained range metadata without rereading or retaining the handoff text.
- Activity metadata resolution receives only the bounded selected facts from a metadata-only
  `thread/read` response. The app never receives a backend `ThreadSummary`, history collection,
  source tree, preview, cwd, or backend thread name through this adapter.
- App projections accept only facts from their final compact or paged owner. No mounted or detached
  shell adapter consumes a backend `ToolActivityEvent` aggregate or consumes or reconstructs the
  removed `TurnStreamEvent`, materialized `ThreadItem`, `ThreadSummary`, or collection-backed
  `UserInput` graph.
- Each status rate-limit query supplies its bounded exact active-model interest to the backend
  scanner and retains only the one bounded matching bucket or an ambiguity/unavailable fact. The
  process projection never owns a complete backend bucket map or unmatched bucket collection.
- Activity adapters write no durable conversation authority and never reconstruct provider records
  from derived app facts. Durable activity records remain owned by the
  [Syndic conversation-history system](../../../doc/systems/syndic-conversation-history/design.md).
- Notice and attention adapters accept only bounded typed records and exact eligibility facts, then
  route them to the [notifications feature](../../../doc/features/notifications/design.md). This
  package neither selects visible treatment nor interprets dismissal or sound eligibility.
- One process-wide notification-audio lane consumes only feature-authorized eligible sound events
  and owns at most one active read/decode/playback plus the latest one waiting event. A waiter
  retains only bounded event and sound-selection metadata: it retains no file, source handle,
  encoded buffer, decoder, decoded buffer, or playback handle and acquires none until promoted
  after the prior active attempt reaches terminal.
- App configuration names finite nonzero encoded-source and decoded-audio byte limits. Before a
  promoted event retains a file or source handle or issues a source read, the lane proves the exact
  encoded extent and reserves that exact encoded byte charge. Failure to prove or reserve the
  extent ends the attempt without retaining or reading the source.
- With the encoded charge held, bounded supported-WAV header inspection must derive the exact
  decoded byte extent. The lane reserves that exact decoded charge before allocating decoded
  storage or beginning decode; an unsupported, malformed, truncated, or over-limit header fails
  before either action.
- The reader may transfer the encoded source and its one existing charge to the decoder, and the
  decoder may transfer the decoded buffer and its one existing charge to playback; neither transfer
  duplicates or forgets a charge. Encoded and decoded reservations may coexist only for the same
  active event and only while its decoder still needs the encoded source. The lane releases the
  source, encoded buffer, and encoded charge as soon as the decoder no longer needs them, and it
  releases the decoded buffer and decoded charge at that event's playback terminal.
- Reservation denial and every source-open, read, header, decode, device, or playback failure and
  every cancellation release the exact source, buffer, decoder, playback handle, and encoded or
  decoded charge owned at that cut. Waiting-event replacement drops metadata only. Orderly app
  disposal clears the waiter, cancels and joins active audio work, releases its stage-owned charges
  and resources plus process-wide audio state, and never promotes another event. The adapter
  executes the feature decision but does not infer attention eligibility or alter turn outcomes.

## Settings And Themes

- The GPUI settings adapter consumes the app-neutral settings-window package and the mounted
  composition supplied by the [settings GUI](../../../doc/features/settings/gui.md) and
  [theming GUI](../../../doc/features/theming/gui.md).
- The adapter sends caller-validated scalar mutations through typed Beryl-home settings commands
  and returns their exact typed outcomes. The [settings feature](../../../doc/features/settings/design.md)
  owns draft, validation, Apply, and visible result policy.
- Theme documents, role resolution, repository commands, active selection, preview, and editor
  presentation consume the [theme runtime system](../../../doc/systems/theme-runtime/design.md) and
  the typed `beryl-state` theme-domain service. This package never parses or serializes repository
  documents and never owns repository or active-setting durability.
- This package assembles the process-wide appearance/preview coordinator and owns GPUI window
  adapters, exact window-set publication, cache invalidation, and bounded UI/tool bridges. It holds
  only bounded manifest pages and finite resolved appearances supplied by `beryl-state`; it exposes
  no GPUI or Settings handles to theme repository workers.
- The pre-GUI `ThemeRuntime` is the one process-wide composition point for the generation-bound
  `beryl-state::ThemeService`, exact repository observation, bounded state-owned change
  subscription, and appearance coordinator. Its bounded drain coalesces watcher hints, treats
  overflow as a full repository refresh, routes feature and dynamic-tool commands through the same
  typed state command path, and accepts only committed or reconciled-exact-new Settings appearance
  outcomes for durable publication.
- A confirmed Settings result replaces the runtime's durable Settings, active-theme, observed-
  document, and resolved-base identity before attempting the all-window barrier. Adapter rejection
  leaves current appearance unchanged, reports one content-free pending-durable-application fact,
  and permits an explicit retry of that exact retained base without replaying or reinterpreting the
  Settings operation. Only a successful atomic window commit advances current appearance and ends
  preview.
- Valid externally edited active documents arrive only as fully resolved, identity-bound
  `beryl-state` results and use the same whole-window publication barrier as Beryl-authored saves.
  This package owns no filesystem watcher implementation, physical repository handle, parser, or
  serializer: `ThemeRuntime` consumes only the bounded subscription and exact reread APIs supplied
  by `beryl-state`. Invalid startup input selects the supplied built-in fallback, while an invalid
  live-edit result retains the prior coherent appearance and localized failure state.
- A full refresh installs a candidate repository observation and active-document identity only
  after the candidate active document is present, current for that observation, valid, completely
  resolved, and atomically applied. Full and document-only external refresh candidates are
  transactional: adapter rejection retains the complete prior repository, active-document,
  durable-base, pending-durable, and current-appearance state, records that a reread is needed, and
  cannot be applied through the Settings durable retry path. Missing, invalid, unreadable, stale, or
  otherwise inapplicable active content likewise retains the complete prior coherent snapshot until
  a later fresh reread and publication succeeds.
- Retirement shuts down the subscription and releases repository observations, adapters, previews,
  and appearance generations. Diagnostics remain fixed-size and content-free while combining app
  publication/watch counters with state-owned session, mutation, reconciliation, gate, and reread
  counters. A fresh same-home service receives no prior observation, cursor, subscription, preview,
  or reconciliation handle.
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
- The mounted lifecycle-yield, branch-resolution, and theme tools each keep their feature-owned
  schema and authorization; the generic tool bridge does not combine their permissions.
- Tool responses report durable admission, rejection, deferral, conflict, or bounded failure accurately; request acceptance is not turn completion or downstream job completion.
- Secret-like values and unbounded content are rejected or redacted before diagnostic retention.

## Diagnostics And Isolated Child Control

- This package owns one process-wide diagnostic-child process supervisor for the
  [diagnostics feature](../../../doc/features/diagnostics/design.md). It is the sole app-side owner
  of at most one diagnostic child, its bounded stdio request/response channel, request correlation,
  and lifecycle status from spawn until confirmed exit or completed cleanup; windows, backend
  sessions, and dynamic-tool calls cannot duplicate or adopt that process authority.
- Supervisor diagnostics expose bounded content-free process, memory, renderer, retained-state, settings-window, transcript-frame, media, and catalog summaries through explicit snapshot builders.
- Diagnostic reads never require loading nonresident conversation history, rendering hidden rows, scanning full catalogs on the GPUI thread, or querying CAS history.
- Isolated-child controls dispatch through the same exact Beryl-home window, thread, composer, stop, popup, scroll, and activation command paths used by direct interaction.
- Child control requests use exact child-known ids and expected state, reject ambiguity or stale targets, and never mutate private state behind those command paths.
- Diagnostic retention is bounded by record count and bytes and excludes transcript text, draft text, titles, root paths, search text, credentials, capability tokens, raw tool payloads, and other user content unless a separately authorized diagnostic contract explicitly requires a redacted value.

## Concurrency And Responsiveness

- The GPUI thread performs no blocking filesystem, Fjall, process, transport, protocol, history, Markdown, image, persistence, or model work.
- Background work is keyed by exact durable identity plus revision or cancellation generation. Completion applies only when every target fact still matches.
- Correctness-sensitive commands use revision checks and short typed commits, not long-lived locks across external work or await points.
- A replacement service does not lock old and new connection-authority gates together or transfer a
  router lane, loaded-thread registration, or projection from the retired service. It establishes
  every required projection only after whole-stack publication, from fresh service authority and
  durable Syndic/binding facts.
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
- Worker-queue construction consumes configured capacity and scheduling-class inputs, enforces those
  bounds, and returns typed completion or cancellation outcomes. It does not define process-wide
  priority among feature work; cross-boundary resource rules remain in the
  [bounded-resource system](../../../doc/systems/bounded-resource-dataflow/design.md).
- Collection adapters expose revision-bound pages, stable identities, and content-free diagnostics.
  Row realization, focus, and tooltip mechanics remain in the linked feature GUI and widget
  authorities, including the [conversation-thread](../../../doc/features/conversation-threads/gui.md)
  and [activity-panel](../../../doc/features/activity-panel/gui.md) compositions.
- Quiet backend streams are not failures, and ordinary bounded request timeouts do not impose an inactivity timeout on live turns.

## Dependency Boundary

- This crate may depend on `gpui`, Beryl widget/adaptor packages, `beryl-model`, `beryl-home-store`, Beryl metadata-domain packages, `syndic-storage` only through higher-level Syndic service boundaries, `beryl-backend`, and transcript-host/provider packages as allowed by their own designs.
- Renderer-facing modules must not depend directly on `syndic-storage`, `beryl-home-store`, `beryl-backend`, Fjall, or raw app-server protocol types.
- Storage, backend, and provider workers return typed bounded results that contain no GPUI entities.
- Cycles are prevented by keeping pure identities in `beryl-model`, physical-store ownership in `beryl-home-store`, Syndic records in `syndic-storage`, backend protocol integration in `beryl-backend`, and shell composition in this crate.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
