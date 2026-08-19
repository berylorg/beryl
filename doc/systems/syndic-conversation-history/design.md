# Goals

Define Syndic as Beryl's durable thread, draft, conversation-history, projection, reference, and replay system for agent work captured by Beryl.

Let Beryl render selected conversation history from Syndic-owned storage and projections while retaining Codex App Server as the live execution, auth, sandbox, approval, skill, MCP, and enterprise-policy authority.

Keep canonical history, transcript-view records, Markdown projections, and resource metadata below the GPUI transcript presentation stack.

## Non-goals

- Replacing Codex App Server authentication, model execution, sandboxing, approval handling, skills, MCP, subagents, managed configuration, rate-limit handling, or enterprise policy enforcement.
- Importing, cataloging, or backfilling Codex App Server history. The sole exception is the exact
  correlated terminal-turn repair snapshot defined by the CAS-live system.
- Treating Syndic storage as a cache over Codex App Server history.
- Owning Beryl runtime/root registries and availability, window claims, session layout, durable host
  jobs, or catalog query indexes.
- Garbage-collecting turns, resources, or projections that are unreachable from named threads.
- Rendering operational activity, raw reasoning, command logs, or tool internals as parent transcript narrative.
- Storing OpenAI, ChatGPT, Codex, or app-server authentication secrets.

# Decisions

## Documentation Set

- [Syndic concepts](concepts.md) is the normative supplemental Syndic domain model. It is authoritative for current vocabulary
  and accepted model statements about turns, threads, turn items, canonical messages, Markdown
  projections, Syndic references, heavy item references, lazy history access, replay, and stop
  operations. This primary design owns Syndic terminal-repair concepts, snapshot storage and
  selection, `FinalizingHistory`, and gate-release lifecycle.
- `doc/systems/cas-live-syndic-transcript/design.md` owns CAS-live source ingestion, repair
  eligibility and correlation, the sole historical-request authorization and adapter, and the CAS
  projection system contract.
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
- A current draft is a durable typed pre-submission metadata record with stable id and selector
  revision, one exact immutable published combined draft-root reference, and one closed submission intent: ordinary,
  branch-context, or replacement. The reference binds both the composite sequence piece-tree root
  and the persistent marker-identity-index root as one revision. Sequence leaves are UTF-8 text or
  zero-width image markers; each marker includes its stable identity, same-anchor order, and final
  label ordinal, while Beryl asset references resolve that marker to bytes.
- An ordinary current draft has no parent or branch context. It is only mutable unsent composer
  state and remains independent of later committed-tail movement. A branch-context intent names
  the exact immutable envelope whose source turn is the branch point; a replacement intent names
  its exact target and selected-path proof. The closed intent makes those cases mutually exclusive.
- Beryl transcript presentation may derive one synthetic readonly context group from that immutable envelope at the branch boundary. The group remains presentation-only and keeps stable semantic identity when first submission transitions the context-owning draft into a submitted turn.
- Only the draft's published combined root, selector revision, and mutable timestamps change during
  autosave or flush publication. Thread
  owner and submission intent change only through their explicit typed operations; ordinary
  selected-path parentage is not draft state.
- A branch-context or replacement draft may exist only with an idle gate and zero live accepted
  work. Promotion's idle gate with positive next-turn work therefore proves that the untouched
  current draft is ordinary without reading or fencing its record.
- Draft edits stage immutable combined sequence/index successors through bounded commands inside one
  explicitly identified durable editor-candidate session. One atomic candidate-adoption command
  writes the immutable settlement and root and advances only that session's newest-candidate head;
  widget `Committed` means this exact adoption, not current-draft publication. A separate autosave
  or flush publication may later select an eligible candidate as the durable current draft.
  Incomplete admitted staging remains unreachable from draft reads but stays under its session's
  active-operation custody until terminal settlement. Cancelled, conflicting, or superseded work
  retains immutable terminal closure. Only never-admitted, never-claimed staging from a cleanly
  disposed session is an orphan candidate for future garbage collection.
- A physical record or chunk is bounded, but one logical draft has no fixed whole-content byte ceiling. Storage preserves a draft even when later CAS request assembly rejects it against an exact provider/model limit.
- An idle-thread submission first flushes the active editor candidate session, then requires one
  sealed `ComposerV1` materialization proven to come from the exact published combined root selected
  by that flush. Its atomic publication validates that root-bound
  materialization and the sealed asset-set proof against the combined root's sequence marker
  digest/count and identity-index count, then selects the then-current committed tail as the
  ordinary turn's parent under the exact thread,
  draft, root, and input-gate revisions. It transitions the same draft identity to a submitted
  turn, creates one typed canonical user-input item referencing the sealed content and set proof,
  advances the thread tail, and creates its replacement current draft. Replacement
  submission instead derives the new turn parent from its explicit replacement target. First
  branch-discussion submission derives its parent from the exact source turn in the draft-owned
  context envelope and requires that source to agree with the thread's selected branch point.
- First acceptance of draft markers in either idle or queued/steering admission atomically advances
  the owning thread's permanent image-label frontier and creates at most one immutable local origin
  span, regardless of marker or newly reserved label count.
  Later delivery transitions reuse that authority and never maintain separate label collections.
