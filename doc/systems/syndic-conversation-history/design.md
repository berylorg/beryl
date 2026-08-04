# Goals

Define Syndic as Beryl's durable thread, draft, conversation-history, projection, reference, and replay system for agent work captured by Beryl.

Let Beryl render selected conversation history from Syndic-owned storage and projections while retaining Codex App Server as the live execution, auth, sandbox, approval, skill, MCP, and enterprise-policy authority.

Keep canonical history, transcript-view records, Markdown projections, and resource metadata below the GPUI transcript presentation stack.

## Non-goals

- Replacing Codex App Server authentication, model execution, sandboxing, approval handling, skills, MCP, subagents, managed configuration, rate-limit handling, or enterprise policy enforcement.
- Importing or backfilling old Codex App Server transcript history into Syndic from historical read APIs.
- Treating Syndic storage as a cache over Codex App Server history.
- Owning Beryl runtime/root registries and availability, window claims, session layout, durable host
  jobs, or catalog query indexes.
- Garbage-collecting turns, resources, or projections that are unreachable from named threads.
- Rendering operational activity, raw reasoning, command logs, or tool internals as parent transcript narrative.
- Storing OpenAI, ChatGPT, Codex, or app-server authentication secrets.

# Decisions

## Documentation Set

- `concepts.md` is the supplemental Syndic domain model. It is authoritative for current vocabulary
  and accepted model statements about turns, threads, turn items, canonical messages, Markdown
  projections, Syndic references, heavy item references, lazy history access, replay, and stop
  operations.
- `doc/systems/cas-live-syndic-transcript/design.md` owns the CAS-live source ingestion and CAS projection system contract.
- `doc/systems/bounded-resource-dataflow/design.md` owns cross-package fixed-page streaming and the
  risk-based page, payload, queue, cache, and concurrency rules; Syndic retains ownership of typed
  durable staging and atomic semantic publication.
- `crates/syndic-storage/doc/design.md` owns the reusable storage package boundary, storage engine, persistence API, and on-disk state contracts.

## Product Boundary

- Syndic owns stable threads, immutable execution bindings, intrinsic thread attributes and exact
  usage observations, committed conversation tails, exactly one current durable draft per thread,
  ordinary submitted turns, provider-operation turns, ordered turn items, provider/source metadata,
  canonical event records, compact thread summaries, transcript-view records, presentation-only
  activity query indexes, projection records, and resource metadata.
- User-visible transcript history is read from Syndic transcript views once the selected history has been captured by Syndic.
- CAS remains the source of live execution events, not the read authority for captured transcript presentation.
- Syndic records preserve external execution identities, including Codex App Server thread ids,
  turn ids, and item ids when they are available, so Beryl can still target exact backend
  operations such as stop, branch, resume, or generated-title production.
- Missing external identities remain absent rather than inferred.

## Thread And Draft Model

- A Syndic thread is a first-class stable named reference. A turn existing in the DAG does not create a thread.
- Each thread record owns a stable thread id, one committed conversation-tail id when submitted history exists, exactly one current draft id, and a revision covering those mutable bindings.
- A top-level thread owns canonical root-lineage facts. A branch-discussion thread additionally owns
  one immutable parent-thread binding, lineage depth, chain digest, and deterministic ancestor skip.
  Syndic exposes exact total parent count and revision-bound top-to-bottom ancestor cursor pages;
  callers never construct the complete thread lineage to render breadcrumbs.
- Every thread also owns compact inherited and current image-label frontiers. Immutable local
  origin spans bind each monotonic frontier advance to one admitted owner and sealed asset-set
  proof; a child inherits only its parent's frontier and resolves inherited origins through bounded
  lineage traversal instead of copying a history-sized label map.
- The visible submitted transcript path is obtained by walking immutable turn parents backward from the committed tail to a root. The current draft is excluded.
- A current draft is a durable typed pre-submission metadata record with stable id and revision, one
  exact sealed content-manifest reference, and one closed submission intent: ordinary,
  branch-context, or replacement. The manifest's bounded ordered chunks encode the exact composer
  atoms; each image-marker atom includes its stable marker identity and final label ordinal, and
  Beryl asset references resolve that marker to bytes.
- An ordinary current draft has no parent or branch context. It is only mutable unsent composer
  state and remains independent of later committed-tail movement. A branch-context intent names
  the exact immutable envelope whose source turn is the branch point; a replacement intent names
  its exact target and selected-path proof. The closed intent makes those cases mutually exclusive.
- Beryl transcript presentation may derive one synthetic readonly context group from that immutable envelope at the branch boundary. The group remains presentation-only and keeps stable semantic identity when first submission transitions the context-owning draft into a submitted turn.
- Only the draft's sealed content reference and mutable timestamps change during autosave. Thread
  owner and submission intent change only through their explicit typed operations; ordinary
  selected-path parentage is not draft state.
- A branch-context or replacement draft may exist only with an idle gate and zero live accepted
  work. Promotion's idle gate with positive next-turn work therefore proves that the untouched
  current draft is ordinary without reading or fencing its record.
