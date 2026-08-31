# Draft Storage

This supplement is normative only for Syndic draft, editor, piece-tree, marker, candidate,
staging, edit-history, materialization, restoration, and sealed-text storage. The package entry point
controls scope and rigor; [`design-schema-v7.md`](design-schema-v7.md) controls persisted bytes.

## Draft And Root Authority

A current-draft record binds stable draft and owning-thread identities, a selector revision, one
immutable `DraftPieceRootReferenceV1`, its matching immutable
`DraftEditHistoryFrontierReferenceV1`, one closed submission intent, and timestamps. Root and
history authority always move together. A current draft never names a mutable editor frontier.

`DraftPieceRootReferenceV1` binds the draft, one closed build identity, optional sequence,
marker-identity-index, and marker-order-commitment root identities, their complete summaries, and
one combined-root digest. `DraftLogicalExtentV1` is the checked UTF-8 byte length and logical line
count; it accompanies every public root, candidate, range, restoration, and settlement fact that
needs logical extent.

Draft leaves are either nonempty valid UTF-8 text or zero-width markers. A marker stores a stable
marker id, same-anchor order key, final label, and exact `AssetId`, never image bytes. Composite
positions order the boundary before all markers, markers by order key and stable id, and the boundary
after all markers at an absolute UTF-8 anchor. Position-bearing requests include the exact root and
a closed gap witness; storage validates UTF-8 boundaries, marker occurrence, order, and adjacency by
bounded authenticated reads.

The sequence tree, marker-identity index, and marker-order commitment are immutable authenticated
structures. Nodes have fanout at most 128 and height at most 64. Unchanged records may be shared by
multiple roots. A marker-id lookup proves stable occurrence facts but does not discover an absolute
position; position validation additionally supplies a composite-position witness. No uniqueness,
edit, restoration, or ordinary read scans the full draft or marker set.

## Root Semantics

The combined root authenticates the immutable sequence, marker-identity index, marker-order
commitment, complete summaries, and logical extent as one authority. Canonical empty roots select
no structure records; text-only roots use the empty marker authorities; and marker-bearing roots
must select agreeing sequence, identity, and order structures. Empty text has zero logical lines;
nonempty text has one more logical line than its checked newline count.