- The draft-to-turn transition preserves the draft's exact 128-bit identity payload while changing its typed identity from `SyndicDraftId` to `SyndicTurnId`; it does not allocate an unrelated submitted-turn identity.
- A context-bearing draft's owner descriptor follows that same deterministic typed transition while its immutable context envelope remains byte-for-byte unchanged.
- Context admission proves that the source turn was on the named source thread's selected path. Scoped context resolution does not require that turn to remain on the thread's later mutable selected path; replacement edits may move the named thread tail while the immutable context source remains durably referenced.
- After first submission, the context envelope remains owned by that deterministically typed first submitted-turn identity. A later replacement path in the discussion may move away from that turn without moving, deleting, or invalidating the thread's stable context owner.
- Input submitted during an active turn or compaction likewise flushes first, requires the exact
  published combined root's sealed `ComposerV1` materialization, and is atomically frozen into one immutable ordered
  accepted-input record referencing that sealed content, compact head of its immutable
  paged marker-reference set, one route-generation identity, and the complete source
  thread/draft/root/gate revision plus source/replacement draft admission proof. The record is the
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
  active-steering, `RepairRequired`, `FinalizingHistory`, compacting, or stopping state. It retains the
  accepted-order high-water mark, selected route-generation/head revision, and exact checked `u64`
  live-route accounting so writer admission never scans historical accepted inputs and logical
  backlog size never becomes a memory ceiling. A proven-terminal publication enters the named
  finalization state atomically; only exact durable completion of the bounded item and selected-
  transcript convergence returns the gate to idle.
- A `RepairRequired` gate names one exact correlated terminal turn, its capture-gap provenance, and
  its durable target-scoped request disposition, initially `Available` and permanently `Consumed`
  after the sole claim. It blocks same-thread successor promotion, fork, replacement execution,
  rollback, and compaction until either one complete repair snapshot head is selected or explicit
  incomplete authority is fixed, the chosen outcome enters `FinalizingHistory`, and bounded
  finalization releases the gate. It never blocks unrelated threads.
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
  its initial unbound binding and content reference. Scoped operation reconciliation and late-response reads must
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
  For a terminal successor, that durable consumed record's exact cause, claim, and successor
  witnesses are the sole authentication for delayed finalization release; no process-local state
  participates.
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
  immediate `Stopping(stop nonce)` successor revision. Every live, scoped reconciliation, safe-reopen, terminal,
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
  `FinalizingHistory` while preserving all next-turn work. Matching provider-operation
  terminal publication consumes the stop record into the compaction terminal successor, restores
  the compacting gate solely for dedicated provider-operation finalization, and preserves
  `NextTurn(Compaction)` routes. If projection authority is lost first, one atomic target-specific
  abandonment consumes the stop and live operation, retains its receipt, retires the projection,
  preserves next-turn work, and publishes the applicable source-less incomplete lifecycle.
  Recovery invalidates the prior foreground connection and every handle or process authority from
  that service generation. A fresh service reacquires a typed Syndic handle and reads the exact
  durable thread/gate/stop natural closure before abandoning an admitted or claimed stop; it does
  not inherit or revive the old request capability.
- Scoped stop reconciliation validates gate, stop record, target, state, complete cause and claim transition
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

## Durable Draft Piece Tree, Candidate Sessions, And Marker Identity Index

- The autosave backing for a current draft is one durable immutable combined draft root, not
  `ComposerV1`. It binds a copy-on-write composite sequence piece tree and a copy-on-write marker-
  identity index keyed by stable marker id. Sequence leaves contain either nonempty UTF-8 ranges or
  one zero-width marker; bounded sequence nodes retain child extents, disjoint composite search
  envelopes, and integrity summaries. Those envelopes authenticate one bounded descent to a byte
  boundary, exact sequence marker, or adjacent-marker gap even when an arbitrarily long marker run
  consumes no UTF-8 bytes. Unchanged subtrees in either structure are shared by reference between
  candidate roots.
- The marker-identity index maps each stable marker id only to stable occurrence facts: final label,
  same-anchor order key, and sequence marker-leaf identity and digest. It stores no absolute UTF-8
  anchor, composite position, sequence ordinal, or rebased location. Its immutable bounded-fanout
  nodes commit disjoint stable-id search envelopes, checked record counts, child identities, and
  child digests; its bounded leaves commit those occurrence facts. A caller that needs to validate
  location supplies the exact composite position or anchor witness; storage authenticates the id
  lookup and then performs one bounded sequence descent at that supplied location to verify the
  named occurrence. ID-only location discovery is not required. Sequence ordering summaries are
  never treated as global marker-id absence authority.
- Every combined root has one closed build identity. Direct canonical-empty creation is scoped by
  draft and its deterministic empty operation; every editor-built candidate, including a streamed
  sealed-content import, is scoped by exact draft, editor session, and operation. Its canonical
  summary binds the sequence
  root identity, height, logical UTF-8 length, logical line count, ordered-piece count, marker
  count, ordered marker digest, and sequence digest together with the identity-index root identity,
  height, record count, and identity-index digest. A zero-byte sequence has zero logical lines; a
  nonempty sequence has its checked newline count plus one, including a final empty logical line
  when its last byte is newline. Every leaf and internal-node summary commits checked byte,
  newline, and derived line aggregates, and every node, sequence-root, and combined-root digest
  commits those summaries. One domain-separated combined-root digest commits both root summaries.
  Readers and mutations bind to the complete combined reference and summary; a current-draft
  selector revision and an editor-candidate revision are distinct authorities. Either detached root,
  a detached digest, or a byte offset detached from that exact root is not authority.
