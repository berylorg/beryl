# V7 Persisted Schema

This supplement is the sole authority for the persisted `syndic` byte format at schema V7. It owns
the complete 67-primary plus 23-index family inventory, family and record versions, natural keys,
canonical values, tags, integer encoding, digest preimages, decode rejection, public schema bounds,
and structural proofs. The package entry point controls scope and rigor. No other supplement may
change persisted bytes.

## V7 Domain Schema

- The stable logical domain name is `syndic` at domain schema V7. Every family uses keyspace schema
  V1 and one exact package-owned record version selected per family. `source-events`,
  and `accepted-inputs` use record V3; `accepted-route-leaves` uses record V4; `input-gates` uses
  record V5;
  `accepted-route-generations` and `turns` use record V3; `threads`, `drafts`, `turn-states`,
  `accepted-order`, `content-manifests`, `canonical-items`, and `execution-snapshots` use record V2.
  `draft-mutation-staging-pages`, `draft-piece-build-fragments`, `draft-piece-leaves`,
  `draft-marker-identity-index`, `draft-marker-order-commitments`, `draft-marker-seals`, and
  `draft-editor-candidate-sessions` also use record V2. `draft-piece-builds`,
  `draft-piece-build-progress`, and `draft-piece-settlements` use replacement record V4; every other
  V7 family uses record V1. V6 domain values and prior records in those three replaced families are
  rejected rather than accepted, migrated, dual-written, or adapted.
- The Rust boundary values remain `DraftPieceBuildRecordV1`,
  `DraftPieceBuildProgressReceiptV1`, and `DraftPieceSettlementV1`. Those suffixes name their
  semantic API shapes; the enclosing family codec version is V4 and the digest domains are
  `/v4`. V4 replaces the prior whole-range Applying semantics with exact partial continuation.
  Unchanged cursor field widths do not establish compatibility: V3 in-flight builds, progress
  receipts, and settlements are rejected at the version boundary, with no legacy transition reader.
- The primary families are `threads`, `image-label-authority-heads`,
  `draft-image-label-protection-heads`, `thread-executions`,
  `thread-attributes`,
  `thread-usage`, `thread-catalog-summaries`, `drafts`, `draft-piece-roots`,
  `draft-piece-nodes`, `draft-piece-leaves`, `draft-marker-identity-index`,
  `draft-marker-order-commitments`, `draft-marker-seals`,
  `draft-marker-label-admission-capacity`, `draft-marker-label-admission-heads`,
  `draft-marker-label-admission-nodes`,
  `draft-marker-label-admission-receipts`,
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
  `draft-marker-label-admission-capacity`, `draft-marker-label-admission-heads`,
  `draft-marker-label-admission-nodes`,
  `draft-marker-label-admission-receipts`,
  `draft-editor-candidate-sessions`,
  `draft-mutation-staging-heads`, `draft-mutation-staging-pages`,
  `draft-mutation-staging-progress`,
  `draft-piece-build-progress`, `draft-edit-history-frontiers`, `draft-edit-history-transitions`,
  and `draft-historical-root-adoptions` are distinct primary families. The marker index uses tagged
  internal-node and leaf records, and the candidate-session family uses tagged head and immutable
  receipt records. Marker-order commitments use tagged immutable internal-node and leaf records;
  marker seals use compact durable cursor/lifecycle records. Build progress instead requires its own append-only family so canonical proposal
  fragments remain the only values in `draft-piece-build-fragments`.
- The 23 index V7 families are `draft-by-thread`, `thread-parent-index`,
  `image-label-origin-spans`, `turn-children`, `accepted-order`, `accepted-route-generations`,
  `accepted-ready-sources`, `accepted-next-sources`, `turn-items`, `activity-query-entries`,
  `activity-query-sources`, `item-source-events`, `cas-item-index`, `transcript-path-turns`,
  `transcript-view-entries`, `stable-item-projections`, `item-projections`,
  `projection-resources`, `binding-heads`, `cas-thread-index`, `cas-thread-bindings`,
  `cas-turn-index`, and `provider-observation-chunks`.
- The complete V7 inventory is exactly 67 primary plus 23 index families, or 90 total. Family names,
  natural key encodings, and the complete primary/index inventory are closed. A release
  registers exactly the implemented owned families it exposes and never registers an empty
  placeholder for an unimplemented family.

### Draft Root Canonical Encodings

V1 draft structure digests are domain-separated SHA-256 over canonical package encodings. In these
formulas, `H` is SHA-256 and `LP(x)` is the unsigned big-endian `u64` length of `x` followed by
`x`:

- Empty sequence root: `H(LP("syndic/draft-sequence-root/v1/empty"))`.
- Empty marker-identity-index root:
  `H(LP("syndic/draft-marker-identity-index-root/v1/empty"))`.
- Empty marker-order-commitment root:
  `H(LP("syndic/draft-marker-order-commitment-root/v1/empty"))`.
- For canonical sequence-summary bytes `S`, identity-index-summary bytes `I`, and canonical
  `DraftMarkerCommitmentV1` bytes `M`, every combined root is
  `H(LP("syndic/draft-combined-root/v1") || LP(S) || LP(I) || LP(M))`.

The canonical empty combined root selects no sequence, identity-index, or marker-order-commitment
root. Every height, byte, newline, line, piece, marker, identity, and count is zero, and the three
empty digests above are exact. Empty text has zero bytes, newlines, and lines. Nonempty text has
logical line count equal to its checked newline count plus one, including a final empty logical line
after a trailing `0x0A`.

`CanonicalEmptyDraftRootBuildOperationIdV1` is the first 16 digest bytes, without UUID-bit
rewriting, of
`H(LP("syndic/canonical-empty-draft-root-build-operation/v1") || LP(draft_id_bytes))`, where
`draft_id_bytes` is the exact 16-byte `SyndicDraftId` payload.

`DraftPieceRootNaturalKeyV1` is the 16-byte draft id, one closed one-byte build tag, and its fixed
payload. `DirectCanonicalEmpty` is exactly 33 bytes and carries the 16-byte canonical-empty build
operation. `EditorCandidate` is exactly 49 bytes and carries the 16-byte session id followed by the
16-byte caller-owned operation id. Equal operation ids in different sessions occupy different
root, build, fragment, and settlement namespaces.

