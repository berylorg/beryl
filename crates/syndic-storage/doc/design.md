# Goals

Provide the reusable production storage boundary for Syndic-owned durable threads, current drafts, submitted conversation state, source events, reference metadata, transcript views, unfinished rebuildable projection state, and finalized immutable projection history.

Support low-latency Beryl reads over large captured conversation histories while keeping UI memory growth bounded.

Support short durable write commits for live CAS event ingestion, streaming assistant output, generated artifacts, and future concurrent user or subagent activity.

## Non-goals

- Calling OpenAI, ChatGPT, Codex App Server, or any model provider.
- Owning authentication, token refresh, sandboxing, approvals, skills, MCP, enterprise policy, or live execution.
- Owning GPUI transcript presentation, renderer residency, scroll behavior, or widget state.
- Owning Beryl thread presentation metadata, runtimes, roots, window sessions, settings, installed themes, or asset lifecycle policy.
- Opening a separate physical database outside the Beryl-home store or exposing raw Fjall access to callers.
- Persisting access tokens, refresh tokens, API keys, cookies, bearer headers, or app-server listener capability tokens.

# Decisions

## Storage Engine

- `syndic-storage` uses Fjall only through `beryl-home-store`; it has no direct Fjall dependency or physical database handle.
- The package owns Syndic record schemas, codecs, typed queries, mutation validation, and batch contributions for its private keyspace family.
- The physical database, home lock, serialized writer, and persistence barrier are owned by `beryl-home-store` under `doc/systems/beryl-home-storage/design.md`.
- Callers interact through typed Syndic APIs rather than Fjall keyspaces, byte encodings, transaction handles, or home-store domain registration.
- Readers may perform bounded cursor and point reads while writes commit through short revision-checked home-store commands.

## Pure Values And Dependency Boundary

- Stable cross-package Syndic identities, typed revisions, discussion-context owner/digest facts, external CAS identities, and exact CAS generation facts belong to `beryl-model`.
- Syndic-specific lifecycle, ordering, immutable parent/context, transcript-position,
  recovery-budget, selected-path, CAS-represented-prefix, and CAS-lineage proof values belong to
  this package and perform no clock observation, identity generation, filesystem access, provider
  work, or policy lookup.
- An idle submission preserves the draft identity's exact 128-bit payload while changing its typed identity from `SyndicDraftId` to `SyndicTurnId`.
- A context-bearing draft's typed context-owner descriptor follows the same transition without rewriting its immutable envelope.
- Discussion-context envelope V1 preserves the exact selected UTF-8 text and computes its typed context digest as SHA-256 over those exact bytes; the pure constructor observes no clock and accepts creation time from its caller.
- One accepted input keeps one `SyndicAcceptedInputId` and its exact resolved marker atoms while moving between pending, steering, retryable, and next-turn queue dispositions. There is no second queued-input identity or queued asset-owner identity.
- An accepted-input steering disposition names the exact active Syndic turn. Fresh native CAS
  lineage establishes an empty selected path. Continuation, resume, fork, and recovered injection
  require an exact non-empty committed tail. Syndic publishes no valid in-place-rollback lineage.
- Public value serialization, where explicitly provided, is a transport value shape only. Versioned package codecs and exact domain declarations remain the sole persisted-schema authority.

## Public Boundary

- This package implements the storage boundary consumed by `doc/systems/syndic-conversation-history/design.md` and `doc/systems/cas-live-syndic-transcript/design.md`.
- The package exposes operations for constructing its opaque domain handle, creating threads and
  current drafts, revisioned draft updates, atomic draft submission/replacement, accepted-input
  admission, exact steering-delivery claim/outcome transitions, committing live-source events,
  advancing the exact terminal item-finalization frontier, reading historical summaries, reading
  thread/draft/turn metadata, reading immutable branch-context envelopes by context-owner identity,
  reading transcript-view pages, reading projection records, reading resource metadata, and reading
  resource byte ranges, plus reading bounded logical UTF-8 pages from one exact sealed content
  reference.
- Public APIs use Syndic identities and revisions as their stable boundary.
- External execution ids, including CAS thread ids, turn ids, and item ids, are stored as source metadata and never become the only primary key.
- Public reads are bounded by caller-supplied limits or explicit range requests.
- Branch-context reads return the exact bounded immutable envelope and owner revision; they do not manufacture transcript-view records or synthetic turns.
- Public writes keep each metadata record, content chunk, mutation batch, and read page bounded. They do not reject a logical draft or canonical text merely because its complete content exceeds one record.
- Public mutations require expected revisions for every correctness-sensitive thread, draft, binding, or accepted-input record they change.
- Recovery preparation is a read-only public boundary with two explicit request scopes. Current-path
  preflight assembles a complete recovery-complete selected path before input admission and returns
  the native empty prefix without model metadata when that path is empty. Pending-turn restart
  preparation retains the current behavior of assembling only the pending selected turn's parent.
  Recovery-complete means `complete`, `interrupted`, or `failed` with the complete finalized item
  frontier and explicitly excludes `incomplete`. The returned domain `source_revision` records
  assembly provenance only; later recovery-proof publication uses the then-current domain revision
  plus exact current thread, selected-path, and binding expected revisions rather than the older
  global revision.

## Logical Records

