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

Candidate provenance validates one exact root, history frontier, and candidate generation. An
unchanged opening candidate derives its authority from the session's opening durable checkpoint:
its root remains the opening root, and its live history is the exact fork of the opening durable
history for that session. Opening inherits the durable generation, which need not be zero. Later
adoptions require the exact committed ordinary-edit or historical-adoption evidence for their
checkpoint. These evidence rules are shared by preparation reads and transactional validation;
valid provenance alone does not establish publication eligibility.

Syndic authenticates whether one exact live candidate is already represented by the current durable
selector/root/history checkpoint. Exact published/newest equality or the unchanged opening's exact
session-fork correspondence may establish that relationship. Opening correspondence requires the
immutable opening authority and unchanged candidate identity at any inherited generation; equal
roots or generations, or absence of a journal, do not suffice. It neither publishes the private
fork nor makes later adopted candidates durable. Callers retain the live candidate identity and
durable checkpoint separately, and stale selector or candidate authority cannot authorize use of
another checkpoint.

Ordinary final disposal may normalize an authenticated unchanged opening's newest history to its
published history in the same atomic command as the exact disposal receipt. It requires current
durable correspondence and no operation custody, preserves the current draft and its selector, and
leaves the disposed published/newest pairs byte-equal. Receipt replay and reconciliation cover the
complete source and normalized terminal session state; substituted forks and later adopted dirty
states are rejected. This is normal disposal, independent of unpublished-target abandonment.
An exact disposal receipt authenticates its recorded source and terminal session history after a
later draft replaces the disposed editor's draft. Mutable current-selector eligibility is checked
at admission and is not a continuing prerequisite for validating that committed historical receipt.

Captured publication validates the captured checkpoint even when the live session has advanced.
Current-session eligibility, custody, and publication preconditions remain separate checks; proving
the current head does not prove an older captured candidate. Publication moves the captured root
and matching history together and cannot mark a newer candidate published merely because the
captured operation completed.

## Durable Mutation Staging

### Marker Readiness Inputs

Syndic accepts candidate, cut, accepted-origin, and fresh ordinary AssetId selectors through its
bounded opaque label-readiness lifecycle. Fresh selectors contain no label. Candidate/cut pages
use source-only proof composition; accepted and fresh pages each use their own homogeneous Asset
witness factory shape through the generic HomeStore boundary. Fresh witnesses receive only bounded
AssetIds; accepted witnesses retain their sealed-proof/label/AssetId inputs. The package has no
production dependency on Beryl-state and publishes no dependency-private proof facts to the app.