Every structure record repeats its owner, tag, natural identity, digest, and applicable aggregate.
Canonical decode rejects unknown tags, key/value disagreement, noncanonical counts or options,
invalid UTF-8, overflow, trailing bytes, empty or overlapping envelopes, aggregate mismatch, and
digest or root-summary disagreement. Canonical byte equality, not digest equality, establishes
replay.

### Draft Mutation Staging Canonical Encodings

`DraftMutationOperationIdV1` is an opaque caller-owned 16-byte identity. Encoding version V1 is the
exact byte `1`. `DraftMutationStagingIdentityV1` is the exact 48-byte concatenation of draft id,
editor-candidate session id, and operation id.

The V1 staging head repeats that identity and retains the bounded canonical `MutationBeginV1` and
digest; session and predecessor candidate generations; predecessor root, history, logical extent,
caret, and directed selection; encoding version; independent source/proposal lane frontiers; latest
progress-receipt key/digest; and exactly `Receiving`, `Finished`, `Building`, `Cancelled`,
`Rejected`, `Conflict`, or `Error`. `Finished` repeats both final lane frontiers and intended
successor positions. The head contains no page payload, receipt chain, root graph, candidate root,
or current-draft authority.

The staging-head digest is SHA-256 over the length-prefixed exact ASCII domain
`syndic/draft-mutation-staging-head/v1`, canonical key, and every canonical head field except the
selected receipt digest. The complete selected receipt key, including transition ordinal, remains
in the preimage.

A staging-page key is exactly 57 bytes: the 48-byte staging identity, lane tag `SourcePage = 0` or
`ProposalPage = 1`, and unsigned big-endian one-based `u64` lane ordinal. The immutable value
repeats the key, exact input and successor cursors, positive item and retained-byte ceilings, prior
and successor cumulative identities, checked cumulative counts and bytes, canonical count-framed
page bytes, progress transition ordinal, and page digest.

The page digest is SHA-256 over the canonical length-prefixed sequence of exact ASCII domain
`syndic/draft-mutation-staging-page/v1`; complete page key; progress transition ordinal; input and
successor cursors; ceilings; prior cumulative identity; successor totals; and exact count-framed
page-item bytes. It excludes exactly the successor cumulative identity and page-digest field, with
no placeholders. The successor cumulative identity is SHA-256 over the canonical length-prefixed
exact ASCII domain `syndic/draft-mutation-staging-lane/v1/link`, prior cumulative identity, and page
digest, in that order.

A staging-progress key is exactly 56 bytes: staging identity followed by unsigned big-endian
one-based `u64` transition ordinal. Its immutable value repeats the key; exact prior receipt
key/digest, absent only at ordinal one; command kind; optional affected page key/digest or finish
digest; complete before/after lane frontiers; before/after staging-head digests and lifecycle;
candidate-session custody before/after tags and endpoints; optional build endpoint; one closed
terminal-evidence union; and receipt digest. The receipt contains no page bytes.