- Logical record names mirror the Syndic history and CAS-live capture system contracts; this package owns their stored representation and typed API behavior.
- A thread record stores stable thread identity, committed conversation-tail id when any, current draft id, thread revision, optional parent-thread handoff binding, and optional branch-context owner id. External execution metadata belongs to revisioned binding records rather than the thread record.
- A current-draft record stores stable draft identity, owning thread id, draft revision, one exact sealed content reference, immutable parent turn id when any, optional immutable typed branch-context owner identity, optional replacement-edit intent containing the exact target and bounded selected-transcript proof, and timestamps. The context envelope itself occupies the separate `context-envelopes` family.
- A content-manifest record stores content identity, optional canonical-item owner, encoding, lifecycle, exact chunk frontier, encoded and logical lengths, atom and marker counts, and a chain digest. Content-chunk records store bounded ordered encoded bytes. Building content is unreachable; ownerless sealed content is immutable and content-addressed; item-owned live UTF-8 content has a deterministic item-derived identity and may append only before it becomes finalized.
- A canonical user-input item references the exact sealed draft content and an exact count of separately ordered marker-resolution records. Each resolution retains marker identity, final label ordinal, and durable asset id; it does not embed image bytes or duplicate text.
- An accepted-input record stores input frozen from a draft for active-turn steering or later-turn queueing, including one stable accepted-input identity, owning thread, permanent order, current disposition and lifecycle, the exact admission input-gate revision, the sealed content reference, exact marker-resolution count, and admission state. A steering disposition retains the exact binding, execution snapshot, Syndic turn, CAS thread, and known-or-explicitly-unknown CAS turn proof.
- One input-gate record per thread stores its independent monotonic revision, idle/pending-turn/steering/compaction/stopping state, accepted-order high-water mark, and exact live steering, next-turn, and logical-byte counters. Permanent accepted order and bounded live routing are distinct authorities.
- A turn record captures turn identity, turn kind, immutable parent relationship, deterministic
  ancestor skip, origin thread, immutable chain proof, and creation time. Mutable lifecycle,
  source-event frontier, item frontier, contiguous finalized-item frontier, and terminal or
  incomplete facts belong to its matching turn-state record.
- Turn kind distinguishes ordinary user turns from provider-operation turns such as context compaction.
- CAS projection binding records store the current external execution projection state when present, including valid, stale, unbound, or active binding status, external CAS ids, and immutable execution snapshot identity needed by higher-level systems.
- A CAS projection binding record is keyed by Syndic thread view and binding revision, and stores
  the exact current selected-path proof used to classify the binding. A structurally distinct
  CAS-represented-prefix proof records only the committed prefix CAS already owns; a pending
  submitted tail is never included before `turn/start`.
- Valid binding records store the CAS runtime target, CAS thread id, native-or-recovered lineage
  mode, lineage proof, and exact cumulative native CAS turn count needed by higher-level
  orchestration before it may request CAS-native execution. The native count is not Syndic turn
  depth; recovery items and provider-operation turns do not increment it.
- A recovered-lineage proof records the exact injected Syndic prefix, sequence proof, completion
  time, and exact loaded process/thread generations that established the fresh CAS prefix. This
  establishment provenance remains distinct if ordinary later CAS turns advance the represented
  prefix, and it never authorizes replaying the injected prefix. Activation under recovered
  lineage requires the same loaded generation and cannot start before injection completed.
- Active binding records additionally store the accepted immutable execution snapshot id and
  active submitted turn. The snapshot stores its exact selected path, represented base prefix,
  represented-base native CAS turn count, execution binding, loaded process/thread generation,
  and start time without an accepted-input vector or a mutable optional CAS-turn field.
- A separate one-way active-CAS-turn record publishes the exact returned CAS turn identity for one
  snapshot. Its reverse CAS-turn index records the checked next native CAS turn count, and its
  same-thread input-gate transition commits atomically; a different second identity is a collision
  rather than a snapshot rewrite.
- An active binding advances to valid only from a terminal event carrying that exact published CAS
  thread and turn identity, and increments the native CAS turn count exactly once. Resume preserves
  that count, inclusive fork seeds it from exact source correlation, and fresh lineage starts at
  zero. A source-less lifecycle update cannot claim that CAS accepted or now represents the
  submitted turn. No mutation publishes an in-place-rollback lineage as usable authority.
- Losing active projection authority uses one atomic abandonment mutation rather than an ordinary
  stale publication. It retires the CAS thread, publishes exact stale provenance, and returns the
  gate to the same submitted turn. Undispatched `Admitted` or `Retryable` steering routes move to
  ordered next-turn work under `ProjectionLost`; a possibly dispatched `Delivering` route becomes
  terminal `DeliveryUnknown`, leaves no live route or counter, and remains permanent
  accepted-input history without automatic replay. If no exact activation event was admitted, the
  still-pending turn may be rebound through a fresh projection. Once activation was admitted, the
  turn is never replayed automatically; the pending gate withholds competing work until a later
  source-less interrupted, failed, incomplete, or unknown-terminal update converges local
  lifecycle without restoring external authority.
- Stale binding records preserve the old execution binding, CAS thread id, any observed lineage,
  prefix, exact observed native turn count, and generation facts, bounded reason, and timestamp as
  non-authorizing provenance. Retirement from known usable authority preserves its exact count; an
  unestablished abandoned thread may retain known zero or no count. A newly returned inclusive
  fork may instead retain its exact nonzero native position only when its nonempty observed prefix
  and `Fork` establishment proof agree. The old CAS thread remains
  permanently unavailable for another Syndic thread.
- Native resume continuity retains the exact prior usable prefix as establishment provenance while
  allowing the new represented-prefix proof to carry a later source-thread revision for the same
  tail and digest. This distinction prevents a local selected-path revision from being
  misrepresented as a newer CAS history-establishment event.
- Unbound binding records represent a view with no usable CAS projection and may store the reason that no projection exists.
- A source-event record stores one normalized turn activation, typed item start, typed coalesced item
  delta, typed item completion, or status-only turn-ending outcome with a monotonic per-turn sequence
  number and bounded payload or sealed provider-frame reference. One turn-ending status stores the exact provider or local execution
  outcome independently from an optional typed history-incomplete reason. A locally incomplete
  outcome requires a reason; a provider-complete outcome may also carry a reason when the observed
  history cannot be published complete. Activation, provider content, item completion, and
  successful turn completion require exact CAS turn or item identity. A source-less event may only
  record local interrupted, failed, incomplete, or unknown-terminal convergence while the current
  projection is stale or the thread is unbound; a still-usable valid binding is not sufficient
  because it represents only the pending turn's parent. Local convergence cannot manufacture
  provider activity or success.
