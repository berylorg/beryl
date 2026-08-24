# Goals

Provide the reusable production storage boundary for Syndic-owned durable threads and their
intrinsic properties, current drafts, submitted conversation state, source events, reference
metadata, compact thread summaries, transcript views, unfinished rebuildable projection state, and
finalized immutable projection history.

Support low-latency Beryl reads over large captured conversation histories, thread lineages, and
activity projections while keeping UI memory growth bounded.

Support short durable write commits for live CAS event ingestion, streaming assistant output, generated artifacts, and future concurrent user or subagent activity.

## Non-goals

- Calling OpenAI, ChatGPT, Codex App Server, or any model provider.
- Owning authentication, token refresh, sandboxing, approvals, skills, MCP, enterprise policy, or live execution.
- Owning GPUI transcript presentation, renderer residency, scroll behavior, or widget state.
- Owning Beryl runtime/root registries and availability, window sessions and claims, settings,
  installed themes, durable host jobs, catalog query indexes, or asset lifecycle policy.
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

- Stable cross-package Syndic identities, typed revisions, image-label ordinals,
  discussion-context owner/digest facts, external CAS identities, and exact CAS generation facts
  belong to `beryl-model`.
- Syndic-specific lifecycle, ordering, immutable parent/context, transcript-position,
  recovery-budget, selected-path, CAS-represented-prefix, and CAS-lineage proof values belong to
  this package and perform no clock observation, identity generation, filesystem access, provider
  work, or policy lookup.
- An idle submission preserves the draft identity's exact 128-bit payload while changing its typed identity from `SyndicDraftId` to `SyndicTurnId`.
- A context-bearing draft's typed context-owner descriptor follows the same transition without rewriting its immutable envelope.
- Discussion-context envelope V1 preserves the exact selected UTF-8 text and computes its typed context digest as SHA-256 over those exact bytes; the pure constructor observes no clock and accepts creation time from its caller.
- One immutable accepted-input record is the permanent admission receipt. It keeps one
  `SyndicAcceptedInputId`, the expected source thread, draft, and gate revisions, source and
  replacement draft identities, exact source combined draft root, root-bound resolved content and asset
  proof, admission time, and one route-generation identity. Revisioned generation heads plus
  bounded per-input leaves resolve
  pending, steering, retryable, next-turn, and terminal delivery state without copying a target
  proof into every record. There is no second queued-input identity or queued asset-owner identity.
- A resolved accepted-input steering view names the exact active Syndic turn through its selected
  route-generation state. Exact ready and delivering reads admit only an immutable input whose
  current route leaf is respectively `Routed` plus `Admitted`/`Retryable`, or `Routed` plus
  `Delivering`, whose selected generation is a live steering target, and whose steerable gate,
  active binding, execution snapshot, one-way CAS-turn publication, loaded generation, and CAS
  identities all agree. Each read uses a fixed set of twelve point reads, stabilizes the gate,
  route head, and binding head with first/last reads, and never scans route membership. Missing or
  stably ineligible input returns no view, inconsistent relationships are corruption, and an
  anchor race is a concurrent change. Fresh native CAS
  lineage establishes an empty selected path. Continuation, resume, fork, and recovered injection
  require an exact non-empty committed tail. Syndic publishes no valid in-place-rollback lineage.
- Public value serialization, where explicitly provided, is a transport value shape only. Versioned package codecs and exact domain declarations remain the sole persisted-schema authority.

## Public Boundary

- This package implements the storage boundary consumed by `doc/systems/syndic-conversation-history/design.md` and `doc/systems/cas-live-syndic-transcript/design.md`.
- The package exposes operations for constructing its opaque domain handle, creating threads and
  current drafts, opening, reading, publishing, and disposing bounded editor-candidate sessions,
  durable two-lane pre-finish mutation staging and reconciliation, authenticated finish-to-builder
  custody transfer, revisioned predecessor-linked combined sequence/index/marker-commitment candidate transactions, bounded draft text
  and marker reads, compact restoration validation, durable edit-history frontier reads and
  retention, exactly-once ordinary transition append, direct authenticated historical-root
  adoption for undo/redo, and exactly-once durable candidate settlement and replay,
  exact-root bounded marker sealing, exact-root streamed `ComposerV1` materialization, atomic draft submission/replacement, accepted-input
  admission, revision-bound ready-steering source and candidate pages, revision-bound next-source
  pages, atomic accepted-input promotion and exact reconciliation, exact steering-delivery
  claim/outcome transitions, exact stop admission, dispatch claim, safe reopening, terminal
  consumption, restart abandonment and reconciliation, exact context-compaction admission,
  dispatch claim and request reconciliation, CAS-turn publication, provider-terminal settlement,
  lifecycle-continuation settlement, stop handoff and restart consumption, committing live-source
  events, capture-gap terminal publication, scoped repair-required reconciliation, explicit
  incomplete convergence, advancing
  the exact terminal item-finalization frontier, exact terminal-turn historical repair with
  CAS-source provenance, reading historical summaries, reading
  thread/draft/turn metadata,
  reading thread-lineage pages, point-reading image-label frontier and origin authority, reading
  activity-query pages, reading immutable branch-context envelopes by context-owner identity,
  reading transcript-view pages, reading projection records, reading resource metadata, and reading
  resource byte ranges, plus reading bounded logical UTF-8 pages from one exact sealed content
  reference.
- The public pre-finish page boundary is one storage-neutral batch preparation, mutation-
  contribution, and reconciliation family. Preparation consumes one authenticated source staging
  head, its matching editor-candidate session, and one nonempty boxed slice of physical-page inputs;
  it returns one opaque prepared batch owning the exact source closure, every canonical target page
  and matching progress receipt, the final head and session, and checked aggregate counts. The
  mutation operation uses that prepared value for one Syndic `DomainMutation` contribution without
  materializing another page collection. Reconciliation uses the same bounded prepared closure and returns only
  `SourceSelected` or `TargetSelected`; invalid, overflow, occupied-identity, partial, or corrupt
  closures return typed errors and no successor authority. These APIs contain no GPUI or widget
  type.
- The batch boundary reuses `DraftMutationStagingHeadV1`, `DraftMutationStagingPageV1`,
  `DraftMutationStagingProgressReceiptV1`, `DraftEditorCandidateSessionV1`, their existing canonical
  codecs, and their existing three staging families plus the candidate-session family. It adds no
  persisted family, record tag, command kind, schema version, widget-page identity, compatibility
  adapter, or operation-wide durable history.
- The public post-finish builder boundary accepts only the exact draft/session/operation identity,
  authenticated current build endpoint, and one bounded command ceiling no greater than 256
  physical staging pages/items, 256 fragments, and 65,536 inserted UTF-8 bytes. Preparation derives
  the next source or proposal staging window by point-reading durable
  storage at the endpoint's lane frontier and returns an opaque prepared command owning only that
  bounded source/target closure. It accepts no staging-page bytes, caller fragment, ordinal-one
  restart request, prefix proof, or app-built edit reconstruction. Status and same-home resume return
  the authenticated current endpoint or terminal settlement and use the same preparation boundary.
  Window acquisition has at most two page/receipt reads per physical page plus the exact fixed nine-
  endpoint allowance and the checked 34,144,256-byte complete encoded-value ceiling defined under V5
  bounds; item-specific structure reads remain under their separate tree/index limits.
- The package exposes checked maximum mutation-footprint descriptors for exactly two public durable-
  start operations: idle draft submission and accepted-input promotion. Each descriptor derives
  its maximum record count and encoded key-plus-value bytes from the operation's package-owned V5
  record shapes and declared field bounds with checked arithmetic. It is stable for a given schema
  contract and is testable without opening a home or constructing caller payloads.
- A Syndic footprint describes only the records contributed by that Syndic operation. It does not
  accept a caller-provided byte estimate and does not include Beryl-state asset-owner transfer,
  home-store participant metadata or home revision, Fjall journal framing, a capture reserve,
  filesystem allocation, or admission policy.
- Routine open validates the persisted domain and family registration/schema declarations and
  reacquires one fresh typed handle. It does not walk application records, resume a physical cursor,
  or publish work merely because a prior service observed it. Exhaustive record walks are reserved
  for an explicit schema-validation boundary, scrub, background-maintenance pass, or
  corruption-evidence investigation.
- Recovery and ambiguous-outcome reconciliation start from an exact durable natural anchor supplied
  by the fresh service. One fixed-work read follows only that anchor's bounded natural closure: the
  current gate, blocking turn state, binding head and binding, referenced execution snapshot and
  active CAS turn when present, selected route generation, and matching stop or compaction record
  when named. It publishes only closed typed cases and double-observes the mutable anchors. A
  mismatched or missing authority pair is coherent corruption; anchor drift is concurrent change.
- Recovered-pending and accepted-work discovery use their explicit compact source families and
  revision-bound bounded pages. A physical input-gate-family sweep is not routine startup or reopen
  authority. A deliberately exhaustive work audit is background maintenance or corruption
  investigation and cannot itself dispatch, replay, or consume work.
- Public APIs use Syndic identities and revisions as their stable boundary.
- External execution ids, including CAS thread ids, turn ids, and item ids, are stored as source metadata and never become the only primary key.
- Public reads are bounded by caller-supplied limits or explicit range requests.
- Every exact Syndic family defines stored and decoded schema limits enforced through the bounded
  home-store read boundary. Ordinary point values carry no accounting wrapper; their limits pass
  before publication. Natural pages, text/range reads, route and lineage queries, activity
  queries, provider-observation reads, and recovery metadata report checked item, stored-byte, and
  decoded-byte totals where useful.
- Composite reads retain only a fixed number of bounded constituents. They do not concatenate a
  complete logical collection or retain a data-dependent collection of pages merely because each
  individual read is bounded.
- Branch-context reads return the exact bounded immutable envelope and owner revision; they do not manufacture transcript-view records or synthetic turns.
- Public writes keep each metadata record, content chunk, mutation batch, and read page bounded. They do not reject a logical draft or canonical text merely because its complete content exceeds one record.
- Public mutations require expected revisions for every correctness-sensitive thread, draft,
  binding, route, or accepted-input record they read or change. Accepted-input promotion does not
  read or mutate the current draft record; it preserves the exact thread-to-draft reverse binding
  while advancing that index's enclosing thread revision.
- Same-home recovery invalidates every prior-generation handle, read value, cursor, worker, and
  process-local execution capability. A fresh service reacquires a fresh typed handle from the
  validated registration and reads the exact durable thread, gate, turn, selected-path, binding,
  snapshot, and lineage closure needed to establish new external execution authority. No authority
  survives from the failed service generation; durable records are the only input to the new
  service's fresh authority acquisition.
- Recovery preparation is a read-only public boundary with two explicit request scopes. Current-path
  preflight proves a complete recovery-complete selected path before input admission and returns one
  compact replayable cursor-source proof, or the native empty prefix without model metadata when
  that path is empty. Pending-turn restart preparation applies the same two-pass boundary only to
  the pending selected turn's parent. Its immediate predecessor alone may instead be exact
  authority-lost tail context when its latest source event is the matching source-less
  `Incomplete(AuthorityLost)` terminal, its nonempty item frontier is fully finalized with no open
  or history-blocking item and no provider-observation issue, and every item passes the ordinary
  recovery proof. Earlier ancestors must remain recovery-complete.
  `RecoveryAssembly::Ready` contains only the version, thread and selected-path proof, represented
  prefix, exact item and UTF-8 byte totals, sequence digest, and source revision. It contains no
  item collection or item-text accessor. The selected-path and represented-prefix relationship
  re-establishes the applicable current-path or pending-parent eligibility contract on cursor open.
- A ready proof opens one opaque non-cloneable `RecoveryCursor`. Each read fills a caller-supplied
  fixed page with at most one nonempty 65,536-byte valid UTF-8 range and returns that page plus its
  one-based sequence ordinal, closed user-input-text or assistant-output-text role, declared item
  byte length, item-local offset, item terminal, and sequence terminal. The requested byte ceiling
  is applied before storage access and may be smaller than the physical page. The terminal page is
  returned
  only after exact item/byte totals and the shared V1 digest accumulator agree; the next
  revision-checked read returns one exact EOF, and later reads reject. Opening, every page read, and
  EOF bind to the proof's exact source revision and selected path. Internal projection text-source
  identity is not part of this public recovery boundary.
  Recovery-complete means `complete`, `interrupted`, or `failed` with the complete finalized item
  frontier, no history-incomplete disposition, and no provider-observation issue; it excludes
  `incomplete`. Authority-lost tail context is a separate scope-bound eligibility fact; it does not
  make that predecessor recovery-complete or change its stored lifecycle. The returned domain
  `source_revision` records preflight provenance only; later
  recovery-proof publication uses the then-current domain revision plus exact current thread,
  selected-path, and binding expected revisions rather than the older global revision.

## Logical Records

- Logical record names mirror the Syndic history and CAS-live capture system contracts; this package owns their stored representation and typed API behavior.
- A thread record stores stable thread identity, committed conversation-tail id when any, current
  draft id, thread revision, optional immutable parent-thread handoff binding, thread-lineage depth,
  chain digest and deterministic ancestor skip, inherited and current image-label frontiers, and
  optional branch-context owner id. Top-level threads use the canonical root lineage facts and a
  zero inherited label frontier.
- A thread-execution record is keyed one-to-one by stable thread id and stores the exact immutable
  `ExecutionBinding` accepted at creation. It has no replacement mutation or mutable record
  revision. A child thread inherits its parent's exact value. CAS binding and execution-snapshot
  records retain execution copies as provenance but must agree with this canonical record.
- A thread-attributes record is keyed one-to-one by thread id and owns a nonzero monotonic
  attributes revision, optional accepted generated title with its exact source user turn, sealed
  content identity and digest, selected-path digest, thread revision, and generation time, and the
  ordinary, open-branch-discussion, or archived-branch-discussion state.
  Generated title acceptance and open-to-archived transition are one-way.
- A thread-usage record is keyed one-to-one by thread id and owns a nonzero monotonic usage revision
  plus an optional exact token-usage observation. The observation retains nonnegative last and
  total counters, optional positive model context window, observation time, and the exact immutable
  `ExecutionBinding`, binding revision, CAS thread, managed-process generation, loaded-thread
  generation, connection generation, and monotonic provider-control ordinal. Usage publication
  requires the current valid or active route and cannot advance thread or attributes revisions.
- A current-draft record stores stable draft identity, owning thread id, selector revision, one exact
  immutable combined `DraftPieceRootReferenceV1`, one exact published
  `DraftEditHistoryFrontierReferenceV1`, one closed `DraftSubmissionIntent`, and timestamps. The
  intent is exactly `Ordinary`, `DiscussionContext` with its immutable context-owner identity, or
  `Replacement` with the exact target and bounded selected-transcript proof. It stores no ordinary
  or generic conversation parent. The context envelope itself occupies the separate
  `context-envelopes` family and names the first branch source turn. Draft-owned context validation
  proves the source records and parent-thread binding; it does not require that immutable source
  to equal the child thread's later mutable committed tail.
- `DraftPieceRootReferenceV1` is the combined draft-root reference. It binds the owning draft, one
  closed `DraftPieceRootBuildIdentityV1`, optional composite-sequence root-node
  identity and complete sequence summary, optional marker-identity-index root-node identity and
  complete index summary, optional marker-order-commitment root-node identity and exact
  `DraftMarkerCommitmentV1`, and one combined-root digest. The build identity is either
  `DirectCanonicalEmpty` with the deterministic draft-scoped operation defined below, or
  `EditorCandidate` with exact editor session and caller-owned operation identity for an edit or
  sealed-content import. The referenced immutable
  `draft-piece-roots` record repeats the build identity, all three roots, and summaries. A reference with
  a different owner, build identity, structure identity, summary, or digest is invalid even when its logical byte extent
  happens to match.
- `DraftLogicalExtentV1` is the exact public pair of checked logical UTF-8 byte length and logical
  line count committed by a combined root. Its line count is zero exactly when byte length is zero
  and otherwise equals the committed checked newline count plus one. Every edit settlement,
  candidate root/history head, activation binding, range-source wrapper, and restoration request that names a
  logical extent uses this pair rather than a byte length alone.
- V1 piece and tree digests use domain-separated SHA-256 over canonical package encodings. A text leaf hashes
  its exact UTF-8 bytes and summary; a marker leaf hashes its stable identity, order key, final
  label, and zero-text summary; an internal node hashes its height plus each ordered child identity,
  digest, subtree aggregate, and
  composite search envelope. A marker-identity leaf hashes the stable marker id, final label,
  same-anchor order key, and exact sequence marker-leaf identity and digest. An identity internal node hashes
  its height plus each ordered child identity, digest, checked record count, and disjoint stable-id
  search envelope. The sequence-root, identity-root, and marker-order-root digests each commit their
  exact canonical shape and summary; the combined-root formula below commits all three. Owner and
  the complete closed build identity remain in the enclosing root record rather than content
  digests. A marker-order commitment leaf hashes only its exact marker id and final label; its internal node
  hashes its height plus each ordered child identity, digest, count, and maximum label. The
  commitment root digest therefore authenticates one exact persisted tree shape and ordered leaf
  sequence. It is not `SequentialMarkerSummaryV1`, and semantic equality between
  distinct combined roots still requires bounded comparison when a consumer needs it.
- Every sequence leaf, internal node, sequence root, and combined root carries a canonical checked
  logical text summary with `u64` UTF-8 byte length, newline count, and logical line count. Empty
  text has all three values zero. Nonempty text has logical line count equal to checked newline
  count plus one, so a trailing newline includes the final empty logical line. A nonempty text leaf
  counts exact byte `0x0A` occurrences in its validated UTF-8 payload; a marker leaf contributes
  zero bytes, newlines, and lines. An internal node checked-adds child byte and newline counts and
  derives its line count from the combined byte count rather than adding child line counts. Its
  canonical child aggregates commit all three fields. Sequence-root summary bytes commit all three
  fields, and the combined-root digest commits that complete sequence summary. Overflow,
  byte/newline/line disagreement, or a line count other than zero for empty and newline count plus
  one for nonempty is invalid.
- For the following root hashes, `H` is SHA-256, `LP(x)` is the unsigned big-endian `u64` byte
  length of `x` followed by `x`, and every named domain is its exact ASCII byte string. The empty
  sequence-root digest is `H(LP("syndic/draft-sequence-root/v1/empty"))`; the empty identity-index-
  root digest is `H(LP("syndic/draft-marker-identity-index-root/v1/empty"))`; and the empty marker-
  order-commitment root digest is
  `H(LP("syndic/draft-marker-order-commitment-root/v1/empty"))`. For canonical sequence-summary
  bytes `S`, identity-index-summary bytes `I`, and canonical `DraftMarkerCommitmentV1` bytes `M`,
  every combined-root digest, empty or nonempty, is
  `H(LP("syndic/draft-combined-root/v1") || LP(S) || LP(I) || LP(M))`. A text-only root uses a
  nonempty sequence summary plus the exact empty identity-index summary and empty marker
  commitment. A marker-only root has zero UTF-8 length but nonzero sequence piece/marker, identity-
  index, and marker-commitment summaries.
- The canonical empty combined root has no sequence, identity-index, or marker-order-commitment root-
  node identity; all three heights and every logical byte, newline, line, piece, marker, and identity
  aggregate are zero; and it uses exactly the empty digests above. Any empty root with a node,
  nonzero aggregate, or another digest is invalid; any nonempty marker set missing either marker
  structure, or either marker structure present for a zero-marker root, is invalid.
- `CanonicalEmptyDraftRootBuildOperationIdV1` is the first 16 digest bytes, without UUID bit
  rewriting, of
  `H(LP("syndic/canonical-empty-draft-root-build-operation/v1") || LP(draft_id_bytes))`, where
  `draft_id_bytes` is the exact 16-byte `SyndicDraftId` payload. The package derives this natural
  identity for every draft created directly with the canonical empty root, including initial
  thread creation and replacement-draft creation. An edit that produces empty content retains its
  caller-owned edit identity inside `EditorCandidate(session, operation)`. A root record repeats
  owner and its complete closed build identity. An occupied natural root key with different
  canonical bytes yields `OccupiedIdentityNoncommit` for an editor candidate, or a creation
  collision for direct empty creation, and never selects an alternate identity.
- `DraftPieceRootNaturalKeyV1` encodes the 16-byte draft id, one closed one-byte build-identity tag,
  and its fixed payload. `DirectCanonicalEmpty` is exactly 33 bytes and carries only the 16-byte
  `CanonicalEmptyDraftRootBuildOperationIdV1`; `EditorCandidate` is exactly 49 bytes and carries the
  16-byte session id then 16-byte caller-owned operation id. Thus direct empty creation remains
  draft-scoped, while equal operation
  ids in different editor sessions occupy different root, build, fragment, and settlement
  namespaces. Candidate node and leaf identities are allocated beneath that complete
  `(draft, session, operation)` scope; no digest or operation id detached from the session is a
  natural key.
- Draft piece leaves are exactly nonempty UTF-8 text leaves or zero-width marker leaves. A marker
  leaf stores stable marker identity, same-anchor order key, and final label ordinal but no image
  bytes. `DraftCompositeSearchKeyV1` totally orders the boundary before all markers at an absolute
  UTF-8 anchor, each marker at that anchor by `(order key, marker identity)`, and the boundary after
  all markers. Bounded immutable internal nodes store ordered child references, child digests,
  checked subtree UTF-8/newline/line/piece/marker aggregates, and one relative inclusive-lower/exclusive-upper
  composite search envelope per child. Child envelopes are nonempty, disjoint, contiguous, cover
  the parent envelope exactly, and are committed by the node digest. During descent the reader
  accumulates checked UTF-8 prefixes, translates each relative fence to the root coordinate, and
  binary-searches the at-most-128 child entries. Zero-byte marker-only children at one anchor are
  therefore distinguished by order key and identity rather than by text extent. Unchanged nodes
  and leaves may be referenced by multiple immutable roots; no mutation rewrites a reachable node
  or leaf.
- The persistent marker-identity index is keyed by stable marker id. Each immutable leaf stores one
  id, final label ordinal, same-anchor order key, sequence marker-leaf identity, and sequence-leaf
  digest. It stores no absolute UTF-8 anchor, composite position, piece ordinal, or other rebased
  location. Bounded immutable internal nodes store ordered child references, child
  digests, checked subtree record counts, and disjoint inclusive-lower/exclusive-upper stable-id
  search envelopes. Lookup binary-searches at most 128 children per level and authenticates the
  leaf. Location validation additionally requires a caller-supplied composite position or anchor
  witness and verifies the leaf's stable occurrence facts through one bounded sequence descent at
  that location. ID-only location discovery is not part of the boundary. Ordering summaries in the
  composite tree are not global stable-id absence authority.
- The persistent marker-order commitment tree is independently ordered by exact composite marker
  order but stores no text anchor, order key, sequence-leaf identity, or position. Each immutable
  leaf stores exactly one stable marker id and final label. Bounded-height internal nodes commit
  ordered child identities, structural digests, checked counts, and optional maximum labels. The
  selected root produces `DraftMarkerCommitmentV1`; the combined root repeats and authenticates its
  exact root identity and commitment. A text-only edit reuses this whole tree, while marker insert,
  remove, move, or same-id replacement path-copies only logarithmic paths. Exact undo or redo adopts
  the already authenticated historical combined root and commitment directly.
- `DraftCompositePositionV1` is a package value independent of GUI types. It contains an absolute
  logical UTF-8 byte offset and a closed gap witness: unambiguous, before all markers at the anchor,
  between one exact adjacent ordered marker pair, or after all markers. Transaction, page,
  clipboard, and restoration APIs accept it only with the complete base combined-root reference and
  validate the UTF-8 boundary, marker identities, order keys, and adjacency through bounded tree reads.
- `DraftEditorCandidateSessionIdV1` is an opaque caller-owned 16-byte identity.
  `DraftEditorCandidateSessionV1` is the one bounded mutable head keyed by exact 16-byte draft and
  session identities. It retains only the immutable opening durable-base selector
  revision plus root/history pair, the latest published candidate generation/root and matching
  immutable published edit-history frontier reference plus selector revision, the newest adopted
  candidate generation/root and matching exact live edit-history frontier revision reference,
  monotonic session and dirty generations, and its active or disposed lifecycle. Each published or
  newest root is valid only with its named history reference selecting that same root and candidate
  generation. It also has one fixed-size optional tagged active-operation custody slot. `Staging`
  contains the exact operation id, canonical begin identity, predecessor candidate generation/root/
  history pair, and staging head/progress endpoint. `Building` contains the exact operation id,
  finish-derived canonical proposal identity, predecessor pair, and admitted build/progress
  endpoint. The exact finish-to-build command replaces `Staging` with `Building`; both tags can
  never coexist. The slot contains no proposal payload or receipt chain. The head contains no text, marker collection, whole edit, undo payload,
  root graph, transcript fact, or current-draft authority. The tagged
  `draft-editor-candidate-sessions` family also holds bounded immutable open, publication, and
  disposal receipts keyed by their natural session/operation identities; these make exact replay
  and byte-disagreeing identity collision point-readable without growing the head.
- `DraftMutationOperationIdV1` is an opaque caller-owned 16-byte mutation identity. A valid caller
  assigns it one-to-one to one app-neutral edit transaction and reuses it for that transaction's begin,
  pages, finish, terminal election, and reconciliation; storage never derives it from GUI types.
  `DraftMutationStagingEncodingVersionV1` is the exact one-byte value `1` and selects the canonical
  app-neutral `MutationBeginV1`, lane-page, and `FinishInputV1` encodings owned by this package.
  `DraftMutationStagingIdentityV1` is the exact 48-byte concatenation of draft id, editor-candidate
  session id, and `DraftMutationOperationIdV1`. It is the natural key of the mutable
  `DraftMutationStagingHeadV1` in `draft-mutation-staging-heads`. The V1 head repeats that identity
  and retains one bounded canonical `MutationBeginV1` plus its digest; exact session generation,
  predecessor candidate generation/root/history pair and logical extent; predecessor caret and
  directed selection; exact staging encoding version; independent source- and proposal-lane frontiers;
  latest staging-progress receipt key/digest; and exactly `Receiving`, `Finished(finish)`,
  `Building(build endpoint)`, `Cancelled`, `Rejected`, `Conflict`, or `Error` lifecycle. Each lane
  frontier contains only its next cursor and one-based ordinal, checked cumulative `u64` item and
  canonical-byte totals, and cumulative identity. `Finished` stores the bounded canonical
  `FinishInputV1`, which repeats both exact final lane frontiers and intended successor positions.
  No head field contains page payload, a page collection, a receipt chain, a candidate root, or
  current-draft authority. The head still stores both the selected staging-progress receipt key and
  its digest. Its canonical head digest is SHA-256 over the length-prefixed exact ASCII domain
  `syndic/draft-mutation-staging-head/v1`, canonical key, and every canonical head field except the
  selected receipt digest; it includes the complete selected receipt key and thus its transition
  ordinal. The package point-reads that key, canonically decodes the receipt, recomputes its digest,
  and requires it to equal the independently stored selected receipt digest. Canonical byte
  equality, not either digest alone, proves replay. The four
  terminal lifecycles are immutable and accept no later page, finish, transfer, or outcome change.