- Canonical hashing uses SHA-256 and the exact ASCII domains
  `syndic/draft-sequence-root/v1/empty`, `syndic/draft-ordered-marker-fold/v1/empty`,
  `syndic/draft-marker-identity-index-root/v1/empty`, and `syndic/draft-combined-root/v1`.
  The package encoding contract defines their exact length-prefixed preimages. The canonical empty
  combined root has no sequence or identity-index root node, zero heights and zero
  byte/newline/line/piece/marker/index aggregates, the three canonical empty component digests, and the combined
  digest over both empty summaries. The combined-root digest always hashes the complete canonical
  sequence and identity-index summaries, including for empty, text-only, and marker-only drafts.
- Every newly created canonical empty draft uses the package-owned deterministic empty-root-build
  operation identity derived from its stable draft id. That identity applies to initial thread,
  replacement-draft, and other direct empty-draft creation; an edit whose successor happens to be
  empty retains its caller-owned edit operation identity qualified by its editor session. Each
  closed root-build identity owns its exact root record and reference; shared empty digests are content integrity, not cross-
  draft owner or selector-revision identity. Zero sequence markers require the empty identity index even when text makes
  the sequence tree nonempty; any marker-count/index-emptiness disagreement is corruption.
- A storage-neutral composite position is one logical UTF-8 byte offset plus a constant-size gap
  witness at that anchor. The witness proves the before-all edge, one exact adjacent-marker pair,
  or the after-all edge in the selected root, so positions before, between, and after any number of
  same-anchor markers remain exact without assigning source bytes to markers. A bare byte offset is
  valid only when no gap choice is ambiguous. This contract carries no GPUI or widget type.
- One draft edit transaction names the exact draft, editor-candidate session, predecessor candidate
  revision and combined root, operation
  identity, and a sequence of half-open composite replacement ranges. Ranges are strictly ordered and
  non-overlapping in the predecessor composite order; adjacency is permitted, every endpoint witness is
  validated against the predecessor combined root, and all replacements are interpreted simultaneously
  against that predecessor rather than shifted by earlier replacements. Inserted UTF-8 and marker leaves arrive
  as bounded ordered fragments, marker moves are explicit removal-plus-insertion operations, and
  the envelope names exact intended successor caret and selection positions or one exact mapping
  for its captured base endpoints.
- Two empty replacement ranges at the same composite position are invalid. Callers coalesce their
  inserted fragments into one replacement before staging; storage never invents a tie-breaker or
  lets fragment arrival order choose the successor. Other adjacent non-overlapping ranges remain
  valid.
- An ordinary insert or reinsert proves stable-id absence in the predecessor by authenticated identity-
  index descent. Removal supplies its exact base composite position or anchor witness, proves
  identity-index presence, then verifies label, order key, marker-leaf identity, digest, and exact
  occurrence through one bounded predecessor-sequence descent. A move supplies both predecessor removal and
  successor insertion positions, removes the exact base occurrence first, proves absence in that
  removal-applied index, creates one successor marker occurrence, and replaces only that identity's
  index leaf. Text insertion before any unchanged marker suffix changes no identity-index record.
  These checks and copy-on-write updates take work proportional to the two bounded structure
  heights and never scan the complete sequence tree.
- Building a successor walks only the affected predecessor ranges and bounded paths in both structures,
  writes new leaves and path-copied nodes in bounded steps, and reuses every unaffected subtree.
  The compact mutable build head retains counts, proposal commitments, progress frontiers, and the
  exact latest progress-receipt key and digest only; it is not authenticated by a mutable self-hash.
  Bounded immutable `draft-piece-build-fragments` records hold only canonical replacement,
  inserted-piece, removal, insertion, and move-reconciliation facts. Neither record form embeds or
  retains the whole edit or payload. Construction never reconstructs, decodes, or writes the
  complete draft merely because one range changed.
- Every bounded begin, canonical-fragment stage, successor-construction advance, and terminal
  election names one exact expected source receipt, with `None` valid exactly when the target has
  transition ordinal one, and one
  exact immutable target receipt plus the complete bounded effects of that command. A newly
  committed transition creates that receipt in the dedicated `draft-piece-build-progress` primary
  family. Its natural key is exactly draft, editor session, operation, and a one-based transition
  ordinal. Its canonical fixed-size value contains that key; the exact prior receipt key and digest,
  which are absent only at ordinal one; the exact authenticated canonical-proposal fragment
  endpoint, absent before fragment one and otherwise naming the one-based endpoint key and canonical
  fragment digest, plus its chain; the current phase and relational cursors; working sequence and identity-index
  roots with their complete summaries; source and successor frontiers; next record ordinal;
  optional successor root and build digest; lifecycle; and a domain-separated receipt digest that
  commits every preceding field. A mutable build head or digest detached from this receipt closure
  is not progress authority.
