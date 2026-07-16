# Goals

Define Syndic as Beryl's durable thread, draft, conversation-history, projection, reference, and replay system for agent work captured by Beryl.

Let Beryl render selected conversation history from Syndic-owned storage and projections while retaining Codex App Server as the live execution, auth, sandbox, approval, skill, MCP, and enterprise-policy authority.

Keep canonical history, transcript-view records, Markdown projections, and resource metadata below the GPUI transcript presentation stack.

## Non-goals

- Replacing Codex App Server authentication, model execution, sandboxing, approval handling, skills, MCP, subagents, managed configuration, rate-limit handling, or enterprise policy enforcement.
- Importing or backfilling old Codex App Server transcript history into Syndic from historical read APIs.
- Treating Syndic storage as a cache over Codex App Server history.
- Owning Beryl generated titles, automatic branch-discussion archive state, execution-root bindings, window claims, or session layout.
- Garbage-collecting turns, resources, or projections that are unreachable from named threads.
- Rendering operational activity, raw reasoning, command logs, or tool internals as parent transcript narrative.
- Storing OpenAI, ChatGPT, Codex, or app-server authentication secrets.

# Decisions

## Documentation Set

- `concepts.md` is the supplemental Syndic domain model. It is authoritative for current vocabulary and accepted model statements about turns, threads, turn items, canonical messages, Markdown projections, Syndic references, heavy item references, lazy history access, and replay.
- `doc/systems/cas-live-syndic-transcript/design.md` owns the CAS-live source ingestion and CAS projection system contract.
- `doc/systems/codex-compatible-agent-layer/design.md` owns the constraint checklist for any future Codex-derived or Codex-compatible local agent layer.
- `crates/syndic-storage/doc/design.md` owns the reusable storage package boundary, storage engine, persistence API, and on-disk state contracts.

## Product Boundary

- Syndic owns stable threads, committed conversation tails, exactly one current durable draft per thread, ordinary submitted turns, provider-operation turns, ordered turn items, provider/source metadata, canonical event records, transcript-view records, projection records, and resource metadata.
- User-visible transcript history is read from Syndic transcript views once the selected history has been captured by Syndic.
- The owning execution backend remains a source of live events, not the read authority for captured transcript presentation.
- Syndic records preserve external execution identities, including Codex App Server thread ids,
  turn ids, and item ids when they are available, so Beryl can still target exact backend
  operations such as stop, branch, resume, or title publication.
- Missing external identities remain absent rather than inferred.

## Thread And Draft Model

- A Syndic thread is a first-class stable named reference. A turn existing in the DAG does not create a thread.
- Each thread record owns a stable thread id, one committed conversation-tail id when submitted history exists, exactly one current draft id, and a revision covering those mutable bindings.
- The visible submitted transcript path is obtained by walking immutable turn parents backward from the committed tail to a root. The current draft is excluded.
- A current draft is a durable typed pre-submission metadata record with stable id and revision, one exact sealed content-manifest reference, immutable parent, and optional immutable context/provenance envelope. The manifest's bounded ordered chunks encode the exact composer atoms; each image-marker atom includes its stable marker identity and final label ordinal, and Beryl asset references resolve that marker to bytes.
- Ordinary current drafts have no branch context. A branch-discussion first draft records its source parent and selected-context provenance without creating a context-only Syndic turn, canonical transcript item, or CAS request.
- Beryl transcript presentation may derive one synthetic readonly context group from that immutable envelope at the branch boundary. The group remains presentation-only and keeps stable semantic identity when first submission transitions the context-owning draft into a submitted turn.
- Only the draft's sealed content reference and mutable timestamps change during autosave. Parent, thread owner, and context/provenance fields are immutable.
- Content construction is staged through bounded ordered chunk commits. One final revision-checked command seals the manifest and publishes its draft reference atomically; incomplete or superseded staging remains unreachable durable state until future garbage collection.
- A physical record or chunk is bounded, but one logical draft has no fixed whole-content byte ceiling. Storage preserves a draft even when later CAS request assembly rejects it against an exact provider/model limit.
- An idle-thread submission atomically resolves every draft marker to its durable asset id, transitions the same draft identity to a submitted turn, creates one typed canonical user-input item referencing the sealed ordered content plus exact separately ordered marker resolutions, advances the thread tail, and creates its replacement current draft.
- The draft-to-turn transition preserves the draft's exact 128-bit identity payload while changing its typed identity from `SyndicDraftId` to `SyndicTurnId`; it does not allocate an unrelated submitted-turn identity.
- A context-bearing draft's owner descriptor follows that same deterministic typed transition while its immutable context envelope remains byte-for-byte unchanged.
- Context admission proves that the source turn was on the named source thread's selected path. Reopen does not require that turn to remain on the thread's later mutable selected path; replacement edits may move the named thread tail while the immutable context source remains durably referenced.
- After first submission, the context envelope remains owned by that deterministically typed first submitted-turn identity. A later replacement path in the discussion may move away from that turn without moving, deleting, or invalidating the thread's stable context owner.
- Input submitted during an active turn or compaction is atomically frozen into one ordered accepted-input record referencing the exact sealed content plus exact separately ordered marker resolutions and replaced by a new current draft; steering, pending, and next-turn queue states retain that same accepted-input identity and per-marker asset ownership while changing disposition and lifecycle, and never create a separate queued-input identity or a second active turn.
- Permanent accepted-input order is retained independently from the bounded live steering and next-turn route indexes. Terminal accepted inputs leave the live indexes but remain addressable in accepted history, so a long thread never inherits the live-queue ceiling.
- Accepted-input delivery-unknown is a terminal delivery outcome distinct from delivered and failed.
  It means one provider request may have been dispatched but has no authoritative response. The
  outcome retains the admitted input and provenance in history, leaves every live delivery index,
  and forbids automatic replay.