- Content construction is staged through bounded ordered chunk commits. One final revision-checked command seals the manifest and publishes its draft reference atomically; incomplete or superseded staging remains unreachable durable state until future garbage collection.
- A physical record or chunk is bounded, but one logical draft has no fixed whole-content byte ceiling. Storage preserves a draft even when later CAS request assembly rejects it against an exact provider/model limit.
- An idle-thread submission atomically validates the sealed asset-set proof against the draft's
  marker digest/count and selects the then-current committed tail as the ordinary turn's parent
  under the exact thread, draft, and input-gate revisions. It transitions the same draft identity
  to a submitted turn, creates one typed canonical user-input item referencing the sealed content
  and set proof, advances the thread tail, and creates its replacement current draft. Replacement
  submission instead derives the new turn parent from its explicit replacement target. First
  branch-discussion submission derives its parent from the exact source turn in the draft-owned
  context envelope and requires that source to agree with the thread's selected branch point.
- First acceptance of draft markers in either idle or queued/steering admission atomically advances
  the owning thread's permanent image-label frontier and creates at most one immutable local origin
  span, regardless of marker or newly reserved label count.
  Later delivery transitions reuse that authority and never maintain separate label collections.
- The draft-to-turn transition preserves the draft's exact 128-bit identity payload while changing its typed identity from `SyndicDraftId` to `SyndicTurnId`; it does not allocate an unrelated submitted-turn identity.
- A context-bearing draft's owner descriptor follows that same deterministic typed transition while its immutable context envelope remains byte-for-byte unchanged.
- Context admission proves that the source turn was on the named source thread's selected path. Reopen does not require that turn to remain on the thread's later mutable selected path; replacement edits may move the named thread tail while the immutable context source remains durably referenced.
- After first submission, the context envelope remains owned by that deterministically typed first submitted-turn identity. A later replacement path in the discussion may move away from that turn without moving, deleting, or invalidating the thread's stable context owner.
- Input submitted during an active turn or compaction is atomically frozen into one immutable
  ordered accepted-input record referencing the exact sealed content, compact head of its immutable
  paged marker-reference set, one route-generation identity, and the complete source
  thread/draft/gate revision plus source/replacement draft admission proof. The record is the
  permanent reconciliation receipt after the replacement draft or route advances. One bounded
  mutable route leaf plus the selected route-generation head determines effective steering,
  pending, next-turn, and terminal delivery state. Those transitions retain the same accepted-input
  identity and reference-set ownership; they never create a separate queued-input identity or a
  second active turn.
- Permanent accepted-input order is retained independently from revisioned route-generation state.
  Generations own disjoint contiguous accepted-order intervals, aggregate checked `u64` live counts
  and logical bytes, and expose their leaves through bounded revision-bound pages. A compact ordered
  ready-source record selects each exact steering generation with ready or retryable work, while a
  separate compact ordered next-source record selects each generation with effective next-turn
  work. Their bounded candidate pages advance cursors across scanned non-candidate history, so
  schedulers never merge or retain the complete backlog. Terminal leaves remain addressable in
  accepted history but are omitted from live work results.
- A `Retryable` leaf proves that the prior attempt did not dispatch and another attempt is not
  forbidden; it does not prove a transient cause or immediate scheduling eligibility. Syndic
  exposes that durable state without owning elapsed-time policy, process wakes, or a retry timer.
- When an idle gate has effective next-turn work, one revision-fenced promotion selects only the
  earliest effective accepted input. It creates a fresh pending ordinary turn parented to the
  precommit committed tail and a fresh canonical user-input item referencing the accepted content
  and asset-set proof; accepted-input, turn, and item identities remain distinct.
- That same promotion advances the thread tail, input gate, transcript/summary/activity heads,
  binding head, route generation, live counters, and next-source authority atomically. The
  accepted input remains in permanent order, while its route leaf becomes terminal
  `promoted-to-turn` history with an immutable witness naming the exact fresh turn and item plus
  the source gate, route-head, leaf, and timestamp proof.
- Promotion does not consume, replace, reparent, or revise the current draft. It preserves the
  thread's current-draft binding and matching reverse index while advancing only their enclosing
  thread-revision proof. A concurrent ordinary send, special draft operation, or other tail/gate
  change conflicts at the serialized writer and must reconcile from durable state.
- Accepted-input delivery-unknown is a terminal delivery outcome distinct from delivered and failed.
  It means one provider request may have been dispatched but has no authoritative response. The
  outcome retains the admitted input and provenance in history, leaves every live delivery index,
  and forbids automatic replay.
- Delivery-unknown provenance includes the exact historical active binding, execution snapshot,
  CAS turn correlation, and the old CAS thread's one-way retirement through its exact stale binding.
  It cannot be published while that projection remains usable or through a commit separate from
  projection retirement.