- Each such command compares the stored mutable build head with its named source and target and the
  session head's exact active-operation custody slot. If the
  head selects the source receipt and equals its authenticated state, the command point-reads and
  authenticates that source receipt plus every root or record referenced by the bounded transition,
  and the target receipt key must be absent.
  An ordinal-one target, whether an ordinary begin or a terminal-before-begin election, requires a
  `None` source, an absent build head, an absent target receipt, and an absent custody slot; that
  first admitted transition atomically claims the exact operation and canonical proposal identity.
  Every target ordinal greater than one requires the exact immediately preceding source receipt and
  an already claimed byte-equal custody slot.
  Any occupied target receipt in that state, including canonically byte-identical bytes, is a
  corrupt predecessor-head/occupied-next split and permits neither mutation nor success. Only an
  absent target permits one atomic creation of the target receipt and all named effects followed by
  the build-head advance.
- Exact replay is allowed only when the stored build head already selects the target receipt and
  equals the proposed target-head bytes, and that receipt plus every build, fragment, root,
  settlement, candidate-session head including its custody-slot transition, and other
  same-command effect has canonical bytes equal to the proposed target closure. Differing occupancy
  is an identity collision. A missing effect, an unrelated or later head, receipt deletion or
  replacement, a fork, a build head ahead of its receipt endpoint, or a staged-fragment or effect
  frontier ahead of the authenticated head is corruption or stale authority and fails closed; no
  command repairs such a split or reports it successful.
- Build-head and operation-status reads, reconciliation, candidate-session validation, candidate
  adoption, and settlement closure authenticate the selected endpoint receipt, its immediate
  predecessor when present, their referenced roots and records, and the session head's custody slot
  through bounded point reads. An open or complete endpoint must agree with the exact claimed
  operation/proposal identity; a terminal endpoint must agree with the one settlement that cleared
  that claim atomically.
  Reconciliation applies the same source-versus-target classification: source head plus absent
  target remains pending or proven noncommit as applicable, source head plus any occupied target
  fails closed, and target head returns committed replay only from the complete byte-equal target
  closure. They never walk the receipt chain. Candidate adoption atomically binds its settlement to the
  terminal receipt and that receipt's immediate-predecessor/root closure. Each receipt and the
  retained verification state are fixed-size, and there is exactly one receipt per bounded work
  quantum, so command work and resident state stay fixed while one logical edit remains unbounded.
- Before candidate adoption, the completed sequence and identity-index successors are cross-validated for
  equal marker count and exact stable occurrence-fact agreement across every changed marker and
  reused authenticated subtree commitment. The final revision-checked candidate-adoption command
  atomically writes the combined successor root and summary, immutable settlement proof, and the
  matching editor session's next candidate revision and newest-candidate frontier. It does not
  advance the durable current-draft selector or published frontier. Its committed result returns
  only that adopted candidate root, logical extent, and successor positions after their gap
  witnesses validate against the new sequence tree; neither structure can publish independently.
- Each editor-candidate session is keyed by exact draft and session identity and has one bounded
  durable head. It retains only its immutable durable-base selector revision/root, latest published
  candidate frontier, newest adopted candidate frontier, monotonic candidate/session and dirty
  generations, active/disposed lifecycle facts, and one fixed-size optional active-operation
  custody slot. The slot retains the exact operation id, canonical proposal identity, predecessor
  candidate generation/root, and admitted build/receipt endpoint needed for point-readable
  recovery; it contains no proposal payload or receipt chain. Candidate settlements are exact predecessor-
  linked immutable operations validated by the package. The head contains no text, marker
  collection, whole edit, undo payload, or root graph and is not current-draft authority,
  transcript state, or routine reopen authority.
- Candidate-head conflict supersedes only an edit build whose named predecessor is no longer the
  session's exact newest candidate and adopts none of that edit. Cancellation before final-command
  admission leaves the predecessor current;
  cancellation after admission cannot retract the result. Crash or an indeterminate final command
  remains nonterminal while it is reconciled from the operation identity, base and successor
  combined roots, candidate revision, and complete paired-root summary. An exact retry returns the
  already classified result only through the target-head and complete-closure check above; a
  disagreeing identity reuse can return only the occupied-identity
  `Error` proof defined below.
- Every edit operation owns one natural settlement identity derived from its exact draft, editor
  session, and operation identity, one same-tuple build and candidate-root identity, one canonical
  proposal digest, and exactly one immutable terminal settlement proof. New admission requires those
  per-session natural identities and the session custody slot to be absent. The first admitted
  transition claims that slot for this exact operation/proposal. A different operation or proposal
  while the slot is occupied is a typed same-session concurrent-operation conflict and performs no
  mutation. Continuation requires the stored build head to select
  the command's exact source receipt; replay requires it to select the exact target receipt and the
  complete same-command closure, including the slot's exact before/after bytes, to match canonically
  byte for byte. No unsettled operation in
  another session can block it. Continuation or replay also requires equality of canonical header bytes and,
  for a multi-fragment proposal, ordinal-by-ordinal canonical fragment bytes through a bounded
  fragment-chain comparison. Digests may reject inequality early but never prove equality. A
  different canonical byte at any occupied settlement, build, fragment, progress receipt, or
  candidate-root natural key proves the
  attempted proposal was not admitted and yields only
  `Error(OccupiedIdentityNoncommit)` without mutation; it never changes or replaces the preexisting
  operation. An occupied candidate-root key with disagreeing canonical bytes yields the same exact
  noncommit proof and can never be adopted. Ordinary head drift within the single-owner widget slot is a stale internal completion,
  not an external durable-base conflict: the app consumes an existing exact settlement if present,
  otherwise it settles that transaction as non-adopted `Conflict` and may re-propose the retained
  bounded logical edit only after proving exact position mapping. A durable selector/base mismatch
  instead invalidates the session for publication and is an external conflict.