- Delivery-unknown provenance includes the exact historical active binding, execution snapshot,
  CAS turn correlation, and the old CAS thread's one-way retirement through its exact stale binding.
  It cannot be published while that projection remains usable or through a commit separate from
  projection retirement.
- Exactly one revisioned input gate per thread classifies admission against idle, pending-turn, active-steering, compaction, or stopping state. It retains the accepted-order high-water mark and exact bounded live-route accounting so writer admission never scans historical accepted inputs.
- An active-steering disposition retains the exact binding revision, execution snapshot, Syndic turn, CAS thread, and known-or-explicitly-unknown CAS turn accepted at that gate revision. Missing CAS identity is never inferred from process-local state.
- Every mutation uses expected thread and draft revisions. A conflict rejects the whole mutation rather than creating competing same-thread children.
- Different threads may submit distinct children from the same historical turn without conflict.

## Turn Parentage And Replacement

- A submitted turn has zero or one immutable parent.
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
  activation, typed item start, typed bounded coalesced item delta, typed item completion, and
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
  large strings, vectors, maps, and structured-value leaves remain in bounded chunks. A
  frame-specific logical-text span index selects that snapshot's transcript/projectable fields from
  those same bytes, so typed provider preservation and canonical text never duplicate one large
  payload and a final snapshot can exclude stale delta-derived bytes.
- Provider completion is authoritative for the final public item fields. Its frame may reuse
  unchanged earlier ranges and append changed values, but storage cannot publish the item completed
  until the final frame is sealed, structurally valid, kind-consistent, and fully referenced by a
  durable frontier. Staging remains unpublished across a cut.
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
- A turn whose live stream was interrupted or lost remains durable with an explicit incomplete,
  failed, or unknown-terminal status. Unknown-terminal may accept exact late evidence only while
  the original exact live authority remains usable. Proven loss retires that authority and
  converges the retained prefix as incomplete; reconnect, resume, late subscription, process
  restart, CAS history reads, and GUI projections are not notification replay and do not repair
  that source-event sequence. Source-less incomplete convergence releases the execution block
  rather than leaving the thread permanently locked.

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
- Provider activation, output, item completion, and successful turn completion always retain exact provider identity. A source-less event may only converge a locally interrupted, failed, incomplete, or unknown-terminal turn after its projection is stale or the thread is unbound. A still-usable valid binding represents only the pending turn's parent and cannot authorize local selected-path advancement. Source-less convergence never fabricates provider activity or success.
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
- Projection construction may consume one exact current live or immutable canonical snapshot.
  Source advance atomically makes the selected item projection stale and supersedes incomplete
  work, while completed older generations remain coherent historical snapshots. Consecutive source
  advances may coalesce projection work onto the latest canonical revision. When an advance changes
  only correlation or lifecycle provenance and retains the exact content reference, the new
  generation reuses the prior immutable projections, resources, stable checkpoint, and identities
  rather than reparsing or copying text. Freezing a closed canonical item and advancing the
  finalized-item frontier are distinct durable transitions. A transcript-visible item advances the
  frontier only after its frozen source has a current completed projection set; an operational item
  advances after freezing because it has no transcript projection.
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
- Canonical content chunks have exact encoded-byte indexes plus logical-text span indexes.
  `ComposerV1` text spans skip framing and image-marker bytes, while `Utf8V1` spans map directly.
  Every content reference snapshots exact chunk, ordered-piece, encoded-byte, and logical-text
  frontiers. End-of-input is reached only when both referenced logical text and referenced pieces
  are exhausted, so trailing and marker-only zero-width image pieces remain visible without
  allowing a later live append under the same content id into an older projection generation.
  Textual code and table resources retain immutable logical UTF-8 ranges plus bounded structural
  metadata and preview ranges. Resource reads resolve only the requested indexed range and never
  assemble the complete canonical item or resource as a prerequisite.