- Exactly one revisioned input gate per thread classifies admission against idle, pending-turn,
  active-steering, terminal-history-finalization, compaction, or stopping state. It retains the
  accepted-order high-water mark, selected route-generation/head revision, and exact checked `u64`
  live-route accounting so writer admission never scans historical accepted inputs and logical
  backlog size never becomes a memory ceiling. A proven-terminal publication enters the named
  finalization state atomically; only exact durable completion of the bounded item and selected-
  transcript convergence returns the gate to idle.
- A compacting gate names one caller-generated compaction-operation nonce and the matching
  parentless provider-operation turn. The selected record keyed by thread and nonce retains one
  exact admission `BerylHomeId` and one monotonic closed lifecycle: admitted, one exact dispatch
  claim, independently ordered request disposition and provider observations, and one consumed
  success, failure, cancellation, or authority-loss disposition. Its distinct caller-generated
  attempt nonce is not a backend idempotency key and can authorize at most one
  `thread/compact/start` dispatch.
- Compaction admission requires an idle gate with zero effective accepted-next work and one exact
  valid exclusive binding. It atomically creates the provider-operation turn, provider-operation
  execution snapshot, record, and gate without changing the committed tail, selected path,
  current draft, represented-prefix proof, or native CAS model-turn count. The valid binding does
  not become an ordinary active binding; the selected compaction record is the exclusive remote-
  operation authority until consumed.
- Compaction observations may one-way publish the exact CAS turn, context-compaction item marker,
  ordered thread-status frontier, item lifecycle, and terminal independently of request
  acknowledgement while preserving their source order. Before CAS-turn publication, exact
  thread-scoped active status is correlated only through the exclusive compaction target. After
  publication, every turn-scoped observation must match that identity. Only a matching successful
  terminal after completed marker lifecycle is compaction success. For a successful
  `ContextCompaction` provider-operation turn, that record's matching terminal witness is canonical
  terminal source authority and no duplicate ordinary `TurnEnded` source event is stored.
  Validation permits a complete zero-source-event turn only when one exact compaction record names
  the turn and its terminal status and recorded turn-state revision agree exactly; any missing,
  duplicate, or disagreeing authority is corruption.
  Provider-operation item finalization is bounded but does not rebuild or advance the selected
  conversation transcript. Generic ordinary terminal and pending-turn mutations reject the
  compacting gate.
- A successful manual settlement consumes the record and releases the exact gate to idle. A
  successful lifecycle-continuation settlement performs one serialized choice: if effective
  accepted-next work exists, it consumes the continuation and releases the gate; otherwise it
  creates one pending conversation turn with `BerylLifecycleContinuation` origin and one canonical
  user-role item referencing the exact ownerless sealed fixed-text content and empty asset proof.
  The turn and item identities are domain-separated derivations from the durable record's admission
  home identity and compaction operation. Settlement cannot supply or replace that home identity,
  and the current draft remains unchanged. Writer ordering decides a concurrent admission without
  duplicating either input.
- Every compaction consumption atomically writes one independently keyed immutable settlement
  receipt containing the exact source and successor operation revisions, complete source and
  successor gate snapshots, and chosen settlement. The consumed operation independently commits to
  the complete canonical receipt. A continuation receipt also fixes a parent and selected path that
  must be rederived by appending the continuation to the immutable admission snapshot path, plus
  its initial unbound binding and content reference. Reopen and late-response reads must
  authenticate the operation/receipt pair and the concrete settlement-specific durable successor
  before accepting a later current gate or continuation lifecycle as a descendant; monotonic
  current revision or a self-consistent receipt alone is not historical proof.
- One selected route-generation state retains the exact binding revision, execution snapshot,
  Syndic turn, CAS thread, and known-or-explicitly-unknown CAS turn for every member in that
  generation. The resolved accepted-input view obtains that proof through the generation and its
  exact leaf; the proof is not copied into every accepted-input record. CAS-turn publication changes
  one generation head. Projection loss reclassifies ready/retryable leaves as next-turn and
  delivering leaves whose provider acceptance remains possible as delivery-unknown by one compact
  head transition. When the CAS-live system supplies exact rejection proof for one named delivering
  request while retiring an unconfirmed target, that same atomic transition rewrites only the named
  leaf to next-turn; every other delivering leaf remains delivery-unknown. Neither form rewrites or
  materializes the complete generation. Missing CAS identity is never inferred from process-local
  state.
- A stopping gate carries its caller-supplied operation nonce and, with the one matching live
  stop-operation record keyed by thread and nonce, forms one durable invariant. The record retains
  the exact blocked Syndic and CAS operation target, monotonic revision, fixed first-publication
  revision for every present member of the closed cause set, and an optional dispatch-claim witness
  containing its exact source revision and attempt identity. Causes present at admission name the
  first record revision; every later cause names the one revision that first published it. An exact
  later owner monotonically adds its cause to that same operation. The gate controls input
  admission and scheduler exclusion; the live record controls interruption. Neither half
  independently authorizes a backend request. Consuming live authority retains the complete cause
  and claim provenance plus an inert exact terminal, safe-reopen, or abandonment successor witness.