- A canonical item record stores bounded kind, phase, source, ordering, revision, narrative/resource
  policy, latest sealed provider-frame reference, and item-local source-event frontier for correlated
  submitted input, assistant messages, operational records, generated media references, and
  presentation-only activity records. The provider frame remains mandatory when the normalized
  public variant has fields even when the item contributes no transcript narrative. The API has no
  generic item shape that silently discards history-relevant fields while permitting successful
  turn completion.
- `ProviderItemV1` is the sole item-owned byte authority for provider-created admitted public
  payload. Its
  deterministic, length-delimited, digest-covered start, delta, and completion frames use a closed
  enum for every pinned item variant and a closed recursive structured-value algebra. Raw JSON,
  opaque blobs, ignored-field fallbacks, and unknown future-variant containers are forbidden.
- Materialized and constant-resident frame validation produce the same typed history-support fact.
  Unsupported-but-retained observations remain structurally valid, their first unsupported reason
  accumulates monotonically across the item stream, and later publication cannot reinterpret them as
  complete history. In particular, the pinned Web-search `Other` action is retained while carrying
  `UnsupportedRequiredPayload`.
- The standalone image-generation frame retains exact identity, lifecycle timestamps, status,
  optional revised prompt, and optional `savedPath`. It has no base64 `result` field because the
  backend discards that transport payload before normalization. Syndic has no codec, chunk family,
  or fallback mutation that can store those image bytes in Fjall.
- Standalone image status is closed to the pinned producer's `in_progress`, `failed`, and `completed`
  values; an authoritative completion frame cannot retain `in_progress`.
- Arbitrarily large provider strings and structured leaves remain in bounded ordered content chunks.
  Typed field structure and exact range references remain in the same chunked encoding; a
  frame-specific logical-text span index exposes only that frame's selected narrative and
  operational text over those existing bytes. A completion frame may therefore replace an earlier
  delta-derived view without exposing stale bytes or copying unchanged payload.
  `CanonicalItemPayload::Text` therefore refers to the same provider content rather than storing a
  second copy. Submitted-user correlation instead refers to its already sealed composer content and
  retains only exact provider metadata and correlation proof.
- Provider item lifecycle and owned-resource availability are independent durable facts. A
  generated-media item may record exact provider completion while its resource disposition remains
  `PendingAsset` or typed unavailable; its finalized-item frontier and history completeness do not
  advance. Later Beryl-home asset admission resolves only the resource reference and derived
  finalization frontiers and never rewrites the source item-completion event or provider terminal
  status. Only the retained runtime-local `savedPath` can initiate that admission; missing or
  unusable path evidence never falls back to an inline provider payload.
- A content-byte-span index stores each content chunk's exact encoded start and end offsets,
  ordinal, and digest proof. It permits bounded predecessor lookup and range continuation without
  scanning from the start of a logical content value.
- A content-text-span index stores ordered logical UTF-8 ranges and their exact encoded source
  ranges. `Utf8V1` spans map directly; `ComposerV1` spans skip atom framing and image-marker bytes;
  provider frames use their separate frame-keyed text-span index because an authoritative snapshot
  may revise an earlier delta-derived logical view. Both indexes preserve field and element
  boundaries and allow user, assistant, and operational projection work to use bounded logical-text
  cursors over the same stored provider bytes.
- A content-piece index stores the exact render-significant order of logical text spans and
  `ComposerV1` image markers. Projection and transcript work can therefore preserve authored
  text-marker interleaving without decoding or retaining the complete logical content value.
- Every content summary and immutable content reference carries the exact chunk, ordered-piece,
  encoded-byte, and logical-text frontiers. A live reference therefore snapshots structure as
  well as bytes; later pieces under the same live content id are outside an older reference.
- Projection EOF requires both the referenced logical-text byte frontier and its exact ordered
  content-piece frontier. Zero-width image markers advance the piece frontier without fabricating
  logical bytes, including in trailing and marker-only input, while a later live append cannot be
  mistaken for part of an older generation.
- A provider-item build record retains bounded resumable staging authority for one unpublished typed
  frame: exact item and prior published frontier, frame lifecycle and kind, chunk and byte counts,
  structural digest, and the next bounded encoding frontier. Publication atomically advances the
  content manifest, source event, canonical item and indexes, lifecycle, and transcript staleness.
  A conflicting or abandoned build remains unreachable and cannot satisfy item or terminal audit.
- A provider-frame text-span record is keyed by exact sealed frame identity and logical start. It
  maps that frame's bounded logical view to exact item-content byte ranges, including discontiguous
  reuse of unchanged earlier fields. All spans for an unpublished frame may be staged in bounded
  batches; they become readable authority only when the final publication command selects the frame.
- An item-projection head selects one coherent generation for an item and records whether that
  selection is current or stale against the canonical source revision.
- An immutable item-projection-set record retains the exact source item/content revisions, parser
  version, stable-prefix projection and resource counts, total output counts, stable and complete
  digests, the reusable parser checkpoint, and whether that checkpoint has resolved end-of-input.
- An item-projection-build record retains only bounded resumable state for one uncurrent
  generation: the next canonical byte location, current block classification, bounded undecided
  source, preview state, output ordinals, and resource digest/count state. Whole canonical items
  and whole heavy resources never enter this record.
- A transcript-view head selects one explicit generation and tracks that generation's mutation revision, selected path, frontier, and lifecycle. Transcript-view entry keys include thread, generation, and position so changed paths and bounded rebuilds never rewrite a prior generation.
- A transcript-build record and generation-owned path-turn records retain the bounded two-pass
  selected-path rebuild. The first pass walks parent links tail-to-root and writes exact depth
  records; the second walks depth records root-to-tail and publishes bounded entry batches.