- `draft-mutation-staging-pages` contains immutable tagged `DraftMutationStagingPageV1` records.
  The exact 57-byte key is the 48-byte staging identity, one canonical lane tag (`SourcePage = 0`,
  `ProposalPage = 1`), and an unsigned big-endian one-based `u64` lane ordinal. The value repeats
  that key, exact input and successor cursors, positive item and retained-canonical-byte ceilings,
  prior and successor cumulative identities, checked cumulative counts and bytes, canonical page
  bytes, and a domain-separated page digest. Source and proposal lanes therefore cannot collide by
  ordinal, and a cursor or lane mismatch is canonical byte disagreement rather than continuation.
  The page digest is SHA-256 over the canonical length-prefixed sequence, in order, of the exact
  ASCII domain `syndic/draft-mutation-staging-page/v1`; the complete canonical page key; the progress
  transition ordinal stored separately from that key; input and successor cursors; item and
  canonical-byte ceilings; prior cumulative identity; checked successor cumulative item and
  canonical-byte totals; and exact canonical page-item bytes including their item-count framing.
  Its preimage excludes exactly the
  successor cumulative identity and the page-digest field itself; neither exclusion is represented
  by a zero or other placeholder. After fixing the page digest, the successor cumulative identity is
  SHA-256 over the canonical length-prefixed exact ASCII domain
  `syndic/draft-mutation-staging-lane/v1/link`, prior cumulative identity, and page digest, in that
  order. The stored successor cumulative identity must equal that recomputation. Canonical decode,
  local validation, and storage-backed closure validation recompute and require both the page digest
  and successor cumulative identity. Digests may reject inequality early, but replay still requires
  canonical byte equality of the complete page and target closure.
  The family has no mutable value, terminal tag, or alternate page identity.
- `DraftMutationStagingProgressReceiptV1` is an immutable fixed-size record in
  `draft-mutation-staging-progress`. Its exact 56-byte key is the staging identity followed by an
  unsigned big-endian one-based `u64` transition ordinal. Its value repeats the key; exact prior
  receipt key/digest, absent only at ordinal one; command kind; optional affected page key/digest or
  finish digest; complete before/after source and proposal lane frontiers; before/after staging-head
  digests and lifecycle; exact candidate-session custody before/after tags and endpoints; optional
  build endpoint for the one custody transfer; one closed optional fixed-size terminal-evidence
  union; and its domain-separated receipt digest. `Rejected` evidence contains a closed reason code
  plus the exact rejected begin/page/finish natural anchor and canonical digest. `Conflict`
  evidence remains structurally present and contains the expected same-session candidate generation/
  root/history pair, that same session's exact observed current pair meaning its newest pair, and the
  observed session revision. The observed pair is neither another session nor the durable current-
  draft selector.
  `Cancelled` evidence contains the cancellation-
  request identity, election source lifecycle, and writer-admission disposition. `Error` evidence
  contains either a closed operational code and affected natural anchor or an occupied-identity
  comparison witness with the natural key, stored and requested canonical digests, first differing
  byte offset, and stored/requested byte-or-end values. Every form states exact absence of candidate
  adoption. The receipt
  contains no page bytes. Begin, finish, pre-build terminal election, and finish-to-build transfer
  each append exactly one such receipt. A physical-page batch appends exactly one existing
  source- or proposal-page receipt for each page, chains those receipts consecutively, and
  atomically publishes only the final staging-head and candidate-session custody endpoints. No
  intermediate mutable head or session endpoint is written or observable. The receipt digest is
  SHA-256 over the length-prefixed exact ASCII domain
  `syndic/draft-mutation-staging-progress-receipt/v1`, canonical key, and every preceding canonical
  receipt field, including its before/after head digests; it is a commitment and never substitutes
  for byte comparison of a point-read target closure. Excluding only the selected receipt digest
  from the head-digest preimage makes the head digest computable before this receipt digest, while
  the head's selected key, independently authenticated stored receipt digest, receipt-owned head
  digests, and canonical target-closure comparison preserve the complete closure.
- The closed pre-build terminal-evidence union structurally includes `Cancelled`, `Rejected`,
  `Conflict`, and `Error`; each outcome is proven by the terminal
  staging head, its selected immutable target receipt, the receipt's immediate source closure, and
  that receipt's matching outcome evidence. A terminal-before-begin command has ordinal one, no
  staging source, and an exact `None`-to-`None` candidate-session custody transition. Terminalizing
  an admitted receiving or finished operation instead permits only `Rejected`, `Cancelled`, or
  `Error` with its selected source receipt, exact outcome evidence, and exact `Staging`-to-`None`
  custody transition. Single-slot invariants make the custody predecessor pair equal the session's
  newest pair throughout a coherent admitted staging head, so `Conflict` is reachable only as an
  ordinal-one terminal-before-begin `None`-to-`None` closure when a stale begin's expected same-
  session pair differs from that same session's exact observed current pair, meaning its newest
  pair; the evidence also fixes the observed session revision. An attempt to elect admitted
  `Staging`-to-`None` `Conflict` fails closed without mutation. Neither reachable form may claim the
  other's transition. It creates no
  `DraftPieceEditHeaderV1`, draft-piece build, candidate root, edit-history transition, or
  `DraftPieceSettlementV1`. After finish-to-build transfer, the existing draft-piece settlement
  family is the sole terminal authority. `DraftPieceTransactionOutcomeV1` accepts exactly one of
  those mutually exclusive terminal closures and still exposes only `Committed`, `Rejected`,
  `Conflict`, `Cancelled`, or `Error`; only the draft-piece closure may produce `Committed`.
- `DraftEditHistoryTransitionV1` is one immutable compact same-draft root transition. It stores the
  exact predecessor and successor combined-root references, before/after caret and directed
  selection, ordinary-edit or history-action kind, checked one-based `u64` journal depth/ordinal,
  predecessor journal link, undo/redo stack links, canonical operation identity, cumulative encoded-
  byte position, fixed `u64` ancestor-presence bitmap, exactly 64 closed authenticated ancestor-
  reference slots, and digest. At depth `d`, bit and slot `k` are present exactly when `2^k < d`:
  level zero is the exact prior journal transition, and level `k > 0` is the exact `2^k` ancestor
  obtained byte-for-byte from the level-`k - 1` ancestor's own level-`k - 1` slot. Every other slot
  uses the one canonical absent encoding. The digest commits the complete value including depth,
  bitmap, present references, and absent slots. Local decode and ordinary point reads reject a
  duplicate, missing, extra, wrong-depth, locally inconsistent, digest-invalid, or noncanonical
  reference. Append admission separately proves every present reference was constructed from the
  authenticated exact predecessor and belongs to that selected journal lineage before commit.
  It stores no inverse text, marker registry, root graph, proposal payload, or copied root content.
  Both roots and every present ancestor must authenticate to the same draft and exact journal
  lineage.
- `DraftEditHistoryFrontierReferenceV1` is either the deterministic canonical-empty history for a
  newly created empty draft or one exact immutable snapshot produced by a named editor-session
  frontier publication or atomic sealed-content direct-root selection. The snapshot repeats its
  selected root and candidate generation when applicable, retained journal/stack heads, floor,
  budget and policy revision, and exact availability. A sealed-content direct-root selection
  creates a fresh-baseline snapshot bound to that import operation and imported root, with no
  transition or stack heads and both undo and redo unavailable. A current draft references only an
  immutable form, never a mutable live-session head, and may never select a root without its matching
  history reference.
- Every operation that selects a root outside ordinary candidate publication selects its matching
  history authority in the same atomic result. Canonical-empty draft creation selects canonical-
  empty history. Replacement-edit and composer-history recall imports select a fresh-baseline
  snapshot at the sealed imported root with exact `undo unavailable` and `redo unavailable` facts;
  the import itself is not an undoable text transition. Undo and redo are the only direct candidate-
  root selectors that preserve and move existing transition history, and they require the
  authenticated retained transition described below. Any other root-only selection is invalid.
- `DraftEditHistoryFrontierV1` is one bounded mutable record selected by an editor-candidate session.
  It stores the exact current candidate root and generation; journal head, undo and redo heads, and
  oldest eligible transition; the exact head and floor journal depths and cumulative positions;
  monotonic immutable-record cumulative encoded-byte position; exact retained encoded bytes;
  configured budget and retention-
  policy revision, monotonic frontier revision, and exact undo/redo availability. Its immutable
  receipts make exact replay and byte-disagreeing frontier-operation collision point-readable.
  Opening a fresh session forks its mutable head from the current draft's exact immutable published
  reference, preserving every immutable ancestor reference byte-for-byte; it never discovers or
  adopts an unpublished head from an earlier session. Independently appended siblings remain valid
  only in their own frontiers and never gain membership through cumulative order or digest equality.
- `DraftEditHistoryStoredChargeV1(record)` is the checked `u64` sum of the exact bytes emitted by
  that record family's key codec and the exact bytes emitted by its canonical value codec. The
  value charge includes every field actually encoded, including a canonical key repeated inside
  the value; the family key and repeated value key are therefore both charged. The charge includes
  no family name, domain-registration bytes, Fjall journal or table framing, allocator metadata,
  cache residency, compression estimate, or filesystem allocation.
- The charged retained set is exactly one editor-candidate session's current mutable live frontier
  head plus every transition and link record retained through that head. Immutable publication or
  fresh-baseline snapshots and immutable operation receipts are excluded from this edit-history
  retention budget and remain under their separate bounded lifecycle and storage ownership. A
  newly opened canonical-empty or fresh-baseline session initializes `retained_encoded_bytes` to
  the exact charge of its newly stored live head. A forked session initializes it to the exact
  charge of its new stored live head plus every immutable transition and link record retained
  through its imported frontier links; shared physical records are charged once in that frontier's
  logical retained set, never once per reference field. Its immutable-transition
  `cumulative_encoded_bytes` starts at the imported cumulative position, or zero when it retains no
  transition/link record; the mutable frontier charge is not an indexed transition position.
- An ordinary `Committed` candidate adoption writes the new transition and advances the candidate
  session and history frontier in the same command. It clears redo and may advance the oldest-
  eligible floor according to the configured durable byte budget. For source frontier `F`,
  successor frontier `F'`, newly retained immutable transition/link record set `A`, and evicted
  immutable transition/link record set `E`, the exact successor charge is
  `F.retained_encoded_bytes - charge(F) + charge(F') + sum(charge(A)) - sum(charge(E))`, with every
  conversion and operation checked as `u64`. `charge(F')` uses the final canonical successor shape
  after its optional links and tags are selected; its counters, revisions, and digest are fixed-
  width, so their numeric values do not create a sizing iteration. The new transition's cumulative
  position is the prior immutable cumulative position plus the exact charges of the transition/link
  members of `A`; live-frontier replacement and eviction do not change that
  monotonic append position. A no-change settlement changes neither history record nor either
  counter.
- Candidate adoption selects the oldest authenticated eligible transition/link prefix needed for
  `E` directly from the selected journal head. It derives the exact checked eviction amount from
  the pre-eviction successor charge, checked-adds that amount to the authenticated cumulative
  boundary at the source floor, and uses the result as the threshold. From the already authenticated
  head it examines skip levels highest-to-lowest, following a reference only when the referenced
  same-lineage ancestor's authenticated cumulative position remains at or above that threshold and
  the ancestor is eligible. It follows at most one committed target reference per level, so at most
  64 transition point reads and fixed state select the unique oldest eligible ancestor that reaches
  the threshold while its exact prior-lineage boundary does not. Each target must pass local key/
  value, codec, shape, digest, depth, and cumulative-position agreement. Ordinary selection trusts
  its correctly committed witness lineage rather than recursively re-proving derivation or root
  adjacency, then validates exact floor/head references, retained accounting, root pins, and
  availability. A digest-valid sibling at an
  overlapping cumulative position is never visited and remains immutable and independently
  branchable. The ordered draft/cumulative-position/session-tie-break key may support a non-
  authoritative seek hint, but that hint never selects the floor and a sibling result cannot cause
  failure or redirect adoption. Adoption subtracts only the selected-lineage charges while
  advancing the logical floor and applies that availability change in the same atomic command as the
  successor without historical-root adoption. Ordinary editing commits whenever eligible eviction
  suffices. A typed history-
  capacity-unavailable result is permitted only when the
  final authority cannot retain the required non-evictable live-head and transition/link closure
  within the configured budget. Overflow, underflow, an encoded length not representable as `u64`,
  or disagreement between stored counters and recomputed exact charges fails closed. No command
  writes a transition/frontier/candidate-history successor above budget. The transition cumulative-
  byte positions drive the bounded head-origin threshold comparison without loading complete
  history.
- `DraftHistoricalRootAdoptionV1` is a dedicated immutable settlement keyed by draft, editor
  session, and operation. It authenticates the source frontier, selected retained transition,
  direction, same-draft target root, target caret and directed selection, and exact successor
  candidate/history frontiers. `Committed` directly selects that existing immutable root under one
  new candidate generation and atomically moves the undo/redo heads. `Rejected`, `Conflict`,
  `Cancelled`, and `Error` prove exact noncommit. The command never constructs a root, accepts
  detached digest membership, streams inverse bytes, or expands one undo into multiple edits.
- Retained transitions are durable root pins. Advancing the eligibility floor removes undo/redo
  reachability and the logical history pin but deletes no root, transition, stack link, node, leaf,
  or content and performs no root or content copy; physical
  reclamation remains unavailable until the future explicit garbage-collection design accounts for
  every current-draft, candidate-session, materialization, submission, and history reference.
- Each candidate edit or sealed-composer import has one `draft-piece-builds` record keyed by exact
  draft, session, and caller-owned operation identity, with a closed edit-successor or sealed-
  composer-import kind. An edit build is created only by the authenticated finish-to-build custody
  transfer. It retains the exact staging identity and finished staging-head/receipt reference,
  predecessor candidate generation and combined root, predecessor caret and directed selection,
  canonical proposal-header bytes and
  digest, cumulative checked replacement, move, item, and byte counts, authenticated consumed
  source- and proposal-lane frontiers, explicit finish-input identity, sequence-path, identity- and
  marker-commitment removal/insertion, changed-occurrence count/digest, bounded pending marker-
  effect, and cross-validation frontiers,
  compact checked summaries, and intended
  successor-position digest. A sealed-composer-import build additionally retains the exact sealed `ComposerV1` reference,
  canonical proposal-header bytes and digest, bounded canonical
  text/marker cursor, and output frontiers for all three structures. Both retain the proposed combined-
  root summary and lifecycle:
  `Open`, `Complete`, `Committed(settlement)`, `Rejected(settlement)`, `Conflict(settlement)`,
  `Cancelled(settlement)`, or `Error(settlement)`. The last five are terminal and can never return
  to open/complete or adopt another root. The build record never embeds or retains the resident
  whole edit, replacement vector, or inserted payload. It names its latest immutable progress-
  receipt key and digest rather than authenticating mutable progress with a self-hash. The header's
  declared counts, final chains, and successor positions are derived from `FinishInputV1`; build
  validation and reconciliation derive the next natural staging-page key from the retained lane
  frontier, point-read only that bounded page and receipt closure, and never accept caller-resupplied
  fragments as authority. Bounded immutable fragment records contain only ordered canonical
  replacement envelopes, inserted pieces, and self-contained marker insert/remove/move/same-id-
  replacement effects; their canonical bytes and one-based ordinal chain authenticate the proposal
  endpoint and are never overloaded with progress records. Digests
  may reject a mismatch but never replace byte equality. Unpublished nodes and leaves in either
  structure are unreachable from current-draft reads.
- `DraftPieceBuildProgressReceiptV1` is the canonical immutable fixed-size transition receipt in
  the dedicated `draft-piece-build-progress` family. Its natural key is exactly draft id, editor-
  candidate session id, operation id, and one-based transition ordinal. The value repeats that key
  and retains the exact prior receipt key and digest, absent only for ordinal one; the exact
  authenticated canonical-fragment endpoint, canonically empty before any fragment and otherwise
  naming its one-based key and canonical fragment digest plus its chain; the exact staging identity
  and finished staging-head/receipt reference; current phase and relational cursors; authenticated
  consumed source- and proposal-lane staging frontiers; working sequence, marker-index, and marker-
  order-commitment roots with their complete summaries; source and successor structure frontiers; changed-occurrence count/
  digest frontier; bounded pending marker-effect state; next record ordinal; optional successor root
  and build digest; lifecycle; and its
  domain-separated receipt digest. The digest is SHA-256 over the exact ASCII domain
  `syndic/draft-piece-build-progress-receipt/v1`, the canonical key, and every preceding canonical
  value field. It is a commitment, not a substitute for point-reading the referenced closure.
- Every build-transition request separately names its exact expected source receipt, with `None`
  valid exactly for transition ordinal one, and its exact target receipt, target build head, exact
  session-custody before/after state, and complete set of
  bounded same-command effects. Source and target are never inferred from target-key occupancy.
  When the stored head selects the source and equals its authenticated state, the target receipt
  must be absent; even byte-identical target occupancy is a corrupt split. Exact replay requires the
  stored head already to select the target receipt and equal the proposed target-head bytes, and
  every receipt, build, fragment, root, settlement, candidate-session head and custody-slot
  transition, and other
  same-command effect to equal the proposed target closure canonically byte for byte.
- `DraftPieceSettlementV1` is an immutable record keyed by draft, editor session, and operation
  identity. It repeats
  the canonical proposal-header bytes and digest, declared fragment count and terminal fragment-
  chain commitment, exact predecessor candidate generation/combined root and history frontier,
  optional exact build digest, terminal
  outcome, settlement-command source basis, terminal progress-receipt key and digest, that receipt's
  immediate-predecessor/root closure, and complete outcome-specific proof. `Committed`
  stores a closed edit-adoption or sealed-import-selection proof. Edit adoption stores the successor
  candidate generation/combined root and history frontier, complete combined-root summaries including
  marker commitment, logical extent,
  positions, adoption receipt, and terminal build digest. Sealed-import selection instead stores
  the imported root and fresh-baseline history reference, exact unavailable undo/redo facts, the
  current-draft selector/root/history before and after triples, and the session published/newest
  root/history pairs made equal by that same command. `Rejected` stores the closed invalid-envelope/
  fragment reason and exact
  absence of candidate adoption. `Conflict` stores the different newest candidate
  generation/combined-root/history pair observed by the writer and exact absence of candidate adoption.
  `Cancelled` stores the pre-adoption cancellation-election witness and exact absence of candidate
  adoption. `Error` stores the closed operational reason plus exact absence of candidate adoption or the immutable occupied-
  identity canonical-byte comparison witness that proved this mismatching proposal was never
  admitted. Every no-change form proves that the proposed edit made no change from its predecessor.
- Public `DraftPieceSettlementProofV1` is either that stored settlement record for an admitted
  operation or `OccupiedIdentityNoncommitProofV1` for a proposal that never gained admission. The
  latter binds the requested canonical proposal header and fragment-chain comparison to the first
  immutable occupied settlement, build, fragment, progress-receipt, or candidate-root natural key whose canonical
  bytes differ;
  stored/requested digests are supporting commitments only. It can produce only `Error`, and replay
  returns the same durable occupied anchor. It does not create a second settlement at an already
  occupied operation key or alter the operation that owns that key.
- Replacement ranges are half-open, strictly ordered, and non-overlapping in the exact predecessor
  composite order. Adjacent ranges are valid. Every replacement is interpreted against the same
  predecessor combined root, never against a partially edited successor, and each range's start must be at
  or after the prior range's end. Two empty ranges at the same exact composite position are rejected; the
  caller must coalesce their inserted pieces into one range and one canonical fragment order before
  begin. Each non-move insert or reinsert proves absence in the predecessor identity index. Each removal
  supplies its exact predecessor composite position or anchor witness, proves index presence, and verifies
  order, label, marker-leaf identity, digest, and occurrence through one bounded predecessor-sequence
  descent. Marker moves name one removal and one insertion position for the same identity in the
  same proposal; after the authenticated removal, the insertion proves index absence, creates one
  successor marker leaf at the supplied successor position, and replaces only that identity's
  index leaf. Text insertion before unchanged markers changes no identity-index record. Bounded move
  reconciliation requires both positions and exactly one successor occurrence. Final uniqueness
  never scans the complete sequence tree.
- A draft-composer materialization record binds one exact combined draft-root reference and
  `ComposerV1` format version to one exact sealed ownerless content reference and repeats both
  summaries and digests. Its build record retains only the exact source combined root, next
  composite cursor, output content frontier, streaming encoder state, checked counts, and chain digests needed to
  resume bounded output, plus a closed open, cancelled, failed, explicitly superseded, or sealed
  lifecycle. Explicit supersession names the exact same-root successor build operation; none of the
  nonsealed terminal states publishes a mapping. It never becomes the current-draft backing or
  changes a draft revision.
- A content-manifest record stores content identity, optional canonical-item owner, encoding, lifecycle, exact chunk frontier, encoded and logical lengths, atom and marker counts, and a chain digest. Content-chunk records store bounded ordered encoded bytes. Building content is unreachable; ownerless sealed content is immutable and content-addressed; item-owned live UTF-8 content has a deterministic item-derived identity and may append only before it becomes finalized.
- A canonical user-input item references the exact sealed draft content and matching compact sealed
  asset-reference-set proof. The content-bound `SealedContentMarkerSummary` independently
  authenticates content identity/full digest and its embedded `SequentialMarkerSummaryV1` must equal
  the Asset proof's content-neutral summary. Syndic content pieces retain ordered marker identity and final label;
  the Beryl-state set retains the matching asset identity and first-occurrence disposition. Syndic
  does not duplicate those paged marker-to-asset records or embed image bytes.
- An accepted-input record stores immutable input frozen from a draft for active-turn steering or
  later-turn queueing: one stable identity, owning thread, permanent order, complete source
  thread/draft/gate revision proof, source and replacement draft identities, sealed content
  reference, exact source combined draft root and sealed materialization mapping, compact sealed asset-
  reference-set proof, admitted timestamp, and route-generation identity. The selected generation
  state retains the
  exact binding, execution snapshot, Syndic turn, CAS thread, and known-or-explicitly-unknown CAS
  turn proof once for every member; one bounded route leaf owns only that input's delivery state.
- One accepted-route generation owns one disjoint contiguous permanent-order interval, immutable
  membership through `accepted-order`, a revisioned selected state head, target or next-turn
  disposition, and checked `u64` aggregates for ready/retryable, delivering, next-turn, terminal,
  and live logical bytes. One compact `accepted-ready-source` record makes a currently selected
  exact steering generation with ready or retryable work scheduler-visible, while one distinct
  compact `accepted-next-source` record does the same for effective next-turn work. Neither copies
  generation members.
- An `AwaitingTerminal` route target retains the complete exact prior steering target but classifies
  every routed admitted or retryable member as effective `NextTurn(UnknownTerminal)`. Its
  generation has no ready or delivering aggregate and no ready-source row. Queue-only admission
  allocates later `NextTurn(UnknownTerminal)` generations while the input gate continues selecting
  the retained awaiting-terminal target needed for late evidence and active abandonment.
- A bounded ordered ready-source page discovers those compact records by thread and route
  generation. A revision-bound candidate page accepts one exact source record, walks its permanent
  accepted-order interval from the cursor's last scanned ordinal, and returns only compact
  `Admitted` or `Retryable` routed input identity, ordinal, lifecycle, and leaf-revision facts.
  The cursor advances over delivering or terminal rows as well as returned candidates. It rejects
  gate, route-head, source, or generation drift; the separate fixed-work ready-input read remains
  the authority that reopens the complete target and execution proof after local worker admission.
- A terminal promoted route leaf retains one bounded immutable witness containing the source gate,
  selected route-head and leaf revisions plus the exact fresh successor turn, canonical item, and
  promotion timestamp. Its accepted-input record and permanent-order entry remain immutable.
- One input-gate record per thread stores its independent monotonic revision,
  idle/pending-turn/steering/awaiting-terminal/repair-required/finalizing-history/compaction/stopping
  state,
  accepted-order high-water mark, selected route-generation/head revision, and exact checked `u64`
  live steering, next-turn, and logical-byte counters. An awaiting-terminal gate names the
  unknown-terminal blocking turn while its selected route retains the exact prior steering target.
  A repair-required gate names the exact proven-terminal ordinary turn, correlated CAS thread and
  turn, closed capture-gap provenance that makes historical repair necessary, and one target-scoped
  request disposition: `Available` or the immutable
  `Consumed(request-attempt nonce, source gate revision, successor gate revision)`.
  Permanent accepted order and durable paged generation/leaf routing are distinct authorities.
- A compacting gate stores one caller-generated compaction-operation nonce and the exact parentless
  provider-operation turn. It selects one bounded current record keyed by `(Syndic thread,
  operation nonce)`. The record retains the exact `BerylHomeId` from admission and owns an
  immutable provider-operation execution snapshot containing the exact valid binding revision,
  represented-prefix proof and native count, runtime and managed-process generation, loaded-thread
  generation, and CAS thread. It never reuses the ordinary active-binding activation mutation or
  increments the native count.
- Compaction-operation and request-attempt nonces are distinct caller-owned 128-bit natural
  identities. The caller-supplied provider-operation turn id must have the exact operation-nonce
  payload, and the caller-supplied snapshot id must equal the documented app derivation over the
  complete admission target. Storage allocates no identity and rejects turn, snapshot, operation,
  or existing durable collisions before mutation. Admission is revision one. One immediate
  dispatch-claim revision names the sole
  attempt and its source revision; later compatible revisions independently retain the closed
  request disposition, ordered thread-status frontier, one-way CAS-turn publication, context-
  compaction marker/item frontier, and terminal observation. Exact replays are idempotent,
  conflicts or gaps are invalid, and the attempt never authorizes a second backend request.
- Compaction admission atomically requires an idle gate, no effective accepted-next aggregate, an
  exact current valid binding and reverse authority, and no colliding provider-operation identity.
  It creates the provider-operation turn, source-free turn state, execution snapshot, operation
  record, and compacting gate while leaving thread tail, selected path, draft, binding head,
  represented prefix, and native count unchanged.
- Queue admission against compaction appends ordinary accepted input as `NextTurn(Compaction)` with
  its stable accepted identity and permanent order. It advances only the gate's compatible route
  and aggregate descendants; it neither changes the compaction target nor creates steering
  authority.
- Dedicated compaction mutations publish the exact CAS turn, admit only matching provider source
  events, transition a stop handoff to the non-consumed `Stopping(stop nonce)` relationship, and
  settle completed-marker-then-successful-terminal, interrupted terminal with exact idle-or-non-
  idle status evidence, failed terminal, local nondispatch, rejection, completion-unknown,
  cancellation, or authority-loss successors. Only the later stop terminal or abandonment
  successor consumes a handed-off operation. Generic live-
  terminal gate code rejects provider-operation turns and cannot convert them to `PendingTurn` or
  selected conversation transcript work.
- A provider-operation stop witness retains both the exact source compaction revision and its
  immediate `Stopping(stop nonce)` successor revision. Live reads, scoped stop validation, safe reopen,
  terminal handoff, and abandonment cross-check those revisions with the compaction record and any
  later settlement receipt; a matching nonce at an impossible revision is corruption. Each
  provider-specific safe-reopen, matching-terminal, and abandonment successor additionally retains
  the exact source `Stopping` descendant revision and immediate successor compaction revision that
  consumed it. Its validator reauthenticates the source against the admission handoff and then the
  exact successor or a fully witnessed later provider descendant; loose revision floors are never
  post-stop ancestry.
- A matching successful terminal retained by the compaction record is the provider-operation
  turn's canonical terminal source authority. The mutation records its exact turn-state revision
  and stores no duplicate ordinary `TurnEnded` source event. Domain validation permits a complete
  turn with zero source events only when exactly one compaction record targets that turn and its
  terminal status and recorded turn-state revision equal the turn state. Absence, multiple
  authorities, an ordinary terminal source event, or any disagreement is coherent corruption.