- Public transcript/path/membership pages are capped at 256 records and 65,536 stored encoded
  bytes. Textual-resource reads return at most 65,536 requested bytes with an exact continuation.
  These are residency limits, not history or turn-size limits.
- Image, attachment, and other externally owned byte payloads may use Beryl-home sidecars under
  their owning feature contracts. Phase 7 does not copy canonical Markdown text into sidecars or
  require whole-resource sidecar admission.
- Every transcript generation has explicit bounded path-collection and entry-publication state.
  Path collection walks immutable parents tail-to-root into generation-owned depth records;
  publication then walks those records root-to-tail and assigns contiguous stable positions. Only
  the completed generation becomes the selected current head.
- An in-progress generation is bound to the exact current broad thread revision. Once complete,
  draft rotation or accepted-input admission may advance that broad revision without rebuilding an
  unchanged transcript. The completed generation remains current only while its captured revision
  is not from the future and its committed tail and selected-path digest still match exactly.
- A live canonical item may publish coherent projection generations. Immutable projection and
  resource identities and revisions exclude generation. One generation-independent membership
  range owns the item's immutable closed prefix; a generation-owned membership range owns only
  the provisional end-of-input suffix for that live source snapshot. Sets, optional resumable
  builds, and heads select a coherent merged logical range. Closed block groups are reused by exact
  reference without replaying from byte zero, while the open trailing group may be superseded after
  later appended source. Immutable end-of-input promotes its exact output to the stable range.
  Once the turn is proven terminal and its current projection generation is published, all
  selected projection and resource records are finalized immutable history.
- Projection format V1 uses bounded GFM block recognition. Recognized blocks retain typed structure
  and exact source ranges. Undecidable, malformed, unsupported, or deliberately bounded-out syntax
  becomes ordered source-preserving fallback spans; this may reduce styling locally but never
  drops bytes, changes authored text, or expands memory with the complete item.

## Execution And Policy Boundary

- Codex App Server remains the execution and policy authority for CAS-backed turns.
- CAS-backed execution retains Codex authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandbox behavior, approval policy, skills, MCP, subagents, rate limits, and tool execution.
- Syndic storage and projection code must not broaden or bypass CAS policy decisions.
- Future Syndic-owned execution may be designed only after satisfying the constraints in `doc/systems/codex-compatible-agent-layer/design.md`.

## Persistence Boundary

- Syndic durable history is not Beryl GUI-local settings and is not a bounded resident presentation cache.
- Syndic records occupy a private logical keyspace family inside the one physical Beryl-home database defined by `doc/systems/beryl-home-storage/design.md`.
- Syndic storage APIs receive opaque domain-scoped access from the home-store boundary and never expose Fjall handles, key encodings, or transactions to callers.
- Syndic storage must never persist access tokens, refresh tokens, API keys, bearer headers, cookies, or app-server loopback capability tokens.
- Durable source events and projections must redact or reject protocol fields that are secrets or policy-private control data.
- Unfinished or stale derived projections can be rebuilt or invalidated from canonical history plus resource metadata. Finalized projections remain durable exact history.
