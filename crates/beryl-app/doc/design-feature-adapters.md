# Feature Adapters

This supplement is normative only for its bounded beryl-app feature-adapter role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

## Adapter Rule

- Adapters translate exact typed service facts and commands into bounded GPUI or tool-worker
  projections. They do not define visible policy, stored representation, system lifecycle policy,
  or dependency-private mechanics.
- Every asynchronous result is fenced by exact home/service generation and applicable feature,
  window, thread, draft, turn, asset, request, presentation, and publication revisions. Stale
  results are discarded and release capacity.
- Renderer-facing adapters consume prepared bounded facts only and perform no blocking storage,
  filesystem, backend, decode, history, or model work on the GPUI thread.

## Discussions, Images, And Transcript

- Branch-discussion adapters transport exact selection provenance, context-owner descriptors,
  durable-job commands, and structured tool outcomes. Workers receive no GPUI, store, repository,
  or window handles, and the adapter invents no resolve, archive, parent, retry, or replacement
  authority.
- Image adapters bind bounded GPUI atoms and preparation results to exact draft, marker, asset, and
  presentation revisions. They do not become byte, sidecar, reference, label, decode, or recovery
  authority. Visible behavior is in the
  [image-assets feature](../../../doc/features/image-assets/design.md) and durability in the
  [image-assets system](../../../doc/systems/image-assets/design.md).
- Each conversation surface uses one transcript host through the
  [transcript shell boundary](../../../doc/systems/transcript-presentation/shell-boundary.md).
  Adapters accept activation seeds, bounded live fragments, demand facts, and narrow commands and
  expose only prepared snapshots.
- Transcript adapters never call `syndic-storage`, retain full history, derive narrative from CAS,
  or let renderer callbacks initiate history transitions. Retirement cancels demand, rejects late
  results, and releases bounded state under the
  [transcript-presentation system](../../../doc/systems/transcript-presentation/design.md).