- Consumed compaction records retain the complete claim, observation, and settlement witness. A
  separately keyed immutable settlement receipt is written in the same atomic command and repeats
  the exact predecessor/successor operation revisions, complete predecessor/successor gate
  snapshots, and settlement. The consumed operation independently commits to the complete
  canonical receipt value. A consumed operation and receipt must agree exactly, and the public
  recovery read must authenticate the concrete settlement-specific lifecycle, binding,
  accepted-work, or continuation successor, before the current gate may be accepted as a later
  validated descendant. Lifecycle-continuation receipts additionally retain the initial parent,
  selected path, unbound binding revision, and fixed content reference; validation rederives that
  topology by appending the continuation to the immutable admission snapshot path, then accepts
  later ordinary lifecycle progress without requiring the continuation to remain pending. A
  compact-start response
  reconciler may classify a late matching empty acknowledgement as already
  implied by an exact terminal successor without mutating it. It separately classifies same-
  attempt completion unknown as `TerminalAlreadySettled`: the terminal lifecycle, gate, and
  terminal-chosen binding successor remain exact while the app retires connection authority. A
  late incompatible rejection, proven-nondispatch result, attempt identity, or provider
  observation remains a collision.
- Successful lifecycle settlement accepts the caller-derived continuation turn and canonical-item
  ids plus the ownerless sealed content and exact empty asset-set proof, but no home-id input. It
  verifies the two documented domain-separated identity hashes from the durable operation's
  admission `BerylHomeId`, the fixed content digest and one-text-atom shape, and zero
  markers/assets. Under the serialized current gate and accepted-next aggregates it either releases
  the gate for already effective user work or creates exactly one pending
  `BerylLifecycleContinuation` conversation turn and canonical user-role item with the unchanged
  current draft. Its parent and selected path extend the admission snapshot's immutable selected
  path exactly. Its immutable consumed witness names the operation, turn, item, content, gate, and
  result that won, permitting exact reconciliation without another continuation identity.
- A stopping gate carries the current caller-supplied stop-operation nonce and selects exactly one
  live stop-operation record by `(Syndic thread, operation nonce)`. That record stores the
  immutable exact blocked-operation target: Syndic thread and turn,
  ordinary-turn or closed provider-operation kind, binding revision, execution snapshot id,
  runtime and managed-process generation, loaded-thread generation, CAS thread, and one-way-
  published CAS turn. Context compaction is the currently defined provider-operation kind and is
  stop-eligible only after its compaction record has one-way published that exact CAS turn. The
  record also stores the caller-supplied operation
  identity, monotonic revision, fixed first-publication revision for every present member of the
  nonempty closed cause set, and either `Admitted` or one
  `DispatchClaimed(source_revision, caller_attempt_id)` witness. Causes present at admission name
  revision one. The gate names the same blocked operation and route authority. A missing half,
  target disagreement, transition-revision gap or duplicate, or provenance later than the current
  record is invalid durable state, and a historical claimed attempt is never an idempotency key for
  another backend request.
- Consuming live stop authority does not delete or reuse its keyed record. The same atomic terminal,
  safe-reopen, or abandonment command changes it to one closed consumed disposition containing the
  exact bounded successor witness already required for reconciliation. Consumed records are inert
  identity receipts: they cannot become current again or authorize dispatch. The consumed record's
  exact cause-first revisions, optional dispatch-claim witness, and terminal successor witness are
  the sole authentication for any delayed finalization release.
- Stop causes form a fixed closed set distinguishing selected-operation control, diagnostic
  control, healthy-home window close, and Beryl-owned interrupting approval. An exact later caller
  monotonically adds its cause while joining the existing record; it does not create another
  operation or dispatch identity. Adding a cause is fixed work and stores the immediate successor
  revision as that cause's immutable first-publication witness. Exact reconciliation proves that
  witness is the checked immediate successor of the caller's source revision even after later
  compatible cause, claim, or consumed descendants. It conflicts atomically with safe reopen,
  terminal consumption, and abandonment, so a caller cannot authorize an external side effect from
  a stale cause join.
- The record's fixed transition provenance is canonical. Admission occupies revision one. Every
  later revision through the current record is occupied exactly once by one newly published cause,
  the sole dispatch claim, or the consuming disposition. Initial causes may share revision one;
  no later transition revision may be absent, duplicated, reordered, or overwritten. The public
  typed boundary exposes the closed cause-first revisions and optional claim source revision rather
  than asking callers to infer history from the aggregate cause set or current revision.
- Stop-operation and attempt nonces are distinct 128-bit caller-owned natural identities. The
  operation's durable identity is `(Syndic thread, operation nonce)`, while an attempt is scoped to
  that operation. The retained consumed record makes operation-nonce reuse on the same thread a
  collision. Storage validates non-reuse and exact agreement but does not allocate either nonce or
  derive it from external execution ids.
- One keyed bounded stop-admission read stabilizes the current gate, turn and turn state, binding
  and heads, execution snapshot, published CAS turn, CAS reverse indexes, accepted route sources,
  and matching live stop record when present. For ordinary execution it also stabilizes the
  selected steering route and active binding; for compaction it instead stabilizes the live
  compaction record, provider-operation snapshot, and valid binding. The complete observation
  shares the canonical target and live authenticators used by reconciliation. It returns a closed
  ordinary-admissible, provider-operation-admissible, already-stopping, or ineligible
  classification. Cross-pass selector drift is concurrent change; a stable missing or
  contradictory relationship is invariant failure. Callers never assemble stop authority outside
  this package.
- A turn record captures turn identity, turn kind, immutable parent relationship, deterministic
  ancestor skip, origin thread, immutable chain proof, and creation time. Mutable lifecycle,
  source-event frontier, item frontier, contiguous finalized-item frontier, and terminal or
  incomplete facts belong to its matching turn-state record.
- Turn kind distinguishes user-authored ordinary turns, Beryl lifecycle-continuation conversation
  turns, and provider-operation turns such as context compaction. Provider-operation turns are
  parentless ownership roots and never advance a thread tail; lifecycle-continuation turns carry
  explicit non-user origin but otherwise advance and execute on the selected conversation path.
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
  prefix, and it never authorizes replaying the injected prefix. The recorded generation identifies
  the injection event. Activation under recovered lineage requires one exact loaded generation
  supplied by the current non-cloneable projection capability, cannot start before injection
  completed, and must retain the proof's managed-process generation. Its loaded-thread generation
  may differ only after the app proves the exact overlapping same-process handoff.
- Active binding records additionally store the accepted immutable execution snapshot id and
  active submitted turn. The snapshot stores its exact selected path, represented base prefix,
  represented-base native CAS turn count, execution binding, loaded process/thread generation,
  and start time without an accepted-input vector or a mutable optional CAS-turn field. Its loaded
  generation is the actual current execution session. Under recovered lineage its process component
  equals the injection proof; its thread component may differ after an exact app-owned handoff.
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
  stale publication. The request names stable binding, route-generation, semantic-target, and
  disposition identity. Inside the serialized mutation, storage validates that identity against
  the current compatible route and gate and consumes their actual shared authority; callers do not
  fence abandonment on a previously read shared route-head or gate revision. It retires the CAS
  thread, publishes exact stale provenance, and returns the gate to the same submitted turn.
  Undispatched `Admitted` or `Retryable` steering routes move to ordered next-turn work under
  `ProjectionLost`; a possibly dispatched `Delivering` route becomes terminal `DeliveryUnknown`,
  leaves no live route or counter, and remains permanent accepted-input history without automatic
  replay. Ambiguous command completion reconciles the stale binding, retired reservation,
  successor gate and route head, projection-loss generation, and optional named leaf together. The
  projection-loss generation persists the exact source binding, gate, and route plus whether the
  command was generic or named one exact rejected input and source leaf revision.
  Reconciliation compares that complete bounded abandonment witness; a matching binding,
  successor aggregate shape, or other abandonment mode is never exact evidence.
- The abandonment command may additionally name exactly one current `Delivering` route whose
  normalized CAS response proves that request was rejected before acceptance but provides no
  machine target verdict. The same atomic mutation revision-checks that input, route generation,
  steering target, gate, and active binding, then rewrites only that named route leaf to ordered
  next-turn work under `ProjectionLost` with a last-transition proof naming the exact rejection
  abandonment. Every sibling `Delivering` route without equivalent exact non-acceptance proof still
  becomes `DeliveryUnknown`; this exception never comes from diagnostic message text or a separate
  pre-retirement mutation.
- If no exact activation event was admitted, the still-pending turn may be rebound through a fresh
  projection. Once activation was admitted, the turn is never replayed automatically; the pending
  gate withholds competing work until a later source-less interrupted, failed, incomplete, or
  unknown-terminal update converges local lifecycle without restoring external authority.
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
- Draft or accepted-input admission may likewise advance the broad thread revision without
  changing the selected tail or digest. Native planning treats that proof as a compatible
  descendant of the projection request and current binding. If activation follows, the active
  binding advances only the represented prefix's source-thread revision to the current compatible
  proof; its CAS thread, native count, execution, tool profile, and original establishment lineage
  remain exact. Mutation validation, publication reconciliation, and scoped binding reconciliation use the
  same relation.
- An exact recovered-lineage handoff preserves its original `RecoveredInjectionProof` unchanged
  while admitting a new loaded-thread generation under the same managed-process generation. The
  binding therefore records how the in-memory CAS history was seeded without pretending that
  injection ran again. Storage does not authorize cold resume; the app must supply the
  non-cloneable overlapping-subscription proof for this transition.
- Unbound binding records represent a view with no usable CAS projection and may store the reason that no projection exists.
- A source-event record stores one normalized turn activation, typed item start, typed coalesced item
  delta, typed item completion, typed provider-observation issue, or status-only turn-ending outcome
  with a monotonic per-turn sequence number and bounded payload, sealed provider-frame reference, or
  compact sealed-observation reference. One turn-ending status stores the exact provider or local execution
  outcome independently from an optional typed history-incomplete reason. A locally incomplete
  outcome requires a reason; a provider-complete outcome may also carry a reason when the observed
  history cannot be published complete. An interrupted pinned-CAS terminal uses
  `ForcedAbortOrderingUnproven` unless release-scoped authority proves it is a no-later-item
  barrier. Activation, provider content, item completion, and
  provider-observation issues require exact CAS turn or item identity. Successful turn completion
  requires exact CAS turn identity. A source-less event may only
  record local interrupted, failed, incomplete, or unknown-terminal convergence while the current
  projection is stale or the thread is unbound; a still-usable valid binding is not sufficient
  because it represents only the pending turn's parent. Local convergence cannot manufacture
  provider activity or success.
- A provider-observation issue identifies one structurally valid, exact-route sealed observation
  that conflicts with the current durable item lifecycle. The compact payload retains the immutable
  observation build identity and digest-covered frontier, exact CAS item id, and a closed conflict
  reason. Its atomic mutation validates the referenced sealed build and proves the conflict against
  the current canonical frontier before advancing source sequence, turn revision, activity, and
  transcript staleness. It records the turn's first monotonic issue fact without creating,
  replacing, completing, or revising a canonical item. A legally admissible observation is rejected
  from this issue mutation and must use normal provider-frame publication. Observation sealing
  retains a structurally valid start for a completion-only kind; normal frame preparation rejects
  it, while the issue mutation may admit it only as `CompletionOnlyItemStarted`.
- A canonical item record stores bounded kind, phase, source, ordering, revision, and closed
  narrative/resource policy for correlated submitted input, assistant messages, operational
  records, generated media references, and presentation-only activity records. Its source is
  exactly one of sealed composer content, normal live-provider authority with its latest sealed
  provider-frame reference and item-local source-event frontier, or terminal-repair authority with
  an opaque package-local snapshot reference, item ordinal/digest, provenance, and snapshot-backed canonical
  manifests/ranges. Transcript-visible text selects the matching closed composer, provider-
  narrative, or repair-snapshot projection source. A provider frame remains mandatory for a normal
  live normalized public variant with fields, but a repaired item requires no fabricated provider
  frame or source event. The API has no generic item shape that silently discards history-relevant
  fields while permitting successful turn completion.
- An activity-query head names one exact query owner, runtime activity period, root source turn,
  checked source count and aggregate admitted source frontier, query revision,
  logical/running/completed row and stored-byte counts, full-order completed retention cutoff, and
  current lifecycle. The same period may accumulate later turns; thread switching, turn completion,
  and a later turn do not rotate it. Process restart, managed-runtime teardown or replacement, and
  same-home recovery that replaces runtime services retire it. Its records remain durable
  provenance, but current-period queries cannot select rows from that retired period. One immutable
  source-membership row per
  observed owner or direct-child turn binds that period to an exact
  `activity_start..=source_frontier` interval, active/terminal state, and optional exact terminal
  child final-answer item/range. Ordered activity entries contain only bounded row metadata, exact
  Syndic/CAS item and source-event identity, lifecycle state, and compact GUI-derived facts; they
  never duplicate command, reasoning, handoff, or provider payloads or bind an unrelated later
  projection revision. A child handoff requires its membership key to be absent and inserts one
  inactive terminal member. The final-answer source event must be immediately followed by the
  terminal event, and the retained row's item/range fact must equal the membership exactly. It never
  refreshes or converts a prior child membership. The rejected shapes are recorded in
  `doc/failures/syndic-phase13-activity-handoff-membership-authority.md`.
- `ProviderItemV1` is the sole item-owned byte authority for normally captured provider-created
  admitted public payload. Its
  deterministic, length-delimited, digest-covered start, delta, and completion frames use a closed
  enum for every pinned item variant and a closed recursive structured-value algebra. Raw JSON,
  opaque blobs, ignored-field fallbacks, and unknown future-variant containers are forbidden.
- The observed pinned Web-search catch-all is the sole explicit lossy exception: backend ingress
  emits the closed `Other` marker and structurally consumes the unknown action payload through fixed
  discard state. Syndic retains only that marker and typed unsupported-history evidence; it does not
  retain raw JSON, field names, payload bytes, or a generic future-variant container.
- The sole constant-resident frame validation path produces the typed history-support fact.
  Unsupported-but-retained observations remain structurally valid, their first unsupported reason
  accumulates monotonically across the item stream, and later publication cannot reinterpret them as
  complete history. In particular, the pinned Web-search `Other` action is retained while carrying
  `UnsupportedRequiredPayload`. No materialized provider-frame compatibility path remains.
- Constant-resident validation also returns the exact bounded assistant phase and submitted
  composer-content reference needed by atomic publication. Every sealed frame snapshot retains a
  resumable item-stream state containing exact provider identity and kind, the next frame ordinal,
  the original start timestamp, completion state, and cumulative history support. Advancing one
  frame therefore preserves lifecycle and timestamp proof without rescanning or materializing the
  preceding item stream.
- The standalone image-generation frame retains exact identity, lifecycle timestamps, status,
  optional revised prompt, and optional `savedPath`. It has no base64 `result` field because the
  backend discards that transport payload before normalization. Syndic has no codec, chunk family,
  or fallback mutation that can store those image bytes in Fjall.
- Standalone image status is closed to the pinned producer's `in_progress`, `failed`, and `completed`
  values; an authoritative completion frame cannot retain `in_progress`.
- `ReasoningTextObserved` retains exact item identity and `content_index` only. The provider's raw
  reasoning delta is a backend-discarded private wire field, not part of the admitted normalized
  grammar; Syndic has no text field, content range, chunk, codec member, or diagnostic escape hatch
  for those bytes.
- Authoritative completion frames for command execution, file change, MCP tool calls, dynamic tool
  calls, collaboration tool calls, and standalone image generation reject their kind's
  `in_progress` status independently of backend validation.
- Arbitrarily large provider strings and structured leaves remain in bounded ordered content chunks.
  Typed field structure and exact range references remain in the same chunked encoding. A selected
  provider narrative view exposes only transcript-visible provider text over those existing bytes:
  start and delta frames extend one item-owned append generation. Every view carries exact span and
  logical-byte frontiers plus one chain digest over ordered span provenance and logical ranges. Each
  span retains the frame that selected the field plus its exact physical source range in the same
  immutable stream. For `AgentMessage` and `Plan`, completion performs a bounded byte-for-byte
  comparison against that complete append generation and never selects another narrative. An equal
  completion field may reuse the proven prior ranges without copying cumulative text. A mismatch
  retains the exact completion frame as evidence, keeps the live append generation selected, and
  records typed history incompleteness. Submitted-user correlation instead selects its already
  sealed composer content and retains only exact provider metadata and correlation proof.
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
  ranges. `Utf8V1` spans map directly and `ComposerV1` spans skip atom framing and image-marker
  bytes. Provider narrative uses its separate generation-keyed span index because provider-frame
  structure is not a cumulative text value. Both indexes preserve field and element boundaries and
  allow bounded logical-text cursors over the original stored bytes.
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
- A provider-observation build record owns connection-scoped unattached staging for exactly one
  pinned lifecycle or delta wire observation whose route may trail its size-unbounded item fields.
  It retains only compact parser/structure state, observation identity, field frontier, chunk counts,
  byte counts, and digests while typed observation chunks hold the bounded staged bytes. It names no
  Syndic turn, canonical item, source sequence, or published provider content.
- Completing structural decode seals one compact non-cloneable observation handle. After the app
  proves the trailing CAS thread/turn route against one admitted lane, the exact target consumes that
  handle through bounded reads. If the durable item lifecycle admits the observation, consumption
  stages its provider-item build. If that exact lifecycle rejects an otherwise valid observation,
  consumption produces a private-constructible compact issue reference derived from the sealed
  handle. Only the final target publication makes either the provider-frame effects or the issue
  source event authoritative. Missing, mismatched, cancelled, or retired routes leave the
  observation build unreachable and never visible as history.
- The public issue-reference boundary exposes exact immutable identity and closed reason accessors,
  but not a constructor from caller-supplied build facts. Scoped issue reconciliation point-reads and replays
  the referenced observation under fixed buffers, verifies its seal, identity, item evidence,
  canonical bytes, digest, and actual lifecycle conflict, and folds the same first-issue fact from
  source order. An absent, unsealed, mismatched, admissible, or ambiguously referenced observation is
  corruption or a typed mutation rejection, never permission to retain an unverifiable issue.
- A provider-item build record retains bounded resumable staging authority for one unpublished typed
  frame and its target narrative-view change when applicable: exact item and prior published
  frontiers, frame lifecycle and kind, chunk, byte, narrative-span, and logical-byte counts, exact
  digests, and the next bounded encoding frontier. Publication atomically advances the content
  manifest, selected narrative view, source event, canonical item and indexes, lifecycle, and
  transcript staleness. A conflicting or abandoned build remains unreachable and cannot satisfy
  item or terminal audit.
- A provider-narrative-span record is keyed by provider content, narrative generation, and logical
  start. It maps one exact logical range to the immutable frame that selected the field and one
  encoded byte range in the same ProviderItemV1 stream. The physical range may be earlier than the
  selecting frame when the typed field uses exact proven reuse. Start and delta staging append after
  the current generation frontier. Completion stages its exact provider frame and a bounded
  comparison against that generation but creates no narrative spans or generation. Staged spans
  become readable authority only when the final publication command selects their complete
  referenced view.
- An item-projection head selects one coherent generation for an item and records whether that
  selection is current or stale against the canonical source revision.
- An immutable item-projection-set record retains the exact canonical item revision and closed
  projection source, parser version, stable-prefix projection and resource counts, total output
  counts, stable and complete digests, the source-typed reusable parser checkpoint, and whether that
  checkpoint has resolved end-of-input.
- An item-projection-build record retains only bounded resumable state for one uncurrent generation:
  the exact closed projection source, its next composer-piece, provider-narrative, or repair-
  snapshot canonical-range cursor, current block classification, bounded undecided source, preview
  state, output ordinals, and resource digest/count state. A repaired source reads the published
  snapshot-backed canonical ranges directly and never requires `ProviderItemV1` ranges or synthetic
  live lifecycle records. Whole canonical items and whole heavy resources never enter this record.
- A transcript-view head selects one explicit generation and tracks that generation's mutation revision, selected path, frontier, and lifecycle. Transcript-view entry keys include thread, generation, and position so changed paths and bounded rebuilds never rewrite a prior generation.
- A transcript-build record and generation-owned path-turn records retain the bounded two-pass
  selected-path rebuild. The first pass walks parent links tail-to-root and writes exact depth
  records; the second walks depth records root-to-tail and publishes bounded entry batches.
- A transcript build's semantic source identity is its committed tail plus selected-path digest.
  Its captured broad thread revision is a monotonic lower-bound observation, not identity. Active
  and completed builds may therefore predate draft-only or accepted-input thread revisions while
  the current thread retains that exact tail and digest and the captured revision is not from the
  future. A selected-path change supersedes an active build.
- A history summary record owns an independent nonzero projection revision and exposes the owning
  thread revision, committed tail and selected-path digest, exact captured-history completeness,
  and last captured activity for catalog-summary rebuilds without storing title text, parent-thread
  metadata, or selected-thread GUI state. Every semantic successor advances the summary revision;
  semantic no-ops publish no successor.
- V1 history-summary `complete` is true exactly when the selected transcript frontier is current and every selected-path turn has a known terminal lifecycle other than explicit `incomplete` with its finalized-item frontier equal to its item frontier; pending, active, incomplete, unknown-terminal, a current repair-required gate, unfinished item finalization, or stale projection state makes it false. An empty current transcript is complete.
- V1 history-summary `last_activity_at` is the maximum of the current draft's update time and every selected-path turn's submission and turn-state update times. Summary publication is atomic with every mutation that changes a contributing fact; explicit schema validation, scrub, background summary maintenance, or corruption investigation may recompute both derived values with bounded traversal.
- A thread-catalog-summary record is a rebuildable compact Syndic projection keyed by thread id. It
  contains one resolved generated, history-derived, or absent title source; the immutable execution
  binding; automatic archive state; last activity; completeness; bounded lineage facts; exact
  execution, attributes, history-summary, and thread source witnesses; and its own nonzero projection
  revision. Stale source witnesses invalidate it instead of allowing Beryl to recompute precedence.
- The history-derived title builder streams the exact eligible canonical user input through bounded
  content reads and applies the algorithm in `doc/features/conversation-threads/design.md`. It never
  loads a complete input or selected path merely to construct a title.
- One stable public preparation read returns either the exact source-current compact summary or an
  opaque checked successor. The successor exposes its exact post-publication summary for same-home
  cross-domain composition but only Syndic can turn it into mutation authority.
- Exact-current preparation can contribute one validation-only Syndic participant to a
  heterogeneous home command. Prepared replacement publication and that command's Beryl catalog
  join may therefore use the same all-or-nothing home commit without transferring title precedence
  or canonical-source validation to Beryl.
- Background catalog maintenance or explicit validation recomputes a history-derived title only when every catalog source witness is current.
  A source-stale catalog summary remains valid rebuildable state when its retained payload and each
  independently current source relationship are otherwise well formed.
- An immutable projection record stores one typed Markdown block/span or one resource reference,
  its exact source range and block-group provenance, and the bounded inline source or preview
  consumed by the transcript provider. Its identity and revision are generation-independent.
- An immutable resource metadata record describes one exact logical-text range in its closed
  composer-content or provider-narrative backing, or a later feature-owned external backing, plus
  generation-independent revision, kind, preview range, byte length, digest, and bounded code/table
  structural metadata. Metadata reads never load backing bytes.
- Closed item-projection membership is keyed only by item and logical projection ordinal. Every
  later generation reuses that exact immutable stable prefix. Outputs that exist only because a
  live source snapshot reached its current end remain in the generation-owned suffix; a later
  source revision may supersede them without rewriting the stable prefix. Once immutable source
  resolves the same end-of-input, those exact outputs may join the stable prefix. Public reads
  merge the two membership ranges behind one contiguous logical ordinal space.
- Projection and resource identities and revisions exclude generation. An unselected build record
  is optional resumable work rather than selected authority; losing it requires rebuilding that
  generation but does not corrupt canonical history or a published set.

## V5 Domain Schema

- The stable logical domain name is `syndic` at domain schema V5. Every family uses keyspace schema
  V1 and one exact package-owned record version selected per family. `source-events`,
  and `accepted-inputs` use record V3; `accepted-route-leaves` uses record V4; `input-gates` uses
  record V5;
  `accepted-route-generations` and `turns` use record V3; `threads`, `drafts`, `turn-states`,
  `accepted-order`, `content-manifests`, `canonical-items`, and `execution-snapshots` use record V2;
  every other V5 family uses record V1. V5 is a clean replacement
  schema: no prior-record decoder, migration path, or compatibility adapter exists.
- The 61 primary V5 families are `threads`, `thread-executions`, `thread-attributes`,
  `thread-usage`, `thread-catalog-summaries`, `drafts`, `draft-piece-roots`,
  `draft-piece-nodes`, `draft-piece-leaves`, `draft-marker-identity-index`,
  `draft-marker-order-commitments`, `draft-marker-seals`,
  `draft-editor-candidate-sessions`, `draft-piece-builds`,
  `draft-mutation-staging-heads`, `draft-mutation-staging-pages`,
  `draft-mutation-staging-progress`,
  `draft-piece-build-fragments`, `draft-piece-build-progress`, `draft-piece-settlements`,
  `draft-edit-history-frontiers`, `draft-edit-history-transitions`,
  `draft-historical-root-adoptions`,
  `draft-composer-builds`,
  `draft-composer-materializations`,
  `content-manifests`,
  `content-chunks`, `content-byte-spans`, `content-text-spans`, `provider-narrative-spans`,
  `content-pieces`, `context-envelopes`, `turns`, `turn-states`, `input-gates`, `accepted-inputs`,
  `stop-operations`, `compaction-operations`, `compaction-settlement-receipts`,
  `accepted-route-generation-heads`,
  `accepted-route-leaves`, `source-events`,
  `provider-observation-builds`, `provider-item-builds`, `terminal-repair-snapshots`,
  `terminal-repair-item-pages`, `terminal-repair-content-pages`,
  `terminal-repair-media-pages`, `canonical-items`,
  `activity-query-heads`, `item-projection-heads`, `item-projection-sets`,
  `item-projection-builds`, `transcript-view-heads`, `transcript-builds`, `projections`,
  `resources`, `history-summaries`, `bindings`, `execution-snapshots`, and `active-cas-turns`.
- `draft-marker-identity-index`, `draft-marker-order-commitments`, `draft-marker-seals`,
  `draft-editor-candidate-sessions`,
  `draft-mutation-staging-heads`, `draft-mutation-staging-pages`,
  `draft-mutation-staging-progress`,
  `draft-piece-build-progress`, `draft-edit-history-frontiers`, `draft-edit-history-transitions`,
  and `draft-historical-root-adoptions` are distinct V5 primary families. The marker index uses tagged
  internal-node and leaf records, and the candidate-session family uses tagged head and immutable
  receipt records. Marker-order commitments use tagged immutable internal-node and leaf records;
  marker seals use compact durable cursor/lifecycle records. Build progress instead requires its own append-only family so canonical proposal
  fragments remain the only values in `draft-piece-build-fragments`.
- The 23 index V5 families are `draft-by-thread`, `thread-parent-index`,
  `image-label-origin-spans`, `turn-children`, `accepted-order`, `accepted-route-generations`,
  `accepted-ready-sources`, `accepted-next-sources`, `turn-items`, `activity-query-entries`,
  `activity-query-sources`, `item-source-events`, `cas-item-index`, `transcript-path-turns`,
  `transcript-view-entries`, `stable-item-projections`, `item-projections`,
  `projection-resources`, `binding-heads`, `cas-thread-index`, `cas-thread-bindings`,
  `cas-turn-index`, and `provider-observation-chunks`.