- An incomplete transcript build requires the exact current broad thread revision. A completed
  current build may predate draft-only or accepted-input thread revisions; it remains authoritative
  exactly while its committed tail and selected-path digest still equal the current thread and its
  captured source revision does not exceed the thread revision.
- A history summary record exposes the owning thread revision, committed tail and selected-path digest, exact captured-history completeness, and last captured activity for Beryl-home catalog joins without storing title text, parent-thread metadata, or selected-thread GUI state.
- V1 history-summary `complete` is true exactly when the selected transcript frontier is current and every selected-path turn has a known terminal lifecycle other than explicit `incomplete` with its finalized-item frontier equal to its item frontier; pending, active, incomplete, unknown-terminal, unfinished item finalization, or stale projection state makes it false. An empty current transcript is complete.
- V1 history-summary `last_activity_at` is the maximum of the current draft's update time and every selected-path turn's submission and turn-state update times. Summary publication is atomic with every mutation that changes a contributing fact; reopen recomputes and validates both derived values with bounded traversal.
- An immutable projection record stores one typed Markdown block/span or one resource reference,
  its exact source range and block-group provenance, and the bounded inline source or preview
  consumed by the transcript provider. Its identity and revision are generation-independent.
- An immutable resource metadata record describes one exact canonical logical-text range or a
  later feature-owned external backing, plus generation-independent revision, kind, preview range,
  byte length, digest, and bounded code/table structural metadata. Metadata reads never load
  backing bytes.
- Closed item-projection membership is keyed only by item and logical projection ordinal. Every
  later generation reuses that exact immutable stable prefix. Outputs that exist only because a
  live source snapshot reached its current end remain in the generation-owned suffix; a later
  source revision may supersede them without rewriting the stable prefix. Once immutable source
  resolves the same end-of-input, those exact outputs may join the stable prefix. Public reads
  merge the two membership ranges behind one contiguous logical ordinal space.
- Projection and resource identities and revisions exclude generation. An unselected build record
  is optional resumable work rather than selected authority; losing it requires rebuilding that
  generation but does not corrupt canonical history or a published set.

## V2 Domain Schema

- The stable logical domain name is `syndic` at domain schema V2. Every family uses keyspace schema
  V1 and one exact package-owned record version selected per family. Changed `content-manifests`,
  `source-events`, and `canonical-items` records use V2; the new `provider-item-builds` family and
  `provider-frame-text-spans` family and every unchanged family use V1. There is no V1-domain
  compatibility decoder or migration adapter inside this replacement rework.
- V2 primary families are `threads`, `drafts`, `content-manifests`, `context-envelopes`, `turns`,
  `turn-states`, `input-gates`, `accepted-inputs`, `source-events`, `canonical-items`,
  `provider-item-builds`,
  `item-projection-heads`, `item-projection-sets`, `item-projection-builds`,
  `transcript-view-heads`, `transcript-builds`, `projections`, `resources`, `history-summaries`,
  `bindings`, `execution-snapshots`, and `active-cas-turns`.
- V2 exact reverse and ordering families are `content-chunks`, `content-byte-spans`,
  `content-text-spans`, `content-pieces`, frame-keyed `provider-frame-text-spans`,
  `input-marker-resolutions`, `draft-by-thread`, `thread-parent-index`, `turn-children`, permanent
  `accepted-order`, live-only `accepted-steering`, live-only `accepted-next-turn`, `turn-items`,
  `item-source-events`, `cas-item-index`, `transcript-path-turns`, `transcript-view-entries`,
  generation-independent `stable-item-projections`, generation-owned suffix `item-projections`,
  `projection-resources`, `binding-heads`, permanent per-revision `cas-thread-bindings`,
  `cas-thread-index`, and `cas-turn-index`.
- Index values retain the authoritative identity plus the revision or digest needed to prove agreement. Empty marker values are not sufficient index authority.
- Binding records are immutable revisioned history keyed by thread and binding revision. `binding-heads` selects exactly one current record per thread. `cas-thread-bindings` records immutable ordered membership for every CAS-bearing binding revision, while `cas-thread-index` permanently assigns each CAS thread identity to one Syndic thread, its first and latest binding revisions, and one-way retirement at the first stale or abandoned revision. Reopen requires the membership sequence, binding history, and reservation frontiers to agree exactly. After retirement, that CAS thread cannot authorize execution for either the original owner or another thread. Only agreement with the current valid or active binding head and a non-retired reverse record authorizes execution; a retired index entry is provenance, not live authorization.
- Immutable turn topology and mutable lifecycle/frontier facts occupy separate `turns` and `turn-states` families so later event commits cannot rewrite parentage through a lifecycle update.
- Every non-root immutable turn stores one deterministic 128-bit ancestor skip. Its target depth is
  `max(1, depth & (depth - 1))`; roots store no skip. Reopen proves the skip names the exact
  ancestor at that depth. Selected-path membership therefore uses constant resident memory and at
  most 2,080 bounded turn point reads for the complete `u64` depth domain instead of an unbounded
  parent walk or a per-turn jump table.
- A context envelope is keyed by its typed draft-or-submitted-turn owner. First submission moves the same exact envelope bytes and owner payload from the draft identity type to its deterministic submitted-turn identity type.
- `DiscussionContextRange` uses half-open absolute canonical logical UTF-8 byte coordinates within
  the source item, never projection-local coordinates. The range must lie within one finalized
  source projection and is resolved through bounded logical-range reads over the content indexes.
- That submitted-turn context owner remains stable after first submission. Reopen requires its immutable parent to agree with the context source turn but does not require the owner turn to remain on a later replacement-selected discussion path.
- Interrupted and superseded item-projection generations, transcript generations, path records,
  generation-owned indexes, and build records remain coherent derived state but are not selected
  authority. Immutable projections and resources referenced only by that state remain retained
  until future garbage collection.