- The Code Save adapter binds one native destination choice and one bounded writer to the exact
  host, source binding/root and action identity. It uses the existing selected-history Source
  demand, retains at most one admitted source page/write in flight, and performs filesystem work
  off GPUI. A uniquely created same-directory staging file owns incomplete output; it cannot
  replace the destination. Complete publication is one atomic filesystem operation after current
  source/action qualification. Replacement requires native-confirmed overwrite authority for the
  accepted path; a new-file choice uses non-replacing publication and refuses a later collision.
  A nonblocking one-shot
  election is the publication linearization point: cancellation/retirement wins before it, while
  admitted publication finishes afterward. Foreground retirement never waits on a filesystem call.
  Cancellation, refusal, failed acquisition/write/publication and retirement before election close
  the worker-owned writer and remove its exact staging file off GPUI. An OS cleanup refusal
  releases worker handles and returns a distinct terminal failure; one staging file may remain
  at the selected directory, without claiming disposal or scheduling automatic effect retries.
  The adapter neither reopens retired demand nor retries a destination effect.
  Save admission acquires one opaque lease through a weak capability from the invoking window's
  existing running-resource worker lifetime. Save admission leaves the two slots required by the
  existing read-capacity predicate under its nine-worker limit; total retained work after Save
  admission is at most seven. Capacity refusal opens no picker or writer and queues no later retry.
  The lease covers the pending native choice, then
  transfers to the background writer through terminal close/cleanup or elected publication; it
  retains no service, provider or GPUI entity. The existing close/Exit resource-drain predicate
  awaits this lease before native removal or process quit. Suspension fences new Save admission
  and cancels pre-election actions without blocking GPUI; already elected publication settles.
  A pending native choice is awaited through its ordinary completion, with no late action revival.
  Typed picker cancellation remains distinct from picker failure. The
  [transcript feature](../../../doc/features/transcript/design.md#selection-copy-quote-and-menus)
  owns the visible command policy and outcomes.

## Activity, Status, Notices, And Audio

Activity height writes have process-owned bounded custody, separate from the selected-window read
loop. Admission reserves one outcome slot per window before executing a command. Cancelling a
read loop or closing its widget cannot drop an admitted command's original outcome. A pending,
indeterminate or committed local-finalization outcome blocks replacement in that window; recovery
may release old-generation custody only after publishing a different Home generation. The custody
registry retains no Home, graph, window or query resources and is bounded by the restorable-window
limit. Ordinary completed outcomes can be replaced by a later explicitly committed resize.
Published Activity identity and its resource reference are supplied by the owning Home publication
transition before exposing Activity. Publication returns the exact immutable identity with the
published store, adding no fallible admission after publication succeeds. Initial and recovery
graphs use this same transition; candidate failure retains its original typed cleanup ownership.

The Activity widget maps its canonical secondary command-button colors to the existing
`button.secondary` normal/hover/pressed/disabled roles, its command typography to
`button.secondary.label`, and its focus ring to `focus.ring`. Tooltip fill uses `tooltip` and
tooltip text/typography use `tooltip.text`. Activity-specific roles remain direct canonical roles.

- Running-thread attachment consumes a worker-prepared canonical transcript seed from a bounded
  source provider. The provider validates exact Syndic history authority, tail range and placement,
  shares immutable snapshot payloads and retains only one prepared attachment. Its final election
  uses the graph's existing Home mutation observer and nonblocking coherent-Home admission.
  Attachment may show an exact local unavailable fallback for missing/stale projection or a source
  record exceeding presentation admission; refresh failure preserves the coherent resident snapshot.
  Neither attachment nor refresh loads CAS history, dispatches work or registers a second capture
  consumer. This narrow source provider does not move storage access into the GUI transcript adapter.

- The published running-thread reader also carries the process-issued selected-transcript
  construction capability governed by the
  [presentation system](../../../doc/systems/transcript-presentation/design.md#selected-transcript-construction).
  Ordinary refresh invokes its bounded canonical steps off GPUI before the read-only provider
  prepares an attachment. The reader retains original command disposition across cancelled or
  stale delivery; existing worker retirement joins admitted settlement.

- Activity and status adapters expose stable revision-bound pages and statically bounded facts.
  They materialize no complete activity history, backend bucket map, raw command, CAS history, or
  provider aggregate.
- One runtime-activity-period identity scopes process-wide activity across turns and switches.
  Runtime teardown, replacement, restart, or same-home replacement ends it; late facts cannot enter
  the next period.
- Activity readers capture bounded request authority before storage work and preserve that exact
  runtime scope across failed initial or page reads. Consuming prepared results revalidates their
  original authority without waiting on runtime locks held by storage work. A busy publication
  fence invokes no consumer update and permits later revalidation of the same retained result;
  it never certifies current eligibility or replaces an ended period's authority.
- The window Activity adapter carries weak graph/window publication ownership and the exact
  selected-thread identity in addition to the service-issued runtime-period and query authority.
  Auto visibility derives from accepted-parent/compaction facts independently of query success or
  row count. Initial failure cannot become an empty hidden panel. No GUI callback performs storage
  work or decorates labels through a synchronous backend request.
- Incoming Activity models additionally validate their exact demand and preparation lifetime.
  Already accepted viewport interaction uses a separate lightweight no-I/O gate over the exact
  observed query/claim/Home interval and original runtime period. It retains weak publication and
  runtime authority, never a collection slot, page model or Home handle. Replacing a page demand
  therefore does not retire viewport focus or pin every collection slot. Busy or a changed Home
  observation suppresses input pending fresh observation without tearing down focus/capture;
  actual selection, graph or period retirement enters the widget's specified inert teardown.
  Fully prepared resident pages may publish while another page is pending or failed, with exact
  logical count and honest missing rows; unfinished extractor values never publish.
- Arbitrary viewport demands use a cancellable off-GUI cursor walk: each quantum reads at most one
  existing bounded page, checks the current demand and yields before another read. Retain at most
  eight opaque cursor checkpoints, a fixed resident page window and one pending demand; neither
  cursors nor row models grow with logical row count. Backward or distant jumps may restart from
  the initial cursor or a retained checkpoint. Missing pages remain visibly unmaterialized; no
  constant-time seek is promised and no ranked durable index is required by this adapter.
- Page projection reads only bounded exact canonical/source ranges and admitted normalized
  metadata off GPUI. It supplies stable identity, status and bounded agent/activity display values;
  source payload, full provider aggregates and complete collections never enter widget state.
  Existing compact handoff facts supply byte counts. Optional metadata completion updates exact
  realized row identities and cannot resume a thread or enter another runtime period.
- A metadata request captures the exact prepared window and a realized CAS thread before optional
  asynchronous metadata work. Completion accepts only normalized `ThreadReadMetadata` for that
  target and revalidates the captured query, runtime period, demand, selection and Home observation.
  Its bounded nickname may decorate that exact row; `model_provider` never supplies a model suffix.
  Request and completion remain owned by the worker across busy publication so revalidation needs
  neither another backend request nor fresh authority. Successful decoration revokes preceding
  prepared row snapshots. Raw strings paired retrospectively with a newer window cannot enter the
  viewport metadata completion path.
- An initial query failure retains its original prepared request for explicit Retry. Page failure
  retains the exact collection and continuation; failure on the first page after successful open
  retries that page on the opened collection, not the completed one-shot initial request.
  Query revision staleness instead invalidates the
  cursor walk and prepares a replacement collection; it cannot be retried forever against the old
  head. Busy publication retains the same prepared result and invokes no consumer update.
- Retry keeps its failure feedback while pending and disables repeated activation. Command
  delivery carries the original opaque failed-request identity, so a late button activation cannot
  retry a newer demand's failure. Coherent
  success clears feedback; repeated failure enables Retry again. A failed scope still observes
  current claim, query head and original runtime authority each bounded step, so actual query or
  runtime replacement retires the old scope without requiring Retry. A changed Home observation
  interval yields to fresh observation before classifying a query or selection change. Cache
  retention counts only pages overlapping the current demand, with at most four distinct resident
  pages; earlier cached pages cannot consume a new demand's capacity.
- Height persistence uses State's window-presentation participant bundle off GPUI, preserving
  original command disposition through failure/reconciliation and rejecting late window results.
  The host provides a coherent fresh page window before restoring interaction after period change;
  old rows, resize capture, focus and tooltip anchors become inert together.
- Notice adapters accept bounded typed records and exact eligibility and route them to the
  [notifications feature](../../../doc/features/notifications/design.md); they do not choose
  treatment, persistence, dismissal, or sound eligibility.
- `RuntimeFailureReader` holds only weak runtime and home ownership, exact home and
  service identity, and bounded typed observation facts. Window publication additionally fences it
  by the graph's weak restoration lifetime and exact State window claim. Reads authenticate the
  selected durable execution binding before and after observation and execute off GPUI through the
  existing single awaited status-observation loop; each window retains only its latest result.
  `SelectedRuntimeFailureObservation` carries unknown or the exact execution binding and original
  typed unavailable failure; it supplies no command or successful-recovery authority.
- An unavailable observation preserves the runtime owner's original typed failure and opaque
  attempt identity. Missing ownership, missing runtime state, closed publication, contention or
  unavailable read authority yields unknown, never fabricated failure or successful recovery.
  Unknown retains an already observed failure only for the same exact selection and publication.
  Selection or publication retirement ends that presentation eligibility. Runtime readiness alone
  does not prove the selected thread's required backend operations usable; clearing a failure as
  recovered requires the separately accepted exact selected-thread recovery result.
- The runtime notice contribution is suppressed only by the actually published native-lineage
  prompt for the same exact selected binding and blocking condition. Matching thread, runtime or
  binding alone cannot establish that the blocking condition is the same. It retains no editor, history,
  execution session or recovery capability. Retry availability follows its owning product gate;
  observation and runtime cleanup readiness do not grant recovery admission.
- The selected-runtime notice's Retry adapter captures its exact record/condition revision,
  selection and publication lifetime and submits only to the
  [selected runtime Retry owner](design-live-projection-and-scheduling.md#selected-runtime-retry-ownership).
  Enabled state requires that owner's current admission eligibility, including completed runtime
  cleanup and no overlapping recovery. The adapter retains one pending presentation identity;
  duplicate activation observes it rather than admitting another operation. The same persistent
  notice and command remain published while pending, with Retry unavailable.
- An exact unsuccessful completion updates that record with bounded typed failure feedback and
  recomputes eligibility; it never stacks a notice, clears the failure or launches another attempt.
  Cancellation without confirmed usability leaves the still-eligible blocking condition visible.
  Only a current service-issued usability result, revalidated for that selected binding at
  publication, removes the recovered condition. Unknown observations cannot undo this result or
  resurrect an older failure; a newer exact failure remains eligible. Selection change ends only
  the old presentation scope, not a shared process recovery or another window's notice.
- Publishing the native-lineage prompt for the same blocking condition suppresses this notice
  through the existing arbiter contract; the prompt's explicit command owner remains distinct.
  Late Retry completion cannot dismiss a newer condition or the prompt. Notice Retry preserves
  the mounted editor, local edits, focus and transcript selection; it mounts no replacement editor
  and acquires no draft content or history collection.
- Successful startup preparation supplies only the opened home's durability classification to
  Notifications. Native main-window publication offers its startup warning through the existing
  arbiter; later-created windows use the same startup classification. Admission history belongs to
  the surviving window root and remains intact when recovery replaces its notice bindings.
  The notice owner retains at most one cancelable warning timer, bound to the exact visible
  record and arm; retirement releases it and stale expiry cannot affect another notice.
- One process audio lane owns at most one active open/read/decode/playback attempt and one latest
  waiting metadata-only event. It reserves configured encoded and decoded bytes before acquisition,
  moves charges with resources, and releases all handles, buffers, work, and charges on every
  failure, cancellation, replacement, terminal, and disposal cut.
- The running process owns this lane independently of Beryl-home service publication. Recovery,
  window detachment and publication replacement neither duplicate it nor retain a home service in
  audio work. Irreversible final teardown closes audio admission, discards waiting metadata, stops
  playback and awaits its owned worker before quitting GPUI. Reversible Exit attempts do not end
  the lane. Its ingress retains only bounded sound kind/path metadata and grants no thread or
  execution authority. Terminal-event and attention adapters remain separate consumers; selected
  status observations and restored state cannot substitute for those event sources.
- The lane rejects paths exceeding 32 KiB of stored metadata, encoded WAV input over 8 MiB,
  decoded samples over 16 MiB, duration over 30 seconds, channel counts other than mono/stereo,
  and sample rates outside 8–192 kHz. Unsupported WAV formats remain best-effort failures.
  The bounded decoder admits validated RIFF PCM 8/16/24/32-bit and IEEE float32 in supported
  16/18-byte format headers; unsupported variants, inconsistent extents and nonfinite samples fail.
  Decoded storage moves into an owned playback iterator without a second decoded buffer.
  Cancellation is checked before and after synchronous regular-file calls and between bounded
  acquisition/decode blocks. Final drain awaits an in-flight OS call and worker release off GPUI;
  this contract does not promise a bounded shutdown wall time or preemptible synchronous file I/O.
- Ordinary sound eligibility is a source-owned one-shot attempt for an exact live home/service,
  thread and turn incarnation, after successful direct typed parent `TurnEnded` publication.
  Carry the admitted source's pending `TurnKind` through activation: ordinary user turns and
  generated discussion parent handoffs are eligible classes; lifecycle continuation, provider
  operations and maintenance are excluded. Complete, Interrupted and Failed live outcomes qualify;
  uncertain completion, driver errors and activation cancellation back to pending do not.
  History convergence, repair, restored rows and selected-status observations cannot emit this
  attempt. Mark the attempt once even when eligibility, settings access or audio admission fails;
  no historical deduplication set, replay queue or notification retry is retained.
  A live source completing before the running-process adapter is bound consumes a diagnosed
  unavailable-adapter attempt; later process handoff does not replay it.
- The source adapter reads current bounded `EndTurnSound` settings off GPUI and revalidates its
  original home/service authority before offering metadata directly to the process audio ingress.
  Run it outside storage/runtime publication locks and never await WAV playback or gate terminal
  history convergence on sound. Final metadata admission holds only the original service's
  admission guard so service retirement cannot interleave between validation and offer; settings
  and attention reads precede that guard. The adapter retains no execution capability or GUI handle.
  A future local failed-terminal publisher must use the same exact once-only cut after its
  authoritative terminal publication, rather than treating nondispatch as terminal failure.
- The running process owns one desktop-attention monitor and sendable latest attention/focus
  facts. Main and Settings native focus updates originate on GPUI; source workers consume only
  those scalar facts and the monitor's idle/lock/lid/display snapshot. Known active triggers use
  the Notifications OR policy; unknown/unsupported facts do not trigger or suppress another
  known active fact. Service/window publication replacement preserves this process custody.
  Irreversible final teardown closes the adapter/monitor and drains its worker off GPUI alongside
  the audio lane. No additional event queue or worker-local GUI ownership is introduced.

## Settings And Themes

- The settings adapter hosts the app-neutral settings window, sends caller-validated scalar
  mutations through typed home commands, and returns exact outcomes. Draft, validation, Apply/OK,
  and visible result policy stays in the
  [settings feature](../../../doc/features/settings/design.md).
- The theme adapter consumes typed theme service and assembles the process-wide appearance/preview
  coordinator, GPUI window adapters, exact window-set publication, cache invalidation, and bounded
  UI/tool brokers from the
  [theme-runtime system](../../../doc/systems/theme-runtime/design.md).
- Repository parsing, serialization, watching, mutation, and durability stay outside. The app
  retains only bounded manifest pages, finite resolved appearances, exact publication identities,
  and typed reconciliation or retry custody.
- One atomic window-set barrier publishes a complete appearance generation. Rejection or stale
  identity preserves the prior appearance. Replacement transfers no cursor, subscription, preview,
  observation, reconciliation descriptor, or publication authority.

### Settings Application Ownership

- The process composition owns one auxiliary Settings window controller and distributes non-owning
  open/reveal commands to main-window controllers. The external `gpui-settings-window` directly owns
  the preheated OS window. Settings receives no Session window/claim record. Opening uses the latest
  coherent prepared presentation immediately; unavailable or slower feature pages remain localized.
  Main-window close does not destroy this process-owned window or transfer its draft to that window.
- One Settings service belongs to each published app-service generation. Initial and recovery graph
  preparation construct it from that generation's Home service reference, State Settings handle and
  existing theme service/runtime composition. Only the published graph exposes its non-owning
  reader/commands. The controller owns the window-local staged draft; the service owns admitted
  command custody and process-lifetime unresolved-key exclusions independently of GUI observers.
  Temporary exclusive appearance-worker custody does not retire that published generation.
  The running owner retains a fixed set of its existing generation-qualified ordinary read facades
  for that exact flight: running-thread/thread-creation, model selection, exact status/Stop,
  Activity and Catalog query access. Facades keep their owning currentness checks and remain
  published while the mutable owner is off GPUI. They do not reacquire, copy or retag service
  authority. The flight drops these retained facades when the owner returns; genuine retirement,
  unavailable graphs and generation replacement retain their existing invalidation semantics.
  Executable creation/admission services remain subject to their ordinary live-owner gates.
  An already admitted New Window retains its exact hidden shell in the original bounded
  construction entry while that exact appearance flight temporarily prevents publication. This
  remains original preparation with unchanged capacity, deadline and cancellation, not another
  parked controller or queued construction. It does not reacquire or replay the command. After
  original services return and current Settings gates are projected, an owner/home-generation-
  qualified wake resumes the existing construction driver. Publication revalidates all original
  source, selection, cancellation, generation, appearance, Home, Exit and native guards. A true
  refusal follows original abandonment; a successor appearance flight retains the same wait.
  Initial Settings preheat and recovered-service rebinding precede ordinary projection, but cannot
  lend the mutable service owner to appearance work until ordinary command mounting and interrupted
  recovery completion have finished. Exit, shutdown and final teardown retain exclusive custody.
  Deferred appearance requests remain queued under their original identities until admission opens.
  Across generation replacement, the process composition retains home-qualified compact exclusions,
  original intent/outcome facts and the installed Home-registry operation handle. Old executable
  Home/State/theme handles retire; the registry handle preserves only its original reconciliation
  scope under Home ownership and never admits a replacement command.
- Ordinary scalar preparation reads only the closed five-key State schema, including exact absence,
  record/schema revisions and values. Bounded cursor or point reads plus an unchanged Settings
  domain revision prove one coherent preparation; a race returns a typed changed/unavailable result,
  not a mixed snapshot. Feature validation distinguishes absent clean defaults from invalid saved
  values and supplies row-local availability/feedback without silently accepting invalid values.
  No backend configuration or arbitrary key/value extension is admitted.
- Prepared presentation has exact home/service generation and source revision. GUI draft identity
  additionally includes the Settings window instance and monotonically advancing draft revision;
  commands bind that original draft, selected route and operation identity. Late loads, page results,
  command observations or callbacks cannot attach to a replaced source/window/draft by coincidence.
  Navigation preserves the complete draft; Reset replaces only its named scalar from the proven
  baseline. Apply/OK consume one immutable, completely validated modified draft.
- The scalar baseline and staged updates have at most five entries. Developer Instructions retain
  the feature's separate 60 KiB saved and 64 KiB draft caps; every editor mutation route enforces
  its hard cap before mutation, including paste/drop and IME commit. Other text uses the owning
  typed scalar/path limits. Bounded undo and feedback cannot retain unbounded rejected payloads.
  Ordinary sections/detail rows are statically bounded by the finite schema and external widget
  limits. Installed-theme and role collections use revision-bound bounded pages and fixed residency;
  they never become resident vectors to populate a picker or selected detail.
- Off-GPUI preparation validates the entire modified scalar set and, when needed, prepares the exact
  active-theme appearance. Before enabling Apply or modified OK, one coalesced read-only qualification
  validates the staged theme for its exact source/window/draft identity. Qualification claims no
  operation identity, Home receipt or command custody, and actual Apply still performs its own
  preparation. One typed State `ApplySettings` contribution publishes every modified
  scalar atomically with captured expected records/revisions or none. No repository file write or
  theme-document draft is part of that scalar batch. A stale preparation is a definite refusal;
  there is no automatic resubmission of an edited or rejected draft.
- The service admits at most one scalar command/reconciliation flight at a time and retains the
  exact original `CommandOutcome`, receipt, later failure, local finalization and reconciliation
  capability before any fallible observation or delivery. Caller cancellation before admission may
  retract the request. An `Indeterminate` result synchronously installs its move-only Home custody
  into the already reserved original registry scope before any translation or observation; the
  service retains its original failure and installed handle, never a copied descriptor.
  Cancellation, window detachment and observer loss after admission cannot discard or duplicate it.
  Only exact confirmed commit/`ExactNew` cleans the matching draft and
  permits OK to hide. Definite noncommit/`ExactOld` restores that same staged draft with feedback.
- Indeterminate scalar custody applies the external window interaction gate and the existing
  running Exit Settings-reconciliation gate. Repeated Apply, OK, close or Exit cannot queue a second
  command. Reconciliation only inspects/retries the original capability; it never reissues updates.
  Healthy-home inability to prove a complete outcome excludes exactly the original modified keys
  for that process lifetime, retains bounded row-local unresolved feedback, and releases the global
  interaction/Exit gates. Unrelated keys may form a later valid draft after the old local draft is
  discarded. No whole-domain or whole-home failure is inferred from scoped uncertainty.
- Unresolved-key exclusions are bounded by the five-key schema; overlapping updates remain refused,
  while disjoint requests retain ordinary revision validation. The Settings service does not create
  another durable journal or reconciliation registry. Home-owned reserved/collision custody follows
  its existing limits and disposal rules, and capacity refusal is explicit rather than queued.
- One bounded applied-settings source is the operation-time boundary for App scalar consumers,
  with at most five validated values/defaults and separate per-key availability. State remains the
  durable authority; this source is a generation-qualified active projection, not another journal.
  It starts from a coherent validated startup snapshot and publishes every confirmed modified value
  atomically before notifying consumers through coalesced generation/revision-qualified invalidations.
  Staged fields and unproven intended values never enter it. While the original command is pending,
  it retains the prior proven active values; after scoped unresolved classification, the affected
  keys become unavailable rather than being guessed or trusted through a raw State reread. Healthy
  disjoint keys remain usable. Fresh same-process preparation preserves those exclusions.
- Compaction timeout, outbound Developer Instructions and sound resolve their operation-time
  values through that applied source. Unavailability follows each owning feature's existing refusal
  or best-effort policy: do not start work with guessed timeout/instructions, and do not play or
  repeat a sound whose setting is unavailable. Already admitted operations retain their captured
  configuration. State reads remain for coherent source preparation and proof-qualified refresh,
  not a second activation path around pending or unresolved settings.
  Autosave additionally consumes a validated prepared interval: initial,
  newly selected and recovered composers receive the current generation's proven interval, and
  committed interval changes update current mounts through their existing timer/publication APIs.
  Pending or refused drafts do not reschedule timers. Obsolete interval completions cannot replace
  a newer source. An unavailable interval prevents timer admission/rescheduling while preserving
  ordinary drafting and explicit checkpoint/submission flush authority. Structural unavailability
  uses existing composer/Home availability policy.
- Active-theme selection remains an ordinary scalar in this draft. Its exact prepared appearance
  reaches the existing ThemeRuntime only after confirmed Settings commit or reconciled new outcome.
  Appearance application failure preserves the durable batch and matching clean Settings draft,
  retaining the previous coherent appearance under the theme-runtime contract. The Settings window
  participates in the same appearance window-set barrier as other themed OS windows.
  A same-process recovered unresolved active-theme key remains unavailable; it does not select a
  raw newly observed identity or flash a fallback. Only bounded prior coherent resolved presentation
  may remain visible as context while a freshly constructed runtime has no publishable durable
  selection. Old runtime, repository and appearance-publication capabilities never transfer.
- Repository actions, theme editing, Save/Save As and previews retain distinct theme-runtime
  ownership, bounded source/draft identities and original outcomes. They do not enter scalar
  Apply/OK. Their cancellation/close/Exit restrictions follow the owning theming contract; notably
  Save As never rebinds or cleans the original document draft or activates the new installed id.
  A generation-owned bounded theme broker performs typed repository I/O away from GPUI, retains
  admitted original outcomes independently of observers and drains before service retirement.
  Repository-file uncertainty that cannot resolve before retirement becomes the theme system's
  explicit unresolved read-only context; it is not adopted into a fresh Theme service or translated
  into scalar Home-registry custody. Fresh reads cannot answer or clean that original operation.
  Orphan old shell Settings sources are not a production dependency or a persistence adapter.
- Structural Home failure fences new work and presents existing Home unavailability. Retirement
  cancels source/page preparation, drains admitted work and transfers only bounded original intent
  and outcome custody needed by explicit recovery; it releases old Home/State/theme handles before
  old-generation disposal. The Home registry remains the sole original descriptor owner and outlives
  service generations under its existing contract. A fresh same-home candidate first joins/observes
  that exact original scope, subordinate to the original `CommandOutcome` and receipt; fresh State
  reads may then qualify its already classified old/new side without writing or retrying the scalar
  operation. Natural records alone cannot upgrade a pending/collision scope, reopen a closed scope,
  or reinterpret original committed/noncommitted evidence. Foreign home, contradictory records or
  insufficient evidence cannot become success; unresolved exclusions remain for the same process.
  No old execution capability admits a replacement-generation command.
- A retained GUI draft may remain bounded read-only context during Home unavailability. Executable
  editing/commit authority returns only with a coherent fresh baseline and exact original-operation
  classification; it is never silently rebased onto changed records. Cancel/close may discard a
  non-reconciling unavailable draft and later opening starts from freshly proven current values.
  Process termination loses the window draft as specified by the Settings feature. Final shutdown
  closes/releases the native window and subscriptions, joins/drains command custody and source
  workers, and releases the service before Home disposal; abandoned waits retain explicit cleanup
  custody rather than fabricating completion.

## Discussion Creation Adapter

- The non-GUI creation operation owns one exact captured request, stable child/draft identities,
  timestamp and typed Syndic source witness. Its admission uses the configured bounded operation
  capacity and never retains more than the contract's selected-text limit per admitted operation.
  Cancellation before commit drops unpublished preparation; it cannot cancel a committed creation.
- Process-owned operation capacity survives home-service replacement. Retained audit evidence shares
  the same bounded operation slot and immutable selected text, without retaining an old home service.
  Preparation captures the process admission epoch; execution holds that admission through commit
  outcome handling and any synchronous registry handoff. Shutdown/reopen cannot revive old preparation.
- The app composes Syndic creation with the exact unclaimed Beryl catalog row in one `SyncAll`
  command. No session/window claim or backend request belongs to that command. Exact created
  outcome returns the same discussion identity for later ordinary activation; activation failure
  neither repeats creation nor deletes the discussion.
- Reconciliation combines only the exact Syndic creation closure and catalog row under stable
  typed revisions. An indeterminate outcome transfers custody synchronously to the sole home
  registry before responding or cancelling. Exact old grants ordinary noncommit handling; exact
  new reconstructs the committed creation result; partial or conflicting state stays unavailable.
  Evidence retained before execution shares the exact attempt's disposition and installed registry
  handle. Visible natural records alone cannot turn an unresolved indeterminate attempt into success;
  the audit must join that scope and honor pending or collision outcomes before reporting creation.
  GUI busy state, selection presentation and activation retry remain feature-owned.

## Discussion Resolution Admission

- The non-GUI admission boundary consumes the broker's exact discussion, resolving turn and CAS
  request identity under the matching home and projection-service generations. Its scoped live
  command remains owned through durable execution and the caller's typed outcome publication or
  custody transfer. The ordinary broker remains the sole correlated response owner; lost response
  delivery cannot roll back admission or authorize a replacement attempt.
- Historical request lookup precedes fresh-attempt eligibility and returns the original job even
  after terminal completion or a later attempt. A different request while live preserves the
  original payload. Fresh admission derives immutable child/parent/context facts and the parent
  accepted-input frontier from bounded typed reads. Future-turn input defers without mutation;
  archived discussions or parents reject fresh admission.
- One home command composes the prepared State admission with Syndic's pending discussion gate.
  Original handle, domain and home revisions fence preparation; Syndic repeats the active CAS,
  archive and empty-queue checks at the writer. No candidate path admits a new resolution.
- Process operation custody retains uncertain audits in their existing bounded reconciliation
  slots even if a response handler drops its copy. Exact-job lookup recovers that audit; its slot
  stays unavailable until natural State/Syndic evidence and the home registry agree. Collision
  remains unavailable. Retained audits hold no old home service; fresh candidate handles can
  reconcile them. The same custody rule applies to subsequent handoff settlements.

## Discussion Parent Admission

- Parent delivery is a non-GUI handoff operation owned by the process coordinator and its exact
  home generation. A configured reconciliation slot owns one job, stable generated input/turn/item
  identities, prepared sealed visible text and bounded exact outcome witnesses. Input id derives
  from the admitted job; turn/item ids and timestamp are captured once. The State resolution is
  the only payload source; no later tool response, selection, draft or window supplies replacement
  content. Preparation follows the handoff system's exact visible-text contract.
- Ordinary admission reads one exact waiting-parent job, asks Syndic for current parent eligibility,
  and returns waiting without admission when temporary gates remain. A ready result joins one
  Syndic generated-input mutation with the exact State starting-parent transition in one home
  command. State witness identity and resolution agree with the Syndic receipt and sealed content;
  mismatches reject preparation. Process admission remains held through writer outcome and any
  synchronous registry handoff. The normal provider dispatcher remains a later owner.
- An archived parent joins one Syndic proof-checked child-gate release and the State terminal
  `ParentArchived` transition. The same operation can converge an already-admitted pre-append job
  through explicit candidate access. Candidate convergence never prepares or creates a new parent
  input, starts CAS, or transfers ordinary ready work into publication.
- Audit evidence shares the admitted slot and retains no old service reference. Uncertain outcome
  installs its exact home reconciliation scope before returning, and no parent dispatch or tool
  success is authorized until both participants and that registry scope agree. Exact old grants
  noncommit handling; exact new returns the same immutable parent identities or terminal failure;
  partial or conflicting evidence stays unavailable. Candidate reconciliation uses fresh handles
  and the same bounded closures. Cancellation and service replacement cannot revive old prepared
  commands; cancellation never rolls back committed parent input.
- Each slot retains at most the admitted resolution and one canonical visible-text preparation,
  bounded by 262,144 and 262,168 UTF-8 bytes respectively, two bounded State job records and the
  fixed-count Syndic command closure. Ordinary content chunking and record ceilings apply; no
  staging stream, queued job collection or retry can extend these per-operation bounds. The
  original job page is released before unrelated work is admitted.
- Explicit retry names the exact job and captured job revision. It requires the same retryable
  checkpoint, pending child gate and immutable parent provenance, including any accepted parent
  Syndic/CAS identities. Stable original home revisions fence these source observations before
  changing only State's job/live pair. Retry preserves the attempt and payload and performs no
  Syndic mutation or CAS request. Candidate and ordinary scans do not invoke retry automatically;
  outcome uncertainty uses the same retained process custody and exact State witness.
- The ordinary execution owner supplies exact activated-parent cancellation identity and a closed
  proven-nondispatch disposition: CAS rejection, proven delivery failure or execution fencing.
  This trusted internal evidence is not a tool or UI parameter; possible dispatch cannot construct
  it. Preparation authenticates the starting-parent job, child gate, immutable generated input and
  exact unaccepted activation, then joins Syndic cancellation and State retryable failure in one
  command. The admitted input, turn, payload and discussion gate remain intact. Bounded process
  custody retains the State witness and cancellation request; ordinary and fresh candidate audits
  require both natural outcomes and the home registry to agree. Candidate access first reconciles
  any original uncertain command and never invents new nondispatch evidence.
- Before generated parent dispatch, ordinary execution reserves one existing handoff reconciliation
  slot and authenticates the exact starting-parent job, input and turn. This noncloneable reservation
  remains execution authority, not cloneable request policy or a parked native-lineage payload.
  Proven nondispatch settlement consumes the reserved slot without reacquisition. A healthy writer
  conflict may reprepare the same local settlement from fresh sources while retaining that slot
  and exact reason; it never repeats CAS dispatch. Uncertainty transfers to existing process audit
  custody. Generation disposal fences further preparation and preserves required outcome custody.
- Converting a dispatch reservation to exact nondispatch synchronously transfers the proof into
  the existing bounded process settlement owner. The retained entry owns the same slot, home and
  original generation, exact job revision, cancellation identity and closed reason; it retains no
  home service, payload, session or ordinary execution capability. The ordinary owner and any
  prepared attempt prevent overlapping preparation. Commit releases the proof; cancellation,
  fencing, known noncommit and caller disposal do not. Submitted uncertainty keeps its original
  audit alongside the proof until registry and natural evidence agree.
- A fresh same-home candidate can settle retained proofs only after ordinary proof ownership and
  outstanding prepared attempts have retired. It processes one existing slot at a time before
  generic CAS-live convergence, authenticates current State/Syndic sources, and never sends CAS
  requests. Exact-old audit reconciliation permits fresh atomic cancellation using the retained
  reason; exact-new reconciliation releases it; collision or failure leaves custody intact and
  stops recovery. Candidate retries retain the slot without reviving the old process permit.
  Complete graph recovery must mount this prefix and final disposal must account for pending
  proofs before claiming successful cleanup.
- The shared ordinary start boundary requires this settlement authority for generated inputs and
  rejects missing or foreign authority before activation. Scheduled leases carry it alongside
  assets and tools; direct execution supplies the same authority. Capacity refusal arms one
  coalesced execution wake under the slot-admission lock. Slot release wakes the current ordinary
  scheduler outside that lock; no per-job waiter collection or replacement home-mutation observer
  is introduced. Paused jobs remain ineligible even when unrelated execution wakes arrive.
- After proven nondispatch, the same bounded ordinary worker may reprepare only following a typed
  physical command conflict or concurrent source change. Every attempt rereads exact sources and
  checks cancellation and generation; identity drift and other failures stop local preparation,
  while uncertainty transfers to retained audit custody. These attempts never repeat CAS dispatch
  and retain only one reservation, reason and current command. Verify progress after successive
  unrelated commits and responsive disposal during contention.

- Before-activation failure custody retains the reserved pending-dispatch evidence and exact
  execution binding instead of an activation-cancellation request. Only confirmed runtime, root
  or CAS preparation failure can construct that private event; generic not-ready results cannot.
  Settlement revalidates the current pending turn, dispatch provenance, binding and captured job
  revision, then publishes a State-only retryable transition under the exact home revision.
  Candidate recovery uses fresh bounded pending-dispatch and thread-execution reads; it preserves
  the original input and never manufactures an activation to cancel. The same process-owned
  proof slot survives owner disposal, failed preparation and uncertain commit. Runtime recovery
  does not erase an already captured event; replacement identity or possible dispatch rejects it.

## Tools And Lifecycle Yield

- Every persistent conversation lineage uses one canonical versioned, deterministically ordered
  conversation-tool registry. Its exact identity is SHA-256 of canonical serialization;
  continuation, resume, and fork require the same profile.
- The generic broker authorizes exact connection, registration, loaded generation, CAS thread,
  turn, call, and installed tool before bounded argument ingress. Registry membership describes
  capability and grants no mutation authority.
- Feature sinks incrementally admit closed schemas and yield one non-cloneable bounded typed
  request. Routing retains no `serde_json::Value`, raw spool, complete request clone, or second
  response owner.
- Lifecycle-yield, branch-resolution, and theme tools keep separate feature schemas and
  authorization. Unknown tool, invalid envelope, schema failure, cancellation, loss, and handler
  failure produce one typed response or connection failure under exact dispatch state.
- Branch dispatch preserves the validated CAS thread/turn/call identity in a distinct typed
  handler context alongside ordinary home/service and Syndic correlation. Construction stays
  inside the ordinary dispatcher; payload parsing cannot manufacture that context. The handler
  borrows correlation independently of the non-cloneable request and sole response owner.

## Diagnostics

- One process-wide supervisor is the sole app owner of at most one diagnostic child, its bounded
  stdio channel, request correlation, and lifecycle custody through exit or cleanup.
- Executable child thread switching resolves one registered ordinary main shell: the sole shell
  is unambiguous even when minimized; with multiple shells only the exact active registered shell
  is eligible. Missing or ambiguous selection is unavailable, never a first-window substitute.
  Validate the bounded thread identity before invoking that shell's ordinary activation command.
  The existing command owns off-GUI preparation, cancellation, exact source admission and coherent
  editor/transcript publication; diagnostic dispatch creates no alternative activation owner.
- A child switch response distinguishes an already current target, admitted opening work and
  failure. Admission is not completion. Subsequent bounded UI observation reports the selected
  claim, pending target, activation failure and published transcript identity independently, so a
  previous coherent transcript cannot be mistaken for the requested target's publication. Timeout
  or channel loss never causes another activation attempt or target substitution.
- Resident transcript observation reports content-free identity, revisions, initial frame/scroll
  placement, pending preparation and aggregate resident/resource facts under the existing protocol
  frame bound. Any reported row slice obeys the requested limit capped at 64. Observation reads
  already retained state only and cannot launch demand, shape text, prepare images or retain source
  content. Normal bounded resource preparation remains post-publication work; neither its pending
  state nor completed-media readiness gates a child switch or changes initial scroll placement.
- Child controls use the same exact window, thread, composer, stop, popup, scroll, and activation
  command paths as direct interaction. They never mutate behind those paths or substitute a target.
- Diagnostic snapshots are fixed-size and content-free where required. They do not load nonresident
  history, render hidden rows, decode media, scan catalogs on GPUI, query CAS history, or retain
  user content, paths, credentials, capabilities, or raw tool payloads outside an authorized bound.