- Every valid admitted edit transaction settles exactly once through the closed public outcomes
  `Committed`, `Rejected`, `Conflict`, `Cancelled`, or `Error`, and every replay that passes the
  target-head and complete-closure check returns its durable settlement proof. `Committed` returns
  the exact adopted candidate revision, combined root, extent, and
  successor positions. `Rejected` proves the envelope or a fragment was invalid before edit publication.
  `Conflict` proves a different newest candidate predecessor and absence of this operation's adoption.
  `Cancelled` proves cancellation won the terminal election before publication. `Error` proves an
  operational failure and exact noncommit. That sole terminal settlement atomically clears the
  session's matching custody slot; a missing, different, or prematurely cleared slot is corruption,
  not noncommit or replay. The latter four publish none of the proposed
  replacement.
- `Committed` writes the settlement proof atomically with both successor structures, the combined
  root, session/candidate revision and newest-candidate advance, terminal progress receipt, and
  committed build state, and clears the matching active-operation custody slot.
  It does not publish the current draft. Each no-change
  outcome atomically writes its settlement proof with the exact base/proposal facts, absence of an
  operation successor, outcome-
  specific noncommit witness, terminal progress receipt, and terminal build state. A terminal
  election before ordinary begin uses a `None` source, atomically claims the previously absent slot,
  and creates the ordinal-one terminal receipt, build, settlement, and cleared target slot in that
  same command. Terminal
  `Rejected`, `Conflict`, `Cancelled`, and `Error` builds can never resume or commit, and an
  immutable settlement can never change outcome.
- `Indeterminate` is reconciliation custody, never a public terminal edit outcome. After writer
  admission, a settlement alone cannot classify the command. Reconciliation returns its immutable
  settlement proof only when the stored build head selects the terminal target receipt and every
  same-command effect matches the target closure byte for byte. A stored source head with absent
  target leaves the operation pending while the exact terminal election is retried or completed; a
  stored source head with any occupied target fails closed and is never repaired or reported
  successful. The exact
  per-session active-operation slot retains reconciliation custody while that live session is owned.
  Fresh activation never adopts it as draft authority; an impossible corrupt closure
  keeps the storage/service fail-closed while reconciliation continues; it does not fabricate a
  sixth outcome or falsely claim noncommit. Unadmitted staging for an operation that never claimed
  the slot may remain an unreachable orphan. Once admitted, work remains claimed until its one
  terminal settlement; its immutable terminal records remain durable closure rather than
  disposable orphan authority and never become a fallback successor.
- Draft text pages, marker pages, composite-position validation, and restoration validation bind
  one exact combined root with fixed page and retained-byte bounds. An exact-root read validates
  its paired root record, summaries, requested paths, digests, and cross-structure marker facts but
  does not require it to remain the current draft root. Marker-id lookup authenticates through the
  identity index; location validation additionally requires the caller's composite position or
  anchor witness and verifies the named occurrence through one sequence descent. Ordered marker
  paging remains a sequence-tree traversal. A current-root read separately stabilizes the mutable durable draft selector
  around that exact-root traversal and returns concurrent change rather than combining selections.
  Historical combined roots remain valid. An explicit live-session read may stabilize the named
  candidate-session head and newest candidate, but routine startup and fresh activation never
  discover or adopt such a session. Restoration retains only the exact published combined-root
  binding, compact positions and gap witnesses, scroll continuation, and bounded undo/redo
  frontier; it never embeds the tree, a whole marker registry, or draft-sized inverse content.
- Fresh range-backed editor activation first stabilizes the durable thread/draft selector and exact
  closed combined-root reference, then opens one caller-identified candidate session only if that
  selector and root still match. The resulting exact session head repeats the durable-base selector
  revision and complete root-build identity and initializes its published and newest frontiers to
  that root. The active range source binds the exact draft, session, session generation, candidate
  generation, complete root reference, and logical byte/line extent returned by that open; a digest,
  draft id, or selector revision alone cannot activate it. An identical canonical open against the
  identical active session returns `ExactReplay(head)`; the same request against the identical
  disposed session returns `StaleDisposed(head)`. Reuse of that session identity with different
  canonical request or base bytes returns `OccupiedIdentityCollision(proof)`. Only an absent fresh
  session whose expected selector drifted returns `SelectorConflict(current selector)`, so
  activation restabilizes from durable authority rather than adopting an older or unrelated
  candidate session. A newly opened head has no active-operation custody.