- Stop transition provenance is a fixed closed ledger rather than an event log. Admission is
  revision one. Every later record revision is accounted for exactly once by one newly published
  cause, the sole dispatch claim, or the consuming disposition; the occupied transition revisions
  are contiguous, nonduplicated, and never overwritten. Canonical validation rejects gaps,
  duplicate transition revisions, a cause whose first-publication revision is later than the
  record, a claim whose source does not immediately precede its publication, or a consumed witness
  whose source is not the exact preceding live revision. This bounded provenance lets an earlier
  cause join or dispatch claim reconcile exactly across every later compatible descendant without
  confusing two histories that have the same final cause set, attempt, and current revision.
- Stop admission is one atomic target-operation election. For an ordinary active target it requires
  no delivering member in the selected steering generation, publishes the stopping gate and stop
  record, removes that generation from ready steering, and changes its admitted or retryable
  members to effective next-turn work under the shared `Stop` route authority without scanning
  leaves. For a compaction target it instead requires the exact published CAS turn and live
  compaction record, changes that record to name the stop, and publishes the gate/stop pair without
  creating a steering generation or changing existing `NextTurn(Compaction)` routes.
- A provider-operation stop admission witness retains the exact source compaction revision and its
  immediate `Stopping(stop nonce)` successor revision. Every live, reopen, safe-reopen, terminal,
  and abandonment read cross-authenticates that transition; the stop nonce alone is insufficient.
  Provider-specific safe-reopen, matching-terminal, and abandonment witnesses each retain their
  own exact source `Stopping` revision and immediate compaction successor as the next link in that
  chain. A later current operation is accepted only through its ordered request/provider evidence,
  and an abandonment receipt must equal the retained source and successor revisions exactly.
- Input accepted after stop admission belongs to a later ordered route generation and remains
  ordinary accepted-input history. The stopping gate's exact target selects
  `NextTurn(Compaction)` for a provider-operation stop and `NextTurn(Stop)` for an ordinary stop;
  input never becomes draft state, joins a stopped generation, or receives a replacement identity.
- Before any backend stop byte may be issued, one mutation changes the exact stop record from
  admitted to dispatch-claimed with a caller-generated attempt identity and the exact live source
  revision that the claim consumed. Exact reconciliation uses that immutable claim witness across
  later cause joins or consumption; it may never invent another attempt or infer dispatch from
  diagnostic text.
- Only a local failure proven to precede every request byte, while the same exact target authority
  remains current and no interrupting-approval cause is present, may atomically consume the stop
  record through a target-kind-specific safe reopen. An ordinary target restores active steering
  with a fresh empty generation. A compaction target restores the exact `Compacting` gate and live
  compaction-record relationship with no steering generation and leaves every accepted route
  `NextTurn(Compaction)`. Reopening controls never retroactively steers queued input. Interrupting
  approval makes even local nondispatch an abandonment disposition because the post-denial stop
  obligation cannot be waived.
- A source-pinned provider rejection may prove that no core interruption was enqueued without
  proving that the requested target remains current. That disposition cannot safely reopen. Absent
  an already observed matching terminal, CAS-live uses the ordinary stop-abandonment mutation to
  retire the uncertain projection, preserve next-turn work, and publish source-less incomplete
  convergence.
- Once stop dispatch is possible, the stopping gate and claimed record remain until exact terminal
  evidence or projection-authority loss wins the target-operation election. A response accepting
  interruption is not terminal history and cannot release the gate. No timeout, malformed
  response, transport loss, or restart turns the claimed operation into retryable interruption.
- Matching ordinary terminal publication consumes the record's live authority and enters ordinary
  terminal-history-finalization while preserving all next-turn work. Matching provider-operation
  terminal publication consumes the stop record into the compaction terminal successor, restores
  the compacting gate solely for dedicated provider-operation finalization, and preserves
  `NextTurn(Compaction)` routes. If projection authority is lost first, one atomic target-specific
  abandonment consumes the stop and live operation, retains its receipt, retires the projection,
  preserves next-turn work, and publishes the applicable source-less incomplete lifecycle.
  Startup performs abandonment for admitted or claimed stop records because the old foreground
  connection generation cannot be recovered.
- Reopen validates gate, stop record, target, state, complete cause and claim transition
  provenance, route-generation authority, and aggregate counters together. A missing half, target
  disagreement, gapped, duplicate, future, or otherwise impossible transition provenance, or stop
  generation that still exposes ready or delivering work is corruption, never repair or replay
  authority.
- Every mutation names expected revisions for the correctness-sensitive records it reads or
  changes. Draft admission fences both thread and draft; accepted-input promotion leaves the draft
  untouched and fences the thread, gate, route, binding, and current-draft reverse binding needed
  for one selected-path advance. A conflict rejects the whole mutation rather than creating
  competing same-thread children.
- Different threads may submit distinct children from the same historical turn without conflict.

## Intrinsic Thread Properties