The exact natural-key layouts, empty and combined digest formulas, build-operation derivation,
canonical encodings, and decode rejection rules are owned solely by
[Draft Root Canonical Encodings](design-schema-v7.md#draft-root-canonical-encodings).
Semantic equality never substitutes for the exact canonical-byte equality required by replay.

## Editor Candidate Sessions

A `DraftEditorCandidateSessionIdV1` is a caller-owned opaque identity. The mutable session
head contains only:

- The immutable opening selector/root/history checkpoint.
- The latest published selector/root/history checkpoint.
- The newest candidate generation/root/live-history checkpoint.
- Monotonic session and dirty generations.
- Active or disposed lifecycle.
- One fixed-size optional `Staging` or `Building` custody slot.

Open, publication, and disposal receipts are immutable and point-readable. The published and newest
checkpoints may coincide but remain distinct. A root/history mismatch, generation regression,
disposed-head advance, or clean disposal with unequal published/newest checkpoints is corruption.
A fresh session forks only from the current draft's published authority; it never adopts unpublished
state from an earlier process or session.

## Durable Mutation Staging

`DraftMutationOperationIdV1` is one opaque caller-owned identity reused for one transaction's begin,
pages, finish, build, settlement, and reconciliation. Source and proposal lanes advance through
immutable bounded pages and receipts selected by one mutable staging head.

The staging head retains the canonical begin request and digest, exact predecessor
generation/root/history/extent and positions, independent source and proposal lane frontiers, the
selected progress-receipt key and digest, and one closed lifecycle:
`Receiving`, `Finished`, `Building`, `Cancelled`, `Rejected`, `Conflict`, or `Error`.
It contains no page payload, whole edit, root graph, candidate root, or current-draft authority.
Head decode is local; bounded natural-closure validation separately reads the selected receipt and
requires the complete acyclic head/receipt relationship.

The exact operation and staging identities, encoding byte, lane tags, key lengths, value fields,
digest preimages, cumulative-link formula, and receipt encoding are owned solely by
[Draft Mutation Staging Canonical Encodings](design-schema-v7.md#draft-mutation-staging-canonical-encodings).

## Candidate Builds And Settlement

An authenticated finish atomically moves session custody from `Staging` to `Building`. Builds consume
bounded source/proposal windows directly from durable staging authority; callers cannot resupply
pages, fragments, prefix proofs, or restart reconstructions.

A build belongs to one exact draft/session/operation and retains compact source and proposal
frontiers, working structure roots and summaries, fixed marker-effect state, the latest progress
receipt, optional successor root, and a closed lifecycle. It never retains a whole edit, replacement
vector, inserted payload, marker registry, or root graph. Immutable predecessor-linked progress
receipts make each bounded continuation and its complete same-command effect closure replayable.
Target occupancy while the head still selects the source is corruption, including byte-identical
occupancy.

The exact build, fragment, progress-receipt, and settlement keys, codec versions, value fields,
digest domains, and outcome-proof encodings are owned solely by
[Draft Build And Settlement Canonical Encodings](design-schema-v7.md#draft-build-and-settlement-canonical-encodings).

The immutable settlement is keyed by draft/session/operation, retains the canonical proposal identity,
exact source and terminal progress closure, and exactly one of `Committed`, `Rejected`, `Conflict`,
`Cancelled`, or `Error`. `Committed` binds the successor candidate root and matching history.
Noncommit outcomes prove absence of candidate adoption. An occupied natural key with differing bytes
returns an immutable occupied-identity noncommit proof and never selects another identity.

Replacement ranges are half-open, ordered, non-overlapping, and interpreted against one predecessor
root. Adjacent ranges are valid. Repeated empty ranges at one position require distinct closed marker
effects in canonical marker order. Moves prove one predecessor occurrence and one successor
occurrence. Final uniqueness is authenticated through bounded sequence and index paths.

## Edit History

Every current or candidate root has matching edit-history authority. Canonical-empty creation selects
canonical-empty history. Sealed imports select a fresh baseline with undo and redo unavailable.
Ordinary committed edits append one immutable transition and advance the candidate and live frontier
atomically. Undo and redo directly adopt an authenticated retained historical root and move stack
heads; they do not stream inverse bytes or reconstruct the root.

A transition binds predecessor/successor roots, positions, kind, depth, prior journal/stack links,
operation identity, cumulative position, and one fixed authenticated ancestry witness. Local decode
validates the complete canonical witness; append admission additionally derives it from the exact
authenticated predecessor. The exact transition key, bitmap, slot, digest, and frontier encodings
are owned solely by
[Draft Edit-History Canonical Encodings](design-schema-v7.md#draft-edit-history-canonical-encodings).

The frontier binds current root/generation, journal/undo/redo heads, retention floor, depths,
cumulative positions, exact retained encoded bytes, nonzero configured byte budget, policy and
frontier revisions, pins, and availability. Stored charge is the exact canonical family-key bytes
plus canonical value bytes, including repeated keys and excluding family names, HomeStore framing,
compression, cache, allocator, and filesystem estimates. All arithmetic is checked `u64`.

Retention advances by bounded logarithmic work only along the selected authenticated lineage. It
removes logical availability and pins without physically deleting roots, transitions, nodes,
leaves, or content. A capacity-unavailable result is valid only when the required non-evictable
closure cannot fit.

## Materialization, Restoration, And Text Reads

A draft-composer materialization binds one exact combined root and `ComposerV1` to one ownerless
sealed content reference. Its resumable build retains only the source root, bounded composite cursor,
output frontier, streaming encoder state, checked totals, and chain commitments. Only `Sealed`
publishes the mapping. Cancellation, failure, and explicit same-root supersession publish none.

Exact-root text and marker reads name the complete immutable root. Text demands are 4 through 65,536
bytes. Marker pages request 1 through 256 objects and 1 through 65,536 retained canonical bytes.
Draft text, marker, piece, validation, and materialization input pages contain at most 256 records and
65,536 payload bytes. Larger logical results return authenticated cursors.

Compact restoration validates the exact root, logical extent, cursor, UTF-8 boundary, marker gap,
and adjacency facts. It streams bounded pages and does not retain a draft-sized piece or marker
collection. A sealed-content text read returns at most 65,536 nonempty valid UTF-8 bytes from one
exact immutable content reference and never exposes building content or provider transport bytes.