- A forward text demand starts at its exact requested UTF-8 anchor; a backward text demand ends at
  its exact requested anchor. Each returns a positive, caller-bounded run of complete UTF-8 scalars
  unless the requested side is already at the corresponding document edge. A validation demand
  returns a source-selected bounded scalar-safe window containing the candidate byte coordinate, or
  exact beginning/end edge facts when no bytes exist on that side, so the consumer can distinguish
  a scalar boundary from a coordinate inside a scalar without clamping or rounding. Every response
  repeats the exact root and range, checked byte and logical-line facts, authenticated preceding and
  following continuation or document-edge facts, and the source-selected UTF-8-safe edges.
- Marker paging is bidirectional over one exact byte interval or anchor. Requests carry a positive
  object ceiling, positive retained-byte ceiling, and optional exclusive authenticated composite
  cursor. Direction selects the adjacent window, while returned markers always retain canonical
  `(anchor, order key, marker identity)` order. Each page repeats its exact root and covered range,
  reports authenticated preceding and following marker or edge facts, and distinguishes completion
  on the requested side from an exact continuation cursor. Count and retained-byte ceilings apply
  to every page even when arbitrarily many markers share one zero-width anchor.
- External gap translation uses only bounded exact-root proofs: the first marker at an anchor proves
  the before-all edge, the last proves the after-all edge, and an authenticated adjacent pair proves
  an interior gap; authenticated absence proves the unambiguous gap. Reverse translation validates
  the named first, last, or adjacent facts against the same root. These proofs use bounded tree
  descents and successor/predecessor steps, never a whole-anchor or whole-draft marker scan.
- Exact-root operations remain valid for immutable historical roots. Current-root wrappers stabilize
  the complete thread/draft/reverse selector set before and after the exact-root operation;
  candidate-root wrappers stabilize the exact named active session head and candidate frontier.
  Neither wrapper can combine facts selected from different revisions. Malformed coordinates,
  cursors, UTF-8, limits, order, ranges, edge facts, or continuations are rejected; stale binding,
  session, candidate, or selector generations return stale or concurrent change as applicable;
  missing or digest-, summary-, owner-, build-identity-, or cross-index-inconsistent records fail
  closed as absence or invariant failure. No range-source request scans a whole draft or marker set,
  constructs an edit, publishes autosave state, materializes `ComposerV1`, or submits content.
- Autosave snapshots one exact candidate frontier, then atomically advances the current-draft
  selector and the same session's published frontier only if the draft's durable base and the
  package-authenticated session lineage still match and the active-operation custody slot is
  absent. Later edits may advance the newest candidate
  during publication. Completion clears only its captured dirty generation; a newer candidate
  stays dirty. Validation point-reads the captured immutable settlement/frontier and the compact
  session head and never walks the candidate chain.
- Candidate adoption does not advance the Beryl-state current-draft asset-owner head. Publication
  atomically pairs the Syndic selector/session-frontier advance with exactly one Asset-domain case
  against the single `CurrentDraft(draft id)` owner head: a changed marker set replaces that head
  with its newly sealed set, or removes it when the last marker is removed; an unchanged nonempty
  marker set uses a validation-only assertion of the existing exact head and proof and reuses its
  already sealed set; and an already marker-free draft uses a validation-only assertion that the
  single head is absent. No per-root owner head or zero-marker synthetic set exists. Rejected,
  conflicting, cancelled, failed, superseded, or abandoned candidates never become durable asset
  owners, and their inert sets remain future-GC orphans.
- The app host, not Syndic or the widget, owns the bounded undo/redo frontier. It advances only with
  candidate adoption, preserves on every noncommit or indeterminate outcome, clears redo on a new
  adopted ordinary edit, and moves one bounded entry only after adopted undo or redo. Autosave and
  flush never mutate it. No session record contains inverse content or a root graph.
- No typing, paste, deletion, marker edit, undo, redo, autosave, or dirty-draft flush constructs a
  full `ComposerV1` successor. Autosave publishes the already adopted combined candidate and does
  no work proportional to unchanged draft length. A flush drains an admitted publication,
  reconciles an ambiguous writer outcome by exact operation identity, and repeats from the newest
  eligible dirty frontier until the session's published and candidate frontiers agree.
- Fresh activation creates a new editor-candidate session from only the durable current-draft
  selector. A crash may therefore lose all edits adopted after the last successful autosave while
  preserving the last published draft exactly. Staging for a disposed old session may be an
  unreachable orphan only when no transition ever admitted it or claimed that session's slot; an
  admitted operation must reach its one terminal settlement before clean disposal. An already
  disposed old session's unclaimed orphan records do not block a fresh session because every key is
  session-qualified. Ordinary switch, close, Exit, and submission flush before their transition.
  Session disposal and every safe ownership release require equal published/candidate frontiers and
  an absent custody slot; disposal then makes later candidate adoption or publication stale without
  deleting immutable terminal roots or receipts.
- Consumers that require canonical composer content use a separate bounded streamed
  materialization from one exact immutable combined root. The materializer walks sequence text and
  marker leaves in composite order, emits unreachable bounded `ComposerV1` chunks and indexes,
  verifies end-of-root plus the paired-root summary and output summary, then seals one immutable
  root-bound content reference. Its cursor, input pages, output batch, and retained state stay bounded even though
  total materialization I/O is proportional to that exact combined root.