- Every named thread has exactly one immutable execution record keyed by its Syndic thread id. It
  stores the exact runtime id, configured root id, and canonical runtime-native root path accepted
  at creation. Every ordinary or child creation publishes it atomically with the thread, draft,
  history summary, attributes, usage seed, initial compact catalog summary, and initial unbound CAS
  projection. Child creation inherits the exact parent execution binding. No rebind mutation
  exists.
- CAS binding and execution-snapshot records retain execution copies as projection provenance, but
  the immutable thread execution record is canonical. Creation, CAS-binding publication, reopen,
  and recovery reject a missing or orphan execution record or any valid, active, stale, or snapshot
  copy that disagrees with it.
- A separate revisioned thread-attributes record owns the optional accepted generated title and
  automatic archive lifecycle. Generated title acceptance is one-way and records the exact source
  user turn, sealed content identity and digest, selected-path digest, thread revision, and caller-
  supplied generation time that proved eligibility. Archive state is exactly
  ordinary, open branch discussion, or archived branch discussion; only exact successful handoff
  can publish the open-to-archived transition.
- A separate revisioned thread-usage record owns the latest exact selected-thread token observation:
  nonnegative last and total counters, optional positive model context window, observation time,
  and the exact immutable `ExecutionBinding`, binding revision, CAS thread, managed-process
  generation, loaded-thread generation, connection generation, and monotonic provider-control
  ordinal. Publication requires that route to be the thread's current valid or active binding and
  rejects a stale ordinal or any wrong-thread or wrong-binding observation. Usage churn advances
  neither the thread revision nor
  title/archive revision.
- `HistorySummaryRecord` remains the authority for selected tail, selected-path digest, captured-
  history completeness, and last activity. It stores no title text. A title-summary builder reads
  canonical user content through bounded Syndic cursors according to the exact algorithm in
  `doc/features/conversation-threads/design.md`.
- A rebuildable compact thread-catalog summary resolves accepted-generated over history-derived over
  absent title, and copies only the resolved title/source, immutable execution binding, automatic
  archive state, last activity, completeness, bounded lineage facts, and exact source revisions.
  Its own projection revision and source fence make staleness explicit. Beryl may copy that summary
  into its catalog but never treats the copy as intrinsic authority.

## Turn Parentage And Replacement

- A submitted turn has zero or one immutable parent.
- A provider-operation turn is always a parentless ownership root and never becomes a thread's
  committed tail, selected-path member, or conversation-turn parent. A Beryl lifecycle-
  continuation turn is instead parented to the then-current committed tail and carries explicit
  non-user origin.
- Every non-root turn also retains one deterministic immutable ancestor skip. Storage validates
  the exact target on reopen and uses it to answer selected-path membership with constant memory
  and a fixed `u64`-domain work ceiling; it never materializes a whole path for this proof.
- Provider events may update turn-owned items, source records, status, projections, and metadata only while the turn remains live and before proven-terminal publication. They never create, remove, or restore parent edges and never rewrite finalized history.
- Replacement editing creates a new turn from the edited turn's parent and moves only the selected thread's committed tail and current-draft binding to that new path.
- A replacement target is always an ordinary user-authored turn. Provider-operation turns such as context compaction cannot carry replacement-edit intent.
- Replacement-edit intent is a typed durable current-draft fact. It names the exact target turn and selected-path proof separately from mutable composer content, survives restart, and is removed explicitly on cancellation or consumed by accepted replacement submission.
- Original turns and descendants remain unchanged and may still be reachable through another thread.
- Submitted turns stopped by the user, disconnected, interrupted, or recovered without a proven terminal event remain durable with explicit lifecycle state.
- Beryl exposes no named-thread deletion workflow. Unreachable turns, items, resources, projections, and provider identities remain durable until the future explicit garbage-collection design.

## CAS Live Source Boundary

- Codex App Server may feed Syndic through live turn-start and turn-stream events.
- The ingestion boundary normalizes provider traffic into an exact per-turn sequence of turn
  activation, typed item start, typed coalesced item delta, typed item completion, and
  status-only turn-ending events whose exact execution outcome is independent from an optional
  typed history-incomplete reason. Externally sourced item events retain the exact CAS thread, turn,
  and item tuple; missing or mismatched members reject the whole event. Every observed public item
  in the pinned CAS contract receives an exact closed typed provider representation plus its
  canonical narrative/resource policy, or exact correlation with already durable submitted input.
  A fieldless activity projection is not a substitute for public provider fields. Unknown,
  malformed, or unresolved history-relevant input receives a typed unsupported-history outcome that
  prevents history-complete publication without rewriting the provider outcome.
- A provider-created item owns one versioned typed content stream containing immutable start, delta,
  and completion frames. Bounded records refer to exact sealed frame ranges and digests; arbitrarily
  large strings, vectors, maps, and structured-value leaves remain in bounded chunks. Each sealed
  frame also retains bounded resumable lifecycle state—provider identity and kind, next frame
  ordinal, original start timestamp, completion state, and cumulative history support—so admitting
  the next frame never requires rescanning the complete item stream.
