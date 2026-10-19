# Draft Storage

This supplement is normative only for Syndic draft, editor, piece-tree, marker, candidate,
staging, edit-history, materialization, restoration, and sealed-text storage. The package entry point
controls scope and rigor; [`design-schema-v7.md`](design-schema-v7.md) controls persisted bytes.

## Detached Immutable Draft Reads

- The package implements the typed draft contribution to
  [detached shutdown read sources](../../../doc/systems/beryl-home-storage/design.md#detached-shutdown-read-sources).
  Acquisition names one exact immutable candidate/root, after the caller proves its selected
  close-ready binding. It uses ordinary historical-root typed reads and never follows a later
  current-draft selector. The completed source retains immutable identity facts and its temporary
  backing only, with no HomeStore, service, candidate mutation or history-adoption capability.
- One opaque file contains a fixed header, contiguous UTF-8 text and ordered fixed-size marker
  records. Marker records preserve their full typed identity, anchor, order, label and AssetId.
  Checked root summary counts determine the complete reservation before acquisition. This private
  transient encoding is not part of persisted V7, and no app consumer encodes or parses it.
- Acquisition is a cancellable bounded page traversal. It verifies contiguous complete text,
  expected byte and marker counts, valid UTF-8 and marker anchors, and exact marker ordering before
  sealing. Failure or cancellation returns no readable source and releases its private backing.
  No full text, marker vector or size-dependent in-memory index is retained.
- Detached text and marker demands preserve ordinary exact-root range semantics: UTF-8 boundary
  validation, forward/backward text, inclusive ranges, exact-anchor scope, directed marker paging,
  exact cursors, same-anchor continuation and terminal edges. Fixed-record search and bounded
  file reads replace live storage access. Malformed demands, overflow, short reads and decoding
  failure are typed refusals, never empty-success or substitute provenance.
- Verify ordinary-versus-detached demand equivalence for Unicode, empty and marker-only roots,
  dense same-anchor markers, direction/cursor boundaries and beyond-residency ranges. Verify
  resource refusal, acquisition cancellation, incomplete backing rejection and independent paging
  after the originating home closes. Export and reads perform no durable mutation.

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

Open, publication, disposal, and replacement-intent cancellation receipts are immutable and point-readable. The published and newest
checkpoints may coincide but remain distinct. A root/history mismatch, generation regression,
disposed-head advance, or clean disposal with unequal published/newest checkpoints is corruption.
A fresh session forks only from the current draft's published authority; it never adopts unpublished
state from an earlier process or session.

Candidate opening also exposes a sealed single-domain current command for the same prepared
opening mutation. Following Home's current-command contract, serialized admission captures only
physical Home/domain revisions; the original prepared selector, thread/draft revisions, root,
history, session and operation identities remain exact writer-time guards. This is one execution,
without retry, source recapture or substituted authority. Ordinary caller-fenced opening remains
available with its original revision-conflict behavior. Both command shapes share opening
validation, atomic writes, cancellation and outcome reconciliation.

Candidate provenance validates one exact root, history frontier, and candidate generation. An
unchanged opening candidate derives its authority from the session's opening durable checkpoint:
its root remains the opening root, and its live history is the exact fork of the opening durable
history for that session. Opening inherits the durable generation, which need not be zero. Later
adoptions require the exact committed ordinary-edit, sealed-import selection, or historical-adoption evidence for their
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

Explicit borrowed same-home recovery candidate access also supports ordinary final-disposal
preparation, execution and exact receipt classification through fresh Syndic handles. It preserves
the same authenticated source, clean checkpoint, no-custody and revision requirements. The caller
settles any original pending disposal outcome through its retained original command handle before
preparing remaining cleanup. Already committed disposal is validation-only and is never repeated;
proven noncommit or original typed never-admitted custody permits only the same authenticated
remaining disposal intent. Foreign homes, substituted sessions/receipts, conflicting facts and
terminal uncertainty refuse cleanup or publication. Candidate access grants no editor adoption,
claim replacement or ordinary admission, and leaves V7 encodings unchanged. This supplies canonical
predecessor cleanup for [interrupted New Thread recovery](../../../doc/systems/backend-runtime/design.md#interrupted-new-thread-during-same-home-recovery).

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
Candidate/cut selectors may originate in a different live conversation. Source proof still validates
the exact latest origin candidate or committed cut successor, and derives foreign allocation from
the authenticated origin thread and label. It never accepts caller-selected treatment or fabricated
accepted provenance. Local preservation and foreign allocation share the ordinary selector shape,
source-only proof and exact operation replay; destination changes do not extend source lifetime.
Private clipboard begin accepts an opaque validation-only source-fence contribution, qualified to
the exact captured origin and destination readiness/operation. It point-validates active/latest
origin candidate authority and exact cut settlement closure in the same HomeCommand as destination
MutationBegin. It cannot alter or substitute the proof's selectors or assignments, writes no source
record and introduces no durable format field. Already committed begin replay/reconciliation uses
the exact ordinary immutable settlement; it does not rerun the historical mutable-origin gate.
The origin check is consumed inside Syndic's sole mutation participant; it is not a second
same-domain HomeCommand participant.
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

### Staging Page Outcome Custody

One prepared staging page batch has one move-only outcome flight after execution. The flight binds
the exact original Home store instance, Syndic handle/generation and prepared batch; callers cannot
substitute a page, receipt, cached source classification or discovered reconciliation handle.
Syndic owns submission: it consumes the prepared batch, constructs and executes its exact page
command, and captures only that execution's actual outcome into the flight. No public constructor
accepts an arbitrary prepared batch and unrelated outcome. Preparation or submission admission
errors return the unexecuted prepared batch; after execution, every failure preserves the flight.
Test-only execution cut points may inject faults without changing the production ownership boundary.
It retains the actual definitive failure or committed receipt, and installs an indeterminate
outcome's original reconciliation custody once while retaining that exact handle and failure.
It owns no second page payload allocation and retains at most one command outcome per batch.

Resolving borrows the flight mutably and preserves it on every error. A committed local-finalization
capability is consumed at most once; subsequent exact reads retain the original receipt and the
recorded finalization result rather than repeating local finalization. Failed reconciliation retries
only the original installed handle. ExactOld proves source expectation; ExactNew proves target
expectation; successor/collision or mismatched stored page/session closure fails closed. Resolved
expectation remains coupled to the original flight through later exact-read or authority errors.
The first local-finalization failure is reported and retained. A later resolution may independently
authenticate the original exact page/session closure and resolve that original writer's progress;
success permits target classification while preserving the earlier failure as a diagnostic. It
does not reclassify the failed capability invocation as successful or consume that capability again.

The caller releases the flight and prepared payload only after typed source/target/terminal
classification. Source classification permits retry or cancellation of that exact prepared page;
target classification applies its acknowledgement once without executing the page again.
Page outcome resolution exposes this retained-flight boundary; a destructive outcome-consuming
page resolver is not a supported alternative. Ordinary outcome-free non-writer inspection remains
read-only and cannot authorize writer progress.
Unresolved custody excludes competing page execution and cancellation election. Foreign-store or
generation rejection preserves the flight. The production and test paths use the same outcome
resolution boundary.
The caller's existing exclusive in-flight owner enforces that exclusion; shared immutable prepared
metadata does not grant authority to bypass it or to attach a different outcome to the flight.
Existing durable records, digest preimages, command budgets and public terminal
outcomes remain unchanged. Qualify original evidence and handle retention across read, reconciliation
and finalization failure, repeated resolution, cancellation and exact target adoption.

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

For an edit-successor, an authenticated finish atomically moves session custody from `Staging` to `Building`. These builds consume
bounded source/proposal windows directly from durable staging authority; callers cannot resupply
pages, fragments, prefix proofs, or restart reconstructions.

A build belongs to one exact draft/session/operation and retains compact source and proposal
frontiers, working structure roots and summaries, fixed marker-effect state, the edit's current
coordinate-map descriptor and fixed mapping continuation, the latest progress receipt, optional
successor root, and a closed lifecycle. It never retains a whole edit, replacement
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
Noncommit outcomes prove absence of candidate adoption or direct imported-draft selection. An occupied natural key with differing bytes
returns an immutable occupied-identity noncommit proof and never selects another identity.

### Sealed Input Import And Replacement Intent

Syndic exposes typed preparation, bounded advancement, final contribution, cancellation, and exact
outcome inspection for one sealed-input import. Its caller supplies an exact current draft/editor
selector and session generation, caller-owned operation identity, exact `ReplacementEditIntent`,
canonical user-input item identity/revision, sealed `ContentReference`, expected input-gate revision,
and optional sealed historical Asset-reference-set proof. These are descriptive requests until
storage authenticates their complete source. Raw text, a transcript preview, a digest match, and a
caller-created combined root are not import authority.

Preparation proves an ordinary user-authored target on the selected thread's exact current path and
current transcript entry. The entry must own the named canonical `UserInput` item and revision, its
content reference must be the exact sealed `ComposerV1` source, and image markers must agree with
the immutable submitted item's sealed Asset proof. Repair exclusion and an idle input gate with
zero live accepted work are required. The current draft is ordinary, the active editor session is
clean at its authenticated current checkpoint, its newest root contains no text or markers, and it
has no operation custody. App-owned activation, compaction, resolution, handoff, and presentation
gates remain app responsibilities; storage repeats its own exact gate and source checks at final
admission. A caller first resolves any ordinary dirty-draft flush instead of importing over it.

The import enters the existing session's `Building` custody directly, with its complete canonical
import header and first progress receipt; it creates no mutation-staging identity or edit fragment.
One bounded immutable source stream decodes the canonical format and builds a new sequence,
marker-identity index, and marker-order commitment together. Text stays UTF-8 and markers retain
their exact stable id, source order, final label and AssetId resolved through the qualified historical
set. Import never allocates labels, moves historical ownership, or rewrites the submitted source.
Each quantum appends bounded leaves and updates only bounded authenticated paths, with the existing
fanout and height limits. All three structures and their source cursor advance atomically with one
immutable progress receipt. A quantum cannot publish only the text or only one marker structure.

The source cursor and closed decoding/build phases are owned by
[Sealed Import Canonical Extension](design-schema-v7.md#sealed-import-canonical-extension).
Source reads authenticate the sealed manifest/reference, bounded chunk and piece identity, exact
encoded range, UTF-8 boundaries, marker ordinal and source summary through the existing immutable
content contracts. Credit those exact seal and read guarantees; do not add a duplicate persisted
hash engine or treat an unsealed manifest as equivalent. EOF requires the exact encoded byte, atom,
text and marker totals and sequential marker summary. Missing records, invalid framing, unavailable
assets, changed mutable source eligibility, or disagreeing summaries stop without selecting any
partial root. Work and resident memory remain bounded independently of the input's total size.

For each marker-bearing source window, Syndic prepares the import-step source contribution and
retains its exact move-only expectation/receipt consumer under the
[historical import proof flow](../../../doc/systems/image-assets/design.md#reference-and-label-evidence).
Its source role authenticates the complete header and receipt-selected encoded/piece/marker window.
Syndic consumes only its original matching same-generation proof receipt before constructing any
marker or advancing that window. Bare association
reads, copied set descriptors and caller-supplied AssetIds grant no authority. A refused proof or
unsettled progress command leaves the original cursor and root custody intact; repeated or later
pages cannot replace its evidence. Marker-free windows need only the existing authenticated Syndic
content reads. This adds no label allocation, readiness staging or writer-admission lane.
Prepared import, advance, start, cancellation and termination capabilities retain their originating
typed domain handle and exact opaque store-generation identity through command contribution.
Derivation, proof consumption and execution cannot rewrap them with a foreign or freshly opened
handle merely because numeric generations or durable records match. Read-only original-outcome
lookup instead authenticates the captured durable home identity and exact original receipt
namespace, permitting legitimate same-home recovery and later eligible edits. Indeterminate
lookup still requires the original reconciliation registry/handle custody. Such lookup grants no
new execution or proof authority and cannot revive a retired prepared capability.
Each exact prepared start, cancellation or termination command retains bounded local original
outcome evidence shared only with clones of that exact command. Original indeterminate custody is
installed and its handle retained before classification or reads; access failure cannot lose the
only resume path. Resume uses only that original registry scope, never enumeration, a new command
or a replacement handle. Known committed/noncommitted evidence also survives failed exact reads.
An unresolved complete cross-domain scope must be classified before an existing Syndic receipt
can establish logical success. Terminal collision stays closed, and unavailable classification
retains evidence for explicit same-scope recovery or retry. No process-local state is persisted.
An outcome lookup with no retained original observation returns unavailable; a Syndic receipt
alone cannot determine whether an unobserved two-domain operation has unresolved custody.
Passing an original outcome to a foreign-home lookup retains that evidence before rejecting
the lookup target, permitting a later exact same-home resume without repeating the operation.

After source EOF, bounded tree cross-validation visits each sequence leaf and authenticates every
marker against the identity index and order commitment; aggregate closure additionally requires
equal marker counts and the source's exact ordered marker/asset association. The EOF transition
first validates completed source folds, then resets the existing fold slots and independently
recomputes both marker summaries from the checked trees. Completion compares these tree folds to
the basis content/Asset summaries; coherence among the three trees alone is insufficient. It does
not decode a second source stream. Only its complete closure creates the final combined root and reusable
import-selection contribution. The current draft and selected conversation path stay unchanged
through this preparation. Before final writer admission, cancellation returns a nonselecting terminal
outcome and releases only the operation's custody; immutable unpublished build records remain
subject to the existing retention and abandoned-session lifecycle.

The revision-checked final replacement-start contribution selects the imported root and immutable
fresh-baseline history together, installs the exact replacement intent, advances the draft selector,
and updates session published/newest pairs together to that same baseline. The session remains
clean, with no active-operation custody. The same command replaces the existing mutable Session-key
history frontier with the exact canonical session fork of the imported baseline, advancing that
mutable frontier revision once. It binds both before/after values in the import receipt; key and
canonical digest distinguish this mutable fork from the immutable baseline selected by the session.
The first ordinary edit proves their exact recorded correspondence, appends the first real history
transition, and advances the mutable frontier revision once before adopting its Session-key
successor. Failed adoption leaves the immutable baseline selected. There is no second history reset
or stale pre-import frontier that can authorize a candidate.
The baseline has no journal, undo or redo links and both
Undo and Redo unavailable; import is not an ordinary edit or a history transition. Its receipt owns
the complete before/after draft, session, root, history, source and build/progress closure. Opening,
restoration and later provenance checks consume the recorded successor, never infer an import from
root equality or absence of history. The selected path, historical turns and descendants, accepted
label authority, and historical Asset owner remain unchanged.

Import selection follows ordinary draft publication's activity projection: the history-summary
revision advances once and last activity is the maximum of the prior activity and supplied
publication timestamp. It preserves the summary's thread revision, tail/path and completeness.
The final receipt binds its exact before/after summary and DraftByThread closure; cancellation
changes no activity projection.

An empty transition history still charges its exact encoded frontier head. Baseline and mutable
Session-fork charges follow existing snapshot/session conversion, including repeated key bytes;
their different keys do not imply equal encoded-head charges. Both must fit the existing retained
budget before import selection. Capacity refusal preserves the predecessor without inventing a
transition or waiving head accounting.

The caller composes that Syndic contribution with one Asset-domain participant. For an image input,
the participant asserts the unchanged exact historical `SubmittedTurnItem` head and creates the
absent `CurrentDraft` head over the same sealed reference set. For a marker-free input it asserts
both heads absent. Do not combine separate validation and mutation participants for that domain or
use draft-to-turn ownership transfer to remove the historical head. The opaque complete-command
closure includes these exact Asset facts for reconciliation; Syndic acquires no dependency on the
Asset implementation package. Local Asset availability is required before start becomes eligible.

Replacement-intent cancellation is a separate revision-checked metadata operation on the same
active, quiescent session. It requires the exact current replacement intent, draft selector and
session generation and no admitted mutation, publication, submission or import custody. It clears
only that intent and advances the draft selector and session's corresponding published selector
binding atomically with an immutable cancellation receipt. Current published root/history and
newest candidate root/live-history remain exactly as before, including when the editor is dirty;
it neither flushes the newest state nor adopts a root. Session generation advances once, while
candidate and dirty generations, logical extent, policy, history availability and actual history
records remain unchanged. The immutable opening checkpoint remains opening provenance. If it was
also the last published checkpoint, only the mutable published selector revision follows the new
draft selector; no opening receipt is rewritten. The caller retains caret, directed selection,
markers, scroll state and undo/redo presentation. Cancellation has no Asset effect or selected-path
mutation.

Start and intent cancellation use existing original-handle exact outcome custody. Proven success
publishes once, proven noncommit preserves the predecessor, and ambiguity retains the complete
prepared intent/receipt evidence until exact classification. Collision is terminal unavailable,
never permission to repeat the operation, invent another identity or restore noncommit. Committed
receipt validation authenticates immutable recorded closure after later eligible edits; current
mutable selector checks apply at admission and cannot invalidate that historical receipt. Process
loss before start commitment restores the last published draft; process loss after commitment
restores its exact replacement intent and imported or subsequently saved content.

Verification covers ordinary byte-vector preservation, explicit import-kind rejection, bounded
Unicode/text/image decoding, dense same-anchor markers, source and Asset unavailability, complete
three-structure closure, fresh-baseline history, stale draft/path/entry/gate/session proofs, atomic
cross-domain publication, cancellation of dirty and clean sessions, original-handle acknowledgement
loss and collision, later-edit receipt replay, and reopen/restoration. Independent semantic review
includes adversarial checks for partial selection, mixed source authority, duplicate admission,
historical owner removal and inferred import permission.

### Build Coordinate Mapping

An edit-successor build interprets every source position against its original predecessor. A
build-private persistent alignment maps those positions into the effective coherent sequence:
active working roots while a marker effect exists, otherwise the build's working roots. Frozen
pre-effect published roots do not require a second current map. Sealed-composer imports have no
edit-coordinate mapping; absence of a map never identifies or admits a mapless edit.

Coordinates are the checked sum of preceding UTF-8 bytes and marker occurrences. Text contributes
its byte length and a marker contributes one unit. Canonical `u128` arithmetic preserves the
existing independently representable `u64` byte and marker extents. Sequence-summary descent maps
a working unit cut to a piece boundary, logical UTF-8 offset and marker ordinal; all text cuts
still require valid UTF-8 boundaries. Text-leaf splitting alone changes no coordinate measure.

The alignment is a monotone path of positive `Copy(n)=(n,n)`, `Deleted(n)=(n,0)` and
`Inserted(n)=(0,n)` runs. Its source-cut function `f(x)` selects the greatest working coordinate
on the path at source cut `x`, including every inserted run there. Inserting `n` working units at
`a` leaves `f(x)` unchanged when `f(x)<a` and adds `n` otherwise. Insert at the leftmost path
position with working coordinate `a`, before its entire deleted plateau. Deleting working
`[a,b)` preserves cuts at or before `a`, collapses interior cuts to `a` and subtracts `b-a` from
cuts at or after `b`. Covered copies become deleted runs; covered inserted units disappear.
Never reorder inserted and deleted runs to normalize a map. Adjacent equal-kind runs are allowed.

The initial map is canonical Empty for zero predecessor units or Identity for one implicit copy
of all predecessor units. Its measure comes from authenticated predecessor authority. This emits
no node during the five-effect finish-to-builder transfer. The first mapping change materializes
bounded immutable nodes. Node layout, normalized occupancy, deterministic identities and bounds
are owned by [the mapping schema](design-schema-v7.md#build-coordinate-mapping-encoding).

Every actual text deletion, text insertion chunk, marker removal and marker insertion first proves
one exact working splice. Mapping runs before sequence surgery: the current map and coherent roots
remain unchanged while one pending map is built. A deletion retains fixed `a` and a decreasing
`end`; each command descends by target summaries to `end-1` and changes only that map leaf's
intersection with `[a,end)`. Zero-target subtrees are skipped by summaries. The step preserves
source measure and removes a positive number of target units. It never traverses the whole
interval or assumes one sequence leaf corresponds to one mapping run. An insertion changes one
map leaf. Each leaf update normalizes its one ancestor path before emission.

A separate MapComplete-to-Ready control command checks the final splice totals and clears proof
scratch before sequence surgery. A text mutation installs its sequence target and pending map
together. Marker surgery installs the pending map only with its coherent sequence/index/order
triple. Until that installation, a pending map is never used as the map for the old sequence.
Structural-only changes need no mapping splice. No completed build, candidate or normal effect
publication retains partial mapping work.

Removal of an original marker additionally proves that its entire original source unit remains
Copy, maps its start, and authenticates the exact original occurrence in the current sequence and
identity index. A moved or replaced marker has a deleted original unit and an inserted successor;
its new leaf cannot recreate original removal authority. No subtraction from a recent logical
frontier locates an arbitrary original occurrence.

Completed source ordering uses the actual original fragment end: the shared boundary for every
empty marker effect and the actual end for text. Source consumption is represented by the map,
not an increment past a removed marker. Before fragment publication, separate map and sequence
proofs refresh the actual successor of that retained original end. Publication installs the
completed source boundary/unit and successor boundary/logical offset together, advances progress
once and clears the fragment's mapping state. Legitimate text continuations inherit the prior
source endpoint and append their chunks without repeating its replacement.

Mapping nodes are immutable operation-owned build evidence. Current/pending descriptors and
selected/prior receipts protect their closure through cancellation, noncommit and ambiguous
outcomes. Admission cleanup, disposal and history eviction do not reclaim them. Superseded nodes
remain with other immutable build material until future explicit garbage collection proves them
unnecessary. They are outside admission-capacity and edit-history retention charges. Every mapping
acquisition, emission, probe and enlarged control record uses the same complete build-work ledger.

### Bounded Sequence Range Continuation

The ordinary fragment transitions below apply outside the explicitly closed
[deferred-prefix continuation](#deferred-prefix-and-selected-marker-run), which specializes
frontier publication and active-marker attribution while preserving these exact text splices.

Ordinary replacement removal continues right to left, changing at most one text leaf per sequence
mutation. Planning authenticates both original endpoints, maps each source unit cut and resolves
each working cut in separate bounded proof commands. Equal working marker ordinals prove that the
mapped interval contains no current marker. Removing then has one control-only transition with
equal next/end ranks; it does not iterate original pieces. Each Applying mutation follows its
completed mapping splice. The command selects the
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

An empty remaining interval enters Inserting at piece/byte cursor zero without advancing completed
frontiers. Each insertion chunk follows its proved and completed mapping splice. Exhausted
Inserting performs the separate map/sequence refresh and fragment publication described above.
Only the five removal rules change the Applying interval; proof and mapping controls leave it
frozen. Construction authenticates the selected leaf, both UTF-8 boundaries relevant to that leaf,
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

### Deferred Prefix And Selected Marker Run

A marker-free non-continuation fragment with empty inserted pieces and a nonempty remaining
text-deletion interval may defer its final prefix deletion while
the immediately following pure Remove fragments consume selected markers at that prefix's
original UTF-8 end anchor. This is one ordinary edit-successor build, with unchanged fragment
bytes, identities, ordering and terminal settlement. Each persisted working root remains coherent;
the candidate, live history and public editor positions advance only at ordinary committed adoption.

The existing outer Applying/Inserting cursor retains the prefix fragment ordinal and remaining
text interval. Deferred mode is identified by that ordinal being less than the build-private
marker scan's next-fragment ordinal. Completed source/successor and logical construction frontiers
stay frozen until group completion. The private scan may authenticate the prefix endpoint and
advance through completed removals; it retains only existing fixed next/count/chain state. There
is at most one active removal and one authenticated next-fragment lookahead, regardless of run size.

Each admitted lookahead must be the exact next immutable fragment, a non-continuation pure Remove
with no inserted text, at the same original UTF-8 anchor. The first agrees exactly with the prefix
end gap; later fragments retain ordinary predecessor-gap, occurrence, identity, prior-fragment
ordering and monotone original composite-boundary proofs. Removed original units must still be
Copy before their exact one-unit mapping and sequence/identity/order surgery. Removal cannot borrow
authority from the pending text deletion or skip unrelated fragments.
PreviousStart/PreviousEnd proofs are mandatory against the immediately preceding authenticated
scanned fragment even when its boundary differs from the frozen completed source frontier.

Entry discards only pending text-map scratch while preserving its current map and coherent roots;
immutable emitted nodes retain ordinary closure custody. Completed intermediate removals install
coherent working roots/current map and append their exact private effect count/chain/endpoint once,
then release active proof state before the next bounded removal. The separating text is retained.
EOF, a non-Remove or a different anchor ends the run without consuming that lookahead. The final
active removal retains the exact latest original boundary and mapped source-end unit. Fresh prefix
proof and map surgery then delete the remaining text against those active coherent roots; a map
target prepared before marker removal is never reused.

Unknown lookahead acquisition and run-end decisions are separate bounded control commands, never
additional reads inside sequence surgery. The final removal's Removing-to-DerivingInsertionGap
transition under the frozen prefix records authenticated run termination and retains its exact
planning source boundary. Subsequent prefix surgery credits that closed immutable derivation and
reads no unknown lookahead. Reopen validates the decision's exact lookahead only for that selected
or immediate predecessor control transition; later receipts retain ordinary bounded local proof
closure, without walking history. Pure-Remove and empty-prefix records have fixed canonical
no-payload bounds; their exact deduplicated constituent inventory shares the existing complete work
ledger. No command gains another allowance or raises the structure, point or byte ceilings.

Decode, endpoint and local receipt validation discriminate this mode through its exact enclosing
cursor/scan/active-fragment relationship. A deferred active root may reflect only receipt-proved
prefix text deletions in addition to its exact marker delta. Identity/order associations, untouched
marker summaries and original proofs remain exact; ordinary marker-root validation is unchanged.

After the prefix interval empties, its outer cursor enters exhausted Inserting with the final
original boundary. Separate refresh proofs resolve the exact successor and enter PublishReady.
Group publication completes the last removal once, installs joined completed frontiers and working
roots, clears active/mapping scratch and resumes at the exact next fragment. Final surviving-gap
ordering remains binding; an unrepresentable final gap is typed noncommit. Missing, stale, malformed
or conflicting lookahead, replay, acknowledgement loss, cancellation and reopen use the same
bounded command/receipt custody and prove exact continuation or nonadoption. No cumulative run
buffer, marker limit, synthetic text, rekey or partial candidate publication is permitted.

### Bounded Marker Effect Continuation

The ordinary outer-cursor and publication rules below apply outside the explicitly closed
[deferred-prefix continuation](#deferred-prefix-and-selected-marker-run). Its marker proofs and
one-unit surgery retain these requirements; only the stated private-scan and joined-publication
transitions differ.

A marker effect is one self-contained, non-continuation fragment with equal canonical source
positions and no text. Insert, Move and SameIdReplacement contain exactly one matching marker;
Remove contains none. Ordinary text edits may precede or follow it.

Activation binds the exact effect and fragment, pre-effect roots and completed frontiers. Published
build roots remain pre-effect until Publishing. The active effect owns its coherent working triple;
the build's one current map is paired with that triple. Pending sequence and identity descriptors
are individual surgery results, never combined roots, candidates or completed effects.

SourceBounds authenticates the shared original boundary. RemovalGap, SourceOccurrence and
SourceIdentity authenticate the original witness, marker/Asset/label/order, leaf association and
charge. Remove and SameIdReplacement require their boundary and removal occurrence to agree;
Move permits an original removal on either side of that boundary. Every effect's actual source end
is its shared boundary. Pure Insert separately proves original identity absence, so an earlier
Remove cannot hide reuse of that identity.

When a nonfirst empty fragment begins at the completed source end, separate PreviousStart and
PreviousEnd proofs authenticate the preceding range. A repeated empty range requires distinct
closed marker effects. Preserve the declared source/proposal and successor anchor/order rules.
Each proof performs at most one authenticated path lookup. AfterAll and Between retain their
two-component original-root proofs; no caller-selected proof jump or saved path replaces them.

A removal then proves its original unit remains Copy in the current map, resolves the mapped
working unit through the sequence and authenticates its exact original identity association.
The one-unit deletion builds its pending map before RemoveSequence, RemoveIdentity and RemoveOrder.
Each command changes one structure. RemoveOrder checks association and local summary deltas, installs
the coherent triple and target map together, and clears its sites and pending descriptors. Bytes
are unchanged; sequence piece, marker, identity and order counts decrease by one. Every structure
normalizes underflow before emission and shares untouched authenticated subtrees.

Separate map and sequence proofs resolve the shared original boundary after removal. FinishPlanning
then performs no lookup: it fills equal outer successor boundaries, actual `base_end=B`, equal
Removing ranks and zero removed-marker count, clears planning facts and enters the empty
Removing/Applying progression. Applying enters Inserting without publishing completed frontiers.

Insertion separately proves working identity absence, exact UTF-8 anchor, order and gap. Equal order
keys reject only at the same anchor; a later-anchor marker continues the after-markers proof.
Insertion is limited by the fragment's logical UTF-8 construction frontier, not by a comparison
between its composite gap and the mapped source cut. The proved site retains its actual boundary
and marker ordinal, with no affine mapped-next-boundary field.

The one-unit insertion builds and completes its map splice before InsertSequence, InsertIdentity
and InsertOrder. The first two retain their pending targets and exact new marker-leaf identity and
digest. InsertOrder checks all associations and summary deltas, installs the coherent triple and
map together, clears sites/pending descriptors, and exhausts the piece cursor once. Bytes are
unchanged; marker/index/order counts increase by one, and sequence piece count increases by one
at a boundary or two on text split. It does not complete the effect.

Exhausted Inserting, including zero-piece Remove, enters active Publishing with a separate
map/sequence frontier refresh. The exhausted outer cursor is frozen proof state during refresh,
not a live position to reinterpret in the changed active sequence. PublishReady retains the
authenticated successor boundary and logical offset. Publishing itself performs no lookup or
surgery: it installs active roots, completed source/successor frontiers and exact scan/count/chain,
consumes matching admission when applicable, clears active state, and advances to the next
Planning or CrossValidating state once.

Publishing composes canonical prepared admission deletion under captured Syndic revision `D0` and
the complete builder ledger. Its sealed result includes exact target puts/deletes, retained closure,
writer head and capacity. Admission rechecks fresh mutable fences and applies that immutable result
once; it does not rebuild a target path or obtain another allowance. Known commits retain their
actual serialized result; ambiguous outcomes use the existing staged-build outcome custody.

Reopen authenticates the selected and immediately preceding receipts, exact closed transition and
referenced roots. It does not repeat prior proof paths, map surgery, affine remapping or admission
deletion. Target occupancy while the source remains selected is corruption even when bytes match.
Cancellation preserves the exact current/pending closure and proves nonadoption; before Publishing
admission remains unconsumed, and afterward cleanup uses the already advanced writer. No referenced
root is released early. Final proposed build shape is checked before publication, including the
mapping-stage grammar and all canonical cursor/root bounds.

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

## Ordinary Accepted-Input Recall

Syndic exposes a dedicated ordinary recall operation over an exact active ordinary editor candidate.
It accepts a compact typed source description derived from exact successful submission acceptance,
including the original source draft/root and sealed materialization, input identity and immutable
Asset association proof. Authentication proves that acceptance and its exact sealed source for idle,
queued and steering inputs, including after promotion. Transcript selection, a caller-supplied digest,
and an arbitrary sealed content reference do not grant recall authority. Read-only reauthentication
may recover the same durable source after same-home reopening; prepared execution remains bound to
its original opaque store generation and domain handle.
The original atomic first-acceptance command records its complete request and accepted result in a
distinct acceptance-purpose form of its existing session-disposal receipt. Source authentication
requires that exact canonical receipt and sealed materialization/disposed-session closure; ordinary
disposal is insufficient. This extends closed record forms without changing the family declaration.
Persisted recall work retains only the exact receipt key/digest and compact thread/content/Asset
descriptors. It never nests the full acceptance request or disposal receipt in repeated build,
progress or settlement records. Every authority-consuming operation loads and authenticates the
original immutable receipt and compares those descriptors before using its source closure. Local
codec validity alone grants no source authority. Existing record and command bounds remain in force.
Termination that only releases an unadopted destination builder proves its exact custody and
terminal outcome independently of source availability; it consumes no source adoption authority.
Source and destination must have the same durable home and exact conversation thread; foreign
sources are typed nonmutating refusals, never relabelled or imported across conversations.

The destination may contain unsent text and markers and may belong to an executing or compacting
conversation whenever ordinary draft mutation is eligible. Recall preserves ordinary submission
intent, does not mutate a submitted input, does not require an empty draft or idle execution gate,
and creates no replacement-edit target. Existing archive, handoff, repair, session/candidate,
operation-custody and exact outcome rules still govern the relevant drafting boundary.

Accepted input roots and nodes are draft-scoped. Recall therefore streams the authenticated sealed
input through bounded decoding/tree work into the destination namespace, retaining stable marker
identity, label, order and exact AssetId under the Asset proof contract. Shared decoding and immutable
tree mechanics have source-neutral responsibilities; replacement and ordinary recall retain separate
typed admission and final-selection semantics. No complete input, marker collection or inverse payload
is materialized. An incomplete builder is never an adopted editor root.

One complete recall adopts its destination root, candidate generation, exact caret/selection and one
ordinary edit-history transition atomically. The durable current-draft selector continues to follow
ordinary autosave/flush publication. It does not select a fresh baseline: it retains eligible prior
undo transitions and clears redo as an ordinary branching edit does.
The operation uses existing configured history retention, staged-work bounds and cancellation rules.
Before admission, cancellation has no editor effect. Once admitted, original receipt/reconciliation
and committed-later-failure custody determine the terminal result independently of read observers.
Exact replay does not append a second transition. Stale source or destination, unavailable assets,
and refused capacity return a typed nonmutating result; uncertainty never becomes noncommit.

A bounded opaque pre-browse checkpoint authenticates one exact candidate root, session, home identity
and compact positions at capture. It retains the root's restoration eligibility independently of
ordinary undo-stack eviction while held; it retains no text, inverse values, Home handle or active
query. The existing immutable-record retention model does not physically delete its root or nodes.
Checkpoint restoration requires the same live destination draft/editor session and a fresh current
candidate, authenticates the retained root and positions, and adopts it with a new ordinary history
transition. It never rolls back candidate generation, journal or operation identity. Retiring the
editor ends executable checkpoint authority; disposal and recovery preserve already admitted command
custody without admitting a restore into a replacement session.

## Edit History

Every current or candidate root has matching edit-history authority. Canonical-empty creation selects
canonical-empty history. Historical replacement imports select a fresh baseline with undo and redo
unavailable; ordinary accepted-input recall instead appends an ordinary undoable transition.
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

Content identity is the canonical content digest, independent of the draft root and build operation.
Builds producing identical canonical bytes use bounded cooperative replay over the same ownerless
content manifest. Each root retains its own encoder and record cursors and independently proves its
complete mapping; finding a sealed manifest or matching digest is not that proof. Competing operations
for the same root may use the first complete, validated exact-root mapping.

A build replays already published chunk prefixes by exact canonical record equality, including byte
spans, text spans and pieces. The shared manifest never moves backwards to a build's private frontier.
Appending a chunk requires exact agreement with the current building manifest's frontier and revision;
another build may complete missing building indexes from the same canonical source. Cancelled, failed
or superseded builds leave their partial shared output available to a later exact replay without
retaining exclusive writer ownership.

Sealed content is immutable. Reuse requires every generated chunk and index record to exist exactly;
missing or contradictory sealed records fail closed and are never repaired by reuse. A successful
replay retains the actual sealed reference and revision, then atomically seals its own complete build
and publishes its root mapping. Every prepared step that observes a shared manifest revalidates that
exact manifest during command preparation, including steps that write no manifest. Concurrent drift
refuses the stale step without partial publication; callers may prepare a fresh bounded step.

Exact-root text and marker reads name the complete immutable root. Text demands are 4 through 65,536
bytes. Marker pages request 1 through 256 objects and 1 through 65,536 retained canonical bytes.
Draft text, marker, piece, validation, and materialization input pages contain at most 256 records and
65,536 payload bytes. Larger logical results return authenticated cursors.

Compact restoration validates the exact root, logical extent, cursor, UTF-8 boundary, marker gap,
and adjacency facts. It streams bounded pages and does not retain a draft-sized piece or marker
collection. A sealed-content text read returns at most 65,536 nonempty valid UTF-8 bytes from one
exact immutable content reference and never exposes building content or provider transport bytes.

Unpublished same-home composer preparation may perform these exact-root text, marker, marker-edge
and compact restoration reads through borrowed home-candidate access and fresh Syndic handles.
It uses the same bounds and authentication as ordinary reads, performs no writes, and does not
open ordinary service access. The caller separately authenticates the retained saved checkpoint
and claims before deriving a replacement composer binding.

## Unpublished Fresh Editor Recovery Disposal

Borrowed same-home recovery candidate access supports preparation and classification of exact
fresh-editor abandonment through fresh Syndic handles. It shares ordinary abandonment's request,
receipt, unchanged opening root/history authentication, revision fences and atomic mutation. It
does not open an editor, publish a draft or adopt unpublished edits. Existing V7 encodings remain
unchanged. Foreign homes, substituted opening/session/disposal identities, later edits, unsettled
operation custody and contradictory receipts fail closed.

The caller retains the original immutable opening and disposal intent and exact outcome evidence,
including a committed opening whose classification failed before an opened head was returned.
Candidate qualification distinguishes an absent original opening from its exact active or already
disposed session using that original opening request. It never substitutes another session or
treats a missing receipt alone as proven noncommit. Original pending opening or disposal outcomes
are settled through their retained reconciliation custody before conflicting recovery commands.
Recovery abandonment has its own exact command outcome and reconciliation custody. Exact committed
disposal is not repeated; exact noncommit permits only the same authenticated cleanup intent.
Collision remains unavailable and retains the evidence. Uncertainty permits no replacement editor.

## Fresh Editor Construction During Recovery

Borrowed private recovery access supports fresh editor-session opening preparation and exact outcome
classification through fresh Syndic handles. It shares the ordinary canonical request, immutable
opening receipt, authenticated durable selector/root/history, revision fences and atomic command
algorithm. It exposes no ordinary healthy-store admission and preserves existing encodings.

The app authenticates its exact committed Active onboarding claim and settles every prior editor
opening/disposal before requesting a fresh session. Syndic requires the supplied durable selector
and opening frontier to remain exact, with no foreign, substituted or occupied session. Original
prepared opening intent and receipt/reconciliation custody remain available through noncommit,
commit-before-classification, ambiguity, cancellation and repeated recovery. An uncertain outcome
does not authorize another session. A committed new opening is classified exactly and not repeated.

Candidate-only initial content reads retain ordinary bounded text/marker demand authentication.
Opening or constructing content grants no app service, native-window or graph publication authority.
Failure after opening requires exact fresh-session abandonment before a successor; the accepted
recovery disposal contract above applies to each attempt's own original immutable opening.

## Retained Composer Candidate Recovery Publication

- Borrowed same-home recovery candidate access supports exact retained editor-candidate
  qualification and publication source capture, preparation and submission before graph
  publication. This boundary shares ordinary provenance, source eligibility, revision fences,
  marker/Asset witnesses, bounded preparation and atomic publication algorithms. It neither
  exposes healthy-store admission nor creates another editor session or publication algorithm.
- Qualification authenticates the retained active session, exact candidate generation/root/live
  history and prior durable selector/checkpoint through fresh typed handles. Current-session
  eligibility, settled staging/building custody and captured-checkpoint provenance remain
  mandatory. Foreign homes, conflicting selectors, disposed sessions and unproven
  candidate facts fail closed. Exact retained same-process recovery is not permission to adopt
  unpublished state from another session or process.
- An already-saved checkpoint returns authenticated saved correspondence without publication.
  An unsaved checkpoint may publish only after the caller settles any original publication
  outcome and proves that this checkpoint has no committed or unresolved publication. An older
  autosave's proven commit is neither repeated nor evidence that newer retained edits are saved;
  fresh qualification uses the authenticated resulting selector before saving those newer edits.
  Preparation retains the exact captured checkpoint even when
  newer facts exist; it cannot substitute the newest candidate or mark a later generation saved.
  Original and recovery publication outcomes have independent ordinary reconciliation custody.
  Proven commit is freshly validated and never repeated; uncertainty authorizes no retry or save.
- Qualification returns typed saved correspondence for the retained candidate and authenticated
  resulting selector/active binding. Ordinary publication's canonical live-to-published history
  normalization remains Syndic-owned and preserves logical undo/redo history, root and generation.
  Correspondence authenticates that exact captured publication and current resulting head; a
  different later checkpoint is refused. Callers neither copy stale pre-save bindings nor derive
  private history mappings. The facts remain bound to the fresh home/generation and confer no
  service or graph publication authority.
- An older publication may advance the editor session generation while preserving a newer live
  checkpoint. A separate move-only publication correspondence authenticates the exact retained
  binding against the receipt's before-head, its older captured checkpoint, unchanged newer
  candidate/root/live history, and the exact current after-head and resulting selector. Fresh
  revision and idle-session closure checks remain mandatory. This correspondence supplies the
  current binding for separate saved observation or save; it is not a saved-checkpoint proof.
  Callers retain the original capture separately from the rebound checkpoint and preserve
  independent original/recovery outcome custody across repeated qualification and retry.
- Candidate publication preserves the existing V7 schema and command/receipt semantics, including
  marker-aware atomic Asset/Syndic proof composition. Access remains private and bounded by the
  owning recovery attempt; publication alone grants no app graph publication, widget attachment,
  claim restoration or interaction-release authority. No raw record access or app-derived proof
  mapping supplies this boundary.
- Verify saved/unsaved checkpoints, inherited generations and live history, marker-aware sources,
  stale/foreign/conflicting session and selector facts, pending staging/building, commit/noncommit/
  indeterminate publication outcomes and reconciliation, duplicate refusal and cancellation
  custody. Independently review provenance, revision fencing and persistence outcomes.

## Candidate Marker Evidence Preparation

- Borrowed same-home recovery candidate access supports seal begin, bounded advancement, status
  and cancel/fail/supersede preparation through the ordinary seal algorithms and authenticated
  source, marker-tree, cursor and frontier readers. Fresh home/generation and revision fences,
  exact source commitments, configured page limits and terminal-state validation remain intact.
- Existing typed seal contributions and State reference-set contributions retain their atomic
  command semantics. The recovery caller preserves the original staging capability and each
  command outcome/reconciliation independently, authenticates committed completion before using
  opaque proofs, and rejects unresolved, colliding or substituted facts. No schema change, raw
  record access, synthetic witness or ordinary healthy admission is granted by these counterparts.
- Verify fresh and resumed marker flights, exact old/new reconciliation, bounded cursor closure,
  source substitution, stale/foreign handles, read and command failures, terminal idempotence and
  collision, and atomic Syndic/Asset preparation. Independent persistence/resource review applies.