- A materialization never changes the current draft, candidate session, or autosave backing. It
  starts for submission only after the required flush selects its exact source root. A cancelled,
  failed, or explicitly superseded build before seal leaves only unreachable staging; a crash
  retains exact-root-bound resumable state. Retry and recovery bind to the exact combined root and
  materialization identity; a later candidate adoption or current-draft publication does not
  conflict with, supersede, or rewrite
  that immutable source or build. It only makes
  the result ineligible for a submission that expected the newer current combined root.
  Materialization remains exact-root-bound until it is cancelled, fails, is explicitly superseded,
  or seals; a
  sealed exact result remains reusable by other canonical consumers naming that root, while a later
  draft root requires a distinct result.
- Submission validates that the active candidate session is clean at the same published root used
  by its materialization and has no active-operation custody. Only the atomic accepted send-and-clear transition disposes that session
  and authorizes the app to clear its editor and undo/redo frontier. Rejection, conflict,
  cancellation, error, or ambiguous writer custody preserves the coherent session/frontier until
  exact reconciliation.
- When an existing sealed `ComposerV1` value becomes editable draft content, replacement and recall
  preparation stream it through bounded canonical text and marker reads into a new immutable piece-
  tree and marker-identity index. Only a final revision-checked draft operation selects that
  complete combined root; no reverse import flattens the value, exposes partial content, or changes
  the submitted source.

## Intrinsic Thread Properties

- Every named thread has exactly one immutable execution record keyed by its Syndic thread id. It
  stores the exact runtime id, configured root id, and canonical runtime-native root path accepted
  at creation. Every ordinary or child creation publishes it atomically with the thread, draft,
  history summary, attributes, usage seed, initial compact catalog summary, and initial unbound CAS
  projection. Child creation inherits the exact parent execution binding. No rebind mutation
  exists.
- CAS binding and execution-snapshot records retain execution copies as projection provenance, but
  the immutable thread execution record is canonical. Creation, CAS-binding publication, and scoped
  recovery reconciliation reject a missing or orphan execution record or any valid, active, stale,
  or snapshot copy that disagrees with it.
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
  the exact target on a scoped lineage read and uses it to answer selected-path membership with constant memory
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
- Beryl must not populate ordinary Syndic transcript history by querying Codex App Server history.
  Only the CAS-live system may submit one pinned, bounded, exact-terminal-turn repair snapshot for
  a turn already marked repair-required.
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
- A turn whose live stream was interrupted, lost, dropped by the bounded store-outage buffer, or
  contradicted by its completion remains durable and enters repair-required state once its exact
  terminal identity and outcome are known. Unknown-terminal may accept exact late evidence only
  while the original exact live authority remains usable.
- A repair-required turn is resolved only by one complete atomic terminal snapshot or by explicit
  incomplete convergence. Reconnect, resume, late subscription, process restart, GUI projections,
  and partial history reads are not notification replay and never repair it.
- A completion/live narrative mismatch is a whole-turn capture gap. The live narrative and exact
  completion may remain visible as explicitly transient evidence, but neither is canonical repaired
  history and Syndic never chooses or splices one representation.

## Terminal Repair Snapshots

- A `RepairRequired` gate carries one durable target-scoped request disposition. Its initial
  `Available` state may atomically become `Consumed` exactly once with the request-attempt nonce and
  claim revision transition before any backend dispatch capability exists. Response loss, process
  loss, recovery, or incomplete convergence never restores `Available`; recovery of `Consumed`
  without a complete staged response converges incomplete without another historical request.
- A selected repair snapshot is canonical authority for one exact already-correlated terminal
  turn. A staged or merely sealed snapshot is not canonical authority, a source-event replay,
  imported conversation, catalog record, or general CAS history cache.
- The snapshot contains the complete ordered final public item view admitted by the pinned release,
  exact CAS and Syndic identities, exact terminal outcome, repair adapter version, release, response
  digest, capture-gap reason, and repair time.
- `syndic-storage` owns the snapshot's opaque package-local build reference and paged item, content,
  and media staging. No shared repair identity crosses into `beryl-model` or becomes a second turn
  correlation. Each stage and the final seal-and-selection mutation reconcile an indeterminate
  store outcome through the existing target/thread/turn and page-ordinal natural identities.
- Snapshot item views are semantic final-item authority. Syndic records their historical-repair
  provenance and does not invent item-start, delta, approval, or source-sequence facts.
- Staging encodes each complete semantic final item into snapshot-backed manifests and exact
  logical ranges that remain noncanonical until selection. Once selected, those ranges, not
  fabricated live events or live provider-frame ranges, are the canonical source for repaired
  narrative and derived projections.
- Staging has hard schema-owned item-count, encoded-byte, page-count, per-page item, and per-page
  byte limits. It is unreachable from ordinary history and projection reads and is never canonical,
  even after every page is present.