- Transcript-visible provider text uses a separate selected narrative view whose spans reference
  exact ranges in that same typed content stream and never copy the text. Start and delta frames
  extend one item-owned append generation in logical order. For `AgentMessage` and `Plan`, the
  completion frame is an equality fence rather than another narrative source: its normalized text
  must equal the complete append generation byte-for-byte under bounded comparison. An equal field
  may reuse already stored ranges without copying it. The selected view retains exact span and
  logical-byte frontiers plus one chain digest over ordered span provenance and logical ranges;
  frame-local structural spans are validation evidence, not a competing durable presentation
  authority.
- Provider completion is authoritative for completion-only and non-narrative final public fields,
  while the live append sequence remains sole narrative authority. Storage cannot publish the item
  completion until its exact frame is sealed, structurally valid, kind-consistent, and its narrative
  equality result is durable. A mismatch retains both provider evidence and the selected live
  narrative, records typed history incompleteness, and never replaces text. Staging remains
  unpublished across a cut.
- Structured MCP and dynamic-tool values use a closed recursive typed value algebra, never raw JSON
  or an opaque byte payload. Submitted-user provider correlation retains provider metadata and an
  exact checked reference to the already sealed user content rather than storing user bytes twice.
- Item lifecycle is variant-specific. An exact completion-only item is valid when its normalized
  kind permits that shape; a typed delta must agree with the already established item kind before
  storage can mutate it.
- Exact replay at an occupied sequence is recognized as already admitted, different data at that sequence is a collision, and gaps are rejected. This distinction is durable reconciliation authority after an ambiguous home-store outcome.
- Beryl must not populate Syndic transcript history by querying Codex App Server historical transcript APIs such as `thread/turns/list`.
- Beryl must not reconstruct missing Syndic transcript history from stale GUI-local projections, activity rows, rendered text, or legacy transcript caches.
- A thread that has no Syndic-captured records renders as empty, unavailable, or incomplete according to the transcript provider contract rather than falling back to Codex App Server history.
- Pinned CAS `turn/completed` is a status-only ordering fence. It can close and audit the set of item
  events already admitted from one uninterrupted full-profile stream, but it does not enumerate or
  prove that set and cannot backfill a missed item.
- An interrupted terminal under exact CAS 0.146.0 carries
  `ForcedAbortOrderingUnproven` history incompleteness unless retained release-scoped evidence
  proves an upstream no-later-item fence. Any same-target source event
  observed after local terminal publication is rejected and retires that connection rather than
  reopening the turn or mutating finalized history.
- A turn whose live stream was interrupted or lost remains durable with an explicit incomplete,
  failed, or unknown-terminal status. Unknown-terminal may accept exact late evidence only while
  the original exact live authority remains usable. Proven loss retires that authority and
  converges the retained prefix as incomplete; reconnect, resume, late subscription, process
  restart, CAS history reads, and GUI projections are not notification replay and do not repair
  that source-event sequence. Source-less incomplete publication leaves a durable
  terminal-history-finalization gate until the already captured prefix and selected transcript
  reach their exact convergence fixed point; only then is the execution block released.
- A completion/live narrative mismatch is the corresponding logical-stream failure even when no
  transport gap was reported. The exact provider completion and terminal outcome remain durable,
  the live narrative prefix remains renderable with explicit incomplete state, and no later event or
  projection may choose the completion text as a repair.

## Activity Presentation Projection

- Activity presentation is a derived, non-transcript index over already admitted provider
  lifecycle records plus exact bounded GUI-derived facts. It does not replace, summarize away, or
  duplicate public provider fields in canonical history.
- One activity-query head names the exact query owner, thread work period, root source, source count,
  aggregate source frontier, revision, logical row count, retained stored bytes, and full-order
  cutoff. One bounded source-membership record per participating turn binds its exact source-event
  interval and active or terminal lifecycle. Terminal child handoff is admitted only after the
  child's exact turn state proves terminal and the final-answer source event is immediately followed
  by that terminal event. The resulting inactive membership begins at the final-answer event and
  ends at the child's immutable terminal source frontier; its compact row must equal the
  membership's exact item and narrative range.
- Each activity entry binds one exact source event, CAS item and turn identity, provider kind and
  lifecycle, full ordering key, and bounded row payload. It does not bind a mutable item-projection
  revision. Running-first and recent ordering live in durable derived index keys, so query
  evaluation does not sort a complete row set in memory.
- Public reads require the exact head revision and return bounded cursor pages whose row strings are
  bounded source projections or compact counters. Running rows remain logically present until
  terminal state without pinning resident page bytes. Completed rows retain the deterministic
  maximal newest prefix that fits both fixed row and stored-byte budgets; a smaller fitting prefix
  is not a valid alternative state.
- Provider publication advances affected canonical state, exact source membership, and activity
  index state atomically. Reopen proves current and retired memberships against their exact turn
  state and terminal frontier in constant memory; a widened interval or retired active member is
  corruption. Rebuildable projection staleness is explicit, and no reader observes mixed activity
  generations.