The receipt digest is SHA-256 over the length-prefixed exact ASCII domain
`syndic/draft-mutation-staging-progress-receipt/v1`, canonical key, and every preceding canonical
receipt field, including before/after head digests. It is a commitment and never substitutes for
canonical byte comparison of the point-read target closure.
- `draft-piece-roots` and `draft-piece-nodes` use immutable V1 codecs. Marker-bearing
  `draft-piece-leaves`, `draft-marker-identity-index`, and `draft-marker-order-commitments` use
  immutable V2 codecs, and `draft-marker-seals` uses its V2 codec directly. Roots use
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
  summaries specified under [Draft Root Canonical Encodings](#draft-root-canonical-encodings).
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
  candidate, generation, dirty, lifecycle, and optional fixed-size active-operation custody facts.
  Those facts are the immutable opening selector/root/history checkpoint, latest published
  selector/root/history checkpoint, newest candidate generation/root/live-history checkpoint,
  monotonic session and dirty generations, active-or-disposed lifecycle, and optional `Staging` or
  `Building` custody. The base, published, and newest checkpoints each pair one exact candidate root with
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
  root/history pairs. Ordinary disposal of an unchanged opening may atomically normalize newest
  history to published history only after authenticating its exact opening fork and current durable
  checkpoint, without custody. The existing disposal receipt binds the complete source and final
  pairs and lifecycle transition; the draft selector is unchanged. This uses existing encodings and
  preserves the disposed-head equality invariant.
- `draft-mutation-staging-heads` uses the exact 48-byte `DraftMutationStagingIdentityV1` key and
  mutable V1 head specified under
  [Draft Mutation Staging Canonical Encodings](#draft-mutation-staging-canonical-encodings).
  `draft-mutation-staging-pages` uses the exact
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

### Draft Edit-History Canonical Encodings

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
### Draft Build And Settlement Canonical Encodings

- `draft-piece-builds` is keyed by the exact 48-byte draft/session/operation identity. Its
  `DraftPieceBuildRecordV1` value repeats the closed edit-successor or sealed-composer-import kind. An edit-successor repeats
  the exact staging identity and authenticated finish receipt, finish-derived canonical proposal-
  header bytes and digest, predecessor candidate generation and exact combined root; an import has
  no staging identity. The record also retains any optional exact sealed
  content source,
  compact declared counts and digests, finished-staging reference, consumed source/proposal staging
  frontiers, ordered fragment, sequence-path, identity-index, marker-order-commitment, fixed marker-
  effect scan frontier, completed effect count and cumulative chain, optional fixed-size active
  effect, and cross-validation frontiers, proposed successor candidate
  generation/combined root, complete canonical combined-root summaries, and
  exactly `Open`, `Complete`, `Committed(settlement)`,
  `Rejected(settlement)`, `Conflict(settlement)`, `Cancelled(settlement)`, or `Error(settlement)`
  lifecycle. The Applying fragment ordinal and `base_end` remain fixed while its two successor
  boundaries encode the exact remaining interval in the selected working sequence. Their field
  order and widths are unchanged within the replacement V4 encoding; the closed partial-removal transitions are owned by
  [bounded sequence continuation](design-draft-storage.md#bounded-sequence-range-continuation).
  Both the build head and each progress receipt carry that same cursor meaning. It contains no
  whole edit, replacement collection, inserted payload, or mutable self-
  hash; its current transition authority is the exact latest progress-receipt key and digest.
  `draft-piece-build-fragments` is keyed by that build plus one-based fragment ordinal and stores
  one bounded exact replacement, inserted-piece, or self-contained marker-effect fragment with its
  preceding chain digest. One continuation or replay command requires canonical build-header bytes
  and every fragment in its bounded target window to match ordinal by ordinal against the one next
  authenticated staged proposal window. The source receipt's fragment endpoint/chain authenticates
  the already consumed prefix, which is never compared again or reconstructed from caller bytes;
  equal header, fragment, or chain digests alone are insufficient.
  Fragment gaps, overlaps, reorderings, repeated empty ranges at one composite position when either
  item lacks a distinct closed marker effect, unknown
  position-witness tags, and a terminal declaration that disagrees with the accumulated counts or
  digest are invalid. This family has no tag or value shape for a progress receipt.
- `draft-piece-build-progress` is keyed by the exact 56-byte draft/session/operation/one-based-
  transition-ordinal identity. Each immutable `DraftPieceBuildProgressReceiptV1` value repeats that key and the exact prior receipt
  key and digest, with `None` valid only at ordinal one; the exact authenticated canonical-fragment
  endpoint, canonically empty before any fragment and otherwise naming its one-based key and
  canonical fragment digest plus its chain; exact staging identity and finished-head/receipt
  reference; current phase and relational cursors; consumed source/proposal staging-lane frontiers;
  working sequence, identity-index, and marker-order-commitment roots and complete summaries; source and successor structure
  frontiers; fixed marker-effect next-fragment/scanned-prefix frontier, completed effect count and
  cumulative chain, optional fixed-size active marker effect; next
  record ordinal; optional
  successor root and build digest; lifecycle; and the SHA-256 receipt digest under exact ASCII
  domain `syndic/draft-piece-build-progress-receipt/v4` over the canonical key and every preceding
  canonical value field. A non-one ordinal without the exact immediately preceding key/digest, any skipped or
  disagreeing transition, or any key/value/digest disagreement is invalid. While the build head
  selects the preceding receipt, this receipt's key must be absent; occupied bytes in that state are
  a corrupt split even when equal. Once the build head selects this receipt, it can prove replay only
  together with byte equality of the complete same-command closure.
- The V4 family encodings of `DraftPieceBuildRecordV1` and
  `DraftPieceBuildProgressReceiptV1`, together with the immutable fragment value shape, own
  the durable continuation fields;
  no further secondary index, operation-page history, or marker-effect map is required beyond the
  declared marker-order-commitment and marker-seal families. A build endpoint locates its next staging page by one natural-key point read from the
  retained staging identity, selected lane, and next lane ordinal, which is `O(1)` in operation
  length.
- `draft-piece-settlements` is keyed by the exact 48-byte draft/session/operation identity. Its immutable `DraftPieceSettlementV1`
  value repeats the key, canonical proposal-header bytes and digest, declared fragment count and
  terminal fragment-chain commitment, predecessor candidate generation/combined-root/history pair,
  optional build digest, terminal
  outcome, source basis, terminal progress-receipt key/digest and immediate-predecessor/root closure,
  and one complete outcome-specific proof. `Committed` is either edit adoption, binding successor
  candidate generation/root/history, complete root summaries, marker commitment, extent, positions,
  adoption receipt, and terminal build digest; or sealed-import selection, binding imported root,
  fresh-baseline history, unavailable undo/redo, current-draft selector/root/history before and
  after, and equal session published/newest root-history pairs. `Rejected` binds a closed invalid-
  proposal reason and absence of adoption. `Conflict` binds the observed newest candidate
  generation/root/history and absence of adoption. `Cancelled` binds the pre-adoption cancellation
  witness and absence of adoption. `Error` binds a closed operational reason or occupied-identity
  canonical-byte comparison witness and absence of adoption. Every no-change form proves the
  predecessor remained selected. The proof is followed by its domain-separated settlement digest. Unknown or
  incomplete outcomes, a no-change outcome naming an adopted successor, a committed outcome
  missing its exact combined-root/history/build/session-head adoption or direct-selection closure, a terminal build without its agreeing
  progress receipt and settlement, a settlement disagreed with by the candidate, receipt closure, or
  historical combined-root closure, or a second canonical value at the same key is corruption.
  Exact settlement replay requires the stored build head already to select the terminal target
  receipt and every same-command effect to equal the stored target closure. It additionally requires the
  request's canonical header and bounded fragment bytes to equal the settlement's retained proposal
  and referenced build fragments; the stored settlement itself must pass canonical decoding and
  exact closure validation. Equal digests are not sufficient for either check.
- The replacement record digests use exact ASCII domains
  `syndic/draft-piece-build/v4`, `syndic/draft-piece-build-progress-receipt/v4`, and
  `syndic/draft-piece-settlement/v4`. The marker-effect chain begins from its one canonical empty
  value under `syndic/draft-marker-effect-chain/v1`; each completed step hashes the prior chain,
  exact fragment natural identity and canonical digest, completed effect count, and post-effect
  sequence/index/commitment root digest. No prior build, progress, settlement, or effect-chain domain
  is valid under V7.
- `draft-marker-seals` stores one V2 durable resumable seal record keyed by exact draft, captured
  combined-root/build identity, exact `DraftMarkerCommitmentV1`, and caller-owned seal-operation
  identity. It retains only the next marker-order-tree cursor, completed marker frontier,
  incremental sequential digest/count/maximum state, independent ordered marker/asset digest, and
  closed `Open`, `Cancelled`, `Failed`, `Superseded`, or `Sealed` lifecycle. Only exact EOF plus
  frontier/count/maximum/commitment/root
  agreement creates the package-issued opaque `DraftMarkerSealProofV1` binding the exact root and
  commitment to `SequentialMarkerSummaryV1` and `OrderedMarkerAssetSummaryV1`; raw summary values and open
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
  pages retain exact field/range bytes; media pages retain finalized asset identity, byte
  digest/length, authenticated adapter/release/runtime/`savedPath` provenance, exact target
  item/resource ordinal, the matching Asset page/entry locator, and the ordered cross-domain media
  commitment supplied by the system command. These locators use the existing target thread/turn
  identities and bounded package-owned references, not a new shared repair identity. Item/resource
  source descriptors retain the exact media-page location so a read never searches the page set.
- Media entries are strictly ordered by one-based snapshot item ordinal and one-based resource
  ordinal within that item. A direct locator contains one-based page ordinal and zero-based entry
  index; page ordinals use the existing `u64` encoding and entry indexes use unsigned big-endian
  `u16`, rejecting an index outside the decoded bounded page. The page digest covers the exact
  canonical entries and locators, excluding its own digest and the successor chain commitment;
  the head advances that chain from the prior commitment and page digest. The final ordered media
  commitment belongs to the sealed head and read reference, not to its own entry preimage.
  Seal admission requires the complete-response frontier and declared media totals, never merely
  the last page currently present. No new record is written per entry when that frontier seals.
- Open or failed repair stages are unreachable from canonical items, transcript projections,
  history reads, catalog reads, and replay. Each bounded page-stage command validates and durably
  commits that page's complete item fields, identities, digests, provenance, or media witnesses while
  advancing the build head's checked totals and family chain commitments. For a repair-media page,
  this package's participant validates exact ordered item/resource membership and direct locators,
  records the noncanonical media witness, and advances only the Syndic build state in the same
  command as the matching immutable Asset page; it publishes no canonical history authority.
- This package's participant in the final repair command validates the exact `RepairRequired` gate
  and consumed request claim, target correlation, terminal outcome, sealed build head, declared
  totals, complete family commitments, adapter/release provenance, and finalized-media commitments.
  It then selects the complete snapshot-backed canonical source, terminal state, projection
  staleness, sealed repair metadata, and exact `FinalizingHistory(target)` successor gate. Missing
  or disagreeing package-local facts reject that participant and leave existing Syndic authority
  unchanged.
- The corresponding Asset participant publishes one compact visibility selector. Ordinary
  resource-metadata reads obtain a selected repair source's exact owner-qualified Asset reference
  from the named snapshot-backed item/resource and media-page locations; they require selected
  snapshot authority and reject unselected stages. Final selection creates no per-resource copy or
  index sweep. Subsequent bounded projection work preserves those locators. This adds no Syndic
  family: source descriptors and the existing repair head/media pages carry the fields.
- Cross-domain staging and final publication are the system-owned `HomeCommand`s defined by
  `doc/systems/cas-live-syndic-transcript/design.md`. This package contributes only its Syndic
  participant and cannot independently assert whole-command success or make partial repair media
  canonical.
- A V7 `input-gates` value canonically stores the exact stopping variant—blocked Syndic turn plus
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
  `UnknownTerminal` next-turn reason. There are no predecessor record decoders because the V7
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
  ancestor at that depth. Selected-path membership therefore uses bounded deterministic lineage
  work and constant resident memory without an unbounded parent walk or per-turn jump table.
- Every non-root thread with a parent-thread handoff binding stores the same deterministic skip
  shape over immutable thread lineage; top-level threads have depth one and no parent or skip.
  A revision-bound lineage query uses the selected leaf's depth and digest to return bounded
  top-to-bottom ancestor pages by exact logical depth without retaining the complete path or a jump
  table. A scoped lineage read validates parent, depth, digest, and skip agreement through bounded point reads.
- An `image-label-authority-heads` key contains exactly one thread id. Its canonical value repeats
  that id and stores a nonzero monotonic head revision, immutable inherited frontier, current
  permanent accepted frontier, and a digest over those fields. The inherited frontier never
  changes; the permanent frontier never decreases or precedes it. Unknown versions, noncanonical
  ordinals, key/value disagreement, revision overflow, frontier regression, or digest disagreement
  are invalid encodings. The head stores no marker, `AssetId`, origin-span collection, transient
  reservation, or broad thread revision.
- A `draft-image-label-protection-heads` key contains exactly one thread id. Its canonical
  `DraftImageLabelProtectionHeadV1` value repeats that id and stores a nonzero monotonic revision,
  protected maximum label, and digest. Creation initializes the maximum from the thread's applicable
  inherited/permanent accepted authority. Allocation commit may increase it; no operation may
  decrease it. Key/value disagreement, zero revision, noncanonical ordinal, revision overflow,
  regression, or digest disagreement is invalid encoding. Thread creation commits this head before
  ordinary readiness can be admitted; readiness never synthesizes a missing head.
- `draft-marker-label-admission-capacity` has one singleton key. Its canonical value stores a
  monotonic revision, the count of all retained admission heads, total retained associations, exact
  total encoded bytes charged by every admission head/tree/replay receipt/terminal cleanup residue,
  the profile limits, and a digest. Every creation, successor, cleanup step, settlement transfer,
  and final removal atomically replaces the operation head and this aggregate value by subtracting
  the exact prior charge and adding the exact successor charge. The production maxima are 64
  retained heads, 65,536 retained associations, and 67,108,864 retained encoded bytes across the
  whole home, including state left by prior process generations. A completely empty V7 admission
  subsystem canonically represents zero by absence of the singleton together with empty head, node,
  and receipt families; first admission atomically creates the zero-to-first-successor singleton,
  and it remains present after later return to zero. An absent singleton beside any admission
  record, arithmetic overflow, aggregate disagreement, or a recorded charge above any limit is
  invalid.
- The isolated marker-label-readiness operation profile admits at most 65,536 retained association
  charges and 67,108,864 exact encoded bytes for that operation across both trees, replay records,
  and cleanup residue under the same accounting rules. Exceeding either own-operation ceiling is
  `OperationTooLarge` even in an otherwise empty home. If that operation fits but the shared
  64-head, aggregate-association, aggregate-byte, or runtime-slot capacity does not, admission is
  `CapacityUnavailable`; actual storage failure remains a storage error. These are semantic
  classifications over the existing constants and encoding, not a promise of 65,536 user markers.
  Checks apply at each bounded staging quantum before candidate adoption; no up-front reservation
  of all future work is promised. Refusal preserves the prior candidate and exact cleanup or
  reconciliation custody.
- `draft-marker-label-admission-heads` keys canonically encode exact draft, editor session, and
  operation identity. Values repeat that owner and commit package-owned request/proof-custody
  authority, lifecycle, ingestion frontier, optional head-selected replay-receipt reference while
  readiness is active, source-order and
  target-id root identities/heights/digests/counts, occurrence commitment, unassigned,
  total-occurrence, and allocating-occurrence counts,
  assignment continuation, remaining builder count, exact retained-association and encoded-byte
  charges and limits, terminal cleanup cursor, and their digest. Each canonical empty root contains
  no node and has count zero.
- `draft-marker-label-admission-nodes` keys add a closed internal-or-leaf tag and operation-local
  opaque record identity to that complete owner. Source-order leaves are keyed by `(assignment
  group, target marker id)` and retain complete validated source-selector/evidence bytes and exact
  `AssetId`; target-id leaves are keyed by target marker id and
  retain admitted page identity, complete validated source-selector/evidence bytes, assignment group,
  and exact `AssetId`, plus either an unassigned disposition or the assigned final label. Those
  occurrence bytes are point-compared for head-selected byte-exact page replay and remain until
  builder consumption.
  Assignment groups are a closed tag followed by its exact payload: `PreserveLabel = 0` and
  nonzero label as unsigned big-endian `u64`; `AllocateLabel = 1`, exact 16-byte source thread id,
  and nonzero label as unsigned big-endian `u64`; `FreshAsset = 2`, asset version byte, 32-byte
  digest, and nonzero length as unsigned big-endian `u64`. Source keys append the 16-byte target
  marker id and order lexicographically by these bytes. A fresh selector encodes only the complete
  AssetId and has no label sentinel. Source/target evidence and the private replay closure bind
  the derived group. Assignment continuation retains the optional prior complete group, AssetId,
  assigned final label, and checked allocation cursor. The head's allocating-occurrence count
  increases only for AllocateLabel and FreshAsset insertions, is fixed at evidence EOF, and bounds
  the reserved range; zero allocating occurrences require no range. After EOF this historical count
  is bounded by the frozen occurrence count, not by the shrinking source or remaining-target count.
  Durable fresh evidence is the exact 42-byte tag-2 little-endian correlation entry below; the
  FreshAsset group's length remains big-endian. Process-only selector discriminants are not V7 tags.
  Decode rejects unknown groups,
  selector/group disagreement, impossible counts, and an assignment incompatible with its group.
  Internal nodes store bounded ordered child identities, digests, checked counts, and disjoint
  tree-specific key envelopes. Both trees have fanout 128 and maximum height 64. Values repeat owner,
  tree/tag, and record identity; unknown tags, wrong owners, malformed envelopes, inconsistent
  counts, over-height trees, or digest disagreement are invalid.
- `draft-marker-label-admission-receipts` keys add the head-selected command identity to the
  complete operation owner. The sole live replay receipt binds the exact durable command/page
  identity, canonical request commitment, byte-equal source and target heads and roots, bounded
  immediate-predecessor path closure retained for target reproduction, lifecycle/custody
  transition, and digest. Each successful successor atomically removes the prior receipt and any
  prior replay-only paths, retains only the newly superseded paths required by its own receipt, and
  selects that receipt from the current head. Digest equality never replaces canonical byte
  comparison. An inert terminal head instead owns a bounded cleanup cursor and retains only one
  compact terminal receipt after operation nodes and replay paths are reclaimed across restart.
- A thread image-label origin span is immutable and maps one admission's monotonic frontier advance
  to its exact admitted owner and compact sealed asset-set proof. A child records its parent's
  current permanent frontier as its immutable inherited frontier and copies no spans. Label lookup finds the
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
  until explicit garbage collection.
- An immutable projection or resource record may also remain unreferenced after an interrupted
  derived write. Explicit scrub or background garbage-collection analysis treats that exact primary record as an unreachable garbage-collection
  candidate, not visible membership. Any reachable membership, set, head, transcript entry, or
  context envelope still requires its complete exact reverse agreement.

## Canonical Admission Deletion Bounds

Admission trees retain the existing node and receipt encodings. Leaves have height one; every
nonroot internal node has 2 through 128 children, and an existing selected internal root may have
1 through 128. New deletion roots collapse as specified by
[canonical admission deletion](design-draft-storage.md#canonical-admission-deletion).
The production profile bounds each tree's leaf count by 65,536. Thus valid admitted roots have
height at most 18, including the one permitted selected unary root. The family height ceiling
remains 64; impossible profile count/height combinations and malformed referenced closures fail
closed before exhausting the command allowance.

A deletion acquires at most 18 descent records and 16 sibling records, and emits at most 18
canonical internal records. Assignment's target replacement additionally acquires and emits at
most 18 records. Its exact retained predecessor closure therefore has at most 52 descriptors,
within the existing receipt vector limit of 128. That list includes superseded siblings required
for reconstruction, in the order defined by the draft-storage contract; it is not merely a path.
No codec version, persisted field or digest domain changes for canonical admission deletion.

For path-node fanouts `c_l` at heights `l = 2..H`, the minimum leaf population is
`1 + sum((c_l - 1) * 2^(l - 2)) <= 65,536`. With the occupancy rules above, the total path
child-reference count is at most 1,155. Disjoint sibling vectors contain at most 1,152 child
references. Deletion therefore acquires at most 2,307 internal child references; its surviving
replacement region emits no more references than that acquired region. These bounds use the
admitted population, not independent maximum fanout at every height.

Including the family key, an admission internal record has 169 fixed bytes plus its children;
a source-order child uses at most 222 bytes and a target-id child 138. The admission-node family
maximum is 65,601 bytes including its key. Source deletion acquisition is bounded by
`2,307 * 222 + 33 * 169 + 65,601 = 583,332` bytes. A target replacement path is bounded by
`1,155 * 138 + 17 * 169 + 65,601 = 227,864`. Their combined current or retained predecessor
closure is at most 811,196 bytes; new source and target records together occupy at most 743,060.
The special collapse to a surviving leaf remains below these bounds because its input population
is correspondingly small.

The conservative complete assignment tree inventory charges 811,196 bytes each for current
acquisition, previous retained-closure acquisition and deletion of that previous closure under
the existing full-record deletion convention, plus 743,060 emitted bytes and at most 2,340
new-node absence-key bytes. This totals 3,178,988 bytes. Controls add eleven conservative 65,601-byte
allowances: six session/label/protection observations across serialized assignment and final
readiness, and five receipt allowances for prior read, new emission, prior deletion, new occupancy
probe and postcommit read. Four head allowances cover public preparation, callback read, emission
and postcommit read; two capacity allowances cover callback read and emission. A canonical
admission head uses at most 756 bytes including its 48-byte key; capacity uses 89 including its
singleton key. Complete control charge is therefore `11 * 65,601 + 4 * 756 + 2 * 89 = 724,813`.

One complete assignment invocation admits at most 153 point attempts and 140 stored node
acquisitions plus emissions. Its conservative encoded charge is 3,903,801 bytes, and its peak with
one additional family reservation is 3,969,402, within the existing 4,194,304-byte command ceiling.
The allowance spans all the observations and effects above; a helper cannot obtain a fresh budget.
Oversize or malformed work rejects before the next acquisition or emission, without partial
authority. Diagnostics expose actual attempts, stored acquisitions/emissions, encoded charges and
peak reservation so verification can distinguish pre-reserved work from a late result-size check.

Selected-target verification is mutually exclusive with fresh assignment. Its conservative tree
inventory authenticates at most 52 retained predecessor records, reconstructs at most 36 emitted
nodes, byte-checks at most 36 selected put records, and probes at most 52 captured deletion keys.
It does not additionally reacquire prior cleanup records or test selected puts as fresh absent
targets. These are at most 140 node point attempts plus 13 control attempts, and at most 124 stored
node acquisitions/emissions. Even retaining the full-record deletion-charge convention, its tree
charge is at most 3,111,892 bytes; with the same controls it is 3,836,705, with reservation peak
3,902,306. These fit the complete assignment ceilings above. Captured command authority supplies
the deleted keys; receipt-local bytes alone do not establish a stateless replay branch. HomeStore
retains its existing engine-owned reconciliation quota; any Syndic-selected verification uses the
shared Syndic allowance and cannot borrow that engine quota.

Normalized target-id deletion acquires at most 389,544 bytes and emits at most 321,408. Its complete
primitive costs at most 1,101,666 bytes under full-record deletion charging, or 714,332 when a
composing acquisition/emission ledger charges the actual deletion keys, including all superseded
keys and put-absence probes. Retained-storage accounting remains exact under either convention.
The composing build command must additionally charge its controls and other effects under the
existing draft-piece command ceiling; this primitive bound does not admit three draft-tree
mutations and admission consumption in one quantum.

### Admission Receipt Transition Metadata

The following specifies the existing producer bytes inside admission receipt source/target
metadata; it does not change a family version or add a field. Ingestion metadata begins with the
504-byte canonical request-authority prefix, bound by the receipt's request commitment. Its order
is home generation; thread id; label revision, inherited and permanent frontiers and digest;
protection revision, maximum and digest; draft and session ids; session and candidate generations;
the existing fixed 327-byte candidate-root reference; and the disposition tag. Integer fields
are little-endian `u64`, identities are their exact bytes, and digests occupy 32 bytes.

Both source and target metadata append the same 81-byte page header: draft/session/operation
identities (48 bytes), page identity (16), little-endian ordinal (8), canonical EOF boolean (1),
and little-endian entry count (8). Source entries encode assignment group then evidence; target
entries encode the same group, target marker id (16 bytes), then the same evidence. Groups retain
their existing 9/25/42-byte forms. Evidence retains the existing candidate/cut/accepted/fresh
correlation forms of 434/450/194/42 bytes under the
[readiness byte contract](#draft-marker-label-readiness-byte-contract). Validate the complete
source/target sequences, exact framing and consumption, canonical tags/counts, request prefix,
owner, page, ordinal and EOF. Do not infer an insertion key from arbitrary opaque byte offsets.

An ingestion receipt selected at entry to assignment is the final EOF entry. For nonempty input,
its selected association index is `entry_count - 1`; that entry supplies exact group, target id,
AssetId and evidence for canonical source/target insertion. A zero-entry EOF requires identical
before/after roots and no retained predecessors. Verification uses exact retained internal paths
and referenced leaf descriptors, simulates canonical insertion/splits and deterministic identity
order, and requires both complete after-root descriptors plus source-then-target predecessor
membership to agree. No new node acquisition is needed beyond the retained closure and current
roots already required by the assignment invocation.

For nonempty prior assignment, source metadata is the old head digest (32 bytes), canonical
assignment group (9/25/42), AssetId digest (32), and little-endian nonzero asset length (8).
Target metadata is exactly 48 bytes: target-before root digest (32), its little-endian count (8),
and assigned nonzero label (8). The retained source leaf supplies the source key, group, evidence
and asset; the retained unassigned target leaf must agree. Derive the association index as
`target_before.count - source_before.count` with checked arithmetic and use the receipt command
with assignment page ordinal one. Stream canonical normalized deletion and target replacement,
including exact sibling order and root collapse, and require both after-roots and the complete
retained list to agree. The historical head-digest prefix is not recoverable command custody.

Transition verification retains only bounded scratch and expected child/root summaries. Hashing
already charged inputs for integrity creates neither physical acquisitions nor emitted records;
it must not invoke an emitting builder or fresh-key probe. Required reused-root checks share the
mandatory current-root cache. The complete assignment inventory above remains unchanged.
Canonical-byte replay and captured deletion-absence checks retain their separate requirements.

## V7 Bounds And Canonical Encoding

- Persisted integer ordering uses unsigned big-endian encoding. Composite index keys order first by their owning identity and then by one-based ordinal or revision. Cursor-only lower or upper sentinels are rejected as stored keys.
- Stable Beryl and Syndic identities use their exact 16-byte payloads. Digests use exact 32-byte values. External CAS identities retain validated UTF-8 and remain bounded by `beryl-model`.
- One terminal-repair build admits at most 262,144 ordered items, 268,435,456 exact encoded item/
  content/media bytes, and 65,536 staged pages across all three staging families. Each page admits at
  most 256 entries and 65,536 encoded bytes. These are hard V7 codec and mutation ceilings, not
  caller-selected budgets: exceeding any count, byte, field, or page limit rejects the repair
  without truncation or partial publication. Normal live-capture terminal-audit page, resident-item,
  and already-admitted-source limits do not lower this independent repair ceiling.
- Repair staging admits one bounded page per short command and advances checked cumulative item,
  byte, media, and page totals plus domain-separated chain digests in the build head and immutable
  family commitments. The final atomic seal selects those already staged paged commitments by
  reading only the compact sealed head, fixed family commitments, gate, and required publication
  witnesses; it never materializes the snapshot, restages page payloads, or walks the page set while
  holding the writer. The Asset witness binds its sealed paged set and compact visibility-selector
  transition; neither participant opens sidecars or copies all metadata/references. Final command
  reads, writes, retained state, and reconciliation are bounded independently of media/item count.
  Fresh recovery may seal or select only a fully staged candidate proved by its compact frontiers;
  it does not rebuild missing stages or reread admitted sidecar bytes.
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
  whole-submitted-input, or whole-provider-item byte ceiling is encoded in V7.
- Every image-label-authority or draft-label-protection head, draft combined-root, sequence
  node/leaf, tagged marker-identity-
  index internal/leaf, tagged
  marker-order-commitment internal/leaf, admission capacity/head/node/replay receipt, marker-seal record, build,
  mutation-staging head, canonical staging page, immutable staging-progress receipt, canonical
  fragment, immutable build-progress receipt, settlement, candidate-session head or
  receipt, and materialization record fits the 65,536-byte value ceiling. Internal
  nodes in all three structures have from 2 through 128 children, except that a selected root node may
  have from 1 through 128; every leaf in one nonempty structure has the same depth, and each height
  is at most 64. An identity leaf contains exactly one stable marker id, final label, `AssetId`,
  same-anchor order key, and sequence marker-leaf identity/digest, with no absolute anchor or
  position. A commitment leaf contains exactly one stable marker id, final label, and `AssetId`,
  with no text position or order key. The canonical empty combined root has none of the three nodes, all heights and every
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
- Batch preparation authenticates the exact source head, selected receipt, and matching session
  custody, derives one bounded consecutive page/receipt batch and final endpoints, and retains no
  operation-wide collection. One atomic Syndic contribution publishes the complete batch and final
  endpoints or none of them. Checked record, encoded-key, and encoded-value ceilings are enforced
  before publication; no prefix, intermediate head, or intermediate session endpoint can become
  durable after success, failure, cancellation, crash, or a persistence cut.
- Source and proposal lane ordinals, item totals, canonical-byte totals, batch page totals, and
  aggregate encoded bytes use checked arithmetic. There is no smaller cumulative operation cap
  below any representable checked-`u64` lane total and no operation-wide 256/257 limit; the separate
  marker-label-readiness operation profile still applies when required. Preparation
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
  payload bytes. One path-copy command reads or emits at most 256 stored records across all three
  structures. This counts acquired existing structure records and emitted structure records
  together; an absent point result is not a stored record. Independently, the complete Syndic
  command admits at most 512 point-read attempts, including absent and repeated reads, and at most
  4,194,304 encoded key-plus-value bytes across acquisition and emission, including control records,
  mutable submission fences, and absence-probe keys. Reads served from the one acquired immutable
  cache are not new acquisitions. Every actual repeated acquisition is charged again. Reserve the
  canonical family maximum before acquisition, then release unused allowance after bounded decode;
  reject before work exceeding any ceiling. Emissions reserve their exact encoded size before
  retaining or submitting them. These ceilings cover preparation and submission together, with no
  fresh helper allowance. Larger replacements and tree repairs continue through
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
  or a special cumulative 256/257 boundary, subject to the separate marker-label-readiness operation
  profile when required. One widget-page payload is released only after its
  complete physical-page batch, every target receipt, final head, final session custody, and
  cumulative identity are durable or exact target reconciliation proves that closure. Later build
  and reconciliation read those staged records in bounded pages rather than asking the caller to
  retain or resupply them.
- Mutable and live continuation state remains `O(1)` in marker count: one fixed scan frontier,
  completed count and chain, one optional fixed-size active effect, and fixed structure cursors.
  Immutable staged fragment records and unreachable working/final tree records may scale through
  cursor pages with the logical edit.
- Independently, one post-finish staging-window command consumes at most 256 consecutive physical
  staging pages and therefore at most 256 one-item page records. One operation-local budget charges
  every read and encoded value across the complete nested authentication path and exposes the
  actual charged totals. Its bounded closure contains the candidate-session head,
  staging head, finished staging receipt, build head, selected build receipt, and immediate
  predecessor receipt, plus one fragment endpoint and the working sequence, identity-index, and
  marker-order-commitment roots for each selected and predecessor receipt. The budget fails before a
  read that would exceed its declared limits; no nested authentication read may bypass it. The
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
- The process-local staged-build outcome verifier admits at most 128 Syndic point-read attempts
  and 8,388,608 charged encoded-value bytes per classification. It reserves 65,536 bytes before each
  attempt, including absent results and repeated reads, and rejects before exceeding either cap.
  The public diagnostics expose attempted reads and charged byte allowance; this is an acquisition
  allowance, not retained memory. Every nested referenced-closure read uses that same reader.
  HomeStore separately owns exact classification of reserved changed effects, which Syndic does not
  scan again. A known package-owned commit uses its captured serialized result without this verifier.
  The conservative referenced-closure bound is 124 reads: 14 first/last observations of staging,
  build, candidate-session, admission, capacity, protection and live-history anchors; 19 selected
  and immediate-predecessor build receipt, endpoint, root, active-effect and scan references; two
  staging receipts; 16 combined-root/structure-root, history-frontier, history-transition and
  settlement references; three admission-root/terminal-receipt references; three split-publication
  occupancy checks; three history boundary references; and at most 64 history-floor selection reads.
  It verifies only the exact selected side against captured command authority. It does not rebuild
  history ancestry witnesses, invoke general frontier authentication, or authenticate two alternative
  endpoints as simultaneously current. The existing staging acquisition and structure-command
  bounds remain separate and unchanged; this outcome flight adds no persisted record format.
- Edit-history transition, frontier, stack-link, and historical-root-adoption values each fit the
  65,536-byte value ceiling and contain only compact roots, positions, links, counters, policy, and
  replay facts. Each transition carries exactly one fixed 64-slot authenticated ancestor array and
  bitmap sufficient for every checked-`u64` journal depth. Append derives that witness from the
  authenticated predecessor with bounded logarithmic work and fixed retained state. The configured
  durable history byte budget is a nonzero checked `u64`, not an entry count and not a function of
  document length. Within those families its charged set is only the
  current mutable live-frontier record and the retained transition/link records; publication or
  baseline snapshots and historical-root-adoption or other operation receipts are excluded. Each
  charged record includes exact family-key bytes plus canonical value bytes, repeated keys inside
  values count again, and Fjall and allocator metadata do not count. Checked successor accounting
  derives the exact required eviction amount and cumulative threshold. Selection follows only the
  authenticated same-lineage ancestry with bounded logarithmic work and fixed state, then validates
  floor/head references, accounting, pins, and availability. No draft-global seek result or valid sibling
  digest establishes membership, failure, or a cutoff. Adoption atomically removes only the selected
  prefix from logical availability without reading or copying content and without physically
  deleting any transition, link, root, node, leaf, or content. A typed history-capacity-unavailable
  result occurs only when the required non-evictable closure itself cannot fit.
- Draft text, marker, composite-piece, and materialization input pages return at most 256 records
  and 65,536 payload bytes. Lookups and exact adjacent-gap validation follow bounded authenticated
  tree paths. A marker page begins from one proven composite search key; more than 256 same-anchor
  markers returns an authenticated `(anchor, order key, marker identity)` cursor rather than
  increasing residency.
- A draft text demand has a byte ceiling from 4 through 65,536 inclusive. A marker-page demand has
  an object ceiling from 1 through 256 inclusive and a retained canonical response-byte ceiling from
  1 through 65,536 inclusive; marker identities, labels, cursors, and preceding/following facts all
  count toward that retained-byte ceiling. Validation windows, marker-edge proofs, and each
  first/last/adjacent-marker proof use those same fixed bounds. The package must reject an
  unrepresentable request rather than widen a page, retain an entire anchor run, or scan a whole
  draft or marker set.
- One marker-id lookup authenticates stable occurrence facts through a bounded identity-index path.
  Location validation additionally requires a caller-supplied composite-position or anchor witness;
  ID-only lookup does not discover location. No uniqueness operation scans the complete sequence
  tree or marker set.
- `draft-marker-identity-index` keys canonically encode the owning draft, closed internal-or-leaf
  tag, and exact opaque record identity. Values repeat owner, tag, identity, and digest. Internal
  child entries are strictly stable-id-envelope ordered and count-prefixed; leaf values contain one
  canonical stable-id mapping. Unknown tags, key/value disagreement, duplicate or overlapping
  envelopes, noncanonical counts, trailing bytes, or a mapping outside its ancestor envelopes are
  invalid encodings.
- `draft-marker-order-commitments` keys canonically encode the owning draft, closed internal-or-leaf
  tag, and opaque record identity. Values repeat owner, tag, identity, structural digest, checked
  count, and optional maximum label; leaves contain one stable marker id, final label, and complete
  `AssetId`. The sequence and marker-identity leaves commit the same association so edits, removal,
  movement, same-id replacement, location proofs, and replay cannot disagree about asset identity.
  `draft-marker-seals` keys and values canonically bind the exact captured root/build identity,
  commitment, operation, cursor/frontier, sequential state, ordered marker/asset state, and
  lifecycle. Unknown tags,
  noncanonical cursors or maxima, key/value disagreement, trailing bytes, or a seal selecting a
  different root or commitment are invalid encodings.
- Every V7 exact-replay classification at an occupied natural key requires equality between the
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
  logical draft, same-anchor marker set, or sealed Composer value. Marker-label readiness has its
  separately declared operation limit; other edits have no smaller whole-operation cap. Progress-transition
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
- Idle submission and accepted-input promotion expose checked package-owned maximum record and
  canonical encoded-byte footprints. An unrepresentable total is a package contract failure; it is
  never saturated, wrapped, or replaced by a caller estimate.
- Home-store page, item, stored-byte, and decoded-byte limit failures remain typed read failures and
  do not imply durable corruption. Mutation preparation and validation perform only operation-
  bounded reads and never wait for resource capacity while holding the serialized writer.

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
  validates each thread's independently revisioned image-label-authority head and immutable local
  origin spans against parent-frontier inheritance, contiguous permanent-frontier advances, and exact admitted
  sealed-set proofs. It scans indexes in bounded pages and never constructs a per-thread used-label
  set.
- The same explicit maintenance validates each draft-label protection head for monotonic agreement
  with accepted authority and committed allocation transitions. It validates the singleton
  admission-capacity totals against all admission heads, head-selected replay receipts, and
  reachable B-tree nodes through bounded pages and authenticated descents;
  ordinary readiness, build, replay, reconciliation, and cleanup never scan a whole draft or all
  operation records.
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
## Draft-Marker Label-Readiness Byte Contract

The label-readiness page protocol is a cross-domain byte contract and is an intentional exception to
the stored-key integer order above. Its HomeStore fixed-32-byte-digest protocol id is
`0x53444d5244595631` and its operation id is `0x5244595041474531`. The SHA-256 correlation domain is
the exact ASCII string `syndic/draft-marker-label-readiness-page/v1`.

The digest preimage is the domain bytes followed directly by little-endian `u64` page ordinal, one
`0` or `1` EOF byte, little-endian `u64` entry count, and the ordered fixed-width entries for the
page's one homogeneous proof shape:

- Candidate/cut-only entries are tag `0`, complete candidate/cut root and marker selector, label,
  and complete asset identity.
- Accepted-only entries are tag `1`, sealed-set id, sequential digest/count/maximum, ordered-asset
  digest/count, entry frontier, asset-chain digest, label, and complete asset identity.
- Fresh-only entries are tag `2`, asset version byte, 32-byte digest, and nonzero length as
  little-endian `u64`, exactly 42 raw entry bytes. They carry no source label, sealed-set proof,
  destination marker identity, or assignment group. Canonical fresh page order is lexicographic
  over complete raw entries, retaining every repeated occurrence in the count and digest.

Optional maximum is zero for absent and its nonzero `u64` otherwise. Asset identity is its version
byte, digest, and nonzero length. Every integer in this correlation preimage is little-endian.
The evidence-byte ceiling counts entry bytes only, excluding the domain, page header, and HomeStore
framing. These correlation integers must not be normalized to the unsigned big-endian encoding used
by persisted ordered keys.

An empty EOF page has EOF byte `1`, count zero, and no entry bytes or witness. Its ordinal is the
exact next ingestion ordinal, initially one or the successor of the last accepted page. It uses
source-only proof composition and the same domain/header digest. The private request and durable
receipt still bind the exact operation, unchanged disposition, ingestion frontier, and complete
source/target root closure. It freezes the existing historical counts without adding associations.
Empty non-EOF and input after the selected EOF are invalid; exact immediate replay remains subject
to the ordinary byte-equal receipt rules.