- The complete V5 inventory is exactly 61 primary plus 23 index families, or 84 total. A release
  registers exactly the implemented owned families it exposes and never registers an empty
  placeholder for an unimplemented family.
- `draft-piece-roots`, `draft-piece-nodes`, `draft-piece-leaves`,
  `draft-marker-identity-index`, and `draft-marker-order-commitments` use immutable V1 codecs;
  `draft-marker-seals` uses its current V1 codec directly. Roots use
  `DraftPieceRootNaturalKeyV1`: draft plus tagged draft-scoped canonical-empty identity, or draft,
  editor session, and operation for every editor candidate. They bind the complete sequence,
  identity-index, and marker-order-commitment roots and summaries. Direct empty-draft creation uses
  `CanonicalEmptyDraftRootBuildOperationIdV1`; edit and import roots use their complete
  `EditorCandidate(session, operation)` identity. Sequence nodes/leaves and tagged identity-index
  and marker-order-commitment internal/leaf records use
  build-scoped opaque identities allocated by their originating build and repeat their owner, kind,
  and digest so unchanged records can be shared by later roots. A combined root becomes mutable
  current-draft authority only through the matching current-draft reference; a caller holding its
  complete exact reference may still request immutable historical integrity reads after selection
  advances. A reachable record whose exact digest and aggregate chain does not reach its selected
  structure root is invalid.
- Every V1 sequence leaf, child entry, node value, and root sequence summary canonically encodes
  checked UTF-8 byte length, newline count, and derived logical line count in that order before its
  piece and marker aggregates. Decoding validates the empty/nonempty line formula at each level and
  the exact checked composition of every parent. These fields participate in the leaf or node
  digest and in the canonical sequence-summary bytes consumed by the combined-root digest.
- A `draft-piece-roots` value with a nonempty sequence selects one node and fixes the root composite search envelope
  from `BeforeMarkers(0)` inclusive through `AfterMarkers(logical UTF-8 length)` exclusive. Every
  node child repeats its relative lower/upper search fences and subtree aggregates; decoding rejects
  an empty, overlapping, gapped, out-of-order, out-of-parent, aggregate-inconsistent, or digest-
  inconsistent envelope. A nonzero marker count also selects one tagged identity-index node whose
  stable-id envelope, checked record count, height, and digest agree with the identity summary.
  Every identity internal record has disjoint ordered child envelopes; every identity leaf contains
  one stable id, final label, same-anchor order key, and exact sequence marker-leaf identity and
  digest, but no absolute anchor or position. A nonzero marker count also selects one marker-order-
  commitment root whose checked count and maximum label agree with the sequence and identity
  summaries. Zero markers require neither marker structure and require both exact empty digests even
  when text makes the sequence nonempty. The canonical empty combined root selects none of the
  three roots and uses the exact empty sequence, index, commitment, and combined digests and zero
  summaries defined above.
- A nonempty combined root is publishable only from a completed build whose base combined root was
  already valid, or from a sealed-content import that derived all three structures from the same bounded
  stream. The build proves each changed marker's exact old/new stable occurrence facts, equal final
  marker, index, and commitment counts and maximum labels, and all successor digests while reusing only authenticated
  unchanged subtrees. A text-only rebase that changes no marker leaf must reuse the complete
  identity-index and marker-order-commitment roots unchanged. Missing or duplicate identity or
  commitment leaves, occurrence-fact
  disagreement, count or digest disagreement, one-sided publication, or a root/build/settlement
  mismatch is corruption. Explicit schema
  validation may compare every mapping in bounded pages; routine edit uniqueness never does so.
- `draft-editor-candidate-sessions` uses canonical tagged V1 keys beneath one exact draft/session
  prefix. Its mutable head repeats the key and only the bounded durable-base, published, newest-
  candidate, generation, dirty, lifecycle, and optional fixed-size active-operation custody facts
  defined above. The base, published, and newest checkpoints each pair one exact candidate root with
  one exact edit-history frontier reference; the published and newest history references are
  distinct fields even when opening canonically derives equal root/history state for both.
  Immutable open receipts own the complete initial paired checkpoints. Publication receipts own the
  captured candidate/history pair, prior and successor current-draft selector/root/history triples,
  and before/after session-head revisions and published pairs. Disposal receipts own the complete
  final published/newest pairs and lifecycle transition. All receipts repeat their exact operation
  identity and canonical request bytes and make a differing reuse a typed occupied-identity
  collision. A missing or mismatched root/history pair, generation regression, published generation
  newer than the candidate generation, selector root/history disagreement after a recorded
  publication, or disposed head that later advances is corruption. A newly opened head has no
  custody; a cleanly disposed head must have no custody and must have byte-equal published and newest
  root/history pairs.
- `draft-mutation-staging-heads` uses the exact 48-byte `DraftMutationStagingIdentityV1` key and
  mutable V1 head described under Logical Records. `draft-mutation-staging-pages` uses the exact
  57-byte identity/lane/ordinal key and immutable V1 page value. `draft-mutation-staging-progress`
  uses the exact 56-byte identity/transition-ordinal key and immutable V1 receipt value. These three
  primary families have no secondary index family: recovery begins from the candidate-session
  custody slot and point-reads its exact staging identity and selected receipt. A head whose lane
  frontier is ahead of its selected receipt, a page ahead of either, a finished head whose final
  declaration differs from its two current lane frontiers, a staging head disagreed with by the
  candidate slot, a building head without the exact atomic `Staging`-to-`Building` transfer receipt,
  or a terminal head whose receipt lacks its lifecycle's exact outcome evidence or required
  reachable custody shape is corruption. In particular, `Conflict` requires ordinal-one `None`-to-
  `None`; an admitted `Staging`-to-`None` `Conflict` is invalid.
  The V1 head codec recomputes the acyclic staging-head digest with the selected receipt digest
  omitted and validates only the value's local canonical structure, key/value agreement, field
  bounds, lifecycle shape, selected receipt key/transition ordinal, and stored digest fields; codec
  decode performs no storage read. Bounded natural-closure reads, explicit schema validation, scrub,
  and corruption investigation point-read the selected receipt key and separately require its
  canonical receipt digest, receipt-owned after-head digest, and complete head/receipt closure to
  agree. Neither local decode nor a storage-backed closure check hashes the receipt digest back into
  the head-digest preimage.
- `draft-edit-history-frontiers` stores one bounded mutable V1 head per exact draft/editor session
  plus deterministic canonical-empty references, immutable session-publication and sealed-import
  fresh-baseline snapshots, and immutable operation receipts under tagged keys. Every immutable
  reference repeats its selected root and exact availability. The head repeats the current candidate root
  and generation; exact journal/undo/redo heads and retention floor; their required journal depths
  and cumulative positions; retained encoded-byte total; nonzero configured byte budget; retention-
  policy revision; frontier revision; root-pin closure; and exact availability. Unknown links, a
  current root that disagrees with the session, a retained total above policy, locally malformed
  head/floor references, or availability across the floor is invalid. Writer admission rejects any
  source or successor whose exact ancestry, cumulative/root adjacency, accounting, or pins disagree
  before atomic publication.
- `draft-edit-history-transitions` stores immutable V1 compact transition and stack-link records
  in the existing family. Transition keys are ordered by exact draft, checked cumulative encoded-
  byte position, and editor-session tie-break; no session-major traversal or secondary index is
  used. Each transition repeats its exact
  predecessor/successor same-draft root references, before/after caret and directed selection,
  transition kind, one-based checked `u64` journal depth/ordinal, prior journal and stack links,
  cumulative encoded-byte position, operation identity, fixed `u64` ancestor bitmap, its exact 64-
  slot closed ancestor array, and digest. Cumulative positions are checked and strictly increasing
  within the committed lineage. The V1 codec requires bit/slot `k` present exactly when `2^k` is
  less than the transition depth, level zero equal to the prior journal reference, every higher
  level byte-equal to the lower ancestor's corresponding level, and every unused slot canonically
  absent. Local decode recomputes the digest over the entire canonical witness but performs no
  storage read. Append admission fully validates the source frontier and head, exact immediate
  predecessor, roots, positions, cumulative and retained-byte accounting, and derives all present
  witness slots correctly before one atomic transition/frontier/session/settlement commit. After
  that commit, ordinary reads trust each referenced immutable transition after its local key/value,
  codec, shape, and digest agreement and do not recursively re-prove skip derivation or root
  adjacency. Digests remain identity, canonical-replay, accidental-local-mismatch, and cheap fail-
  closed decode commitments. This package does not claim to detect a fully self-consistent
  coordinated digest-valid rewrite of transition/frontier/session/receipt authority, hostile
  storage, cosmic bit flips, arbitrary I/O or media corruption, or other post-commit replacement
  when the same database supplies every anchor. The
  family has no codec field for inverse text, marker collections, root graphs, or document payloads.
  This trust boundary adds no proof field, record, family, pin record, or index: the frontier and
  transition families remain the complete edit-history family inventory.
- `draft-historical-root-adoptions` stores one immutable V1 settlement per exact draft/session/
  operation. It repeats the source history frontier, selected retained transition and direction,
  target historical root, restored caret and directed selection, terminal result, and exact
  successor candidate/history frontiers when committed. It is the only direct-root candidate
  adoption schema. A missing transition, root outside the same-draft retained lineage, stale
  frontier, disagreeing replay, collision, or no-change result that names a successor is invalid.
- `draft-piece-builds` is keyed by the exact 48-byte draft/session/operation identity. Its V1
  value repeats the closed edit-successor or sealed-composer-import kind. An edit-successor repeats
  the exact staging identity and authenticated finish receipt, finish-derived canonical proposal-
  header bytes and digest, predecessor candidate generation and exact combined root; an import has
  no staging identity. The record also retains any optional exact sealed
  content source,
  compact declared counts and digests, finished-staging reference, consumed source/proposal staging
  frontiers, ordered fragment, sequence-path, identity-index, marker-order-commitment, changed-occurrence count/digest,
  bounded pending-marker-effect, and cross-validation frontiers, proposed successor candidate
  generation/combined root, complete canonical combined-root summaries, and
  exactly `Open`, `Complete`, `Committed(settlement)`,
  `Rejected(settlement)`, `Conflict(settlement)`, `Cancelled(settlement)`, or `Error(settlement)`
  lifecycle. It contains no whole edit, replacement collection, inserted payload, or mutable self-
  hash; its current transition authority is the exact latest progress-receipt key and digest.
  `draft-piece-build-fragments` is keyed by that build plus one-based fragment ordinal and stores
  one bounded exact replacement, inserted-piece, or self-contained marker-effect fragment with its
  preceding chain digest. One continuation or replay command requires canonical build-header bytes
  and every fragment in its bounded target window to match ordinal by ordinal against the one next
  authenticated staged proposal window. The source receipt's fragment endpoint/chain authenticates
  the already consumed prefix, which is never compared again or reconstructed from caller bytes;
  equal header, fragment, or chain digests alone are insufficient.
  Fragment gaps, overlaps, reorderings, duplicate empty ranges at one composite position, unknown
  position-witness tags, and a terminal declaration that disagrees with the accumulated counts or
  digest are invalid. This family has no tag or value shape for a progress receipt.
- `draft-piece-build-progress` is keyed by the exact 56-byte draft/session/operation/one-based-
  transition-ordinal identity. Each immutable V1 value repeats that key and the exact prior receipt
  key and digest, with `None` valid only at ordinal one; the exact authenticated canonical-fragment
  endpoint, canonically empty before any fragment and otherwise naming its one-based key and
  canonical fragment digest plus its chain; exact staging identity and finished-head/receipt
  reference; current phase and relational cursors; consumed source/proposal staging-lane frontiers;
  working sequence, identity-index, and marker-order-commitment roots and complete summaries; source and successor structure
  frontiers; changed-occurrence count/digest frontier; bounded pending marker-effect state; next
  record ordinal; optional
  successor root and build digest; lifecycle; and the domain-separated receipt digest defined
  above. A non-one ordinal without the exact immediately preceding key/digest, any skipped or
  disagreeing transition, or any key/value/digest disagreement is invalid. While the build head
  selects the preceding receipt, this receipt's key must be absent; occupied bytes in that state are
  a corrupt split even when equal. Once the build head selects this receipt, it can prove replay only
  together with byte equality of the complete same-command closure.
- These existing V1 build, fragment, and progress record shapes own the durable continuation fields;
  no further secondary index, operation-page history, or marker-effect map is required beyond the
  declared marker-order-commitment and marker-seal families. A build endpoint locates its next staging page by one natural-key point read from the
  retained staging identity, selected lane, and next lane ordinal, which is `O(1)` in operation
  length.
- `draft-piece-settlements` is keyed by the exact 48-byte draft/session/operation identity. Its immutable V1
  value repeats the key, canonical proposal-header bytes and digest, declared fragment count and
  terminal fragment-chain commitment, predecessor candidate generation/combined-root/history pair,
  optional build digest, terminal
  outcome, source basis, terminal progress-receipt key/digest and immediate-predecessor/root closure,
  and the complete outcome-specific proof defined above. Unknown or
  incomplete outcomes, a no-change outcome naming an adopted successor, a committed outcome
  missing its exact combined-root/history/build/session-head adoption or direct-selection closure, a terminal build without its agreeing
  progress receipt and settlement, a settlement disagreed with by the candidate, receipt closure, or
  historical combined-root closure, or a second canonical value at the same key is corruption.
  Exact settlement replay requires the stored build head already to select the terminal target
  receipt and every same-command effect to equal the stored target closure. It additionally requires the
  request's canonical header and bounded fragment bytes to equal the settlement's retained proposal
  and referenced build fragments; the stored settlement itself must pass canonical decoding and
  exact closure validation. Equal digests are not sufficient for either check.
- `draft-marker-seals` stores one V1 durable resumable seal record keyed by exact draft, captured
  combined-root/build identity, exact `DraftMarkerCommitmentV1`, and caller-owned seal-operation
  identity. It retains only the next marker-order-tree cursor, completed marker frontier,
  incremental sequential digest/count/maximum state, and closed `Open`, `Cancelled`, `Failed`,
  `Superseded`, or `Sealed` lifecycle. Only exact EOF plus frontier/count/maximum/commitment/root
  agreement creates the package-issued opaque `DraftMarkerSealProofV1` binding the exact root and
  commitment to `SequentialMarkerSummaryV1`; raw summary values and open
  records are not proof. Exact replay returns the same proof, while a disagreeing natural identity,
  cursor closure, tree record, or sealed result is collision or corruption.
- `draft-composer-builds` is keyed by exact source combined root, format version, and caller-owned
  materialization operation identity. Its V1 value stores the source composite cursor, output
  `ComposerV1` manifest frontier, encoder state, exact input/output summaries, and closed `Open`,
  `Cancelled`, `Failed`, `Superseded(successor operation)`, or `Sealed` lifecycle.
  `draft-composer-materializations` is keyed only by exact source combined root and format version
  and stores the immutable sealed content reference, source combined-root digest/summary, and exact
  canonical Composer summary/digest. A second disagreeing sealed result is a collision.
- `terminal-repair-snapshots` stores one package-local V1 build head keyed by the existing target
  Syndic thread and turn natural identity. Its opaque storage-owned generation and references do not
  cross the package boundary and do not create a shared repair identity. The head stores exact CAS
  thread/turn correlation, terminal outcome, capture-gap reason, adapter version, pinned release,
  consumed request-attempt nonce and claim transition, request and response digests, declared item/
  content/page totals, repair time, and open-or-sealed lifecycle.
- `terminal-repair-item-pages`, `terminal-repair-content-pages`, and
  `terminal-repair-media-pages` use package-private V1 codecs keyed by the target plus the opaque
  build generation and one-based page ordinal. Every page repeats its kind and ordinal, retains its
  exact count, encoded-byte length, page digest, and preceding-page chain digest, and rejects gaps,
  duplicate ordinals, trailing bytes, unknown variants, invalid UTF-8, or a field beyond its closed
  bound. Item pages retain complete ordered semantic final-item fields and per-item digests; content
  pages retain exact field/range bytes; media pages retain only finalized asset identity, byte
  digest/length, authenticated adapter/release/runtime/`savedPath` provenance, and the exact matching
  cross-domain media identity and commitment supplied by the system command.
- Open or failed repair stages are unreachable from canonical items, transcript projections,
  history reads, catalog reads, and replay. Each bounded page-stage command validates and durably
  commits that page's complete item fields, identities, digests, provenance, or media witnesses while
  advancing the build head's checked totals and family chain commitments. For a repair-media page,
  this package's participant records the matching noncanonical media witness and advances only the
  Syndic build state; it publishes no canonical history authority.
- This package's participant in the final repair command validates the exact `RepairRequired` gate
  and consumed request claim, target correlation, terminal outcome, sealed build head, declared
  totals, complete family commitments, adapter/release provenance, and finalized-media commitments.
  It then selects the complete snapshot-backed canonical source, terminal state, projection
  staleness, sealed repair metadata, and exact `FinalizingHistory(target)` successor gate. Missing
  or disagreeing package-local facts reject that participant and leave existing Syndic authority
  unchanged.
- Cross-domain staging and final publication are the system-owned `HomeCommand`s defined by
  `doc/systems/cas-live-syndic-transcript/design.md`. This package contributes only its Syndic
  participant and cannot independently assert whole-command success or make partial repair media
  canonical.
- A V5 `input-gates` value canonically stores the exact stopping variant—blocked Syndic turn plus
  16-byte stop-operation nonce—the compacting variant naming its parentless provider-operation
  turn plus 16-byte compaction-operation nonce, and the distinct awaiting-terminal variant naming
  its unknown-terminal turn. Its distinct `RepairRequired` variant stores the exact target Syndic
  turn, exact correlated CAS thread and CAS turn, and a closed capture-gap provenance containing the
  reason, exact bounded terminal/capture-gap witness identity and digest, and optional compact
  provider-observation issue reference that proved it. It also stores the canonical request
  disposition tag and, when consumed, the exact 16-byte request-attempt nonce plus nonzero source
  and successor gate revisions. Unknown disposition or capture-gap tags, an incomplete consumed
  transition, absent correlation, a nonterminal target, or unbounded external identity is an invalid
  encoding rather than an incomplete default. A V3
  `accepted-route-generations` value adds the
  `AwaitingTerminal(exact prior steering target)` authority. V4 route leaves add the closed
  `UnknownTerminal` next-turn reason. There are no predecessor record decoders because the V5
  domain is replacement authority.
- `stop-operations` is a primary family keyed by the exact 32-byte concatenation of Syndic thread
  identity and stop-operation nonce. Its V1 value repeats both key fields and stores the immutable
  target, record revision, four fixed cause-first-revision slots, an optional dispatch-claim source
  revision and attempt nonce, and a closed live-or-consumed state. Live states are `Admitted` and
  `DispatchClaimed`; consumed states are safe reopen, matching terminal, and stop abandonment, each
  carrying its exact bounded successor witness while retaining all earlier fixed provenance.
  Scoped stop reconciliation requires key/value agreement and follows only the selected gate,
  operation, target, claim, and successor natural closure. Proving that every stopping gate selects
  exactly one live record, that no other live record exists, and that every consumed record agrees
  with its successor belongs only to explicit schema validation, scrub, background maintenance, or
  corruption investigation.
- `compaction-operations` is a primary family keyed by the exact 32-byte concatenation of Syndic
  thread identity and compaction-operation nonce. Its bounded V1 value repeats both key fields and
  stores the admission `BerylHomeId`, provider-operation turn, provider-operation execution
  snapshot, exact binding and loaded target, admitted revision, optional dispatch claim and
  attempt, closed request disposition, optional published CAS turn, fixed ordered
  status/marker/terminal frontiers, and a live, stopping, or consumed disposition with its exact
  successor witness. It stores no timeout deadline, lifecycle-continuation intent, accepted-input
  collection, or provider payload.
- `compaction-settlement-receipts` is a primary family keyed by the same exact operation identity.
  Its bounded V1 value stores the exact consumed operation transition, complete source and
  successor input-gate records, settlement, and optional continuation topology. It is created only
  in the atomic consumption command; a live operation with a receipt, a consumed operation without
  one, any operation/receipt mismatch, or an orphan receipt is corruption. Later gate, binding,
  selected-path, and continuation lifecycle descendants remain valid only after the immutable
  receipt authenticates their exact historical predecessor.
- Every compacting gate selects exactly one live compaction record. A record handed to stop names
  the current stop nonce while the stopping gate and stop record name the same provider-operation
  target. A consumed record is inert but remains exact response and mutation-reconciliation
  authority. Scoped compaction reconciliation proves key/value identity, nonce non-reuse,
  contiguous fixed transition provenance, provider-source ordering, gate/stop pairing, turn and
  snapshot agreement, and any named binding, CAS-turn reverse index, item frontier, continuation-
  turn, or queue-release successor within that operation's bounded natural closure. Cross-record
  enumeration is confined to explicit validation, scrub, background maintenance, or corruption
  investigation.
- A V3 `turns` value adds the closed `BerylLifecycleContinuation` conversation origin while
  preserving ordinary-user and provider-operation kinds. A V2 `execution-snapshots` value is a
  closed ordinary-conversation or provider-operation shape; the latter contains no accepted route,
  ordinary active-gate correlation, or native-count increment. `cas-turn-index` covers published
  provider-operation CAS turns as well as ordinary turns and permanently rejects reuse.
- Index values retain the authoritative identity plus the revision or digest needed to prove agreement. Empty marker values are not sufficient index authority.
- Binding records are immutable revisioned history keyed by thread and binding revision. `binding-heads` selects exactly one current record per thread. `cas-thread-bindings` records immutable ordered membership for every CAS-bearing binding revision, while `cas-thread-index` permanently assigns each CAS thread identity to one Syndic thread, its first and latest binding revisions, and one-way retirement at the first stale or abandoned revision. A scoped binding read requires its membership sequence, binding history, and reservation frontiers to agree exactly within the named thread/CAS natural closure. After retirement, that CAS thread cannot authorize execution for either the original owner or another thread. Only agreement with the current valid or active binding head and a non-retired reverse record authorizes execution; a retired index entry is provenance, not live authorization.
- Immutable turn topology and mutable lifecycle/frontier facts occupy separate `turns` and `turn-states` families so later event commits cannot rewrite parentage through a lifecycle update.
- Every non-root immutable turn stores one deterministic 128-bit ancestor skip. Its target depth is
  `max(1, depth & (depth - 1))`; roots store no skip. A scoped lineage read proves the skip names the exact
  ancestor at that depth. Selected-path membership therefore uses constant resident memory and at
  most 2,080 bounded turn point reads for the complete `u64` depth domain instead of an unbounded
  parent walk or a per-turn jump table.
- Every non-root thread with a parent-thread handoff binding stores the same deterministic skip
  shape over immutable thread lineage; top-level threads have depth one and no parent or skip.
  A revision-bound lineage query uses the selected leaf's depth and digest to return bounded
  top-to-bottom ancestor pages by exact logical depth without retaining the complete path or a jump
  table. A scoped lineage read validates parent, depth, digest, and skip agreement through bounded point reads.
- A thread image-label origin span is immutable and maps one admission's monotonic frontier advance
  to its exact admitted owner and compact sealed asset-set proof. A child records its parent's
  current frontier as its immutable inherited frontier and copies no spans. Label lookup finds the
  unique span containing the ordinal, then point-reads the selected Beryl-state set's label-first
  index; lookup at or below the inherited boundary follows validated lineage toward the origin with
  constant resident state. Missing ordinals inside a published span remain reserved gaps.
- A context envelope is keyed by its typed draft-or-submitted-turn owner. First submission moves the same exact envelope bytes and owner payload from the draft identity type to its deterministic submitted-turn identity type.
- `DiscussionContextRange` uses half-open absolute canonical logical UTF-8 byte coordinates within
  the source item, never projection-local coordinates. The range must lie within one finalized
  source projection and is resolved through bounded logical-range reads over the content indexes.
- That submitted-turn context owner remains stable after first submission. Scoped context resolution requires its immutable parent to agree with the context source turn but does not require the owner turn to remain on a later replacement-selected discussion path.
- Interrupted and superseded item-projection generations, transcript generations, path records,
  generation-owned indexes, and build records remain coherent derived state but are not selected
  authority. Immutable projections and resources referenced only by that state remain retained
  until future garbage collection.
- An immutable projection or resource record may also remain unreferenced after an interrupted
  derived write. Explicit scrub or background garbage-collection analysis treats that exact primary record as an unreachable garbage-collection
  candidate, not visible membership. Any reachable membership, set, head, transcript entry, or
  context envelope still requires its complete exact reverse agreement.

## V5 Bounds And Canonical Encoding

- Persisted integer ordering uses unsigned big-endian encoding. Composite index keys order first by their owning identity and then by one-based ordinal or revision. Cursor-only lower or upper sentinels are rejected as stored keys.
- Stable Beryl and Syndic identities use their exact 16-byte payloads. Digests use exact 32-byte values. External CAS identities retain validated UTF-8 and remain bounded by `beryl-model`.
- One terminal-repair build admits at most 262,144 ordered items, 268,435,456 exact encoded item/
  content/media bytes, and 65,536 staged pages across all three staging families. Each page admits at
  most 256 entries and 65,536 encoded bytes. These are hard V5 codec and mutation ceilings, not
  caller-selected budgets: exceeding any count, byte, field, or page limit rejects the repair
  without truncation or partial publication. Normal live-capture terminal-audit page, resident-item,
  and already-admitted-source limits do not lower this independent repair ceiling.
- Repair staging admits one bounded page per short command and advances checked cumulative item,
  byte, media, and page totals plus domain-separated chain digests in the build head and immutable
  family commitments. The final atomic seal selects those already staged paged commitments by
  reading only the compact sealed head, fixed family commitments, gate, and required publication
  witnesses; it never materializes the snapshot, restages page payloads, or walks the page set while
  holding the writer.
- Stop-operation keys are exactly 32 bytes. Stop-operation values use the package's 65,536-byte
  small-record ceiling, but their only variable-width fields are the exact CAS thread and turn
  identities, each limited to 256 UTF-8 bytes by `beryl-model`. Causes use four canonical
  fixed-width revision slots in closed cause order; zero means absent and a nonzero value is the
  exact first-publication revision. The optional dispatch claim canonically contains both its
  source revision and attempt nonce. Every other identity, revision, generation, state, and
  successor field is fixed-width. Unknown state tags, operation kinds, noncanonical cause or claim
  provenance, or a value exceeding either external-identity bound are invalid rather than
  truncation.
- One content chunk carries at most 65,536 encoded bytes, and one staged append command carries a
  fixed bounded chunk count. Content manifests use `u64` counts and lengths; no smaller whole-draft,
  whole-submitted-input, or whole-provider-item byte ceiling is encoded in V5.
- Every draft combined-root, sequence node/leaf, tagged marker-identity-index internal/leaf, tagged
  marker-order-commitment internal/leaf, marker-seal record, build,
  mutation-staging head, canonical staging page, immutable staging-progress receipt, canonical
  fragment, immutable build-progress receipt, settlement, candidate-session head or
  receipt, and materialization record fits the 65,536-byte value ceiling. Internal
  nodes in all three structures have from 2 through 128 children, except that a selected root node may
  have from 1 through 128; every leaf in one nonempty structure has the same depth, and each height
  is at most 64. An identity leaf contains exactly one stable marker id, final label, same-anchor
  order key, and sequence marker-leaf identity/digest, with no absolute anchor or position. A
  commitment leaf contains exactly one stable marker id and final label, with no text position or
  order key. The canonical empty combined root has none of the three nodes, all heights and every
  logical byte, newline, line, piece, marker, and identity aggregate zero, and the exact V1 empty
  sequence-root, identity-index-root, marker-order-commitment-root, and combined-root digests. A text leaf contains at least one complete UTF-8 scalar and no
  more payload than its codec-derived record ceiling; a sequence marker leaf contains exactly one
  bounded marker identity, order key, and label.