- An immutable projection or resource record may also remain unreferenced after an interrupted
  derived write. Reopen treats that exact primary record as an unreachable garbage-collection
  candidate, not visible membership. Any reachable membership, set, head, transcript entry, or
  context envelope still requires its complete exact reverse agreement.

## V2 Bounds And Canonical Encoding

- Persisted integer ordering uses unsigned big-endian encoding. Composite index keys order first by their owning identity and then by one-based ordinal or revision. Cursor-only lower or upper sentinels are rejected as stored keys.
- Stable Beryl and Syndic identities use their exact 16-byte payloads. Digests use exact 32-byte values. External CAS identities retain validated UTF-8 and remain bounded by `beryl-model`.
- One content chunk carries at most 65,536 encoded bytes, and one staged append command carries a
  fixed bounded chunk count. Content manifests use `u64` counts and lengths; no smaller whole-draft,
  whole-submitted-input, or whole-provider-item byte ceiling is encoded in V2.
- Provider structured values accept at most 128 nested list/object containers, matching the pinned
  backend JSON parser's admitted recursion depth. The streaming validator uses fixed bounded depth
  state; string bytes and collection element counts remain chunked and have no smaller per-item cap.
- One composer payload may retain at most 1,024 image-marker atoms under the image-input resource budget. Text and total atom ordering are chunked and are not limited by one physical record.
- One thread may retain at most 256 simultaneously live accepted inputs and 268,435,456 logical UTF-8 bytes across its live steering and next-turn routes. These are live-work safety ceilings, not accepted-history or turn-count ceilings; exact provider and model limits may reject earlier.
- One recovery projection contains from one through 262,144 nonempty canonical text items and at most 262,144 logical UTF-8 text bytes. The item ceiling follows from the byte ceiling and therefore does not introduce a smaller retained-history or turn-count limit.
- Recovery assembly first walks only the exact immutable parent topology and matching
  recovery-complete turn states, adding item frontiers with checked arithmetic. Explicit
  `incomplete` is proven terminal for lifecycle accounting but is not recovery-complete. An item
  total above 262,144 is rejected
  before allocating the item frontier or reading any turn-item index; an accepted total allocates
  exactly that bounded metadata capacity and then reads canonical indexes and text in bounded
  pages and ranges.
- One normalized source-event remains bounded at 262,144 payload bytes. Canonical-item records contain metadata and a content reference rather than whole text. One transcript-view entry or inline projection remains within the 65,536-byte page limit; larger source is represented by ordered projections or resources without ceasing to exist as canonical chunks.
- One metadata-only thread, turn-state, history-summary, binding, execution-snapshot, projection-metadata, or resource-metadata record remains at or below 65,536 payload bytes. Codec ceilings include only codec payload bytes; the home store owns the record-version prefix.
- One projection-construction step consumes at most one 65,536-byte canonical chunk plus a bounded
  UTF-8 carry and undecided Markdown window, and emits a bounded record batch. Persisted undecided
  Markdown never exceeds the accepted 16,384-byte inline-paragraph threshold plus one UTF-8 scalar
  carry.
- Public transcript-entry, transcript-path, item-projection, and projection-resource pages contain
  at most 256 records and 65,536 stored encoded bytes. One public textual-resource range response
  contains at most 65,536 payload bytes. Callers may request smaller byte and item bounds; larger
  requests are clamped and return continuation cursors rather than authorizing a larger allocation.
- Projection format V1 applies the exact paragraph, code, table, preview, and page thresholds in
  `doc/systems/syndic-conversation-history/concepts.md`. Malformed or undecidable syntax is emitted
  as source-preserving spans of at most 8,192 UTF-8 bytes.
- Every bounded collection encodes its exact count before its elements, rejects multiplication or allocation overflow before materialization, and rejects trailing bytes, unknown tags, invalid UTF-8, invalid enum combinations, and noncanonical option encodings.

## V1 Structural Proofs

- Every immutable turn header stores a nonzero depth and a V1 chain digest. A root has depth one and the canonical root digest derived from the domain separator and its exact turn id. A child has its parent's depth plus one and a digest derived from the V1 domain separator, child id, parent id, and parent chain digest.
- Reopen recomputes each root or child digest and checks exact depth progression. This proves every parent chain reaches a root and cannot contain a cycle while retaining only one bounded page and point-read parent records.
- An empty selected path uses the V1 digest of the dedicated empty-path domain separator. A nonempty thread's selected-path digest equals its committed tail's chain digest.
- A pending, active, or unknown-terminal turn must be the committed tail of its origin thread. Because one thread has one committed tail, this is also the bounded durable proof that one thread cannot retain competing execution-blocking turns.
- Reopen proves current structural agreement. Parent immutability and one-way draft consumption are additionally enforced by the absence of any production mutation that rewrites a turn header or recreates a consumed draft; they are not inferred as historical events from one snapshot.
- Reopen replays each canonical item's exact `item-source-events` sequence in bounded memory and
  requires its kind, assistant phase, external CAS identity, typed provider-frame references,
  structural and chunk digests, logical-text spans, completion state, and source-event frontier to
  agree exactly with the canonical item and content manifest. A completed provider item requires an
  exact sealed completion frame even when its presentation policy exposes no narrative text.
- A proven-terminal turn with admitted source events must end at the matching turn-ending source event. Its contiguous finalized-item frontier may advance afterward only over already admitted immutable or terminal-finalized canonical content.
- Projection construction may consume one exact current live or immutable canonical snapshot. Any
  source advance atomically marks a selected projection stale and supersedes an incomplete build;
  completed older generations remain coherent historical snapshots. Terminal item closure is a
  separate two-stage transition: a bounded freeze mutation converts closed canonical content and
  its item reference to immutable source without advancing the finalized-item frontier, then a
  visible item advances only after one current completed item-projection set exists. Operational
  items advance after freezing because they own no transcript projection.
- Unreachable history is valid only when every retained turn and its complete parent closure, indexes, items, projections, resources, and provenance remain internally coherent. A missing parent is corruption, not valid unreachable history.