- One atomic cross-domain whole-turn seal-and-selection mutation validates the staged snapshot's
  exact CAS/Syndic correlation, consumed request claim, terminal outcome, every item identity and
  complete final field, per-page and aggregate digests, adapter/release provenance, and every
  required finalized-media witness. Its Beryl participant atomically publishes only the exact asset
  metadata, references, and resource dispositions proven by matching inert repair-media evidence;
  its Syndic participant selects the compact snapshot head as the turn's sole canonical item
  authority and enters
  `FinalizingHistory`. The selection mutation does not rebuild or publish projections. A durable
  live prefix, process-local outage buffer, GUI text, or partial snapshot is never combined with
  it; rejection preserves the prior authority unchanged.
- Bounded durable work in `FinalizingHistory` rebuilds transcript and every other affected derived
  projection to a fixed point, publishes one coherent transcript presentation generation, and
  only afterward releases the repair gate atomically. No reader observes a partially rebuilt or
  mixed-source generation.
- Terminal failure of repair preserves exact submitted input and any admissible captured evidence,
  fixes the turn's closed incomplete authority and provenance, selects or claims no staged snapshot
  or repair-derived asset, and enters `FinalizingHistory`. Bounded item and transcript work then
  publishes the one coherent incomplete presentation generation before terminal-history completion
  releases the gate. It never guesses canonical item content.
- Ordinary transcript, catalog, replay, and projection reads consume only Syndic after either
  success or incomplete convergence; they never query CAS.

## Activity Presentation Projection

- Activity presentation is a derived, non-transcript index over already admitted provider
  lifecycle records plus exact bounded GUI-derived facts. It does not replace, summarize away, or
  duplicate public provider fields in canonical history.
- One activity-query head names the exact query owner, runtime activity period, root source, source count,
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
  index state atomically. Scoped activity reconciliation proves current and retired memberships against their exact turn
  state and terminal frontier in constant memory; a widened interval or retired active member is
  corruption. Rebuildable projection staleness is explicit, and no reader observes mixed activity
  generations.
- Restart makes a prior runtime activity period ineligible for feature display, but its
  underlying canonical provider records remain ordinary Syndic authority. Activity rows never
  become parent transcript narrative or a persistent activity log feature.

## Canonical History And Projections

- Canonical Syndic history records the source events and normalized canonical items needed for replay, export, diagnostics, and projection rebuilds.
- A normally captured turn derives canonical items from its exact live source events. A successfully
  repaired turn derives them exclusively from its one complete terminal repair snapshot; the two
  authorities are never spliced within one turn.
- A delivery-unknown accepted steering fragment remains user-authored durable accepted-input
  history. Its retained sealed content, accepted order, and unknown-delivery provenance do not
  claim that the provider observed it. Transcript and recovery projections may represent that
  retained user history through their exact accepted-input boundary, but may not execute the
  fragment again automatically or fabricate a provider-sourced canonical item.
- Canonical item records contain bounded metadata, exact content-manifest references, and one
  exclusive source: an item-local source-event frontier for normal live capture or an immutable
  snapshot page/range reference for terminal repair. Ordered canonical content chunks remain source
  authority for arbitrarily large user, assistant, or operational text; projections and resources
  are derived presentation boundaries rather than substitute canonical storage.
- Scoped item validation replays a normally captured item's indexed source sequence in bounded
  memory and requires exact agreement with its kind, assistant phase, external identity, content
  bytes, chunk chain, and completion/finalization state. A repaired item instead validates its
  selected immutable snapshot page/range and repair provenance without inventing a live source
  sequence.
- Transcript projections are derived from canonical history and must preserve stable provenance back to Syndic turn, item, source range, projection, and resource identities.
- Normal source events may advance only before a proven-terminal lifecycle is published; unknown-
  terminal state may still accept exact late evidence. Canonical finalization and projection work
  may advance afterward only from the item's already admitted normal source events or its one
  atomically published immutable repair snapshot authority.
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
- For a repair snapshot, generated media must resolve promptly from the exact authenticated
  `savedPath`. Missing or unusable path/bytes makes the repair incomplete; discarded inline base64
  and similar files are never substitute authority.
- Projection construction consumes one closed exact source: sealed composer content for submitted
  user input, one selected provider narrative generation for normally captured transcript-visible
  provider text, or snapshot-backed canonical logical ranges for repaired transcript-visible text.
  Normal provider parsing walks the narrative-span index directly and reads only the referenced
  `ProviderItemV1` ranges; repaired parsing reads its snapshot-backed ranges and requires no
  `ProviderItemV1` frame or fabricated live lifecycle. Neither path replays an item stream or
  materializes a cumulative item value.
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
  their owning feature contracts. Canonical Markdown text remains in its indexed content backing
  and is never copied into sidecars or subjected to whole-resource sidecar admission.
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
- Routine open validates the durable Syndic registration and schema declaration and reacquires a
  fresh typed handle; it does not enumerate every application record. Recovery and ambiguous-
  outcome reconciliation read only the bounded natural closure of a caller-named durable thread,
  turn, stop, compaction, repair, or staged-page anchor. Exhaustive record walks are explicit schema
  validation, scrub, background maintenance, or corruption-evidence investigation.
- Syndic storage must never persist access tokens, refresh tokens, API keys, bearer headers, cookies, or app-server loopback capability tokens.
- Durable source events and projections must redact or reject protocol fields that are secrets or policy-private control data.
- Unfinished or stale derived projections can be rebuilt or invalidated from canonical history plus resource metadata. Finalized projections remain durable exact history.