- Restart may make a prior process-period activity scope ineligible for feature display, but its
  underlying canonical provider records remain ordinary Syndic authority. Activity rows never
  become parent transcript narrative or a persistent activity log feature.

## Canonical History And Projections

- Canonical Syndic history records the source events and normalized canonical items needed for replay, export, diagnostics, and projection rebuilds.
- A delivery-unknown accepted steering fragment remains user-authored durable accepted-input
  history. Its retained sealed content, accepted order, and unknown-delivery provenance do not
  claim that the provider observed it. Transcript and recovery projections may represent that
  retained user history through their exact accepted-input boundary, but may not execute the
  fragment again automatically or fabricate a provider-sourced canonical item.
- Canonical item records contain bounded metadata, exact content-manifest references, and an item-local source-event frontier. Ordered canonical content chunks remain source authority for arbitrarily large user, assistant, or operational text; projections and resources are derived presentation boundaries rather than substitute canonical storage.
- Reopen replays each item's indexed source sequence in bounded memory and requires exact agreement with its kind, assistant phase, external identity, content bytes, chunk chain, and completion/finalization state.
- Transcript projections are derived from canonical history and must preserve stable provenance back to Syndic turn, item, source range, projection, and resource identities.
- Source events may advance only before a proven-terminal lifecycle is published; unknown-terminal state may still accept exact late evidence. Canonical finalization and projection work may advance afterward only from source events that were already admitted.
- Provider activation, output, item completion, and successful turn completion always retain exact
  provider identity through ordinary source-event history or a dedicated provider-operation
  observation record. A complete source-free `ContextCompaction` turn is authorized only by the
  exact matching compaction terminal witness and is not source-less convergence. Otherwise a
  source-less event may only converge a locally interrupted, failed, incomplete, or unknown-
  terminal turn after its projection is stale or the thread is unbound. A still-usable valid
  binding represents only the pending turn's parent and cannot authorize local selected-path
  advancement. Source-less convergence never fabricates provider activity or success.
- Turn lifecycle records the exact provider or local execution outcome. Captured-history
  completeness is a separate optional typed disposition: a provider-complete turn may remain
  history-incomplete because an observed item is open, unsupported, malformed, or otherwise cannot
  be finalized. Later projection or asset work may advance only the derived completeness frontier;
  it never changes the recorded execution outcome.
- Publishing a proven-terminal lifecycle closes ordinary source-event admission. Any accepted event suffix not yet represented by a complete derived frontier makes that frontier stale in the same durable transition.
- The turn state retains both its complete item frontier and a contiguous finalized-item frontier. Terminal completion may advance that finalized frontier one item at a time without inventing a completion event or accepting new source bytes.
- Provider lifecycle completion and canonical resource finalization remain distinct. A generated
  media item can retain exact provider completion while a typed pending-asset disposition keeps its
  finalized-item frontier and history completeness behind; later asset admission resolves only the
  resource/finalization boundary and never rewrites the provider event.
- Projection construction consumes one closed exact source: sealed composer content for submitted
  user input, or one selected provider narrative generation for transcript-visible provider text.
  Provider parsing walks the narrative-span index directly and reads only the referenced
  ProviderItemV1 ranges; it never replays the item stream or materializes a cumulative item value.
  Source advance atomically makes the selected item projection stale and supersedes incomplete
  work, while completed older generations remain coherent historical snapshots. Consecutive source
  advances may coalesce projection work onto the latest canonical revision. A provider append may
  resume from the exact stable checkpoint of the same narrative generation. Completion seals that
  same source after bounded exact narrative comparison; it creates no new narrative generation.
  When an advance changes only correlation, lifecycle provenance, or an agreeing completion and
  retains the exact selected source, the new generation reuses the prior immutable projections,
  resources, stable
  checkpoint, and identities rather than reparsing or copying text. Freezing a closed canonical item
  and advancing the finalized-item frontier are distinct durable transitions. A transcript-visible
  item advances the frontier only after its frozen source has a current completed projection set;
  an operational item advances after freezing because it has no transcript projection.
- Once a proven-terminal turn has current turn-owned canonical items and item projections, that finalized content and its projection identities, revisions, item-local ordering, text, and resource references are immutable. Later turns, replacement edits, and branches never rewrite them.
- First-time freezing and finalization may still finish retained canonical history after a
  replacement moved the origin thread onto another branch. That work changes the retained turn,
  but it must not stale or reorder the origin thread's selected transcript or change its selected
  history summary unless bounded ancestry proof shows that the turn is still selected.