## Revisions And Ordering

- Thread and draft revisions are monotonic and independently checked.
- One atomic idle-thread submission validates the expected thread, draft, input-gate, and sealed-content revisions, transitions the current draft identity into a submitted turn, updates the committed tail, and creates the caller-named replacement current draft.
- The idle transition preserves the draft identity's exact payload in the typed submitted-turn identity and allocates no unrelated turn id.
- One atomic active-or-queued submission validates the expected thread, draft, input-gate, and sealed-content revisions, freezes the payload into an ordered accepted-input record, and creates the caller-named replacement current draft without creating a competing submitted turn. Later queue movement preserves that accepted-input identity.
- Every thread creation also publishes its initial idle input gate. Active-or-queued submission requires the exact gate revision, advances its accepted-order high-water mark and live counters atomically, and never scans retained accepted history to choose an ordinal or enforce capacity.
- A live accepted input has exactly one live steering or next-turn index. Delivered, terminally
  failed, or delivery-unknown input has neither live index but retains its permanent accepted-order
  entry. Claim, proven-pre-dispatch retry, exact success, and structured rejection each
  revision-check the input and gate, advance their revisions, and update the accepted-order
  revision atomically with the exact live-route and counter change.
- A non-steerable or stale-target CAS rejection changes one delivering accepted input to retryable and replaces its steering route with `NextTurn(SteeringRejected)`. It preserves the input identity, permanent ordinal, original admission gate revision, content, marker records, and admitted timestamp.
- A possibly dispatched steering attempt whose provider response is unavailable changes to terminal
  delivery-unknown only as part of atomic active-binding abandonment. The same transition retires
  the active CAS thread, removes the input's live route and gate accounting, retains permanent
  accepted-input history, and supplies no automatic retry authority. Projection-loss rerouting may
  move only work proven not dispatched.
- Execution snapshots contain no accepted-input vector. Their relationship to accepted steering is expressed by each accepted input's exact target proof, keeping snapshot metadata bounded independently of thread history.
- Branch-discussion creation atomically creates the thread, context-bearing first draft, parent-thread binding, and context-owner identity.
- Starting or cancelling replacement edit revision-checks the current empty draft and atomically sets or clears its exact ordinary-user-turn target and selected-path proof without rewriting submitted turns; provider-operation turns are never replacement targets, and cancellation preserves the mutable payload.
- Starting replacement edit copies the target user item's exact sealed content reference and marker facts into the current draft while retaining the target item as immutable history. The edit alone changes no selected path, committed tail, input gate, or CAS binding.
- Provider event updates never mutate submitted turn parentage.
- Storage maintains monotonic revisions for live canonical items and content manifests while source capture remains open. A committed transcript-visible event advances those canonical revisions and marks the selected transcript head stale; the bounded projection builder advances projection records separately from the already admitted source frontier.
- `capture_item` stabilizes one exact CAS-item index, canonical item, latest provider frame, and owned
  live-or-finalized content manifest by rereading that CAS index. Bounded provider-frame reads expose
  typed structural pages; `capture_item_text_range` returns at most 65,536 logical UTF-8 bytes from
  indexed narrative spans while stabilizing the same exact composite item before and after the page.
  Mutation of that item is concurrent state; unrelated thread and item commits do not invalidate
  either read.
- A typed delta names the exact item kind it is permitted to advance. Live mutation compares that
  kind with the durable CAS-item/canonical-item kind before staging a provider-frame addition and
  rejects any mismatch without publication. Protocol indices retained by typed frames are bounded
  and nonnegative before they cross the storage API. Final publication stores only the sealed frame
  reference in the source event and advances its canonical/content frontiers atomically.
- When a visible canonical source revision advances, its selected item-projection set and transcript
  head become stale in the same commit. A later bounded builder starts a new item-projection
  generation; it may reuse frozen closed-block projections by exact reference but never rewrites a
  prior generation's indexes.
- Multiple canonical advances before publication may supersede intermediate builds and converge
  directly on the latest revision. If the content reference is unchanged, including submitted-user
  correlation and provider-lifecycle-only advances, the new generation reuses the prior stable
  end-of-input checkpoint, projection and resource identities, membership, and digest. It republishes
  exact revision provenance without rereading or reparsing unchanged text.
- The builder resumes a later source revision from the prior generation's durable stable
  checkpoint and digest rather than replaying source from byte zero. `stable_eof_resolved = false`
  means the checkpoint precedes the live snapshot's provisional end-of-input outputs;
  `stable_eof_resolved = true` means immutable end-of-input has been consumed into the stable
  prefix. No current source revision may shrink behind or disagree with that checkpoint.
- A transition to a proven-terminal turn lifecycle closes ordinary source-event admission. The same commit leaves every affected derived frontier current only when it already includes the complete accepted event frontier; otherwise it marks that work stale for bounded first-time completion.
- A current item projection becomes finalized when its source turn has a proven terminal lifecycle. Finalized turn-owned canonical content, projection identity, revision, text, resources, and item-local ordering are immutable.
- A named thread's transcript-view head and entries remain a rebuildable selected-path index: they may advance when the thread tail changes, when another finalized turn is appended, or while unfinished projection work changes state. Rebuilding that view may reference finalized projections but cannot rewrite them.
- Starting a transcript rebuild allocates the head's next generation and records an exact head
  revision/path proof. Bounded path and entry batches populate only that generation. Final
  publication revision-checks the unchanged selected path and every referenced current
  item-projection set before changing the head to Current.