- One pre-finish mutation-page batch admits from one through 257 existing source or proposal pages.
  Every page belongs to the source head's operation and one common lane, is nonempty, has a positive
  item ceiling no greater than 256, contains no more items than that ceiling, and has a positive byte
  ceiling and complete encoded page value no greater than 65,536 bytes. Page keys, one-based lane
  ordinals and transition ordinals, input and successor cursors, prior and successor cumulative
  identities, and checked cumulative totals must be consecutive from the source head. The batch's
  checked maxima are 257 pages, 65,792 canonical entries, and 16,842,752 complete encoded page
  bytes; any page or aggregate overflow or excess rejects before mutation.
- Batch preparation locally validates the supplied source staging head, its selected receipt
  reference, and matching candidate-session `Staging` custody, then derives each page and matching
  progress receipt in order from the prior derived frontier and derives one final head and
  candidate-session endpoint. The prepared value retains only this one bounded batch in boxed or
  slice-owned pages and receipts plus fixed source/final closure; it never retains an operation-wide
  collection. Begin, finish, and pre-build terminal commands carry no page value and continue to
  write only their bounded head/receipt/session closure.
- One Syndic domain mutation contribution reserves and writes every page and progress receipt in
  the prepared batch and replaces the mutable staging head and candidate-session head only with
  their final endpoints. On the one writer snapshot it authenticates the stored source head and
  selected receipt closure and validates the matching source session exactly once, then requires
  every target page and receipt key to be absent. Home-store assembles that single participant on
  the same snapshot. A maximum batch contributes 516 Syndic record mutations: 257 pages, 257
  receipts, one final staging head, and one final candidate-session head. Home-store recomputes the
  exact aggregate record, encoded-key, and encoded-value counts with checked arithmetic before
  admitting its atomic Fjall batch, so the command commits the complete batch or none of it. The
  production profile's existing 16,384-record, 32-MiB encoded-key, and 64-MiB encoded-value ceilings
  accommodate this fixed maximum without widening that dependency contract. No committed prefix,
  intermediate head, or intermediate session endpoint can be observed after success, failure,
  cancellation, crash, or any persistence cut. The one-page fast path uses this same batch
  preparation and contribution with a one-element slice.
- Source and proposal lane ordinals, item totals, canonical-byte totals, batch page totals, and
  aggregate encoded bytes use checked arithmetic. There is no smaller cumulative operation cap
  below any representable checked-`u64` lane total and no operation-wide 256/257 limit. Preparation
  custody retains caller payload until complete batch acceptance or exact target reconciliation;
  neither a source-selected retry state nor an indeterminate or fail-closed closure authorizes
  payload release.
- Batch reconciliation returns `SourceSelected` only when the stored head and candidate session are
  byte-equal to the complete prepared source closure and every target page and receipt key is
  absent. It returns `TargetSelected` only when the stored head and candidate session are byte-equal
  to the prepared final endpoints and every page and receipt in the batch is canonically byte-equal
  to its prepared target. A byte-equal prefix is still partial occupancy. Any partial occupancy,
  replaced, missing, forked, or ahead page, receipt, head, or session, source/target disagreement,
  or occupied natural target identity while the source is selected is collision or corruption and
  fails closed without mutation. Digests may reject inequality early but never replace complete
  canonical byte comparison.
- Cancellation before home-store admits the batch command produces no batch effect. Once a command
  is admitted, cancellation cannot classify or retract it. `Indeterminate` carries sole command
  custody rather than a third reconciliation result or sixth public edit outcome; exact batch
  reconciliation must establish source selection, target selection, or fail-closed partial state
  before the caller performs cancellation or any other terminal handling.
- The finish-to-builder command contributes exactly five Syndic record effects: one immutable
  staging-transfer receipt, one immutable ordinal-one `DraftPieceBuildProgressReceiptV1`, one
  staging-head successor selecting `Building`, one initial draft-piece build-head successor, and
  one candidate-session successor whose sole custody transition is `Staging` to `Building`. It
  carries no staging page, build fragment, tree node, candidate root, history transition, or
  settlement. Its build receipt/head stores the exact finished staging closure, the two initial
  unconsumed lane frontiers, and the canonical empty fragment endpoint. The complete encoded key-
  plus-value sum of those five effects is bounded by and must
  fit the existing 4,194,304-byte draft-piece command ceiling; excess rejects before mutation.
- One post-finish draft-piece fragment command admits at most 256 fragment records and 65,536 inserted UTF-8
  payload bytes. One path-copy command reads or emits at most 256 records across all three structures and
  at most
  4,194,304 encoded key-plus-value bytes. Larger replacements and tree repairs continue through
  revision-bound build frontiers. After the five-effect finish-to-builder transfer above, every
  fragment-stage, path-copy, other build-advance, and terminal build command targets exactly one
  fixed-size immutable build-progress receipt and one compact build-head successor plus one fixed-
  size candidate-session custody before/after state. Each post-transfer nonterminal command
  advances the `Building` endpoint; the sole terminal build command clears it. A new post-transfer
  commit creates the receipt and effects and updates the build and candidate-session heads in the same atomic
  command; exact replay creates nothing and requires the stored build head, session slot, and
  complete target closure already to match. It authenticates only the endpoint receipt and its immediate
  predecessor plus the bounded roots, canonical fragments, and path records referenced by that
  quantum; no command or retained stager walks or retains the receipt chain. A terminal election
  additionally reads only the natural settlement key, proposed combined root when adopting, and one
  editor-candidate session head.
  Candidate adoption never reads or writes the current-draft selector, reverse index, or history
  summary. One publication command point-reads one captured candidate settlement/root, the prior
  and captured commitments, required completed seal proof when changed, the session head, current
  draft and reverse index, and history summary; its sibling Asset participant validates only compact
  owner/proof state. Final publication never traverses the candidate chain or marker tree.
- Those 256 records are one-command build capacity, never a cumulative operation limit. Source and
  proposal pages use checked `u64` cursors, counts, lengths, and cumulative canonical identities;
  explicit finish-input fixes the final totals in the staging head before any build exists. One
  logical edit may consume any representable number of pages without a whole-operation collection
  or a special cumulative 256/257 boundary. One widget-page payload is released only after its
  complete physical-page batch, every target receipt, final head, final session custody, and
  cumulative identity are durable or exact target reconciliation proves that closure. Later build
  and reconciliation read those staged records in bounded pages rather than asking the caller to
  retain or resupply them.
- Independently, one post-finish staging-window command consumes at most 256 consecutive physical
  staging pages and therefore at most 256 one-item page records. Window acquisition performs at most
  two point reads per page, one page plus its staging-progress receipt, and has an exact allowance of
  nine additional endpoint reads: candidate-session head, staging head, finished staging receipt,
  build head, selected build receipt, its immediate predecessor when present, working sequence root,
  working identity-index root, and working marker-order-commitment root. It therefore uses at most
  521 staging-window point reads. At the
  existing 65,536-byte ceiling for each complete encoded value, checked multiplication gives an exact
  34,144,256-byte aggregate encoded-value ceiling for that 521-record acquisition closure. The
  separate bounded sequence/index/commitment descents and path-copy reads needed to apply an item retain their
  existing height, record-count, and 4,194,304-byte command limits.
- The selected build receipt/head is the only continuation cursor. It retains the exact staging
  identity and finished staging-head/receipt reference; for each lane, the next page ordinal and
  input cursor, consumed item/byte totals, and cumulative identity; and the current fragment
  endpoint/count/chain. Storage locates the next page directly at `(staging identity, lane, next
  ordinal)`, authenticates that page and its staging-progress receipt against the retained before
  frontier, and admits a consecutive window within the 256-page/item ceiling. The independent
  fragment-stage ceilings remain 256 fragments and 65,536 inserted UTF-8 bytes per command. Restart therefore
  uses bounded point reads from the current endpoint and never seeks from ordinal one, scans a staged
  prefix, consumes app reconstruction, or accepts caller page bytes.
- A nonempty source-lane window is valid even when it derives no proposal fragment. Its target
  receipt must advance the source consumed-page/item frontier, totals, and cumulative identity by the
  exact window, so every successful source-only command makes durable progress and cannot spin at one
  endpoint.
- Every window transition commits its exact before/after lane frontiers, page/receipt closure,
  fragment endpoint, and bounded effects into the next immutable build receipt. Replay byte-compares
  only that target closure. At the final staging boundary, the consumed lane frontiers must equal the
  `FinishInputV1` declarations and the derived fragment count and chain must equal the finish-derived
  proposal header. Later reconciliation proves prior bytes through those authenticated cumulative
  checkpoints and does not rescan already consumed staging pages or fragments.
- Edit-history transition, frontier, stack-link, and historical-root-adoption values each fit the
  65,536-byte value ceiling and contain only compact roots, positions, links, counters, policy, and
  replay facts. Each transition carries exactly one fixed 64-slot authenticated ancestor array and
  bitmap sufficient for every checked-`u64` journal depth. Append derives that witness from the
  authenticated predecessor using at most 64 point reads and fixed retained state. The configured
  durable history byte budget is a nonzero checked `u64`, not an entry count and not a function of
  document length. Within those families its charged set is only the
  current mutable live-frontier record and the retained transition/link records; publication or
  baseline snapshots and historical-root-adoption or other operation receipts are excluded. Each
  charged record includes exact family-key bytes plus canonical value bytes, repeated keys inside
  values count again, and Fjall and allocator metadata do not count. Checked successor accounting
  derives the exact required eviction amount and cumulative threshold. Starting from the selected
  head, highest-to-lowest binary lifting compares only authenticated same-lineage ancestor cumulative
  positions and selects the unique oldest eligible floor crossing that threshold in at most 64
  transition point reads and fixed state, one locally validated committed target per followed level.
  Admission already established witness derivation and root adjacency; ordinary selection validates
  local target agreement plus floor/head references, accounting, pins, and availability without a
  recursive corruption scrub. No draft-global seek result or valid sibling
  digest establishes membership, failure, or a cutoff. Adoption atomically removes only the selected
  prefix from logical availability without reading or copying content and without physically
  deleting any transition, link, root, node, leaf, or content. A typed history-capacity-unavailable
  result occurs only when the required non-evictable closure itself cannot fit.
- Draft text, marker, composite-piece, and materialization input pages return at most 256 records
  and 65,536 payload bytes. One lookup binary-searches at most 128 authenticated child envelopes per
  level and performs at most 64 sequence-node reads. Exact adjacent-gap validation performs at most
  two such
  descents plus one authenticated successor step. A marker page begins from one proven composite
  search key and advances leaf successors; more than 256 same-anchor markers returns an
  authenticated `(anchor, order key, marker identity)` cursor rather than increasing residency.
- A draft text demand has a byte ceiling from 4 through 65,536 inclusive. A marker-page demand has
  an object ceiling from 1 through 256 inclusive and a retained canonical response-byte ceiling from
  1 through 65,536 inclusive; marker identities, labels, cursors, and preceding/following facts all
  count toward that retained-byte ceiling. Validation windows, marker-edge proofs, and each
  first/last/adjacent-marker proof use those same fixed bounds. The implementation may perform at
  most the bounded descents and successor or predecessor steps named here and must reject an
  unrepresentable request rather than widen a page, retain an entire anchor run, or scan a whole
  draft or marker set.
- One marker-id lookup binary-searches at most 128 identity-index child envelopes per level,
  performs at most 64 identity-node reads, and authenticates stable occurrence facts. Given a
  caller-supplied composite position or anchor witness, presence/location validation then uses at
  most one 64-level sequence descent to verify the occurrence at that claimed location. ID-only
  lookup need not discover location. Insert/reinsert absence uses only the index descent. No
  uniqueness operation scans the complete sequence tree or marker set.
- `draft-marker-identity-index` keys canonically encode the owning draft, closed internal-or-leaf
  tag, and exact opaque record identity. Values repeat owner, tag, identity, and digest. Internal
  child entries are strictly stable-id-envelope ordered and count-prefixed; leaf values contain one
  canonical stable-id mapping. Unknown tags, key/value disagreement, duplicate or overlapping
  envelopes, noncanonical counts, trailing bytes, or a mapping outside its ancestor envelopes are
  invalid encodings.
- `draft-marker-order-commitments` keys canonically encode the owning draft, closed internal-or-leaf
  tag, and opaque record identity. Values repeat owner, tag, identity, structural digest, checked
  count, and optional maximum label; leaves contain only one stable marker id and final label.
  `draft-marker-seals` keys and values canonically bind the exact captured root/build identity,
  commitment, operation, cursor/frontier, sequential state, and lifecycle. Unknown tags,
  noncanonical cursors or maxima, key/value disagreement, trailing bytes, or a seal selecting a
  different root or commitment are invalid encodings.
- Every V5 exact-replay classification at an occupied natural key requires equality between the
  request's canonical identity bytes and the corresponding canonical request bytes retained by the
  occupied record; a command that proposes the whole record requires complete canonical key/value-
  byte equality. These equalities are necessary but not sufficient for build-transition replay:
  the stored build head must already select the proposed target receipt and every same-command
  effect must match. An occupied target while the head still selects the source is corruption even
  when the target bytes are equal. A digest or summary may reject inequality early but never
  establishes equality.
  A multi-fragment draft command compares its canonical build header, authenticated source endpoint,
  and only the bounded page/fragment/effect closure it proposes. A target-selected replay compares
  that complete target closure byte for byte. Earlier fragments are fixed by the authenticated
  source receipt's fragment endpoint and chain and are not rescanned; a detached terminal chain
  digest is never replay authority. The first different canonical bytes classify the occupied
  identity as collision or `OccupiedIdentityNoncommit`, as applicable, and authorize no mutation.
- Draft and materialization counts, UTF-8 lengths, newline and logical-line counts, piece ordinals,
  marker counts, and fragment ordinals use checked `u64`. The bounds above limit one command and resident traversal, not one
  logical draft, edit, same-anchor marker set, or sealed Composer value. Progress-transition
  ordinals also use checked `u64`; one fixed-size receipt per bounded work quantum keeps retained
  state and command work fixed without imposing a smaller whole-edit bound.
- Ordinary candidate adoption, undo, and redo use bounded or logarithmic path and index work and
  never scan the full root or journal. Transition-witness construction from an already authenticated
  predecessor and direct threshold-to-floor selection each perform at most 64 transition point
  reads, but only append admission constructs and fully validates witness derivation. Ordinary
  retained-history selection trusts correctly committed immutable references after local validation.
  An undo or redo may
  require multiple bounded validation and
  reconciliation commands, but its direct historical-root adoption remains one logical operation,
  one candidate generation, and one terminal settlement with no history-sized resident state.
- Provider structured values accept at most 128 nested list/object containers, matching the pinned
  backend JSON parser's configured recursion depth. The streaming validator uses fixed bounded depth
  state; string bytes and collection element counts remain chunked and have no smaller per-item cap.
- Composer text, atom ordering, and image-marker count are logical `u64` domains represented by
  bounded content and marker-index pages. No whole-composer marker ceiling is used as a process
  memory bound; exact provider/model input limits may reject a dispatch without changing stored
  content authority.
- Accepted-route generations and their leaves may contain any representable durable count and
  logical byte total. The input gate and selected generation heads retain checked `u64` aggregates,
  while schedulers, explicit validation, and delivery workers traverse fixed revision-bound cursor
  pages and admit only bounded active work. CAS-turn publication and projection loss change one
  compact generation head and, for the exact-rejection abandonment variant, at most the one named
  leaf rather than every member. Logical backlog size never authorizes a resident route vector or a
  process-memory safety cap.
- One recovery projection contains from one through 262,144 nonempty canonical text items and at most 262,144 logical UTF-8 text bytes. The item ceiling follows from the byte ceiling and therefore does not introduce a smaller retained-history or turn-count limit.
- Recovery assembly first walks only the exact immutable parent topology and matching
  recovery-complete turn states, adding only compact item counts with checked arithmetic. In the
  pending-parent scope, the one immediate authority-lost tail-context exception is authenticated
  independently before its ordinary item proof; no earlier incomplete turn is accepted. Explicit
  `incomplete` remains terminal for lifecycle accounting rather than recovery-complete. An item
  total above 262,144 is rejected
  before starting the replay pass or reading item text. An accepted total retains only compact
  count, byte, revision, tail, and digest proof; canonical indexes and text are replayed later in
  bounded pages and ranges without allocating an item frontier.
- Root-to-tail recovery scans lift the immutable tail to each deterministic depth through stored
  ancestor skips; neither preflight nor cursor retains a path vector. Preflight first proves the
  item ceiling without text reads, then proves role, lifecycle, media exclusion, nonempty item and
  context-byte budgets, and hashes bounded text ranges through
  `beryl-model::RecoveryItemSequenceAccumulator`. Cursor EOF independently repeats the same closed
  sequence proof.
- One normalized source-event metadata record remains bounded and refers to exact sealed provider
  frame ranges; it does not contain a whole provider payload or impose a 262,144-byte event-content
  ceiling. Canonical-item records contain metadata and a content reference rather than whole text.
  One transcript-view entry or inline projection remains within the 65,536-byte page limit; larger
  source is represented by ordered projections or resources without ceasing to exist as canonical
  chunks.
- One metadata-only thread, turn-state, history-summary, binding, execution-snapshot, projection-metadata, or resource-metadata record remains at or below 65,536 payload bytes. Codec ceilings include only codec payload bytes; the home store owns the record-version prefix.
- One projection-construction step consumes at most one 65,536-byte canonical chunk plus a bounded
  UTF-8 carry and undecided Markdown window, and emits a bounded record batch. Persisted undecided
  Markdown never exceeds the accepted 16,384-byte inline-paragraph threshold plus one UTF-8 scalar
  carry.
- Public transcript-entry, transcript-path, item-projection, projection-resource, thread-lineage,
  accepted-ready-source, accepted-ready-candidate, accepted-next-source, accepted-next-candidate,
  and activity-query pages contain at most 256 records and 65,536 stored encoded bytes. One public
  textual-resource range response contains at most 65,536 payload bytes. Callers may request
  smaller byte and item bounds; larger requests are clamped and return continuation cursors rather
  than materializing a larger page. An accepted-ready-candidate or accepted-next-candidate cursor
  binds the thread, gate revision, source generation/revision, and scanned-after ordinal; it can
  therefore advance across non-candidate source ranges without repeating an empty page or
  accepting route drift.
- Projection format V1 applies the exact paragraph, code, table, preview, and page thresholds in
  `doc/systems/syndic-conversation-history/concepts.md`. Malformed or undecidable syntax is emitted
  as source-preserving spans of at most 8,192 UTF-8 bytes.
- Every bounded collection encodes its exact count before its elements, rejects multiplication or allocation overflow before materialization, and rejects trailing bytes, unknown tags, invalid UTF-8, invalid enum combinations, and noncanonical option encodings.
- Footprint tests independently enumerate every record shape in idle submission and accepted-input
  promotion, prove each declared maximum encoded key-plus-value length, and checked-sum the exact
  per-operation record and byte totals. An unrepresentable total is a package contract failure; it
  is never saturated, wrapped, or replaced by a caller estimate.
- Home-store page, item, stored-byte, and decoded-byte limit failures remain typed read failures and
  do not imply durable corruption. Mutation and validation callbacks perform only operation-bounded
  reads and never wait for resource capacity while holding the serialized writer.

## V1 Structural Proofs

- Routine open performs no application-record walk. The proofs in this section are enforced by the
  mutation that publishes each record and rechecked only over a request's bounded natural closure.
  Any every-record or every-thread recheck is an explicit schema-validation boundary, scrub,
  background-maintenance pass, or corruption-evidence investigation.
- Every immutable turn header stores a nonzero depth and a V1 chain digest. A root has depth one and the canonical root digest derived from the domain separator and its exact turn id. A child has its parent's depth plus one and a digest derived from the V1 domain separator, child id, parent id, and parent chain digest.
- Scoped turn-lineage validation recomputes each root or child digest and checks exact depth progression. This proves the named parent chain reaches a root and cannot contain a cycle while retaining only one bounded page and point-read parent records.
- Every thread has the corresponding nonzero lineage depth and domain-separated chain digest.
  Top-level threads use the root form; a discussion child derives its digest from its exact child and
  parent thread identities. Scoped thread-lineage validation checks parent indexes, depth, digest, and skip facts in
  bounded pages without constructing an ancestor set.
- Explicit schema validation, scrub, background label maintenance, or corruption investigation
  validates each thread's inherited/current image-label frontiers and immutable local
  origin spans against parent-frontier inheritance, contiguous span advances, and exact admitted
  sealed-set proofs. It scans indexes in bounded pages and never constructs a per-thread used-label
  set.
- An empty selected path uses the V1 digest of the dedicated empty-path domain separator. A nonempty thread's selected-path digest equals its committed tail's chain digest.
- A pending, active, or unknown-terminal turn must be the committed tail of its origin thread. Because one thread has one committed tail, this is also the bounded durable proof that one thread cannot retain competing execution-blocking turns.
- A `RepairRequired` target is a proven-terminal ordinary turn and must equal the owning thread's
  committed tail. No same-thread successor turn, tail advance, fork, replacement execution,
  rollback, accepted-next promotion, compaction admission, or recovery injection may publish while
  that gate remains current. The repair gate's CAS correlation and capture-gap witness must agree
  exactly with the target's terminal state, CAS reverse indexes, terminal/capture-gap witness, and
  any retained provider-observation issue evidence. The exclusion is scoped to the owning thread
  and operations that target its blocked turn; unrelated threads remain independently mutable.
- Scoped reconciliation proves the named records' current structural agreement. Parent immutability and one-way draft consumption are additionally enforced by the absence of any production mutation that rewrites a turn header or recreates a consumed draft; they are not inferred as historical events from one snapshot.
- Scoped item validation checks each named canonical item according to its exclusive source. A normal live-provider
  item replays its exact `item-source-events` sequence in bounded memory and requires kind,
  assistant phase, external CAS identity, typed provider-frame references, structural and chunk
  digests, selected provider-narrative generation and digests when applicable, completion state,
  and source-event frontier to agree exactly. A terminal-repair item instead validates the exact
  opaque package-local snapshot reference, ordinal/digest, provenance, complete terminal item set, and snapshot-backed
  canonical manifests/ranges without requiring source events or provider frames. A completed normal
  provider item requires an exact sealed completion frame and, when transcript-visible, a durable
  exact equality result against its selected append generation. A retained mismatch must agree with
  the typed repair-required or history-incomplete disposition and may never select completion text
  for presentation.
- Scoped activity-query validation checks the named head, source memberships, and entries against exact owner/period,
  source turn and event interval, CAS item identity, provider lifecycle, handoff range, full ordering
  key, retained stored bytes, cutoff, and logical counters in bounded pages. It proves the exact
  first activity-visible event, the presence of every logically running row, and the deterministic
  maximal newest completed prefix under both fixed retention caps; coherently shrinking those
  records or counters is corruption. A child handoff can be published only after the child turn is
  proven terminal; its inactive membership binds the exact terminal source frontier so later child
  activity cannot make parent authority stale. A
  rebuildable mismatch must carry an explicit stale head and complete a bounded rebuild before the
  query can publish; it cannot expose a mixed generation.
- A normally captured proven-terminal turn with admitted source events must end at the matching turn-
  ending source event. A repaired turn instead requires its exact published terminal-repair
  authority and complete sealed item-set digest. Its contiguous finalized-item frontier may advance
  afterward only over immutable content admitted by that one selected authority.
- Projection construction may consume one exact current live or immutable canonical snapshot. Any
  source advance atomically marks a selected projection stale and supersedes an incomplete build;
  completed older generations remain coherent historical snapshots. Terminal item closure is a
  separate two-stage transition: a bounded freeze mutation converts closed canonical content and
  its item reference to immutable source without advancing the finalized-item frontier, then a
  visible item advances only after one current completed item-projection set exists. Operational
  items advance after freezing because they own no transcript projection.
- An explicit schema-validation boundary, scrub, background-maintenance pass, or corruption
  investigation may enumerate unreachable history. It accepts a retained turn only when its complete
  natural parent closure, indexes, items, projections, resources, and provenance remain internally
  coherent. A missing parent is corruption, not valid unreachable history.

## Revisions And Ordering

- Thread, durable draft-selector, and editor-candidate session revisions are monotonic and
  independently checked. A candidate revision is meaningful only with its exact draft, session,
  combined root, and edit-history frontier revision.
- `DraftEditorCandidateSessionOpenOutcomeV1` is `Opened(head)`, `ExactReplay(head)`,
  `StaleDisposed(head)`, `SelectorConflict(current selector)`, or
  `OccupiedIdentityCollision(proof)`. Open atomically
  validates the expected thread, draft, complete current selector and root, fresh session identity,
  matching history reference, and open-operation identity, then creates one active head whose base,
  published, and newest-candidate checkpoints all name that exact selector, root, matching history
  state, closed root-build identity, and logical byte/line extent. The published checkpoint retains
  the immutable current-draft history reference; the newest checkpoint retains the canonically
  forked live frontier revision that selects the same root. `ExactReplay` requires the same canonical
  open request and head bytes while the
  identical session remains active. The identical request and head bytes against that session after
  disposal return `StaleDisposed(head)`. An occupied session identity with different canonical
  request or durable-base bytes returns `OccupiedIdentityCollision(proof)`, never a stale outcome.
  `SelectorConflict(current selector)` applies only when the session identity is absent and fresh
  but the expected selector has drifted.
  `DraftEditorCandidateSessionReadOutcomeV1` is `Active(head)`,
  `Disposed(head)`, `Absent`, `ConcurrentChange`, or `InvariantFailure`; it is an explicit
  session-id read and never a routine current-draft or reopen scan.
- `MutationBeginV1` is admitted only against the exact active session generation and predecessor
  candidate/root/history pair with an absent custody slot and absent staging/build/settlement/root
  natural keys. It atomically writes ordinal-one staging receipt and receiving head and installs
  `Staging` custody. A page-batch command names the head-selected source receipt, exact lane
  frontier, and one through 257 consecutive page inputs. Preparation derives every next cursor,
  ordinal, prior cumulative identity, page, and matching receipt and requires every target page and
  receipt absent. The one domain contribution atomically appends every page and receipt and advances
  only the final head and slot endpoint. A source-selecting head with any occupied target is
  corruption. A target-selecting head proves exact replay only when the final head, every receipt
  and page, and final custody bytes equal the complete proposed target closure; any disagreement is
  collision/noncommit for that mismatching request and changes nothing.