- A named thread's transcript-view head and ordered entries remain a derived selected-path index. They may advance when the thread tail changes or while unfinished projection work converges, but recomposition only selects and orders finalized historical projections; it does not rewrite them.
- Each transcript-view entry belongs to one explicit view generation. A selected-path change publishes a new stale generation without rewriting the prior generation; bounded rebuild commits populate only that generation, and one current head selects the published generation. Incomplete and superseded generations remain unreachable durable derived state until future garbage collection.
- Recovery may deterministically finish stale or incomplete projection work before finalization. It does not invalidate or reproject already finalized history in place.
- The transcript projection contains user-authored input, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text marked transcript-visible by the source, and generated media intended as assistant output.
- Operational records remain canonical history but are excluded from parent transcript narrative unless a later feature design promotes a bounded summary.
- Markdown parsing, chunking, code/table externalization, and resource reference creation are Syndic projection responsibilities. The GPUI transcript renderer consumes projection records and must not parse raw provider Markdown.
- Projection construction is a bounded durable pipeline rather than one whole-item parse. Each
  item-projection generation records its exact canonical source revision, consumed-byte frontier,
  closed-block frontier, parser state, output frontier, and lifecycle. One construction step reads
  and writes bounded pages only; interruption leaves a resumable or superseded derived generation,
  never a partially current transcript.
- Canonical content chunks have exact encoded-byte indexes plus closed logical-source span indexes.
  `ComposerV1` text spans skip framing and image-marker bytes, while a selected provider narrative
  view maps its own logical range to exact ProviderItemV1 frame ranges.
  Every content reference snapshots exact chunk, ordered-piece, encoded-byte, and logical-text
  frontiers. End-of-input is reached only when both referenced logical text and referenced pieces
  are exhausted, so trailing and marker-only zero-width image pieces remain visible without
  allowing a later live append under the same content id into an older projection generation.
  Textual code and table resources retain immutable logical UTF-8 ranges plus bounded structural
  metadata and preview ranges. Resource reads resolve only the requested indexed range and never
  assemble the complete canonical item or resource as a prerequisite.
- Public transcript/path/membership pages are capped at 256 records and 65,536 stored encoded
  bytes. Textual-resource reads return at most 65,536 requested bytes with an exact continuation.
  These are practical query working-set limits, not history or turn-size limits.
- Image, attachment, and other externally owned byte payloads may use Beryl-home sidecars under
  their owning feature contracts. Phase 7 does not copy canonical Markdown text into sidecars or
  require whole-resource sidecar admission.
- Every transcript generation has explicit bounded path-collection and entry-publication state.
  Path collection walks immutable parents tail-to-root into generation-owned depth records;
  publication then walks those records root-to-tail and assigns contiguous stable positions. Only
  the completed generation becomes the selected current head.
- A generation is bound semantically to its exact committed tail and selected-path digest. Its
  captured broad thread revision is a monotonic lower bound, so draft rotation or accepted-input
  admission may advance that revision without rebuilding or superseding unchanged selected
  history. Active and completed generations remain authoritative only while the captured revision
  is not from the future and the exact tail and digest still match.
- Queued accepted-input admission preserves an in-progress or completed generation when the
  selected tail and digest are unchanged. It advances the current history-summary revision and
  draft activity while preserving derived completeness. Only a selected-path or canonical source
  change invalidates or supersedes the generation.
- A live canonical item may publish coherent projection generations while its selected provider
  narrative remains the same append generation. Immutable projection and resource identities and
  revisions exclude generation. One generation-independent membership range owns the item's
  immutable closed prefix; a generation-owned membership range owns only the provisional
  end-of-input suffix for that live source snapshot. Sets, optional resumable builds, and heads
  select a coherent merged logical range. Closed block groups are reused by exact reference without
  replaying from byte zero, while the open trailing group may be superseded after later appended
  source. Agreeing completion seals the existing append source and promotes its exact end-of-input
  output to the stable range without replacing closed groups.
  Once the turn is proven terminal and its current projection generation is published, all
  selected projection and resource records are finalized immutable history.
- Projection format V1 uses bounded GFM block recognition. Recognized blocks retain typed structure
  and exact source ranges. Undecidable, malformed, unsupported, or deliberately bounded-out syntax
  becomes ordered source-preserving fallback spans; this may reduce styling locally but never
  drops bytes, changes authored text, or expands memory with the complete item.

## Execution And Policy Boundary

- Codex App Server is the execution and policy authority for every executable turn.
- CAS execution retains Codex authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandbox behavior, approval policy, skills, MCP, subagents, rate limits, and tool execution.
- Syndic storage and projection code must not broaden or bypass CAS policy decisions.
- Syndic never becomes an agent-execution provider; unmodified out-of-process CAS remains the sole execution and policy authority.

## Persistence Boundary

- Syndic durable history is not Beryl GUI-local settings and is not a bounded resident presentation cache.
- Syndic records occupy a private logical keyspace family inside the one physical Beryl-home database defined by `doc/systems/beryl-home-storage/design.md`.
- Syndic storage APIs receive opaque domain-scoped access from the home-store boundary and never expose Fjall handles, key encodings, or transactions to callers.
- Syndic storage must never persist access tokens, refresh tokens, API keys, bearer headers, cookies, or app-server loopback capability tokens.
- Durable source events and projections must redact or reject protocol fields that are secrets or policy-private control data.
- Unfinished or stale derived projections can be rebuilt or invalidated from canonical history plus resource metadata. Finalized projections remain durable exact history.