- `Discuss in new branch` may reference only a finalized assistant projection. Branch creation validates the exact source thread, turn, item, projection identity and revision, the absolute canonical logical UTF-8 range within that projection, and the bytes returned by a bounded logical-range read before admitting the immutable context envelope.
- Reopen validation resolves every context envelope back to that exact finalized projection revision and absolute canonical logical UTF-8 byte range through the same bounded logical-range read. It validates immutable source-record agreement but does not require the historical source turn to remain on the named source thread's later mutable selected path. A missing or changed source is corruption rather than a reason to reinterpret the envelope as detached snapshot authority.
- Transcript-view positions are stable, sortable identifiers assigned by storage.
- Cursor reads name the exact transcript generation and return enough position and revision metadata for callers to detect stale provider responses.
- An exact retry at an already occupied per-turn sequence is classified as `SourceEventAlreadyAdmitted` before stale event-local revisions are considered, so ambiguous commit recovery can recognize the durable result without rewriting it. Different data at that sequence is `SourceEventCollision`; a gap or future sequence is `SourceEventSequenceConflict`.

## Write Commit Shape

- Write commits implement the durability and projection-revision requirements of the owning systems at the storage boundary.
- Large content construction uses a bounded sequence of revision-checked commands. Each command atomically appends the next bounded chunk batch and advances its manifest frontier; one final command seals the exact completed manifest and publishes its owner reference. No owner can reference building content.
- A surfaced failure during staging is reconciled against the exact content identity, frontier, and chunk digests before work resumes. A conflicting or abandoned building object remains unreachable and is never overwritten or reinterpreted as another payload.
- Correctness-sensitive operations contribute all Syndic and required Beryl-domain changes to one typed home-store command and are not reported successful until its `SyncAll` barrier completes. Image-bearing admission includes one bounded Beryl-state per-marker reference move whose exact marker and asset facts agree with the admitted Syndic payload.
- Validation rejection, revision conflict, and cancellation observed before writer admission leave the prior Syndic records unchanged. Cancellation after admission does not retract a command.
- A surfaced post-admission storage or persistence failure has an ambiguous durable outcome. Higher-level services retain exact natural identities and caller-owned editor state, gate publication, and reconcile through same-home verification or recovery plus coherent typed reads; they do not infer rollback from the error or blindly replay the mutation.
- Admission reconciliation is intentionally publication-gated and immediate: `Absent` proves the expected current draft still exists and all draft-derived result identities are absent; `ExactSubmitted` or `ExactAccepted` additionally proves the caller-named replacement draft, advanced input gate, immutable admitted owner, exact content and marker set; `Collision` authorizes neither replay nor success. Later mutations need not preserve an earlier admission's exact reconciliation snapshot.
- Live event ingestion stages an arbitrarily large typed provider frame through bounded resumable
  commands while published authority remains unchanged. Its final command writes the sealed frame
  reference, source event, turn lifecycle/frontiers, canonical item/content changes, exact source
  indexes, history activity, input-gate terminal transition when any, and transcript-staleness
  effect in one durable commit. An unreachable staged suffix is retained only for future garbage
  collection and cannot authorize history.
- Live-event, binding, finalization, item-projection, and transcript mutations expose
  narrow current-domain command constructors. The home store captures their physical domain basis
  after writer admission, while each mutation continues to validate its exact logical record
  revisions and never retries semantic conflict.
- Transcript projection building is deferred from live capture. Storage marks the selected transcript head stale and retains enough exact source and canonical data for deterministic bounded projection work.
- Projection work never calls a whole-message Markdown parser. It advances one persisted bounded
  state machine over indexed canonical ranges. A parser crash or process loss resumes from the
  durable generation frontier; a conflicting newer source revision supersedes that generation.
- Streaming assistant text may update the same canonical item and item-owned live manifest repeatedly, advancing revisions without changing stable item or content ids only before finalization. Branch creation remains unavailable during that interval.
- Recovery may complete or rebuild stale or incomplete projection work until it first becomes current under a proven terminal turn. It must not invalidate, rewrite, or reproject a finalized projection.
- History-summary completeness is derived by one shared routine from the current head and selected
  path facts. Live events, canonical finalization, projection publication, and transcript
  publication cannot force a contradictory cached boolean.
- Freezing or finalizing retained canonical history always advances that turn's own immutable
  content and frontier. It invalidates a named transcript and updates its history summary only
  after the deterministic ancestry index proves that the turn remains on that thread's selected
  path. Origin-thread ownership alone is not selected-path membership after replacement.
- Terminal turn status may commit before later cleanup or unfinished projection completion, but the terminal commit must mark any such derived frontier stale. No terminal-plus-current record may admit a later content update.
- Storage writes must not require buffering a full assistant response or full resource payload in memory before committing incremental state.
- A status-only terminal transition audits only the source items already admitted for that turn.
  Callers flush any pending typed delta, use composite item and provider-frame reads for exact
  prefix/completion proof, and scan turn-item indexes with bounded cursor pages. Every completed item
  must have a sealed, structurally complete, kind-consistent final frame whose referenced content
  frontier is durable; unfinished, malformed, unsupported, or undisposed observed items reject
  history-complete publication. The terminal event does not supply a provider snapshot and cannot
  prove or reconstruct an unobserved item. The storage API imposes no smaller whole-turn item ceiling
  and does not expose a raw iterator or global-domain quietness requirement for that audit.
- No API may detach or rewrite a submitted turn parent edge. Replacement edits create a new turn and update only the selected thread bindings.
- Starting or cancelling replacement edit never mutates the current draft's immutable parent. The accepted replacement turn derives its parent from the immutable target turn, while a bounded current transcript-generation entry proves that target belonged to the selected path when edit intent was admitted.

## Ordinary Thread And Draft Mutation Boundary