- `FinishInputV1` names the exact current endpoints, checked totals, cumulative identities, and
  successor positions for both lanes. Finish atomically freezes only a byte-equal receiving head;
  it neither walks pages nor creates a build. One later bounded transfer authenticates the finish
  target and immediate source closure, derives `DraftPieceEditHeaderV1`, creates the ordinal-one
  build receipt/head, changes the staging head to `Building`, and replaces the candidate slot's
  `Staging` tag with `Building` in the same command. The build endpoint begins with the exact finished
  staging closure, both lanes' initial unconsumed frontiers, and the canonical empty fragment
  endpoint. Each advance derives only the next window of at most 256 physical pages/items from those
  frontiers under the 521-read and 34,144,256-byte acquisition ceilings, validates or derives its
  separately bounded maximum of 256 fragments and 65,536 inserted UTF-8 bytes, and persists the
  successor lane and fragment checkpoints. A source-only window advances its durable lane frontier
  even when it creates no fragment. Reconciliation and same-home restart start
  from the exact session slot and current staging/build endpoints and require no caller page,
  ordinal-one restart, prefix seek, or reconstructed proposal.
- Cancellation may win before begin with no mutation. An ordinal-one durable terminal-before-begin
  election may produce `Rejected`, `Conflict`, `Cancelled`, or `Error`; its receipt records `None`
  source and `None`-to-`None` custody. `Conflict` is reachable only there, when a stale begin's
  expected same-session candidate generation/root/history pair differs from that same session's
  exact observed current pair, meaning its newest pair; the evidence also fixes the observed session
  revision. A coherent admitted receiving or finished head retains
  `Staging` custody whose predecessor pair equals the session newest pair, so only `Rejected`,
  `Cancelled`, or `Error` may terminalize it with exact outcome evidence, its selected source, and
  `Staging`-to-`None` custody. Attempted admitted `Conflict` fails closed without mutation.
  Replay and terminal staging-status reads authenticate the immutable evidence and complete target
  closure; neither re-evaluates a valid old terminal `Conflict` against later same-session work,
  another session, publication, or the durable current-draft selector. Once
  transfer installs `Building` custody, the draft-piece terminal election applies. Cancellation
  after final adoption admission cannot retract it. Terminal staging pages remain invisible
  immutable future-GC candidates; an admitted nonterminal head remains claimed and prevents clean
  session disposal, while a distinct fresh session remains unblocked by all session-qualified keys.
- A draft-piece transaction derives its natural settlement key, canonical proposal-header bytes,
  and proposal digest from the authenticated finished staging identity and receipt, exact draft/
  session/predecessor candidate revision, combined root and edit-history frontier, caller-owned
  operation identity, declared ranges and fragments, terminal frontier, and intended successor
  positions. The first build transition names a `None` build source and the exact ordinal-one build
  receipt/head plus custody transfer as its target; admission requires the build, settlement,
  receipt, and candidate-root keys absent and the byte-equal `Staging` slot present. An
  existing operation is continuation or replay only through the explicit source-versus-target head
  classification below. A terminal settlement is replay, and an open/complete build is continuation,
  only when the staged canonical header and authenticated current build endpoint agree and the
  command's bounded source or target closure is canonically byte-equal. Previously consumed page and
  fragment bytes are represented by the receipt's authenticated lane frontiers and fragment endpoint/
  chain and are not rescanned. Digests may reject inequality early but never prove equality. A different canonical byte in an
  unsettled build or preexisting settlement, build, fragment, progress receipt, or candidate root at this operation's natural
  identities proves this proposal was not admitted and returns
  `Error(OccupiedIdentityNoncommit)` with the first immutable occupied natural-key comparison
  witness. A different operation or proposal while the slot is occupied returns the typed inert
  `ActiveOperationConflict(custody)` outcome and mutates nothing. No staging command advances the
  candidate/history frontier or durable draft-selector revision or makes staged leaves readable
  through either selected boundary; only its custody endpoint in the session head advances.
- Source-receipt presence is determined only by transition ordinal, never by command kind. Ordinal
  one names `None` for the finish-to-build transfer; every ordinal greater than one names the exact
  immediately preceding build receipt. Each canonical-fragment stage, successor-
  construction advance, and terminal election names its checked target receipt, build head, slot
  transition, and effects. Before mutation each command point-reads the session head and custody
  slot, the stored build head, its named source receipt when present, and the roots, canonical-
  fragment endpoint, and path records referenced by that bounded quantum.
- An ordinal-one `None` build source requires the build head and target receipt absent and the exact
  finished `Staging` custody present. The command atomically transfers that custody to the exact
  operation/final-proposal `Building` identity. For a target ordinal
  greater than one, if the stored head selects the command's source receipt and equals its
  authenticated state, the target receipt key must be absent. Any occupied
  target in that state, including a canonically byte-identical value, is a corrupt predecessor-head/
  occupied-next split and permits no mutation or successful result. The session slot must already
  contain the same exact operation/proposal identity and authenticated source endpoint. An absent
  target permits one atomic immutable-receipt creation, complete effect publication, build-head
  advance, and custody-endpoint advance.
- Exact replay is allowed only when the stored head already selects the command's target receipt and
  equals the proposed target-head bytes, and the target receipt plus every same-command build,
  fragment, root, settlement, candidate-session head including the exact custody-slot transition,
  and other effect has canonical bytes equal to
  the proposed target closure. Differing occupancy is an identity collision. A missing effect, a
  head that equals neither source nor target, or any
  partial target closure fails closed; the package never repairs a predecessor-head/occupied-next
  split or reports it successful.
- Build-head reads, operation status, reconciliation, candidate-session and candidate-frontier
  validation, and settlement closure point-read the selected endpoint receipt and its immediate
  predecessor when present, plus their referenced root and record closure and the session custody
  slot. An open or complete endpoint must match the claimed operation/proposal and endpoint; a
  terminal endpoint must match the unique settlement that atomically cleared that claim. They never walk the
  receipt chain. Reconciliation returns committed replay only when the stored head selects the
  target and the complete target closure is byte-equal; a source-selecting head with absent target
  remains pending or proven noncommit as applicable, while a source-selecting head with any occupied
  target fails closed. A deleted, replaced, or forked receipt; a head ahead of its endpoint; or
  canonical fragments ahead of the authenticated receipt endpoint is corruption and fails closed
  rather than becoming continuation, replay, or repair authority.
- Replacement ranges are validated in increasing predecessor-root composite order. Each start must
  be at or after the prior end, each start must be at or before its own end, and both endpoints must
  carry exact UTF-8 and same-anchor gap authority for that root. All replacements consume the
  unchanged predecessor coordinate space; staging never offsets a later range by earlier inserted
  or removed bytes. Two empty ranges with the same start/end position are rejected, and the caller
  must coalesce their inserted fragments before begin. The build receipt retains the prior range end
  and successor anchor/order effect frontier, so a later page must continue that exact order without an
  earlier-page buffer. No range or uniqueness validation enumerates the full sequence or index.
- A proposal page's marker insert, removal, move, or same-id replacement is one self-contained
  staging item. Insert facts are accepted successor UTF-8 anchor, stable id, final label, same-anchor
  order key, and checked logical/marker/encoded-byte charges. Removal facts are stable id, label,
  order key, predecessor
  composite position/gap witness, and exact predecessor marker-leaf identity and digest. Move and
  same-id replacement contain both sets with one byte-equal stable id and label. No successor field
  contains a caller-selected gap or immediate-neighbor witness, and no field points to
  a removal on an earlier fragment or insertion on a later fragment. A split effect is invalid.
- When a marker effect is reached, one bounded build transition authenticates any removal against the
  immutable predecessor root and current working identity-index mapping, verifies the exact occurrence
  through one predecessor-sequence descent, and derives the removal-applied successor view. Storage
  then descends its current working sequence/index at the accepted successor anchor and order key,
  derives and authenticates the immediate insertion gap and neighboring leaves, validates the
  supplied charges, and proves the required stable-id absence. The same atomic admission either completes the bounded path copies or stores one
  fixed-size pending effect bound to the exact working roots and validated source/destination proofs.
  While it is pending, no later proposal item advances; bounded continuation commands write only
  unreachable working records, and one final effect command atomically installs the new sequence,
  identity-index, and marker-order-commitment working roots, updates the mapping, and clears the
  pending effect. A pure insert omits the removal half and
  proves absence in both the predecessor and working index; a
  pure removal omits insertion. Destinations may address text already introduced by earlier canonical
  proposal effects. An anchor beyond the current working logical extent or construction frontier is
  a future dependency; an occupied `(anchor, order key)` owned by a different id, an id present when
  absence is required, or disagreeing charges is a collision. Invalid range order, predecessor gap
  witness, source-target, label, leaf identity/digest, future dependency, or collision rejects at the
  natural page rather than pre-scanning, reordering, or retaining prior pages. A later marker is not
  a required witness because its own effect derives neighbors from the then-current working roots.
- A stable marker id has at most one semantic effect in an edit. After removal the working mapping is
  absent; after move or same-id replacement it names the new successor leaf rather than the exact
  predecessor occurrence. A later occurrence of that id therefore cannot revalidate its required
  predecessor source and is rejected in bounded index/descent work. Byte-equal target replay of the
  original command is not a second effect. This existing working identity index, plus one bounded
  pending-effect field in the build receipt/head while path-copy work is incomplete, replaces any
  operation-wide marker map or effect collection.
- Successor construction is a bounded resumable copy-on-write fold over all three structures in canonical
  proposal order. A text edit that leaves marker occurrences unchanged reuses the complete identity-
  index and marker-order-commitment roots even when all following absolute anchors shift. A marker
  insert, remove, move, or same-id replacement path-copies only bounded-height paths in the marker
  commitment tree as well as the affected sequence/index paths. The build head retains only counts,
  proposal digests, staging/structure frontiers, bounded pending-effect state, and its latest progress-
  receipt key/digest, never the whole edit or payload.
- Each completed marker effect advances a changed-occurrence count/digest frontier that commits its
  sequence/index/commitment agreement. Completion requires balanced sequence, identity-index, and
  marker-order-commitment roots whose summaries and digests agree with the build head, equal marker/
  index/commitment counts and maximum labels, the final
  changed-occurrence frontier, and reused authenticated subtree commitments; it does not reread prior
  marker-effect pages. It rejects
  duplicate same-anchor order keys on each affected sequence path, shares unaffected immutable subtrees, and
  checks every aggregate with `u64` arithmetic. Both consumed staging-lane frontiers must equal the
  frozen finish declarations, and the canonical fragment count/endpoint/chain must equal the
  finish-derived header before completion. Global marker uniqueness follows from the keyed index; it
  is never inferred by scanning sequence leaves.
- Final candidate adoption requires the settlement absent, an active session with the exact
  predecessor root/history pair as its newest checkpoint and the exact matching active-operation custody, a
  complete matching build, the exact matching edit-history frontier, and the exact per-session
  candidate-root and history-transition natural keys absent. After the exact successor-charge
  preflight admits successor construction, one command authenticates the current progress endpoint
  and immediate-predecessor/root closure; creates the next terminal progress receipt and immutable
  combined-root record; advances only that session by exactly one candidate generation; marks the new
  generation dirty; appends the compact before/after root, caret, and directed-selection transition
  with its canonical one-based journal depth and complete 64-slot ancestor witness derived from the
  authenticated predecessor in at most 64 point reads; advances the durable history frontier and the
  session head's newest root/history pair; clears redo; applies oldest-eligible exact-charge byte-
  budget floor advancement by deriving the checked cumulative threshold and directly binary-lifting
  the selected head through locally validated committed targets to the unique oldest eligible same-
  lineage floor in at most 64 transition point reads;
  writes the immutable `Committed`
  settlement bound to that terminal receipt and
  predecessor/root closure; terminalizes the build to that settlement; and clears the custody slot.
  Eligible eviction that makes the successor fit never blocks ordinary editing. Only a required
  non-evictable closure that cannot fit produces the typed history-capacity-unavailable terminal
  error without writing a transition, successor frontier, paired candidate/history root, or
  `Committed` settlement; that bounded terminal election then terminalizes the build and clears
  custody.
  Replay and indeterminate reconciliation recompute the exact eviction amount and threshold from
  the recorded source/successor accounting, prove the canonical source-versus-target command closure
  and every atomic publication effect byte-for-byte, repeat the at-most-64-transition-read selected-
  head lifting, and require its unique floor plus complete frontier/session/settlement closure to
  equal the committed result. They do not recursively re-prove witness derivation for already
  committed immutable ancestors. A
  global-order hint or sibling record cannot change that classification.
  It does not read or
  advance the current draft, reverse index, published root/history pair, history activity, or asset
  owner. A changed newest candidate/history pair instead admits one no-change command that verifies absence of
  this operation's adoption, creates and binds the next terminal receipt, writes the immutable
  `Conflict` settlement with the different candidate/history pair, and terminalizes the build.
  Cancellation similarly competes through settlement absence and writes `Cancelled`; whichever
  terminal command is admitted first fixes the outcome.
- Historical-root adoption requires no piece build or candidate-root creation. The command
  authenticates the active session, exact current candidate/history frontier, retained transition
  and stack link, the transition's membership in the selected head ancestry through the same fixed
  skip commitment, same-draft target root and node closure, operation identity, and target caret and
  directed selection. A committed undo or redo directly advances the session to that existing root
  under one new candidate generation, marks it dirty, appends the compact history action and
  immutable adoption settlement, and atomically moves undo/redo heads. An ordinary edit after undo
  clears redo by its ordinary adoption command. Every no-change outcome preserves both frontiers;
  indeterminate completion reconciles the complete source/target closure before returning one of
  the same five terminal results.
- Sealed-composer import final selection is not ordinary edit adoption and appends no transition.
  It requires the revision-checked current-draft root/history pair and a completed import build that
  derived, built, and cross-validated the sequence tree, marker-identity index, and marker-order-
  commitment tree and summary from the same bounded sealed-`ComposerV1` stream, plus an
  absent custody slot, and a session whose published/newest pairs are still equal at that
  predecessor. One atomic command selects the imported root and fresh-baseline history in the
  current draft, advances both session published/newest pairs to that same imported pair so the
  session remains clean, records exact unavailable undo/redo facts, terminalizes the build, and
  writes the kind-specific immutable settlement/selection receipt. Replay validates the complete
  predecessor and successor current-draft and session pairs; collision, conflict, cancellation,
  error, or indeterminate custody cannot expose a root/history mismatch.
- If final adoption observes any record at the candidate-root natural key, it compares the complete
  canonical root bytes in bounded point-read work. Exact bytes are accepted only through matching
  replay authority; any disagreement returns `Error(OccupiedIdentityNoncommit)` naming that root
  key and adopts nothing. Root digest equality alone never proves replay.
- `DraftPieceTransactionOutcomeV1` is the closed public exactly-once settlement:
  `Committed(successor candidate generation, combined root, matching edit-history frontier,
  logical extent, caret, selection)`, `Rejected`, `Conflict`,
  `Cancelled`, or `Error`. Each value contains its immutable `DraftPieceSettlementProofV1`.
  `Rejected` is durably elected after bounded validation proves the proposal or staged fragment
  invalid and verifies no candidate adoption. `Conflict` proves the expected predecessor was not
  the session head and this edit was not adopted. `Cancelled` proves cancellation won before adoption.
  `Error` proves an operational failure or occupied-identity rejection plus exact noncommit. Only
  `Committed` changes the newest candidate/history pair; every terminal outcome clears the matching session
  custody slot exactly once. Replaying the same canonically byte-equal proposal
  returns the same settlement record and never applies another edit only when the stored build head
  already selects the terminal target receipt and the complete same-command closure is byte-equal;
  proposal equality alone is insufficient. A digest-equal but byte-different proposal is occupied-
  identity `Error`, not replay.
- Every new no-change terminal commit requires settlement absence, verifies the operation successor
  absent, authenticates the matching custody slot and current endpoint and immediate-predecessor/root closure when a build
  exists, writes the next immutable progress receipt and outcome-specific settlement, and advances
  the matching build to its corresponding terminal lifecycle and clears the custody slot in one
  command. A pre-build terminal election instead uses the staging head/receipt closure defined
  above and creates no draft-piece build or settlement. A terminal build cannot accept another
  fragment, construction step, final adoption, cancellation, or outcome change. A later build or
  final command returns a terminal result only from its exact staging or build target closure rather
  than interpreting proposal equality or absent candidate adoption as replay authority.
- An intermediate begin, fragment, or path-copy `HomeCommand::Indeterminate` reconciles only its
  named source receipt, target receipt/head, exact fragment, sequence-path, identity-index, move,
  cross-validation effects, and claimed session custody and returns no public terminal outcome. A terminal-election
  `HomeCommand::NotCommitted` likewise returns no settlement; the durable build slot remains the
  exact obligation for retry or a later terminal election. A terminal-election
  `HomeCommand::Committed` returns only the matching durable settlement record and receipt. A
  terminal-election `HomeCommand::Indeterminate` installs sole reconciliation custody and returns
  no public terminal outcome until the source-versus-target head and complete closure classify the
  command. A matching settlement is returned exactly, including all three adopted structure roots and
  summaries after later candidate descendants, only when the stored head selects the target and all
  same-command bytes, including the custody-slot before/after state, match. A source-selecting head
  with absent target and matching claimed custody keeps the operation pending
  and reissues the appropriate terminal election against the same exact build; a source-selecting
  head with any occupied target fails closed. It never infers an outcome from a settlement or any
  structure's shape alone.
- While an explicit live candidate session remains owned, an open or complete build without a
  settlement must agree with its active-operation slot, which retains reconciliation custody; a
  terminal build must name its exact agreeing
  settlement and terminal progress receipt. Any structure or a session-head adoption without its
  `Committed` settlement and receipt closure, a terminal mismatch, or another impossible closure
  keeps that session fail-closed. Routine fresh
  service activation does not scan or resume candidate sessions or their builds: it reads only the
  durable current-draft selector and opens a new session. There is no draft-wide unsettled slot;
  each session has exactly one slot and every build, fragment, progress receipt, settlement,
  candidate root, and reconciliation custody key is session-qualified. Unadmitted staging that
  never claimed a slot may remain an orphan after that old session is cleanly disposed; admitted
  work remains claimed until terminal settlement and cannot be cleanly disposed as an orphan. An
  already-disposed old session's unclaimed orphan records cannot block a fresh session.
- `DraftEditorCandidatePublicationOutcomeV1` is `Published(new selector, published root/history
  pair)`, `ExactReplay(receipt)`, `Superseded(captured generation, current published root/history pair)`,
  `DurableBaseConflict(current selector)`, `SessionDisposed`, or
  `OccupiedIdentityCollision(proof)`. Publication snapshots one candidate generation/root and its
  exact immutable edit-history frontier snapshot, including head/floor references, depths,
  cumulative positions, retained accounting, root-pin closure, and availability, and uses
  one atomic command to require the active-operation custody slot absent and validate its immutable
  adopted settlement, bound terminal progress receipt
  and immediate-predecessor/root closure, the session's base and published root/history pair, and
  the current durable selector/root/history triple; advance the selector and current-draft
  root/history pair,
  reverse index, activity time, and same session's published candidate/history frontiers; and write
  the immutable receipt. The receipt owns the captured pair and complete selector/session
  before/after closure, so replay validates and returns that exact closure rather than deriving a
  history reference from the current mutable frontier. A newer newest candidate/history pair is
  allowed and remains dirty. A generation at or behind the published pair is exact replay or
  supersession, never a second publication. Validation point-reads the captured settlement/root/
  history snapshot, locally canonical head/floor references, accounting and availability, recorded
  direct-threshold result when the captured adoption advanced the floor, receipt endpoint and
  immediate predecessor, and compact head and never walks predecessor settlements, progress
  receipts, or the journal.
- Before publication contribution preparation, this package compares the prior published root's
  exact `DraftMarkerCommitmentV1` with the captured root's authenticated commitment. Equality uses
  bounded point reads and admits only reuse/validation of the existing nonempty CurrentDraft Asset
  head and proof, or exact absent-head validation when marker-free; it performs no marker traversal
  or seal. A changed commitment, including direct adoption of a historical undo/redo root with a
  different marker set, requires a completed `DraftMarkerSealProofV1`. Differently shaped but
  semantically equal commitments may conservatively take this changed path.
- Marker sealing starts from one exact captured candidate generation and complete combined-root/
  build/commitment reference. Each bounded page advances the durable marker-order-tree cursor and
  compact incremental state for the final `SequentialMarkerSummaryV1` and exposes every ordered marker exactly
  once for downstream Asset staging. Seal completes only at exact EOF when the completed frontier,
  count, optional maximum label, commitment-tree root, combined root, and build identity all agree.
  Undo/redo and later editing never replay history or collect markers for sealing; a later candidate
  does not change the immutable captured source. Cancellation, failure, restart, replay,
  supersession, collision, and corruption retain bounded cursor/custody state and never change the
  current-draft selector.
- Changed-marker publication accepts only the package-issued opaque marker-seal proof. For a
  changed nonempty commitment, this package's existing mutating Syndic participant also requires
  the matching opaque Asset proof and validates its `SequentialMarkerSummaryV1` against the seal.
  For changed-to-empty, it instead validates that the seal proves the exact empty sequential
  summary and requires the exact Asset removal contribution; there is no Asset proof or synthetic
  empty set. The enclosing
  `HomeCommand` contains that one Syndic mutation participant and one Asset participant; it contains
  no second same-domain Syndic validation participant. Asset disagreement rejects the whole
  command, and neither domain publishes alone. A caller-constructed summary, detached commitment,
  or app-supplied mapping between them authorizes nothing.
- Publication cancellation is effective only before writer admission. `HomeCommand::NotCommitted`
  preserves the exact prior selector and head. `HomeCommand::Indeterminate` returns sole
  reconciliation custody; only the exact immutable publication receipt plus a coherent selector
  and session-head read may classify it. A stale app completion clears only the captured request
  generation, never a newer dirty generation. Timer-driven and flush-driven publication use this
  same boundary.
- `DraftEditorCandidateSessionDisposeOutcomeV1` is `Disposed(head)`, `ExactReplay(receipt)`,
  `DirtyConflict(head)`, `AlreadyDisposed(head)`, or `OccupiedIdentityCollision(proof)`. Clean
  disposal and safe session-ownership release require byte-equal published and newest candidate
  root/history pairs, the active-operation custody slot absent, and locally canonical frontier head/
  floor references, accounting, pins, and availability, then atomically mark the head
  disposed and record that equality in its receipt. An
  admitted operation must be reconciled and terminally settled before clean disposal; neither a
  crash nor an external conflict permits clearing its slot or abandoning it as inert staging.
  Fresh activation may use a new session identity without scanning that old live session, and an
  already-disposed old session's never-admitted orphan records are session-qualified and inert.
  Disposal deletes no root, settlement, progress receipt, fragment, or asset record.
- `ComposerV1` materialization binds its operation and deterministic output content identity to one
  exact immutable combined root and format version. Each bounded step advances the sequence input
  cursor and content-output frontier together while retaining the content-bound
  `SealedContentMarkerSummary` with its embedded `SequentialMarkerSummaryV1`. Seal requires exact end-of-root, combined-root summary/digest,
  output chunk/text/piece/marker frontiers, and canonical Composer digest; it atomically seals the
  content and publishes the unique combined-root-to-content mapping without changing the draft. Exact retry
  resumes or returns that same result, while a different result at the same natural identity is a
  collision.
- Materialization never reads or fences the mutable current-draft selector after its exact source
  combined root has been accepted. A later candidate adoption or draft publication neither conflicts
  with nor supersedes a materialization. The exact-root build remains
  resumable until cancellation, operational failure, explicit same-root build supersession, or
  seal. Current-root submission eligibility is checked only by the later submission command.
- Idle and active-or-queued submission begin materialization only after the required candidate
  flush and require an exact sealed mapping for the expected published combined root in addition to
  their existing thread, draft, clean editor-session with absent active-operation custody, gate,
  asset-set, and content revisions. A
  later candidate publication makes the older materialization ineligible for that submission
  but does not corrupt or delete it. Sealed output remains reusable for any consumer naming that
  exact root and format.
- One atomic idle-thread submission validates the expected thread, draft, clean candidate session,
  absent session custody slot, input-gate, and combined-root-bound sealed-content revisions, derives the ordinary parent from the current thread tail,
  transitions the current draft identity into a submitted turn, updates the committed tail, and
  creates the caller-named replacement current draft, and disposes the consumed editor session.
  Context-first submission derives its parent
  from the matching draft-owned envelope source, and replacement submission derives it from the
  validated target turn.
- The idle transition preserves the draft identity's exact payload in the typed submitted-turn identity and allocates no unrelated turn id.
- One atomic active-or-queued submission validates the expected thread, draft, clean candidate
  session with absent active-operation custody, input-gate,
  route-generation head, combined root, and root-bound sealed-content revisions, freezes the payload into an
  immutable ordered accepted-input record plus one bounded route leaf, advances that generation's
  aggregates, creates the caller-named replacement current draft, and disposes the consumed editor
  session without creating a competing submitted turn. Later route transitions preserve that accepted-input identity and permanent
  membership.
- The same first-acceptance command independently validates the root-bound sealed `ComposerV1`
  content identity/full digest, requires its `SealedContentMarkerSummary`'s embedded
  `SequentialMarkerSummaryV1` to equal the compact sealed-set proof's content-neutral summary,
  validates the identity-index count, advances the thread frontier monotonically,
  and creates at most one immutable
  local origin span. Per-marker validation already completed through bounded set-staging pages;
  later delivery disposition changes never rescan or rewrite label authority.
- Every thread creation also publishes its initial idle input gate. Active-or-queued submission
  requires the exact gate and selected route-generation head revisions, advances its accepted-order
  high-water mark and checked `u64` live counters atomically, and never scans retained accepted
  history to choose an ordinal or enforce capacity. `FinalizingHistory(turn)` is a non-idle,
  queue-only gate state and is itself the durable recovery source for a proven-terminal turn whose
  bounded canonical and selected-transcript convergence has not yet settled.
  `RepairRequired(turn, correlation, capture gap)` is a distinct non-idle, queue-only gate state and
  durable recovery source for a proven-terminal current tail whose canonical authority cannot be
  finalized until full repair publication or explicit incomplete convergence consumes that exact
  state.
- One immutable accepted-order entry is the input's unique route-generation membership. Its bounded
  route leaf is live while ready, retryable, delivering, or next-turn and terminal otherwise. Claim,
  proven-pre-dispatch retry, exact success, and structured rejection each identify the exact source
  leaf, transition kind, immutable generation, and semantic steering target. Under the serialized
  writer, the mutation validates that stable identity against the current compatible gate,
  generation head, and target, then captures those actual current revisions while updating only
  that leaf, one new generation-state revision/head, compact ready-source or next-source presence
  when needed, and aggregate counters. A sibling admission in the same generation may advance the
  shared gate and route head without invalidating the leaf operation. A ready source exists exactly
  when the selected gate and route head identify one `Steering` generation whose ready/retryable
  count is nonzero; awaiting target identity, projection loss, and a zero ready count leave it
  absent. They never rewrite accepted input or accepted-order authority. Each transition has a
  current-domain command and one fixed-work `Prior`/`Exact`/`Collision` reconciliation read. The
  successor leaf persists the actual consumed gate and selected-route proof, source leaf revision,
  and transition kind as its bounded operation witness. A matching successor state without the
  complete proof is a collision, not publication evidence. Absence of a transition witness is
  valid only for the initial admitted leaf. Exact reconciliation uses that immutable witness plus
  monotonic compatible descendants; it never requires two identical snapshots of a mutable gate or
  generation head. A later route-wide stop, unknown-terminal, terminal-history, or projection-loss
  reclassification is such a compatible descendant: it changes scheduling authority but cannot
  erase or contradict an earlier leaf-local retry or structured-rejection witness.
- `Retryable` is durable non-dispatch eligibility, not a clock, retry count, transient-error claim,
  or command to retry immediately. Ready-source and candidate pages expose it so the app can run a
  bounded retry pass under separate process-lifecycle authority; this package owns no timer,
  process wake, scheduler retry set, or error-cause inference.