Syndic derives preservation versus allocation per authenticated occurrence under the
[Syndic system contract](../../../doc/systems/syndic-conversation-history/design.md). Reuse-only
disposition rejects allocating sources; allocation-permitted disposition supports mixed edits
without relabeling destination occurrences. Dedicated bounded source-order and target-id trees
retain exact derived groups, evidence, targets, and assignment progress. Fresh occurrences group
only by complete AssetId within that operation; all final labels are allocated inside Syndic.
The [V7 byte contract](design-schema-v7.md#draft-marker-label-readiness-byte-contract) owns page
correlation bytes; its admission-family contract owns durable group encoding and replay closure.

Exact evidence EOF precedes bounded assignment and final proof issuance. A stream may close
nonempty evidence through a separate source-only empty EOF page at the exact next
ingestion ordinal, under the captured reuse-only or allocation-permitted disposition. It preserves
all association counts and exact roots and selects assignment through the ordinary durable receipt;
empty non-EOF, changed disposition, skipped ordinal, post-EOF input, and conflicting replay reject.
The final move-only proof enters storage MutationBegin custody; no public label, digest, group,
or binding substitutes for it.
The builder point-consumes assigned targets and verifies complete effect closure before adoption.
While the exact candidate session still owns Staging custody, a bounded target resolver validates
the transferred admission head, staging/readiness binding, captured predecessor and generation,
current target root, and assigned leaf for one marker identity and AssetId. It returns one typed
`DraftPieceMarkerV1` carrying the Syndic-assigned label and requested order key for unchanged
transport into proposal staging. It never consumes the entry, changes a counter, returns a proof,
or scans the target tree. Once finish transfers custody to Building, resolution is unavailable;
the builder remains the sole consumer and validates all exact marker facts at consumption.
Missing metadata, substituted evidence, stale generation, exhaustion, cancellation, and capacity
refusal preserve prior candidate/history and exact typed cleanup or reconciliation custody.

Readiness and assignment distinguish isolated `OperationTooLarge`, aggregate or runtime
`CapacityUnavailable`, and actual storage errors at their public boundaries. Proven noncommit and
`ExactOld` retain the original storage failure even when local cleanup is unavailable; reporting
the failure does not establish cleanup success. A committed result whose local authority is
unavailable retains its exact receipt, later storage failure, and typed unavailability reason. It
does not authorize a readiness proof or reclassify the durable target as refused.

If assignment commits but the readiness read fails, its move-only retry flight retains the receipt
and exclusive exact assignment attempt. Retrying reads without another mutation and may issue a
proof only for that attempt's selected command. For an Assigning head, competing assignment
preparation remains unavailable for capacity until the flight resolves or releases its exact
attempt; releasing the flight does not delete durable custody and permits owner-qualified
cancellation or recovery cleanup.

### Canonical Admission Deletion

Source-order assignment and target-id builder consumption use the same canonical admission-tree
deletion. Assignment removes one source leaf and replaces the matching target leaf's disposition;
builder consumption removes one assigned target leaf. Neither operation changes an unrelated
occurrence, owner, evidence payload or assignment group. The exact successor counts, envelopes,
digests, retained charge, head and capacity transition publish atomically under existing custody.

Deletion returns an empty subtree, a canonical subtree with its actual height, or a transient
underfull internal child vector with its unchanged height. An underfull nonroot uses its left
sibling when one exists, otherwise its right sibling. Before using that sibling, authenticate its
exact parent-bound identity, owner, tree, height, complete summary and canonical occupancy. Combine
the child vectors in their existing key order. Merge when their combined fanout fits 128;
otherwise split at the midpoint into 64 and 65 children. Propagate any resulting parent
underflow before emission. Never persist a transient unary nonroot or retain a fictitious height.

Collapse a singleton selected root to its child, repeating while the selected child is internal
and unary. A surviving leaf is selected directly at height one; an empty root has height zero.
Previously valid selected unary roots remain readable, but their children must satisfy nonroot
occupancy. Deletion emits no unnecessary unary root. Existing invalid nonroot state is corruption;
ordinary deletion does not repair or migrate it.
Only transient unary levels produced by this deletion may collapse beneath the selected root;
a stored nonroot unary node rejects even when collapsing it could hide the invalidity.

Assign deterministic operation-local identities only to final surviving emitted nodes, in bottom-up
repair order and left-to-right within one level. The retained predecessor list contains the
original descent in root-to-leaf order, followed by superseded siblings in bottom-up repair order.
Assignment appends the target-replacement descent after that source list. Every superseded record
appears exactly once; reused children and surviving subtrees never enter this deletion set.

Assignment retains that complete predecessor closure for its selected receipt's exact
reconstruction. Its successor atomically reclaims the previous receipt and previous replay-only
records. Builder consumption instead deletes the complete superseded set with the target/head/
capacity update and ordinary build receipt. Bounded index reconstruction authenticates the exact
retained membership and reproduces deterministic roots, keys and bytes for the selected tree
transition. Whole-command acknowledgement-loss classification instead retains the existing
move-only captured command and HomeStore reconciliation authority. That authority proves the
complete reserved changed closure, including prior receipt and superseded-node deletion absence.
The selected receipt's compact source-head bytes cannot reconstruct deleted cleanup keys or mint
replacement command custody. A repeated assignment request with the same command identity remains
rejected; it is not a stateless command-replay entry point.

Selected-target verification uses captured deletion keys, byte-compares selected put records, and
does not also acquire deleted predecessor cleanup records or probe those puts as fresh absent
targets. Builder replay likewise uses the captured deletion result and ordinary build outcome
authority; it never walks a deleted predecessor admission path. Missing or substituted siblings,
duplicate or unrelated retained entries, cross-owner references and byte-different occupancy fail
closed. Neither reconstruction, reconciliation nor cancellation scans earlier receipt history.

Before reclaiming a selected ingestion or assignment receipt's predecessors, authenticate the
complete transition from its before-roots to the head-selected after-roots. A self-consistent
receipt digest, individually valid descriptors, or membership in an untrusted before-root alone
does not prove that those records were superseded. Decode the existing
[transition metadata](design-schema-v7.md#admission-receipt-transition-metadata), derive the exact
association and deterministic identity sequence, and verify both resulting root descriptors and
the entire ordered retained list. Empty EOF or empty final assignment requires unchanged roots
and an empty retained list.

This structural-integrity check streams expected summaries and hashes through bounded scratch
over already acquired receipt bytes, retained paths and repair siblings. It creates no put
records, fresh-key probes or independently retained tree graph. A reused surviving child/root
uses the current-root acquisition already required by the same assignment, with canonical
occupancy checked before accepting collapse. Ingestion's reused leaf payload is not acquired
again: retained height-two child descriptors, or the current after-root for a prior height-one
root, provide the authenticated envelope. No extra reader or helper allowance is introduced.
These checks prove structural derivation; they do not replace captured-command canonical-byte
comparison or create replay custody from a digest.

One assignment invocation owns an acquisition/emission allowance through public preparation,
serialized assignment and its first committed readiness attempt. Reserve the canonical family
maximum before each physical point acquisition and release unused allowance only after bounded
decode. Charge each emitted record before retaining it, every absence probe, repeated acquisition
and deletion effect before its work. Authority, head, capacity, receipt and node helpers share this
allowance; immutable cache hits do not acquire again. Exact retained-storage charge is separate
from physical acquisition accounting and neither counter can reset in a nested helper. Mutable
authority observations remain actual observations. The
immutable-node cache is confined to one authenticated revision/snapshot boundary unless the
composing command supplies the existing captured-revision seal. It never supplies an earlier
head, capacity, session or authority value in place of a required later observation. The
[schema bound](design-schema-v7.md#canonical-admission-deletion-bounds) includes the public head
read, serialized session/label/protection checks, and the postcommit head/receipt/authority reads.
A later committed-readiness retry remains a bounded read-only attempt that retains the committed
result and cannot repeat assignment or tree mutation.

The bounded deletion result binds its exact source and target roots, final puts, complete
superseded records, ordered replay descriptors and retained-charge delta. It does not create a new
durable cursor or writer lifecycle. A composing builder may reuse that immutable result only
under its existing captured-revision admission and exact mutable head/capacity fences; it must not
reconstruct the same target path through an uncharged second reader. Cancellation, terminal
cleanup, ambiguous outcomes and local writer ownership retain their existing exact authorities.
The standalone deletion and complete assignment bounds do not establish the composing builder's
complete quantum bound. Marker-effect continuation must compose the sealed deletion result with
its other work under one builder ledger before that integration can be accepted.

### Staging Session

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

### Bounded Sequence Range Continuation

Ordinary replacement removal continues right to left, changing at most one text leaf per Applying
command. Planning proves that the remaining interval contains no markers. The command selects the
active marker effect's coherent working roots when present, otherwise the build's working roots.
It preserves marker roots, counts, order commitments, fragment identity, completed source and
successor frontiers, and marker scan/effect progress until that interval is empty. Each nonempty
step strictly decreases working UTF-8 bytes and publishes a canonical sequence root, its exact
remaining interval, the next immutable progress receipt, and the matching build/session endpoint
atomically. No whole-fragment split/join or retry of an unchanged over-budget frontier substitutes
for this progress.

The Applying cursor retains its fragment ordinal and fixed predecessor-source completion boundary
`base_end`. Its `successor_start` and `successor_end` are the remaining interval in the actual
working sequence, rather than immutable original replacement boundaries. Boundaries are piece rank
plus UTF-8 byte offset. For previous start `(s,a)` and end `(e,b)`, exactly these nonempty steps exist:

- With `b > 0` and `e > s`, trim `[0,b)` from text leaf `e`; preserve the start and set the end to
  `(e,0)`. Piece count is unchanged and exactly `b` bytes disappear.
- With `b > 0` and `e == s`, trim `[a,b)` in that leaf, retaining its prefix and suffix in one
  nonempty leaf; both boundaries become `(s,a)`. Piece count is unchanged and `b-a` bytes disappear.
- With `b == 0` and `e-1 > s`, remove the whole text leaf `e-1`; preserve the start and set the end
  to `(e-1,0)`. Piece count decreases by one and 1 through 32,768 bytes disappear.
- With `b == 0`, `e-1 == s`, and `a == 0`, remove that sole covered text leaf; the end becomes the
  start. Piece count decreases by one and 1 through 32,768 bytes disappear.
- With `b == 0`, `e-1 == s`, and `a > 0`, trim that leaf's suffix and normalize both boundaries to
  `(s+1,0)`. Piece count is unchanged and 1 through `32,768-a` bytes disappear.

An empty remaining interval enters Inserting at piece/byte cursor zero and advances the completed
source frontier to `base_end` exactly once. No other Applying-to-Applying cursor transition is
valid. Construction authenticates the selected leaf, both UTF-8 boundaries relevant to that leaf,
and the exact local edit. Reopen authenticates the selected and immediate predecessor receipts,
their exact operation/header/fragment and endpoint relationships, root descriptors, invariant
marker/effect facts, and the closed cursor and summary delta above. It does not repeat prior tree
surgery or rederive the original replacement across already removed leaves. Immutable receipt
publication establishes derivation; local decode and bounded referenced-closure checks retain the
package's existing fail-closed trust boundary rather than claiming protection against coordinated
replacement of every same-database authority anchor.

Sequence deletion preserves actual returned subtree heights. Underfull child vectors exist only
transiently: merge with a sibling whenever their combined fanout fits 128, otherwise redistribute
into canonical children. Every emitted nonroot internal node has 2 through 128 children. A
singleton root collapses while its child is internal; a single surviving leaf retains the existing
selected height-one root-node wrapper. Root descriptors continue to name node records, and the
schema's selected-root occupancy exception remains in force. Unchanged authenticated subtrees
remain shared. Text trimming never splits a retained leaf into two leaves.

One operation-local acquisition cache and ledger covers preparation and serialized submission.
Every Syndic-selected read, emitted record, and target-absence probe is charged before its work;
nested helpers cannot reset the allowance or invoke a broader uncharged authentication path. The
schema owns the separate structure-record, point-attempt, and aggregate encoded-byte ceilings.
The complete closure includes selected/predecessor receipts, source/working roots, referenced
fragments, session and staging custody, optional writer state, exact mutable submission fences,
target absence, and build/receipt/session effects. HomeStore's mandatory engine checks retain
their own engine quotas; Syndic work cannot be reclassified as generic engine work to evade a cap.

Preparation captures Syndic domain revision `D0`, acquires the bounded immutable closure and
target-absence witnesses, and seals them only after observing the same domain revision again and
the same originating handle/home generation. Serialized submission uses a fresh HomeStore revision
and a contribution fenced by captured `D0`; revision admission must precede callbacks reusing sealed
facts. It also checks the exact mutable source build, selected receipt, session, staging and any
writer fence using the same ledger. Any intervening admitted Syndic-domain mutation invalidates the
witness, including fault or retention mutations that advance that domain's revision. The fence
does not detect raw physical corruption after capture that bypasses revision publication; the
package's existing immutable-publication trust boundary excludes that guarantee. Fresh acquisition
still performs fail-closed codec and referenced-closure checks. Raw persisted-corruption tests prove
those fresh-read checks, not invalidation of an already captured witness. Unrelated-domain writes
need not invalidate the long preparation interval. Legacy callers may not replace captured `D0` with a fresh domain
revision. A stale prepared command remains a known noncommit; retry requires fresh construction.
Selected source and progress roots remain protected by existing operation custody. Cancellation
retains that exact custody through its terminal command and subsequent bounded cleanup.

### Staged Build Command Outcomes

Production post-finish submission uses one opaque move-only prepared command with a closed kind:
finished-staging transfer to the builder, consumption of the next durable staging window,
successor-construction advance, or terminal election. Terminal election covers ordinary settlement,
cancellation, rejection, and operational error under their existing exact evidence requirements.
Preparation binds the HomeStore generation and attachment, draft/session/operation and staging
identity, canonical header, writer owner, exact source and proposed target endpoints, and the one
bounded command closure. A package-owned submit operation consumes the prepared command and executes
its associated HomeStore command; it does not accept a caller-supplied `CommandOutcome`.

The command captures its actual authenticated result during serialized preparation and contribution,
including a dynamically selected terminal outcome, writer-consumption successor, or exact replay
target. A preflight prediction is not that result. The capture remains inaccessible as completion
authority until the command outcome proves it. A known commit may return that historical committed
endpoint without repeating a generic status query; it does not assert that the endpoint remains
current after later work. Any returned `CommittedLocalFinalization` is consumed exactly once for
this command and owner, including when subsequent reads or local availability fail. A receipt or a
later retry cannot recreate it. Authenticated nonterminal progress resolves the exact local writer
attempt once before permitting continuation.

Ambiguous submission transfers the original failure and exact HomeStore reconciliation owner into
one move-only outcome flight. Resume consumes and returns that same flight while unresolved and
re-triggers only its exact handle after a failed reconciliation attempt. HomeStore proves canonical
equality of reserved changed effects; Syndic additionally verifies the selected side's referenced
endpoint, immediate predecessor, staging, roots, active effect, session, writer and terminal/history
closure through the charged verifier in [V7 bounds](design-schema-v7.md#v7-bounds-and-canonical-encoding).
It uses the captured authenticated command history and repeats required history-floor selection,
not general ancestry reconstruction. Every nested verification read uses the same budget. No caller
fragment callback, arbitrary starting ordinal, receipt-chain walk, or consumed-prefix replay is
part of this boundary.

Proven noncommit and `ExactOld` preserve the original error and exact source ownership for explicit
retry or cancellation; resume never automatically resubmits the edit. `ExactNew` advances the exact
target once and retains the originating failure evidence. A later successor, stale generation or
endpoint, collision, or failed local finalization cannot become current progress or mint replacement
authority. Typed failures remain observable while unresolved custody is retained. Durable command
classification, transaction outcome, original or later failure, local finalization, and cleanup
progress remain separate; a cleanup failure cannot reclassify a committed edit as refused.

An authenticated committed settlement transfers to a separate cleanup state in the same outcome
flight. A later resume performs at most one bounded empty-writer reclamation command or cleanup
reconciliation trigger; it retains the settled transaction, original receipt, diagnostics and exact
cleanup handle until completion. It authenticates zero remaining targets and the existing settled
admission, capacity and label-protection closure. An authenticated noncommit settlement resolves
local writer ownership only with its exact terminal admission evidence and returns the inert
operation-owned cleanup authority. It does not start unrelated cleanup. Pending or unavailable
flights are custody states, not additional transaction outcomes. Status reads cannot substitute for
these finalization and release transitions.

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