- Empty ordinary-thread creation atomically contributes the thread, empty current draft referencing the canonical sealed empty-composer content, draft reverse index, current zero-entry transcript head, complete history summary, unbound binding revision, and binding head. Natural thread and draft identities are caller-owned inputs.
- Thread-from-tail creation requires one coherent nonempty source-thread selected-tail proof and source activity fact. It creates no turn or transcript-entry copy, gives the new draft that exact tail as immutable parent, publishes a zero-entry stale transcript head, and uses the caller's exact non-regressing timestamp for the draft and history summary.
- Creation rejects any target identity or required-record collision. A bounded stable reconciliation read classifies the natural identity set as absent, exactly committed, or collided after replay or an ambiguous admitted outcome.
- Current-draft reads stabilize the reverse index around the thread and draft reads and reject concurrent or contradictory publication rather than returning a mixed generation.
- A payload save first stages an exact content object without changing the draft. Final publication requires the exact current thread revision, draft identity, draft revision, complete building-content frontier, and expected manifest. It seals the content, advances only the draft revision, replaces the content reference, preserves immutable thread, parent, context, replacement-intent, identity, and creation facts, and updates the draft reverse index and history-summary activity in the same contribution.
- Exact payload equality is a typed caller-visible no-change decision and produces no home command. An update timestamp may equal but never precede current durable draft activity.
- Pre-admission cancellation is owned by `HomeCommand`. A mutation API makes no rollback claim after writer admission; exact current-draft and natural-creation reads are the reconciliation authority after a surfaced ambiguous failure.
- Draft-only thread revision advance does not invalidate a CAS binding whose observed selected-path tail and digest still agree exactly. A tail or digest change publishes a new unbound binding revision for the pending path; it never copies prior valid lineage onto an undelivered turn.

## Sealed Content Text Reads

- One public text-only read accepts an exact sealed `ContentReference`, an absolute logical UTF-8
  start offset, and a caller-supplied payload-byte ceiling no greater than 65,536 bytes.
- The read validates exact content identity, revision, encoding, summary, ownerless sealed
  lifecycle, final manifest frontier, and marker-free authority before resolving only the needed
  `ContentTextSpanRecord` and `ContentChunkRecord` entries. It never assembles or decodes the whole
  encoded composer as an intermediate value.
- Every start and returned continuation is a UTF-8 boundary. A nonterminal page ends at the last
  complete scalar that fits the caller's ceiling; a ceiling too small for the next scalar rejects
  instead of returning an empty nonterminal page or exceeding the bound.
- A missing top-level manifest returns absence. Malformed envelopes and inconsistent referenced
  records return the existing typed read or invariant failures, while a manifest change across the
  bounded assembly returns `ConcurrentChange` rather than publishing mixed state.
- Valid marker-bearing content is rejected explicitly at this text-only boundary. Marker-aware
  projection continues to use ordered content pieces and marker resolutions.
- The result reports the exact logical start, bounded UTF-8 payload, optional next offset, exact
  sealed reference, and checked stored key-and-value byte total for both manifest reads, span-index
  pages, and chunk records.

## Resource Payloads

- Heavy resources are addressed by metadata records and explicit byte ranges.
- Phase 7 code and table resources retain one exact canonical content reference and half-open
  logical UTF-8 range. The content-text-span and content-byte-span indexes map a requested resource
  range to the minimal bounded encoded chunk ranges for both composer and plain UTF-8 content;
  storage does not duplicate those bytes into projection records or sidecars.
- Large externally owned byte payloads may live in sidecar files when their owning later feature
  can admit and range-read them through an explicitly bounded home-store contract. Phase 7 does not
  broaden the current whole-buffer sidecar boundary for textual Markdown.
- Sidecar paths are storage-owned implementation detail and are not exposed as stable public identities.
- Resource writes record media type, exact backing range, byte length, versioned chain digest,
  preview range when available, resource kind, and applicable language, logical-line, table-row,
  table-column, and header facts.
- Resource range reads use half-open resource-relative ranges, reject reversal, overflow, or an end
  beyond the resource length, and return at most 65,536 bytes plus an exact continuation fact.

## Transcript Provider Support

- Storage-backed transcript providers read transcript-view pages, exact immutable branch-context envelopes, projection record sets, resource metadata, and resource ranges from this package.
- The provider boundary may reject missing, stale, oversized, unsupported, or policy-denied reads using typed errors derived from storage state.
- Renderer-facing code must not call `syndic-storage` directly.
- Storage does not own resident-memory policy; it only supplies bounded durable reads.

## Failure And Recovery

- Incomplete turns, failed turns, stream loss, and local ingestion failure are represented explicitly in durable state.
- Storage does not treat CAS reconnect, resume, late subscription, process restart, historical reads,
  or a later status-only terminal event as replay of missing source events. After live authority is
  lost, retained admitted source records remain the exact prefix and recovery converges through an
  explicit incomplete disposition, retaining a typed unsupported-history reason when applicable,
  rather than fabricating a complete item set.
- Proven loss of an active execution session atomically stales its binding and retires the old
  projection. Once no usable projection authority remains, a later source-less terminal transition
  may close local turn capture as incomplete. A possibly dispatched start is never reset to pending,
  and a possibly dispatched steering fragment is never reset to retryable solely because the session
  disappeared.
- Storage startup validates exactly one current draft per thread, matching thread/draft ownership, one-way draft-to-turn identity consumption with no live raw-payload collision, committed-tail reachability, immutable parentage, monotonic revisions, accepted-input ordering, CAS-binding uniqueness, source-event ordering and per-item replay, terminal closure and finalization frontiers, stale projection markers, and referenced resources.
- Unfinished or stale projections can be invalidated and recomputed from canonical items and source events. A finalized projection is durable history and is never an in-place rebuild target.
- Corrupt, missing, or unsupported records produce typed storage errors rather than silent fallback to CAS history or GUI-local caches.
- Unreachable turns and unreferenced sidecars are not startup errors and are not deleted; they remain for the future explicit garbage-collection design.

## Privacy And Redaction

- Storage APIs accept only data that has already crossed the owning system redaction boundary.
- Secret-like fields must be rejected or redacted before durable commit.
- Hidden developer instructions and policy-private control payloads are not transcript content and must not be stored as user or assistant projection records.
- Diagnostic payloads stored durably must be bounded and must not include raw auth headers, tokens, cookies, environment secrets, or capability tokens.