- A source-backed unknown-terminal event against the exact active CAS turn requires a selected
  steering generation with no delivering member. Its final event commit atomically changes that
  generation to `AwaitingTerminal(exact prior steering target)`, transfers every ready/retryable
  aggregate from live steering to next-turn work, removes its ready source, publishes its next
  source when nonempty, and changes the gate to `AwaitingTerminal(turn)`. Leaves, accepted
  identities, permanent order, and logical bytes are unchanged.
- While awaiting terminal evidence, ordinary accepted input enters later
  `NextTurn(UnknownTerminal)` generations and cannot replace the selected retained target. Exact
  late item events advance capture without changing the gate. Exact late activation atomically
  opens a fresh empty steering generation for the same active target while preserving every
  unknown-terminal generation as next-turn work. Exact late terminal publication enters
  `RepairRequired(..., Available)` when the terminal carries an exact required capture gap and correlation, and
  otherwise enters `FinalizingHistory`; active-authority loss uses ordinary abandonment with a
  distinct awaiting-terminal lost-target witness.
- Stop admission is one fixed-work target-kind-specific serialized mutation. An ordinary target
  reads the exact active binding and CAS turn, input gate, selected steering generation, ready-
  source authority, and new stop record; it proves no delivering leaf from aggregates, publishes
  the stopping gate and record, removes ready eligibility, and changes admitted or retryable work
  through the compact `Stop` generation transition. A provider-operation target instead reads the
  exact compacting gate, compaction record and published CAS turn, provider snapshot, valid binding,
  route aggregates, and new stop record; it changes the compaction record to stopping and publishes
  the gate/stop pair without creating a steering generation or rewriting
  `NextTurn(Compaction)`. Both preserve identities, order, and bytes and conflict with matching
  terminal through the same target-operation election.
- Stop-admission reconciliation uses the caller-owned operation identity, exact target, consumed
  source revisions, stop record, gate, and generation-transition witness. It accepts compatible
  path-neutral stopping descendants such as queued admission, cause join, or dispatch claim. If
  terminal, safe-reopen, or abandonment already consumed the current record, that transition's
  bounded successor witness remains the exact result; an aggregate-shape match without the
  operation identity is a collision.
- Stop-period accepted input uses the ordinary bounded admission mutation against the current
  stopping gate. It enters a later next-turn generation, may advance path-neutral gate and thread
  revisions, and does not mutate the immutable stop target. The matching stop record's closed
  target kind selects `NextTurn(Stop)` for ordinary execution or `NextTurn(Compaction)` for a
  provider operation; the admission read never guesses from the blocked turn alone.
- Dispatch claim is a one-way fixed-work mutation from `Admitted` to
  `DispatchClaimed(source_revision, caller_attempt_id)`. The immutable witness names the exact live
  record revision consumed by the claim. Its reconciliation returns `Prior`, `Exact`, or
  `Collision` from the complete gate, target, stop identity, source revision, attempt identity, and
  successor witness, including after later compatible cause joins or consumption. Neither repeated
  reads nor absence of another attempt authorizes a second claim.
- Safe stop reopening requires an exact admitted or claimed record, absence of interrupting-
  approval cause, and the caller's closed local proof that every request byte was prevented while
  the same target authority remains current. The ordinary variant atomically consumes the stop,
  publishes an active-steering gate with a fresh empty generation, and leaves existing next-turn
  work unchanged. The provider-operation variant atomically consumes the stop, changes the exact
  compaction record from stopping back to live, and restores the compacting gate without any
  steering generation or route rewrite. The consumed record and target-specific successor agree
  on operation, attempt when any, source revisions, and transition kind as one bounded witness for
  `Prior`/`Exact`/`Collision` reconciliation.
- A provider rejection that proves no core interruption but lacks a current-target verdict is not a
  safe-reopen disposition. The caller supplies that closed witness to ordinary stop abandonment,
  whose successor records the consumed operation and attempt plus projection retirement and
  source-less incomplete convergence; storage does not parse provider codes or diagnostic text.
- Stop abandonment records one closed caller-classified reason distinguishing provider rejection
  before core interruption, live connection or target-authority loss, and recovery loss of the old
  process generation. The reason is diagnostic and lifecycle provenance, never replay, retry, or
  safe-reopen authority.
- Backend acceptance or acknowledgement of the stop request is not terminal evidence and does not
  consume the stop record, release the stopping gate, or authorize finalization.
- Matching terminal publication consumes the record's live authority into its terminal disposition.
  The ordinary variant changes the gate to `RepairRequired(..., Available)` when the same atomic terminal commit
  carries exact required capture-gap provenance and correlation, and otherwise changes it to
  `FinalizingHistory`; the provider-operation variant
  records terminal in the compaction record and restores `Compacting` only for its dedicated
  bounded item finalization. Both preserve queued generations. The bounded successor retains the
  stop operation and complete cause-first and dispatch-claim provenance. That consumed durable
  record and its exact terminal successor are the only authority for any delayed finalization
  release; no process-local receipt, snapshot, or lifecycle state participates. Stop abandonment
  instead marks the
  record consumed by abandonment while retiring the target projection, preserving effective next-
  turn work, and publishing the target-kind-specific source-less incomplete convergence. Both
  commands expose complete fixed-work
  `Prior`/`Exact`/`Collision` reconciliation; neither state can be reconstructed from a gate-only
  successor or used to replay an interrupt.
- One atomic accepted-input promotion selects the earliest effective next-turn leaf from a
  revision-bound next-source page and requires an idle gate with no terminal-history obligation. It
  creates caller-named fresh pending
  turn and item identities, parents the turn to the precommit thread tail, advances thread,
  transcript, history summary, activity, unbound binding, and input gate authority, transfers one
  next-turn route count and its live bytes to terminal state, and publishes the exact promotion
  witness. It retains the accepted-input and accepted-order records, leaves the current draft
  record untouched, and updates only the draft reverse index's matching thread revision.
- Transcript projection lifecycle is not a promotion eligibility gate. An exact `Current` or
  rebuildable `Stale` head with the selected tail and digest is a valid basis; promotion supersedes
  any active build and advances that exact head to the new pending tail as `Stale`.
- Queued accepted-input admission preserves the selected tail and digest while advancing the broad
  thread revision. It therefore preserves both an active transcript build and an already-completed
  `Current` generation. The admission advances the history summary to the new current thread
  revision, preserves its derived completeness, and advances its draft activity. A later
  path-changing mutation, not a path-neutral admission, supersedes an active build.
- Promotion exposes a fixed-work `Prior`/`Exact`/`Collision` reconciliation contract. `Exact`
  authenticates the complete immutable terminal route witness and exact promoted successor
  identities, then accepts current thread, gate, route, binding, transcript/summary/activity, and
  reverse-index authority when it is a compatible monotonic descendant of that promotion. `Prior`
  requires the complete source parent, its proven-terminal state, and its bounded
  deterministic ancestry proof; missing source lineage is a collision rather than benign absence.
  A later valid accepted-input admission against the promoted pending gate may rotate the current
  draft,
  advance thread and gate revisions, label and accepted-input frontiers, next-turn aggregates,
  history summary and thread-parent revision, and create new route authority. It does not erase
  publication proof while the promoted selected tail, pending-turn identity, original terminal
  route witness, successor records, binding, transcript source, and activity source remain exact
  or compatible. A current-draft save may independently advance the matching draft reverse revision
  and summary activity time. Reconciliation never requires the complete immediate post-promotion
  mutable shape to remain current. Identity or successor reuse, incompatible lineage, an impossible
  descendant, a similar aggregate without the witness, or partial publication is `Collision`.
- Local worker or connection-attempt capacity pressure is not a storage transition. It leaves the
  ready or retryable steering leaf, generation head, gate counters, next-source authority, and
  accepted identity unchanged so a later bounded attempt may still steer the exact active target.
- A closed structured non-steerable CAS rejection changes one delivering accepted input to
  retryable and replaces its steering route with `NextTurn(SteeringRejected)`. It preserves the
  input identity, permanent ordinal, original admission gate revision, content, marker records,
  and admitted timestamp. An exact rejection without a closed target verdict cannot use this
  standalone transition; it uses the named exact-rejection variant of atomic active abandonment.
- A possibly dispatched steering attempt whose provider response is unavailable resolves to
  terminal delivery-unknown only through the same route-generation head transition that atomically
  abandons the active binding and retires the CAS thread. Ready or retryable leaves in that
  generation resolve to next-turn work; delivering leaves resolve to delivery-unknown except for
  the one optional exact-rejected leaf named by the abandonment command. That leaf resolves to
  next-turn work only after the mutation validates its current delivering revision and exact
  generation target and writes its next-turn leaf state. Abandonment carries stable binding,
  target, and generic-or-named disposition identity; the serialized mutation consumes the actual
  current compatible gate and route head, so a sibling admission between loss proof and publication
  cannot turn exact loss into a collision. The generation's retained aggregates update the gate in
  constant command work, so no member vector or generation-wide per-input rewrite is required. The
  permanent accepted-input history remains and supplies no automatic retry authority for any
  delivery lacking exact non-acceptance proof. Projection-loss rerouting affects only work proven
  not dispatched.
- Execution snapshots contain no accepted-input vector. Their relationship to accepted steering is
  expressed once by the selected accepted-route generation's exact target proof, keeping snapshot
  metadata and immutable accepted-input records bounded independently of thread history.
- Branch-discussion creation atomically creates the thread, context-bearing first draft,
  parent-thread binding, validated lineage depth/digest/skip facts, the parent's exact current
  image-label frontier as the child's immutable inherited/current starting frontier, and
  context-owner identity. It copies no label-origin spans.
- Starting replacement edit first streams the target user item's exact sealed content and marker
  facts through a sealed-composer-import build that derives, builds, and validates one complete
  immutable sequence tree, marker-identity index, and marker-order-commitment tree and summary from
  that same bounded stream. Only after all three structures and cross-summaries agree does its final
  command revision-check the current empty draft and atomically
  selects that combined root with a fresh-baseline history snapshot bound to the same import
  operation and root, both undo and redo unavailable, while
  setting the exact ordinary-user-turn target and selected-path proof. It retains the target item
  as immutable history and changes no selected path, committed tail, input gate, or CAS binding.
- Cancelling replacement edit revision-checks the current draft and atomically clears only its
  exact target and selected-path proof; provider-operation turns are never replacement targets, and
  cancellation preserves the selected combined root and its matching history reference unchanged.
- Provider event updates never mutate submitted turn parentage.
- Provider item and lifecycle publication atomically advances the affected activity-query head and
  ordered entry when that event has activity presentation. A bounded GUI-derived fact such as a
  child handoff byte count may contribute only through its exact source identity and checked fixed
  metadata; it cannot carry the source payload. Query-page reads require the exact head revision and
  return stale rather than mixing entry generations.
- Storage maintains monotonic revisions for live canonical items and content manifests while source capture remains open. A committed transcript-visible event advances those canonical revisions and marks the selected transcript head stale; the bounded projection builder advances projection records separately from the already admitted source frontier.
- `capture_item` stabilizes one exact CAS-item index, canonical item, latest provider frame, selected
  provider narrative view when any, and owned live-or-finalized content manifest by rereading that
  CAS index. Bounded provider-frame reads expose typed structural pages;
  `capture_provider_narrative_range` returns at most 65,536 logical UTF-8 bytes by walking the exact
  selected generation's narrative spans and referenced ProviderItemV1 ranges while stabilizing the
  same composite item before and after the page. Mutation of that item is concurrent state;
  unrelated thread and item commits do not invalidate either read.
- A repaired canonical item instead exposes bounded logical-range reads over its immutable snapshot-
  backed manifests and ranges. The read stabilizes the exact repair snapshot, item ordinal/digest,
  canonical revision, and range provenance; it never consults a live CAS-item index, provider-
  narrative generation, `ProviderItemV1` range, or fabricated source event.
- A typed delta names the exact item kind it is permitted to advance. Live mutation compares that
  kind with the durable CAS-item/canonical-item kind before staging a provider-frame addition and
  rejects any mismatch without publication. Protocol indices retained by typed frames are bounded
  and nonnegative before they cross the storage API. Final publication stores only the sealed frame
  reference in the source event and advances its canonical/content frontiers atomically.
- A transcript-visible canonical revision marks the selected transcript head stale in the same
  commit. It invalidates selected item-projection state only when that canonical item owns an actual
  closed `ProjectionTextSource`. Generated media remains transcript-visible through resource
  presentation but owns no Markdown projection; operational text may own a projection source under
  its separate presentation policy. A later bounded builder starts a new item-projection generation
  only for a projectable item; it may reuse frozen closed-block projections by exact reference but
  never rewrites a prior generation's indexes.
- Multiple canonical advances before publication may supersede intermediate builds and converge
  directly on the latest revision. A provider live append may reuse the exact stable checkpoint of
  the same narrative generation. Completion seals that same generation only after bounded exact
  comparison and never starts a replacement projection source. If the closed projection source is
  unchanged, including submitted-user correlation, agreeing completion, and other
  provider-lifecycle-only advances, the new
  generation reuses the prior stable end-of-input checkpoint, projection and resource identities,
  membership, and digest. It republishes exact revision provenance without rereading or reparsing
  unchanged text.
- The builder resumes a later source revision from the prior generation's durable stable
  checkpoint and digest rather than replaying source from byte zero. `stable_eof_resolved = false`
  means the checkpoint precedes the live snapshot's provisional end-of-input outputs;
  `stable_eof_resolved = true` means immutable end-of-input has been consumed into the stable
  prefix. No current source revision may shrink behind or disagree with that checkpoint.
- A transition to a proven-terminal ordinary-turn lifecycle closes ordinary source-event admission
  and requires that turn to be the current committed tail with no same-thread successor. When the
  terminal publication carries an exact required capture-gap reason, exact CAS thread/turn
  correlation, and its matching terminal/capture-gap witness plus any provider-observation issue
  reference, the same commit moves the
  gate to `RepairRequired(..., Available)`; it preserves the captured evidence without treating it
  as repaired canonical authority, marks affected derived work stale, and cannot make the gate idle.
  A proven-
  terminal ordinary turn with complete exact capture instead moves atomically to
  `FinalizingHistory(turn)` and follows normal bounded item/transcript convergence.
- Before any historical backend request, one target-scoped claim mutation requires the exact current
  `RepairRequired(..., Available)` gate, no same-thread successor, matching CAS correlation and
  terminal/capture-gap witness, the supported pinned adapter release, and a caller-generated
  16-byte request-attempt nonce. It atomically advances only that gate to
  `RepairRequired(..., Consumed(nonce, source revision, successor revision))` and returns one opaque
  non-cloneable dispatch claim only from the proven committed successor. `NotCommitted` returns no
  claim. `Indeterminate` enters ordinary operation-scoped reconciliation and returns no claim;
  `ExactNew` may reconstruct the sole claim for the same still-current coordinator, `ExactOld`
  authorizes the same claim command once, and `Collision` authorizes neither. No mutation resets a
  consumed disposition to available.
- Only two atomic mutations may consume a current `RepairRequired` gate. Full repair seal/publication
  selects the complete staged snapshot and moves the exact target to `FinalizingHistory`; explicit
  incomplete convergence preserves the submitted input and admissible captured evidence, records
  the closed incomplete reason and request disposition, selects no staged snapshot or repair-derived
  asset, and moves the same target to `FinalizingHistory`. Full publication requires the consumed
  claim stored in the sealed snapshot; incomplete convergence may close an unavailable authorization
  before claim or a consumed attempt after any possible dispatch. Both retain queued accepted input
  and same-thread exclusion until terminal-history completion reaches its exact fixed point. No
  partial repair, missing-correlation fallback, gate-only rewrite, or later CAS status may release or
  replace the repair gate.
- Terminal-history completion is a separate exact mutation after the bounded item and selected-
  transcript pipelines. It requires the same finalizing gate and proven-terminal committed tail,
  a current transcript build for the exact selected path, and an item frontier that is fully
  finalized or stopped at a structurally valid non-finalizable captured item or pending-resource
  disposition. Its observed gate is a lower-bound proof: at writer serialization it may consume a
  compatible `FinalizingHistory` descendant produced only by path-neutral queued admissions. The
  current gate must retain the same thread, turn, selected route, zero steering work, monotonic
  live bytes, and exact one-for-one accepted-input, next-turn, route-generation, and pre-release
  gate-revision advances. Completion contributes the sole additional gate revision and atomically
  changes only that current gate to idle while preserving its route authority and accounting. A
  failed, ambiguous, or interrupted completion leaves the durable finalizing source discoverable
  and idempotently resumable.
- Scoped repair/finalization reconciliation point-reads only the named thread, current gate, target
  turn and terminal state, CAS correlation/reverse records, capture-gap witness, selected route, and
  repair head when present. It returns `Prior`, `Exact`, or `Collision` from the complete natural
  closure; it never infers release from an idle-shaped gate or scans unrelated history.
- Scoped terminal-finalization reconciliation treats a stale binding's abandoned active predecessor as historical authority,
  not as permanent ownership of the current gate. While that terminal remains the committed tail,
  the gate is its pending `RepairRequired` or `FinalizingHistory` obligation, or idle only when the
  applicable repair/incomplete transition and the same item/transcript fixed-point predicate
  re-prove. Once a later committed tail exists, ordinary current gate ordering governs it and the
  older terminal cannot pin the gate back to its prior phase.
- A current item projection becomes finalized when its source turn has a proven terminal lifecycle. Finalized turn-owned canonical content, projection identity, revision, text, resources, and item-local ordering are immutable.
- A named thread's transcript-view head and entries remain a rebuildable selected-path index: they may advance when the thread tail changes, when another finalized turn is appended, or while unfinished projection work changes state. Rebuilding that view may reference finalized projections but cannot rewrite them.
- Starting a transcript rebuild allocates the head's next generation and records an exact head
  revision/path proof. Bounded path and entry batches populate only that generation. Final
  publication revision-checks the unchanged selected path and every referenced current
  item-projection set before changing the head to Current.
- Branch-context admission may reference only a finalized assistant projection. The storage
  operation validates the exact source thread, turn, item, projection identity and revision, the
  absolute canonical logical UTF-8 range within that projection, and the bytes returned by a
  bounded logical-range read before admitting the immutable context envelope.
- Scoped context reconciliation resolves the named envelope back to that exact finalized projection revision and absolute canonical logical UTF-8 byte range through the same bounded logical-range read. It validates immutable source-record agreement but does not require the historical source turn to remain on the named source thread's later mutable selected path. A missing or changed source is corruption rather than a reason to reinterpret the envelope as detached snapshot authority.
- Transcript-view positions are stable, sortable identifiers assigned by storage.
- Cursor reads name the exact transcript generation and return enough position and revision metadata for callers to detect stale provider responses.
- An exact retry at an already occupied per-turn sequence is classified as `SourceEventAlreadyAdmitted` before stale event-local revisions are considered, so ambiguous commit recovery can recognize the durable result without rewriting it. Different data at that sequence is `SourceEventCollision`; a gap or future sequence is `SourceEventSequenceConflict`.

## Write Commit Shape

- Write commits implement the durability and projection-revision requirements of the owning systems at the storage boundary.
- Candidate construction and draft autosave accept no complete composer payload and never rebuild a
  full `ComposerV1`. Before finish, append-only source and proposal pages, staging receipts, and one
  compact head hold sole durable page custody under the candidate-session slot. The caller releases
  each accepted widget-page payload only after its complete atomic physical-page batch is accepted
  or exact target reconciliation proves every batch effect. Later reconciliation reads the natural
  staging records. Authenticated
  finish transfers that one custody slot to the existing builder; only then does candidate construction stage bounded edit fragments and path-copied
  sequence, marker-identity-index, and marker-order-commitment records, then adopts one compact combined-root reference into
  the exact editor session. Autosave separately publishes an already adopted captured frontier into
  the current-draft selector. The compact build and session records contain no resident whole edit,
  replacement collection, or page payload. Canonical proposal fragments remain one-based immutable
  records in `draft-piece-build-fragments`; each bounded work quantum appends one fixed-size
  immutable `draft-piece-build-progress` receipt and atomically advances the compact build head to
  that receipt. The receipt endpoint, its immediate predecessor/root closure, finished staging
  reference, consumed source/proposal lane frontiers, canonical fragment endpoint/chain, bounded
  pending marker effect, and sequence, index, candidate/history, and published root/history frontiers
  are the durable progress authority. The next bounded staging window is derived only from those
  fields and durable staging custody; no app reconstruction or operation-prefix scan participates.
  Work and writes for an edit are proportional to its inserted fragments, affected base ranges, and
  copied paths in the three bounded-height structures, not to unchanged prefix or suffix length.
- The final ordinary adoption command also appends one compact root transition and advances the
  durable history frontier. Undo and redo instead use the dedicated direct historical-root adoption
  command. Neither path copies inverse content or scans unchanged root/history state; both publish
  candidate and history authority atomically or publish neither.
- Mutation intake, draft-piece construction, candidate adoption, autosave publication, and
  Composer materialization publish no partial result. A crash between bounded steps exposes only
  the prior or new current-draft selector atomically; candidate adoption alone never changes reopen
  authority. Candidate-predecessor conflict terminalizes that exact edit through its immutable
  `Conflict` settlement. Durable-base conflict rejects publication without reclassifying adopted
  candidates. Cancellation, rejection, and operational failure likewise preserve their exact
  noncommit proofs. Pre-finish pages never enter candidate, current-draft, history, materialization,
  submission, or transcript reads. Staging records that were never admitted and never claimed their
  session slot, plus terminal staging pages and unreachable nodes and leaves, may remain future-GC
  candidates. Admitted nonterminal work retains the exact session custody slot until one terminal
  settlement and cannot be disposed or reclassified as unclaimed orphan staging.
- A materialization remains attached to its immutable exact combined root across later autosaves and crashes.
  Recovery resumes from its durable cursor; only cancellation, an operational failure, explicit
  same-root build supersession, or seal ends it. Cancelled, failed, or explicitly superseded
  materialization builds and their unsealed content chunks remain unreachable orphan candidates;
  a current-draft conflict is not a materialization outcome.
- Full `ComposerV1` construction is permitted only through the separate exact-root materializer for
  submission or another named canonical consumer. The materializer streams bounded sequence-tree
  pages into bounded content batches, persists its cursor and digests, and seals only after exact
  EOF verification. It does not update the draft, run on every keystroke or autosave, retain a
  complete composer value, or require output to be restaged after an exact sealed retry.
- Large content construction uses a bounded sequence of revision-checked commands. Each command atomically appends the next bounded chunk batch and advances its manifest frontier; one final command seals the exact completed manifest and publishes its owner reference. No owner can reference building content.
- A surfaced failure during staging is reconciled against the exact content identity, frontier, and chunk digests before work resumes. A conflicting or abandoned building object remains unreachable and is never overwritten or reinterpreted as another payload.
- Correctness-sensitive operations contribute all Syndic and required Beryl-domain changes to one
  typed home-store command and are not reported successful until its `SyncAll` barrier completes.
  Image-bearing admission references one already sealed Beryl-state asset-reference set and swaps
  only compact owner heads whose identity, marker frontier, count, and digest agree with the
  admitted Syndic payload.
- Candidate adoption is Syndic-local and does not change the Beryl-state current-draft asset-owner
  head. Candidate publication compares the captured and prior authenticated
  `DraftMarkerCommitmentV1` values. Equality reuses/validates the existing exact nonempty Asset head
  and proof or validates absent marker-free ownership without a seal or scan. Inequality requires
  this package's bounded completed marker-seal proof. A nonempty successor also requires the
  matching new Asset proof and swaps the single `CurrentDraft(draft id)` head; an empty successor
  requires the seal's exact empty sequential summary and removes the exact prior head without an
  Asset proof or synthetic set. The one Syndic mutation participant validates the root/commitment/
  seal and applicable sequential-summary branch; the single Asset participant validates its own
  owner-head action. There is no same-domain validation participant, per-root head, synthetic empty
  set, caller-constructible bridge, or partial publication.
- Validation rejection, revision conflict, and cancellation observed before writer admission leave
  prior canonical Syndic records unchanged. Draft-piece edits additionally persist their immutable
  no-change settlement through a separate terminal-election command. Cancellation after a terminal
  command is admitted does not retract or replace that command.
- A surfaced post-admission storage or persistence failure retains its typed durable outcome:
  `NotCommitted` proves the command did not commit and carries neither receipt nor descriptor;
  `Committed` carries the exact receipt and any optional later failure without erasing its durable
  successor; and `Indeterminate` carries the failure plus the sole move-only reconciliation
  descriptor and already-reserved registry slot, but no receipt or publishable successor. The
  package exposes no rollback or replay authority from an indeterminate result.
- `ProviderObservationStager::begin` maps those outcomes exactly. `NotCommitted` creates no stager
  and leaves the caller-owned observation identity and begin facts eligible for a newly admitted
  begin attempt. `Committed` returns the exact new building stager with its receipt and optional
  later failure. `Indeterminate` returns only the sole reconciliation custody value and failure; it
  exposes no building stager, receipt, or publication authority.
- `ProviderObservationStager::control` and `fragment` prepare their next state without mutating the
  caller's current stager before the command outcome is known. `NotCommitted` therefore leaves that
  exact old stager unchanged and usable. `Committed` advances it to the exact durable successor and
  returns the receipt plus any optional later failure. `Indeterminate` returns no publishable
  successor and no caller-usable continuation. Until registry handoff the old stager is inert; the
  handoff is the cut after which that process-local stager may be discarded and is never required for
  classification, recovery, or service retirement.
- `ProviderObservationStager::seal` consumes its input stager on invocation for every outcome.
  `NotCommitted` carries neither receipt, descriptor, nor sealed handle; it does not authorize an
  implicit reopen or retry. `Committed` returns the exact sealed handle, receipt, and optional later
  failure. `Indeterminate` returns the failure plus one move-only seal custody guard that privately
  owns the sole home-store reconciliation custody and the inert consumed stager, but no receipt,
  sealed handle, or other publishable successor. The guard exposes only one terminal `install`
  operation: it first installs home custody into the originating registry and only then drops the
  inert stager. It exposes no stager accessor, continuation, retry, publication, or reconciliation
  execution. If ordinary destruction reaches an unconsumed guard, its non-discardable home custody
  field performs fail-closed fallback installation before the inert stager field is dropped; this
  authorizes no acknowledgement or successor and adds no custom guard recovery path.
- The immediate caller of any provider-observation `Indeterminate` outcome must synchronously move
  its descriptor and complete reserved registry capacity into the per-home `beryl-home-store` reconciliation-scope
  registry before translating the result, acknowledging the provider operation, releasing local
  operation state, or observing cancellation or retirement. That handoff preserves custody and
  closes the exact publication scope only; it starts no reread, retry, rollback, publication, or
  reconciliation execution.
- The provider-observation reconciliation hook derives every result from the descriptor-bound
  natural build, frontier, fragment, and seal records; it never receives the old process-local
  stager. `ExactNew` reconstructs the exact durable building stager plus receipt for `begin`,
  `control`, or `fragment`, or the exact sealed handle plus receipt for `seal`. `ExactOld` exposes an
  old continuation only when that operation's direct `NotCommitted` contract permits it and the
  exact same-generation live owner still exists to consume it; otherwise it returns an explicit
  no-successor/abandoned disposition. In particular, `seal` `ExactOld` never authorizes implicit
  reopen or retry. `Collision` exposes no stager, sealed handle, receipt, retry, or publication
  authority and releases every process-local stager associated with that operation.
- Provider-observation cancellation or retirement may abandon an ordinary unpublished stager at any
  time before a command is admitted. After `Indeterminate`, it may discard all process-local stager
  and operation state only after the sole custody value is installed in the registry; that disposal
  does not reinterpret the result as `NotCommitted`. No stager must survive connection or service
  retirement, and a later classification or fresh-service recovery uses only durable natural
  records. On seal, the move-only guard enforces the installation-before-drop ordering while it is
  transferred synchronously to the immediate app recipient; it is never retained as service state.
- Terminal-repair begin, page-stage, and seal commands reconcile `Indeterminate` only by their
  existing natural record identity: exact Syndic thread/turn and CAS thread/turn for the build, plus
  staging family and one-based page ordinal for a page. The point read returns `Absent`, `Exact`, or
  `Collision` from the complete metadata, bytes, counts, and digests. `Exact` authorizes progress
  from the durable successor, `Absent` authorizes the same command once, and `Collision` authorizes
  neither. No caller-visible or shared repair-snapshot identity is introduced.
- For a repair-media stage or final seal `HomeCommand`, this package's reconciliation hook classifies
  only its descriptor-bound Syndic natural records. Stage new-side evidence requires the matching
  Syndic media witness; final new-side evidence requires the selected snapshot and
  `FinalizingHistory` successor. It contributes sealed old/new/collision facts to the system-owned
  cross-domain classifier and never inspects, publishes, or repairs a sibling participant. A stage
  may resume from its durable successor, and a final command may reconstruct its committed result,
  only after the whole-command classifier returns the corresponding exact outcome.
- The repair-request claim command reconciles only the target `RepairRequired` gate, exact
  correlation and capture-gap witness, request-attempt nonce, and source/successor gate revisions.
  `ExactNew` reconstructs the sole non-cloneable dispatch claim only for the same still-current
  coordinator; when that owner no longer exists, the durable consumed disposition remains and fresh
  recovery converges incomplete without dispatch. `ExactOld` proves that no claim capability could
  have existed and permits the same claim command once. `Collision` yields no capability or retry.
- `Absent` admission reconciliation proves the expected current draft and combined draft root still exist
  and all draft-derived result identities are absent. Its source-identity classifier retries at most
  once when that natural anchor changes; it never waits for a quiet whole-domain revision.
  `ExactSubmitted` proves the caller-named replacement draft, advanced input gate, immutable admitted
  owner, exact source combined root, root-bound materialization, content, and marker set. Every accepted-input
  record additionally retains the complete original accepted-admission intent: expected source
  thread and draft revisions, source root and materialization, source and replacement draft
  identities, expected gate revision, content, asset proof, and admission time. `ExactAccepted`
  proves that immutable receipt plus permanent accepted-order and route-leaf identity. It remains
  exact across later valid route, gate, draft, delivery, rejection, activation, terminal, promotion,
  or projection-loss descendants. `Collision` means the durable receipt or permanent membership
  disagrees and authorizes neither replay nor success.
- Live event ingestion stages an arbitrarily large typed provider frame, applicable append-only
  narrative spans, and any completion-equality comparison through bounded resumable commands while
  published authority remains unchanged. Its final command writes the sealed frame reference,
  selected narrative view, equality or mismatch disposition, source event, turn
  lifecycle/frontiers, canonical item/content changes, exact source indexes, history activity,
  input-gate terminal transition when any, and transcript-staleness effect in one durable commit.
  An unreachable staged byte or span suffix is retained only for future garbage collection and
  cannot authorize history.
- Exact-route provider lifecycle conflicts use a separate atomic live-event mutation over the same
  source-order fences. It verifies the compact sealed-observation reference and its inadmissibility,
  writes the issue source record, advances the turn's source and first-issue frontiers, updates
  history activity, and marks transcript work stale in one durable commit. It writes no provider
  frame, canonical item, item index, content generation, or item lifecycle effect.
- Live-event, binding, stop-operation, finalization, item-projection, and transcript mutations expose
  narrow current-domain command constructors. The home store captures their physical domain basis
  after writer admission, while each mutation continues to validate its exact logical record
  revisions and never retries semantic conflict.
- Scoped recovery begins from an exact durable thread/work anchor and point-reads its current input
  gate. A named non-idle `RepairRequired` or `FinalizingHistory(turn)` gate is the sole durable
  repair or terminal-history work source for that closure; no process-local obligation may replace
  it. Recovery cross-checks a repair gate's exact current tail, no-successor state, CAS correlation,
  capture-gap witness, and request disposition. It may claim and dispatch only an exact `Available`
  authorization. `Consumed` permits completion from an already complete durable staged response or
  explicit incomplete convergence, never another backend request or runtime-path reread.
- When the named gate is `Stopping`, its matching current stop-operation record is the sole stop
  authority. Fresh-service recovery may classify and abandon that pair; it may not derive a stop
  from an active binding, enumerate gates to infer work, or issue an interruption.
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
- A turn with a durable provider-observation issue cannot publish a history-complete terminal.
  Normal provider terminal publication retains the provider outcome and uses
  `CompletionMismatch` as its terminal history-incomplete reason. Source-less loss convergence
  retains its primary local loss reason while preserving the separate issue field in turn state;
  neither transition erases or rewrites the issue source record.
- No API may detach or rewrite a submitted turn parent edge. Replacement edits create a new turn and update only the selected thread bindings.
- Starting replacement edit changes only the draft's explicit typed intent and selected combined
  draft root/fresh-baseline history pair; cancellation changes only that intent and preserves the
  selected pair. The accepted replacement turn derives its parent from the immutable target
  turn, while a bounded current transcript-generation entry proves that target belonged to the
  selected path when edit intent was admitted.

## Ordinary Thread And Draft Mutation Boundary

- Empty ordinary-thread creation atomically contributes the thread, immutable execution record,
  initial attributes and usage records, empty current draft referencing the canonical immutable
  empty combined sequence/index/commitment root and deterministic canonical-empty edit-history reference,
  draft reverse index, current zero-entry transcript head,
  complete history summary, initial compact catalog summary, unbound binding revision, and binding
  head. Natural thread and draft identities are caller-owned inputs; the package derives
  `CanonicalEmptyDraftRootBuildOperationIdV1` from that draft id and accepts no caller-selected
  empty-root-build identity.
- Thread-from-tail creation requires one coherent nonempty source-thread selected-tail proof and
  source activity fact. It creates no turn or transcript-entry copy, stores that exact tail only as
  the new thread's selected path, publishes a zero-entry stale transcript head, and uses the
  caller's exact non-regressing timestamp for the draft and history summary. In the same creation
  command it inherits the source thread's immutable execution and publishes the child's initial
  attributes, usage, compact catalog summary, unbound binding revision, and binding head.
- Every mutation that creates a caller-named replacement draft uses the same deterministic empty-
  root-build identity and canonical-empty edit-history reference derived from that replacement
  draft id. Creation rejects any target identity
  or required-record collision. A bounded stable reconciliation read classifies the natural
  identity set as absent, exactly committed, or collided after replay or an ambiguous admitted
  outcome; exactly committed requires canonical bytes for every occupied natural record to equal
  the proposed canonical bytes, not merely matching digests.
- A completed sealed-composer import for replacement editing or composer-history recall is selected
  only after its bounded source stream has derived, built, and cross-validated the sequence tree,
  marker-identity index, and marker-order commitment tree/summary. One revision-checked atomic
  current-draft command then writes the imported combined root
  and a fresh-baseline immutable history snapshot bound to the same draft/import/root identity. Its
  exact visible availability is undo unavailable and redo unavailable. The command requires any
  prior non-import editor session to be absent or cleanly disposed with equal published/newest
  root/history pairs; its named import session must instead be active, clean at the predecessor, and
  advances both pairs to the baseline in the command. Its receipt owns the old and new selector/root/history triples for replay and
  reconciliation. Restart, restoration, and the next editor-session open consume the new pair; none
  may infer history from the imported root or recover the superseded pair by root digest.
- Current-draft reads stabilize the reverse index around the thread and draft reads and reject concurrent or contradictory publication rather than returning a mixed generation.
- An edit starts from one exact candidate-session predecessor, completes its immutable combined
  sequence/index/commitment successor, and adopts it only into that session. Autosave or flush later captures
  one adopted candidate/history pair. Its publication requires the exact current draft identity and
  selector revision/root/history triple, active session base and published root/history pair, absent active-operation
  custody, captured immutable settlement and authenticated combined-root summary/marker commitment,
  the required unchanged-head or changed-seal Asset evidence, captured immutable history snapshot,
  and unique publication operation identity; it takes no expected
  selected-path thread revision. On the serialized writer snapshot it permits a newer candidate
  head, carries the then-current thread revision into the reverse index, advances the draft selector
  and same session's published root/history pair to the captured pair, preserves thread ownership,
  submission intent, identity, and creation facts, and updates history-summary activity atomically.
- Exact combined-root-reference equality is a typed caller-visible no-change decision and produces no home
  command. Semantic equality between differently shaped roots is not inferred from aggregate counts
  or a detached digest; a caller that needs it uses bounded composite comparison. An update
  timestamp may equal but never precede current durable draft activity.
- Draft-edit cancellation is a settlement election owned by `HomeCommand`; it returns `Cancelled`
  only from the committed immutable settlement. A mutation API makes no rollback claim after writer
  admission; exact operation-scoped settlement reconciliation, current-draft reads, and natural-
  creation reads are the authority after a surfaced ambiguous failure.
- Draft-only thread revision advance does not invalidate a CAS binding whose observed selected-path tail and digest still agree exactly. A tail or digest change publishes a new unbound binding revision for the pending path; it never copies prior valid lineage onto an undelivered turn.

## Draft Piece-Tree Range Source And Restoration

- Every exact-root draft read requires one combined `DraftPieceRootReferenceV1` and validates its
  immutable combined-root record, all three summaries, applicable node/leaf digest chains, and requested
  search paths. It does not read
  the mutable current-draft selector before or after traversal and remains a valid historical-root
  integrity read when a newer root becomes current. A missing or inconsistent referenced root is
  absence or invariant failure, not `ConcurrentChange` caused by an unrelated later autosave.
- Opening an editor-candidate session requires the expected thread, draft, complete current-draft
  selector revision, exact `DraftPieceRootReferenceV1`, matching published edit-history frontier,
  fresh caller-owned session identity, and
  caller-owned open-operation identity. One atomic command validates the thread/draft/reverse
  selectors and creates or exactly replays the session-qualified head and open receipt. The initial
  head repeats the complete durable-base root including its closed root-build identity and sets its
  published root/history pair to the current-draft pair and its newest root/history pair to the
  canonically forked live frontier selecting that same root and exact availability, preserving all
  immutable ancestor-skip references byte-for-byte while giving later sibling appends independent
  frontier authority. It sets the
  logical byte/line extent to that root. The identical
  canonical request against the identical active session returns `ExactReplay(head)`; after that
  identical session is disposed, the same request returns `StaleDisposed(head)` and cannot reopen
  it. An occupied session identity with different canonical request or durable-base bytes returns
  `OccupiedIdentityCollision(proof)`, never stale. Only an absent fresh session whose expected
  selector drifted returns `SelectorConflict(current selector)`. Opening authenticates the retained
  history policy revision and locally validates the selected head/floor references, depths and
  cumulative positions, retained-byte accounting, root-pin references, and exact undo/redo
  availability. It does not recursively re-prove immutable skip derivation or root adjacency, load a
  transition collection, or perform a corruption scrub. The returned activation binding
  names the exact draft, session, session generation, candidate generation, complete root reference,
  and checked logical byte/line extent. No detached digest, draft id, or selector revision is an
  activation binding.
- The separate current-root wrapper stabilizes `draft-by-thread`, the thread, draft, and reverse
  index, performs the exact-root read, then rereads those mutable selectors. It returns the bounded
  result plus the exact stabilized selector only if the same root remains selected; selector drift
  is `ConcurrentChange`. The exact-root operation is also the historical-root wrapper; it does not
  consult mutable selectors. Callers that
  need only immutable historical integrity do not request this wrapper.
- The separate candidate-root wrapper requires an exact live session identity, stabilizes its head
  and expected session, candidate, and newest history-frontier revisions around the same exact-root
  read, and returns the named newest candidate root, matching history frontier and availability, and
  logical byte/line extent. A stale candidate or session request
  is rejected: head drift is `ConcurrentChange`, disposal returns the exact disposed head, and
  absence returns `Absent`; a successful result repeats the stabilized session and candidate
  generations, and none can return a page from an older head. It is
  never invoked by routine startup, reopen, thread selection, or restoration. Those paths bind
  exclusively to the durable current-root wrapper, so unpublished candidate sessions cannot become
  user draft authority after a crash.
- An exact-root text demand is one of `Forward(anchor)`, `Backward(anchor)`, or
  `Validate(candidate)`, plus a byte ceiling from 4 through 65,536. `Forward` selects a range whose
  start equals the requested anchor and returns a positive source-selected prefix of complete UTF-8 scalars
  within the ceiling unless the anchor is exact EOF. `Backward` selects a range whose end equals the
  requested anchor and returns a positive source-selected suffix of complete scalars unless the anchor is
  exact BOF. `Validate` returns a source-selected scalar-safe range within the ceiling that contains
  the candidate coordinate, or abuts it when it is exact BOF or EOF, and enough authenticated
  surrounding bytes or exact BOF/EOF facts to
  prove whether that coordinate is a scalar boundary; it does not clamp or round a coordinate
  inside a scalar. Each result repeats the complete root reference, exact selected half-open range,
  atom-free UTF-8 bytes, checked byte/newline/line summary for those bytes, and authenticated
  preceding and following continuation or document-edge facts. A non-edge response that makes no
  progress, crosses the root extent, splits a scalar, exceeds its ceiling, or disagrees with the
  root summary is invariant failure.
- An exact-root marker-page request names a bounded half-open byte interval or exact anchor,
  `Forward` or `Backward`, an optional exclusive authenticated composite search-key cursor, an
  object ceiling from 1 through 256, and a retained canonical response-byte ceiling from 1 through
  65,536. Direction selects the adjacent window, while the returned list always uses strict
  `(anchor, order key, marker identity)` source order. Each page repeats the complete root and
  covered range, counts every retained marker, label, cursor, and edge fact against both ceilings as
  applicable, and returns authenticated immediately preceding and following marker or exact range/
  root edge facts. Requested-side completion is distinct from an exact continuation cursor.
  Arbitrarily many markers at one anchor therefore advance by the exclusive last or first returned
  composite key without consuming text bytes, retaining the complete anchor run, or constructing a
  marker registry.
- Exact-root marker-edge operations provide bounded authenticated proofs for no marker at an anchor,
  the first marker at an anchor, the last marker at an anchor, or one named adjacent ordered pair at
  an anchor. First proves the external before-all translation, last proves after-all, adjacent proves
  an interior gap, and absence proves the unambiguous byte gap. Reverse translation supplies the
  named identities, order keys, and anchor and must reproduce the same first, last, adjacency, or
  absence proof. Each operation uses at most two bounded sequence descents plus one authenticated
  successor or predecessor step and never obtains the proof by paging or scanning every marker at
  the anchor.
- A marker-id lookup descends the stable-id envelopes in the selected marker-identity index,
  authenticates the exact occurrence leaf, and returns only its stable id, final label, same-anchor
  order key, and sequence marker-leaf identity/digest; it neither stores nor discovers an absolute
  location. Stable absence from that authenticated identity leaf/path proves absence for the
  combined root. Location validation separately accepts the caller's composite position or anchor
  witness, performs one bounded sequence descent at that location, and requires the reached marker
  leaf to equal all authenticated occurrence facts. Neither operation scans ordered marker pages.
- A composite-piece page merge-orders text ranges and zero-width markers from the same exact root.
  Position validation and range endpoints return or consume only `DraftCompositePositionV1` gap
  witnesses proven by the bounded first, last, absence, or exact-adjacent operations above and an
  authenticated successor relationship through those search envelopes. A malformed request has an
  out-of-root coordinate, zero or excessive limit, invalid UTF-8 request field, reversed range,
  wrong-root or noncanonical cursor, duplicate or misordered marker fact, or impossible edge/
  completion combination and is rejected before returning partial data. A response-side root,
  summary, UTF-8 boundary, marker adjacency, range, page-ceiling, cursor, progress, or edge-fact
  disagreement is invariant failure. Missing immutable records are absence; current-selector drift
  is `ConcurrentChange` only in the current-root wrapper; candidate/session drift uses the stale
  errors above. No wrapper retries by widening a bound or silently selects a different root.
- A page or proof limit that is outside the fixed public range is malformed. If an otherwise valid
  retained-byte or object ceiling cannot hold the mandatory edge facts or one available source
  object needed for progress, the operation returns a limit error with no partial page or advanced
  cursor. Stale and concurrent outcomes likewise return no page. Storage faults, invalid persisted
  UTF-8, arithmetic overflow, noncanonical encoding, digest or summary mismatch, and impossible
  cross-tree facts return operational or invariant failure and never degrade into absence, stale
  data, clamping, truncation, or a whole-root fallback.
- Compact restoration validation accepts the exact combined root, logical extent, caret and selection
  positions, scroll anchor/continuation, durable edit-history frontier identity, and exact undo/redo
  availability. It authenticates the selected frontier's exact head/floor references, depths,
  cumulative positions, retained accounting, root-pin references, and availability through local
  canonical record agreement without recursively re-proving skip derivation or root adjacency, then validates every
  distinct offset and gap through bounded text and marker reads and returns only exact validated
  positions. It does not translate an older root, clamp an invalid boundary, or return text,
  marker collections, resident pages, layout state, or undo payloads. A frontier that is stale,
  outside retention, wrong-draft, or disagrees with the selected root fails closed rather than
  restoring approximate availability.
- A bounded composite comparison accepts two exact combined roots and advances paired text/marker cursors
  until it proves equality, the first ordered difference, or exact EOF. Counts and digests may
  reject inequality early but cannot by themselves assert semantic equality between different root
  references. Retained state and each step remain within the public page bounds.
- Range-source activation, text and marker demands, position translation, and restoration validation
  are read/session-conformance operations only. They do not stage or settle edits, advance dirty or
  published root/history pairs, autosave the current selector, construct or materialize `ComposerV1`, or
  submit a draft. Those mutations and whole-root streamed consumers remain separate later
  boundaries.

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
- Valid marker-bearing content is rejected explicitly at this whole-content text-only boundary.
  Marker-aware projection first derives exact marker-bounded logical segments from ordered content
  pieces, then uses the separate segment-range read for authored text.
- One bounded segment-proof read accepts the exact sealed content reference plus an optional opaque
  preceding-marker boundary returned by the prior proof. A bare piece ordinal is never accepted as
  authenticated cursor authority. No cursor means the unique leading or whole segment;
  a cursor means the unique segment immediately after that marker. The read derives both logical
  bounds, scans the consecutive ordered piece interval once, and returns an opaque proof plus its
  authenticated following-marker boundary when any. Caller-supplied raw ranges never select a
  segment. Feeding each following-marker cursor into the next proof uniquely walks leading,
  adjacent, trailing, marker-only, and marker-free content.
- Segment-range page reads require that opaque proof plus an absolute page start. They return
  UTF-8-safe continuation without rescanning the complete segment and still reject a contradictory
  marker encountered in the physical page path. One proof plus sequential replay is linear in the
  segment's pieces, spans, payload, and page count.
- Segment reads never return image labels or marker substitutions; those generated fragments remain
  the app/composer projection boundary's responsibility.
- The result reports the exact logical start, bounded UTF-8 payload, optional next offset, exact
  sealed reference, and checked stored key-and-value byte total for both manifest reads, span-index
  pages, and chunk records.

## Resource Payloads

- Heavy resources are addressed by metadata records and explicit byte ranges.
- Code and table resources retain one exact closed composer-content or provider-narrative backing
  plus a half-open logical UTF-8 range. The appropriate content or narrative span index maps a
  requested resource range to the minimal bounded encoded ProviderItemV1 or composer chunk ranges;
  storage does not duplicate those bytes into projection records or sidecars.
- Large externally owned byte payloads may live in sidecar files only when their owning feature
  admits and range-reads them through an explicitly bounded home-store contract. Textual Markdown
  remains in its indexed canonical backing and never uses a whole-buffer sidecar path.
- Sidecar paths are storage-owned implementation detail and are not exposed as stable public identities.
- Resource writes record media type, exact backing range, byte length, versioned chain digest,
  preview range when available, resource kind, and applicable language, logical-line, table-row,
  table-column, and header facts.
- Resource range reads use half-open resource-relative ranges, reject reversal, overflow, or an end
  beyond the resource length, and return at most 65,536 bytes plus an exact continuation fact.

## Transcript Provider Support

- Storage-backed transcript providers read transcript-view pages, exact immutable branch-context envelopes, projection record sets, resource metadata, and resource ranges from this package.
- The provider boundary may reject missing, stale, oversized, unsupported, or policy-denied reads using typed errors derived from storage state.
- The package exposes no GPUI, renderer, row-realization, or resident-cache API. Its transcript
  boundary supplies only bounded durable typed reads; presentation adaptation and resident-memory
  policy remain outside this package.

## Failure And Recovery

- Incomplete turns, failed turns, stream loss, and local ingestion failure are represented explicitly in durable state.
- Storage does not treat CAS reconnect, resume, late subscription, process restart, historical reads,
  or a later status-only terminal event as ordinary replay of missing source events. One dedicated
  repair build may target only an exact repair-required turn and the same correlated terminal CAS
  thread/turn. Bounded page commands stage the complete semantic final-item view behind an opaque
  package-local head, validating item identities, complete fields, per-page digests, provenance, and
  finalized media as each page becomes an immutable commitment. This package's final participant
  validates the exact correlation, consumed request claim, terminal outcome, compact aggregate and
  family commitments, finalized-media commitments, and package-local publication witnesses before
  selecting those already staged paged commitments as whole-turn canonical authority. It runs only
  inside the system-owned cross-domain publication command and does not validate or publish the
  sibling asset participant. Live-prefix equality is not an admission requirement: a known mismatch is a repair
  cause, and neither the prior prefix nor any partial stage is spliced into the replacement. Missing
  identity, ambiguous correlation, nonterminal or incomplete data, invalid provenance, or a limit
  violation leaves the prior canonical authority unchanged and the turn repair-required or
  explicitly incomplete according to the owning convergence command.
- Normal transcript and history reads remain Syndic-backed. The repair mutation is terminal-turn
  ingestion authority only; it is not a CAS-backed transcript provider, general backfill API, or
  permission to rewrite unrelated or already exact operational capture.
- Proven loss of an active execution session atomically stales its binding and retires the old
  projection. Once no usable projection authority remains, a later source-less terminal transition
  may close local turn capture as incomplete. A possibly dispatched start is never reset to pending,
  and a possibly dispatched steering fragment is never reset to retryable solely because the session
  disappeared.
- Scoped restart classification treats a named active binding as possible dispatch, including an activation
  with no active CAS-turn row or source event. A safe pending result requires a pending,
  source-free turn, no selected active route, and a non-active current binding. A selected
  projection-loss route with the matching stale successor instead proves that abandonment already
  committed and authorizes only source-less incomplete terminal convergence.
- An active awaiting-terminal classification requires an unknown-terminal committed tail, the
  matching gate, exact active binding and CAS turn, and a selected `AwaitingTerminal` generation
  with no ready or delivering aggregate. Scoped recovery returns its distinct durable lost-target witness
  and permits only generic active abandonment; it never restores steering, synthesizes activation,
  waits for late evidence from the lost process, or resumes that session.
- A stopping classifier requires the exact gate-record pair and returns admitted or
  dispatch-claimed state without making either replayable. Restart abandonment preserves the
  distinction in the transition witness, consumes the record's live authority into its recovery-
  abandonment disposition, retires the old projection, and leaves accepted next-turn work intact
  before source-less incomplete convergence.
- A compacting classifier requires the exact gate, operation record, provider-operation turn and
  snapshot, valid-or-retired binding relationship, optional CAS turn, marker, terminal state, and
  route aggregates. `Admitted` is proven unissued and may be consumed locally while retaining the
  valid binding. A claimed record with durable proven nondispatch may finish the same local
  consumption and retain the binding without reusing its attempt. A claim with no disposition,
  acceptance, or completion unknown is possible dispatch and authorizes only binding retirement
  plus source-less incomplete convergence. Pinned rejection proves no core admission but still
  requires target retirement. None authorizes compact-start replay.
- Matching marker-then-successful-terminal evidence authorizes resumable bounded provider-item
  finalization and successful compaction consumption even if request acknowledgement is absent.
  Interrupted terminal authorizes interrupted consumption with
  `ForcedAbortOrderingUnproven`; it preserves the binding only with separate exact idle-status
  evidence. Failed terminal, idle-unproven interruption, or successful terminal without the marker
  authorizes failure consumption and binding retirement. Timeout is not stored as a durable
  lifecycle transition and therefore does not change scoped recovery classification.
- A compacting operation handed to stop must retain an exact two-way link between the compaction
  record, its provider-operation target, the stopping gate, and the stop record. Restart consumes
  the paired authorities through the stop abandonment successor. A missing half or disagreeing
  target is corruption, not authority to delete a record or replay either request.
- Compaction consumption preserves every accepted input and exposes the ordinary accepted-next
  readiness transition only after provider-operation item finalization is fixed. Storage records
  no process-local lifecycle intent and cannot reconstruct an automatic continuation at restart.
  A continuation already admitted by the consumed compaction witness is instead an ordinary
  durable `PendingTurn` with exact item/content authority and is returned through the existing
  recovered-pending source without creating another identity.
- Repeating classification after an ambiguous abandonment or terminal commit yields the exact
  successor case or settled idle state. It never manufactures a retry decision from absence,
  rewrites a terminal accepted-input lifecycle, or scans every accepted leaf to authenticate the
  constant-size projection-loss witness.
- An explicit V5 schema-validation boundary, scrub, background-maintenance pass, or
  corruption-evidence investigation may exhaustively validate current-draft uniqueness,
  thread/draft ownership, append-only draft-build progress-receipt continuity and absence of
  deletion, replacement, or forks, one-way draft-to-turn identity consumption with no live raw-payload
  collision, committed-tail reachability, immutable parentage, monotonic revisions, accepted-input
  ordering, CAS-binding uniqueness, source-event ordering and per-item replay, sealed provider-
  observation issue evidence, folded first-issue turn state, terminal closure and finalization
  frontiers, stale projection markers, and referenced resources. Routine open and recovery do not
  perform that every-record walk.
- Unfinished or stale projections can be invalidated and recomputed from canonical items and each
  item's selected source authority: admitted source events for normal capture or immutable snapshot-
  backed ranges for terminal repair. A finalized projection is durable history and is never an in-
  place rebuild target.
- Corrupt, missing, or unsupported records produce typed storage errors rather than silent fallback to CAS history or GUI-local caches.
- Unreachable turns and unreferenced sidecars observed by explicit validation or maintenance are not
  routine-open errors and are not deleted; they remain for the future explicit garbage-collection
  design.

## Privacy And Redaction

- Storage APIs accept only data that has already crossed the owning system redaction boundary.
- Secret-like fields must be rejected or redacted before durable commit.
- Hidden developer instructions and policy-private control payloads are not transcript content and must not be stored as user or assistant projection records.
- Diagnostic payloads stored durably must be bounded and must not include raw auth headers, tokens, cookies, environment secrets, or capability tokens.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `persistent-state-integrity/v1`
