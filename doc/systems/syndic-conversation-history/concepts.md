# Syndic Concepts

This normative supplemental system doc captures the current model for Syndic's conversation,
branching, item, and reference concepts.

It is authoritative for current Syndic vocabulary and accepted model statements.

The primary [Syndic conversation-history design](design.md) owns terminal-repair concepts,
snapshot storage and selection, `FinalizingHistory`, and gate-release lifecycle. The terminal-
repair section below is a non-owning vocabulary summary; the CAS-live system owns repair
eligibility, correlation, the sole historical-request authorization, and adapter behavior.

# Turn DAG

Syndic models conversation history as a directed acyclic graph of turns.

Most turns are one user input and the agent response produced from that input.

Syndic may also record provider-operation turns when the execution provider exposes a turn identity
or turn-scoped operational item for work that is not ordinary user input, such as context
compaction.

Provider-operation turns participate in the turn DAG as parentless ownership roots. They never
become a thread's committed conversation tail, selected-path member, or ordinary-turn parent.
Transcript projections may hide or collapse them when they are not parent transcript narrative.

Every turn has either one parent turn or no parent. A turn with no parent is a root turn.

The parent chain of a turn defines the candidate conversation context for a child turn. When an
execution workflow must reconstruct context from Syndic, it may use only the exact eligible
complete prefix; it never substitutes a summary, suffix, omission, or truncation.

When a turn has more than one child, that is a branch point. Each child represents an alternate continuation from the same prior context.

The graph shape is closer to source-control history than to a flat chat log. The stable object is the turn graph; user-visible threads are views over that graph.

Source-event admission may update turn-owned canonical items and lifecycle only before proven-terminal publication. Bounded finalization and projection work may later consume only events that were already admitted. Neither path creates, removes, or restores graph parent edges or rewrites finalized history. An explicit Syndic graph operation may add a new turn with a new immutable parent edge, but no operation changes an existing turn's parent.

# Threads

A Syndic thread is a stable named reference over one selected path through the turn DAG.

The thread record owns a stable thread id, one committed conversation-tail turn id when submitted history exists, exactly one current draft id, and a revision covering that mutable binding state.

Walking immutable parent links backward from the committed tail to a root produces the thread's visible submitted conversation path. The current draft is not part of transcript narrative.

Different threads may point to the same committed tail and later diverge by submitting distinct child turns. A turn existing in the DAG never creates a thread by itself.

A named Syndic thread owns its intrinsic properties: immutable execution binding, accepted generated
title, history-derived title source, automatic branch-discussion archive state, exact token-usage
observations, lineage, draft, and history lifecycle. Beryl owns application relationships to that
thread, including window claims and selection, runtime/root availability observations, durable host
jobs, and rebuildable catalog copies.

A branch-discussion thread additionally owns the stable parent Syndic thread id used for eventual
handoff, immutable thread-lineage depth/digest/ancestor-skip facts, and the id of the first draft or
submitted turn that owns its context envelope. The envelope source identifies the historical branch
point before first submission; afterward the context-owning turn has that source as its immutable
parent. The parent thread id identifies the mutable handoff destination.

Thread lineage is a logical chain exposed through revision-bound top-to-bottom cursor pages. Its
total parent count does not require Beryl to retain the complete chain or one measurement per
ancestor.

Each thread also has a compact permanent image-label frontier. A child thread inherits the exact
frontier at creation, while immutable origin spans stay with the thread that first admitted their
range and name the matching sealed asset set. Inherited lookup follows lineage with bounded
resident state and resolves the label through that set's point index; no branch copies a historical
label map.

# Current Drafts

Every Syndic thread owns exactly one current draft.

A current draft is durable mutable pre-submission state with a stable id and selector revision. Its
small metadata record selects one exact immutable combined draft root and one closed ordinary,
branch-context, or replacement submission intent. The root binds a copy-on-write composite
sequence piece tree, marker-identity index, and marker-order commitment tree plus its exact
`DraftMarkerCommitmentV1`; text and zero-width image markers may be read in
bounded ranges without making the whole draft resident. An image marker retains its stable identity
and final label ordinal while the image-asset system resolves that marker to durable bytes.

Text-only edits reuse the complete marker-identity and marker-order roots. Marker insertion,
removal, movement, or replacement path-copies only bounded-height paths in all three structures.
The combined root authenticates all three; the commitment is structural root authority and is not
the content-neutral sequential marker summary.

Ordinary drafts have no parent and no branch context; they are only the user's unsent composer
state. A branch-discussion first draft's context intent owns exact selected-text provenance whose
envelope names the selected source turn without turning that source into a generic draft parent. A
replacement intent instead names its exact historical target and path proof. These cases cannot
coexist. Beryl may derive one presentation-only synthetic context group at that branch boundary.

`DiscussionContextRange` is a half-open range in absolute canonical logical UTF-8 byte coordinates within the source item, not a projection-local range. It must lie within one finalized source projection, and admission and scoped context resolution read its exact bytes through bounded logical-range reads over the canonical content indexes.

Range edits build immutable successors in one session-qualified candidate lineage. A committed edit
adopts its exact successor as that editor session's newest candidate; it does not publish the
current-draft selector. Autosave or a lifecycle flush separately publishes the newest eligible
candidate by atomically advancing the selector and the same session's published frontier. Thread
ownership and submission intent remain unchanged; branch context is immutable, while replacement
start or cancellation uses its own explicit revisioned operation. An interrupted publication
exposes wholly the prior or new selected root, never a partial draft. Fresh activation trusts only
that durable selector, so unpublished candidates from an abandoned session are ignored.

The same publication first compares the prior and captured exact draft-marker commitments inside
Syndic. Equal nonempty commitments validation-check and reuse the existing exact image-asset
`CurrentDraft(draft id)` head and proof without a marker scan; equal empty commitments validate head
absence. A changed commitment requires a completed bounded Syndic seal binding the captured exact
root/build/commitment/frontier to `SequentialMarkerSummaryV1` and
`OrderedMarkerAssetSummaryV1`. Changed nonempty also requires a new sealed Asset set proof with
both summaries and swaps the exact head. Changed-to-empty instead
has Syndic validate the seal/root/commitment and require both canonical empty summaries/removal
branch; one Asset participant validates and removes the exact prior head, with no Asset proof or
synthetic empty set. One mutating Syndic participant and one Asset
participant publish atomically or neither does.

A branch-context or replacement intent requires an idle input gate with no live accepted work.
Consequently, an idle gate with queued next-turn work can be promoted without reading or changing
the ordinary current draft.

An idle-thread submission first flushes the editor candidate session, then materializes the exact
selected combined root into sealed canonical `ComposerV1` content. Acceptance resolves every marker
to its exact durable asset identity, independently validates the sealed content identity/full
digest and root-bound opaque draft-marker seal proof through Syndic, requires the content-bound
summary's embedded `SequentialMarkerSummaryV1` and the seal proof's ordered association summary to
equal the Asset proof's respective summaries, selects the then-current committed tail as the ordinary turn's
parent under the exact thread, draft, selected-root, and gate revisions, transitions the same draft
identity into a submitted turn, creates the typed canonical user-input item, advances the thread's
committed tail, and creates its replacement current draft atomically. A later candidate or selector
change makes an older materialization ineligible without mutating it. First branch-context and
replacement submissions derive parentage from their explicit typed provenance instead.

That transition preserves the exact 128-bit identity payload while changing its typed identity from draft to submitted turn. It does not allocate an unrelated turn identity.

Input accepted while another turn is active or context compaction is running follows the same flush
and exact-selected-root materialization rule, then freezes the sealed `ComposerV1` content into one
immutable durable ordered accepted-input record with exact resolved marker facts, one route-
generation identity, and the complete source thread/draft/root/gate revision plus source or
replacement draft admission proof before replacing the current draft. That permanent
receipt remains exact after the draft or route advances. A bounded mutable route leaf plus the
selected generation head resolves steering, pending, next-turn, and terminal delivery state while
retaining the same accepted-input identity and marker ownership; queueing does not manufacture
another queued-input identity or a competing active turn.

When sealed `ComposerV1` becomes editable for replacement or recall, one bounded import stream
derives, builds, and cross-validates the sequence tree, marker-identity index, and marker-order
commitment tree/summary before one complete imported root may be selected.

Terminal publication does not make the input gate idle before derived history settles. It enters
durable `FinalizingHistory` for the exact terminal turn. Input accepted during that state joins
the ordered next-turn route, and promotion remains blocked until bounded item and transcript
convergence reaches a durable fixed point and an exact completion command releases the gate.

A terminal turn with a proven or conservatively suspected capture gap instead enters
`RepairRequired`. That gate blocks same-thread successor execution, fork, replacement execution,
rollback, and compaction. It carries one durable target-scoped historical-request disposition:
`Available` initially, then an immutable consumed request-attempt witness before the sole backend
dispatch capability can exist. Recovery never resets a consumed disposition. Either one complete
terminal repair snapshot is atomically selected or the turn's explicit incomplete authority is
fixed without selecting staged data; both outcomes enter `FinalizingHistory`, publish their bounded
derived history to a coherent fixed point, and only then release the gate. It does not block
unrelated threads.

Accepted-input order is permanent thread history. Separately, one revisioned per-thread input gate
owns the accepted-order high-water mark, exact route-generation head, and checked `u64` live
steering, next-turn, and logical-byte accounting. Route generations own disjoint contiguous order
intervals and expose revision-bound leaf pages. Compact ready-source records let the active-steering
scheduler walk exact targeted ready or retryable work, while distinct compact next-source records
let the next-turn scheduler walk effective queued work one generation at a time. Their cursors
advance across scanned non-candidates without retaining the backlog. Terminal accepted inputs
remain in history. Logical backlog size does not determine resident scheduler memory.

`Retryable` means that the preceding delivery provably did not dispatch and a later attempt remains
legal. It is not itself evidence that the cause is transient or that elapsed time should trigger
another attempt; process lifecycle and exact readiness evidence own that decision.

When that gate becomes idle, one atomic promotion selects the earliest effective next-turn input,
creates one fresh pending ordinary turn and canonical user-input item, and advances the selected
tail. The accepted input remains permanent history with a terminal witness naming that exact
successor. Its content is shared, its compact asset owner moves to the submitted item, and the
current draft remains byte-for-byte and revision-for-revision unchanged.

A delivery-unknown accepted input is terminal accepted-input history: Beryl knows one provider
request may have been dispatched but cannot prove whether the provider accepted it. It leaves every
live route, retains its sealed content and accepted order, and is never delivered again
automatically.
Its provenance also proves that the exact historical CAS thread was retired through a stale binding;
delivery-unknown may not coexist with usable authority for that projection.

One accepted-input route generation retains the exact binding revision, execution snapshot, Syndic
turn, CAS thread, and known-or-explicitly-unknown CAS turn observed at the gate revision. Resolved
input views obtain that proof through the generation plus their leaf; it is not copied into every
accepted-input record. CAS-turn publication, projection loss, stop, compaction, and steering
rejection reclassify a generation or one leaf through revision-checked compact mutations, never by
inferring a target, rewriting every member, or appending another history entry.

# Stop Operations

A stop operation is the durable intent to interrupt one exact active provider operation. It is not
the backend request itself and does not mean the target is terminal.

The current stop-operation record names the Syndic thread and turn, ordinary-turn or provider-
operation kind, binding revision, execution snapshot, runtime and managed-process generation,
loaded-thread generation, CAS thread and turn, operation identity, current revision, and a nonempty
fixed cause set whose members each retain their immutable first-publication revision. Causes present
at admission name revision one; an exact later owner adds its cause at the immediate successor
revision rather than creating another stop. The stopping gate carries the operation nonce that
selects this live record; it excludes steering and next-turn promotion, while the record is the
authority from which the live coordinator may claim one backend request attempt. Consuming that
authority retains every cause and claim witness and leaves an inert durable receipt with the exact
successor witness so the same-thread nonce cannot be reused. An interrupting-approval cause cannot
be safely reopened after local nondispatch.

When matching terminal evidence consumes the stop, that receipt's exact cause-first revisions,
optional dispatch-claim witness, and terminal successor witness are the sole authentication for any
delayed finalization release. Backend acceptance of the stop request is not terminal evidence, and
no process-local state can substitute for the durable receipt.

A stop attempt is the one caller-generated identity durably claimed before any interrupt request
byte may be issued. `Admitted` proves no Beryl stop request has yet been authorized.
`DispatchClaimed(source_revision, attempt)` means that exact attempt consumed the named live record
revision and owns the sole dispatch capability; after process loss it is possible-dispatch
provenance, not retry authority. Every post-admission record revision is occupied exactly once by a
new cause, this sole claim, or consumption, so cause joins and the claim remain exactly
reconcilable across later compatible descendants.

Recovery invalidates every prior-service handle and dispatch capability. A fresh service reads the
exact durable thread, gate, target, and stop-operation natural closure before converging it; the
failed service generation contributes no live authority.

Ordinary stop admission changes already ready or retryable accepted input into effective next-turn
work. Provider-operation stop admission leaves compaction-routed next-turn work unchanged. Later
accepted input is next-turn work under the exact blocked-operation kind. Only a locally proven
unissued stop may reopen the still-exact target: ordinary execution gets a fresh empty steering
generation, while compaction restores its exact compacting gate and record with no steering
generation. A provider rejection without a current-target verdict instead retires the uncertain
projection. Neither path retroactively steers queued inputs.

# Context Compaction Operations

A context compaction operation is durable same-thread provider-operation authority, not an
ordinary turn adaptation. Admission allocates distinct 128-bit operation and request-attempt
nonces. The operation nonce payload deterministically supplies the parentless provider-operation
turn id, while the app derives a separate snapshot id from the complete admission authority. One
mutation creates that turn, snapshots the exact valid CAS binding and loaded execution authority,
and changes the input gate to compacting atomically. The operation turn, record, and gate are one
authority pair even before CAS publishes its own turn id.

The provider-operation turn does not advance the committed conversation tail, selected path,
current draft, represented Syndic prefix, or native CAS model-turn count. The existing valid
binding remains the represented-lineage authority while the compaction record exclusively owns
remote operation admission. A successful compaction may change CAS's internal context while that
same binding continues to represent the same Syndic prefix.

The compaction record retains the exact admission `BerylHomeId`, Syndic thread and provider-
operation turn, operation and attempt identities, binding revision, provider-operation execution
snapshot, runtime and managed-process generation, loaded-thread generation, CAS thread, optional
one-way-published CAS turn, request disposition, ordered thread-status frontier, observed context-
compaction marker, terminal evidence, and consumed disposition. The compacting gate selects that
record. Missing, disagreeing, or reused halves are corruption rather than recovery hints.

For a successful context-compaction provider-operation turn, the record's exact terminal witness is
canonical terminal source authority. The turn remains free of ordinary source events; validation
requires the record's turn, terminal status, and recorded turn-state revision to agree exactly with
the complete turn state and rejects any second or conflicting terminal authority.

Input accepted while the gate is compacting is frozen through the ordinary permanent accepted-
input authority and routed directly to next-turn work. It never becomes steering for the
provider-operation turn. Generic ordinary terminal mutation cannot turn a compaction operation
into a pending conversation turn.

A lifecycle continuation admitted after successful compaction is a conversation turn with exact
`BerylLifecycleContinuation` origin. It is neither a provider-operation turn nor user-authored
accepted input. Its fixed canonical user-role input advances the selected conversation path while
the operator's current draft remains unchanged. Its turn and item identity domain uses the durable
compaction record's admission home identity; settlement has no independent home-identity choice.

# Turn Items

Each turn contains an ordered sequence of lightweight items.

The initial item for an ordinary user turn is the user's input for that turn.

Provider-operation turns may instead start with an operation item, such as a context-compaction item.

Response items can include narrative commentary, final answers, tool activity, command activity, errors, token accounting, and references to generated or attached items.

Items should be metadata-rich enough to render the transcript and reconstruct agent context, but should not embed heavy binary data.

Large outputs are represented as references.

# Source Event Capture

Live provider traffic is normalized into one monotonic per-turn event sequence rather than retained as an unbounded protocol object.

Normal canonical capture covers the full admitted pinned provider union: turn activation, every
admitted public field of narrative and operational item variants through their exact typed
lifecycle, and the turn-ending outcome. Assistant text starts with the provider phase when known,
retains `unknown` when absent, and may refine `unknown` only from later non-conflicting completion
metadata. Operational content remains canonical but is not parent transcript narrative.

Every externally sourced item event retains the exact CAS thread, turn, and item identity. Each canonical item separately indexes the exact source-event subsequence that built it, allowing scoped item validation to reconstruct the item without buffering a whole response or scanning unrelated turn events.

Exact replay at an occupied turn sequence means the event was already admitted. Different data at that sequence is an identity collision, and a gap is an ordering conflict. A proven-terminal event closes the source sequence permanently.

A turn state retains the admitted event count, complete item count, and contiguous finalized-item count. If a terminal turn ends with an open item, finalization may freeze that already captured content and advance the frontier, but it cannot append bytes or manufacture a missing provider event.

Normal capture preserves exact operational as well as narrative provider content. Only during a
durable-store outage may a hard-limited process-local buffer evict operational facts before
narrative, final-output, identity, and terminal facts. Any dropped canonical fact makes the whole
turn repair-required; retained buffer content may support transient presentation but cannot become
a canonical prefix for a later snapshot.

# Terminal Repair Snapshot Vocabulary

This section summarizes vocabulary only. The primary design governs every repair-snapshot storage,
selection, finalization, and release invariant.

A terminal repair snapshot is one complete, bounded, release-pinned historical view of one exact
correlated terminal CAS turn.

It is not notification replay. Snapshot entries are semantic final-item authority and retain
explicit historical-repair provenance without fabricated starts, deltas, approvals, or live source
sequence positions.

Storage stages metadata plus paged item, content, and finalized-media evidence behind an opaque
package-local snapshot reference. That reference is storage-owned, is not a shared repair identity,
and cannot be used as CAS/Syndic correlation. Staging has hard item-count, encoded-byte, page-count,
per-page item, and per-page byte limits.

Partial staging is unreachable from ordinary Syndic reads and is never canonical. An indeterminate
stage or seal reconciles by the existing target thread/turn, correlated CAS thread/turn, staging
family, and page ordinal natural identity rather than by inventing another cross-package id.

One atomic seal-and-selection validates the exact CAS/Syndic correlation, terminal outcome,
complete ordered item identities and fields, page and aggregate digests, adapter/release
provenance, and required finalized-media witnesses. Syndic then selects the complete ordered item
set and enters `FinalizingHistory`, or selects none of it. The primary design governs the later
bounded projection fixed point, coherent generation publication, and gate release. No step splices
a live prefix, outage buffer, GUI projection, or partial snapshot into repaired canonical history.

Missing full-turn view, required thread/turn/item identity, final item field, terminal outcome, or
required media makes the repair incomplete. Similar text, inferred ordering, or guessed paths never
substitute.

Ordinary transcript, catalog, replay, and projection reads remain Syndic-only after repair. The
repair adapter cannot enumerate or backfill unrelated history.

# Canonical Messages

For normal capture, Syndic preserves the original provider message exactly as received through
bounded ordered canonical content chunks referenced by the item's metadata record. For repaired
capture, the one complete terminal snapshot is the exclusive canonical item source for that turn.

Repair publication stores each complete semantic final item in snapshot-backed canonical manifests
and ranges with exact snapshot provenance. Those ranges directly source narrative and other derived
projections; they do not require fabricated live source events, item-start or delta lifecycles, or
`ProviderItemV1` live-frame ranges.

This canonical authority is the source of truth for replay, export, debugging, and projection
rebuilds. Normal and repaired item authority are never combined within one turn.

The canonical message does not have to live in the hot read path. Its manifest and chunks can remain cold as long as they are durable, range-readable, and recoverable. Per-record chunk limits never become a whole-message limit.

Normal Beryl rendering should use Syndic's parsed and indexed projection instead of loading the whole canonical message.

# Markdown Projection

Assistant Markdown should be parsed into a structured block projection.

The projection can split large rendered content into smaller blocks that are easier for Beryl to page, virtualize, and render.

Short paragraphs, short lists, short code blocks, and small tables can remain inline in the turn item page.

Large paragraphs can be indexed into sentence or span chunks for viewport rendering. The projection should still preserve the fact that the text came from one original Markdown paragraph, because splitting it into separate Markdown paragraphs would change output semantics.

Large code blocks should become independently loadable code block resources.

Large Markdown tables should become independently loadable table resources.

Inline Markdown projection uses deterministic storage thresholds:

- A paragraph above 16,384 UTF-8 bytes is indexed into source-preserving span chunks whose payload is at most 8,192 UTF-8 bytes, split only at valid source boundaries.
- A code block is externalized when its body exceeds 4,096 UTF-8 bytes or 64 logical lines. Its inline preview is at most 8 lines and 2,048 UTF-8 bytes.
- A table is externalized when it exceeds 32 body rows, 12 columns, or 8,192 UTF-8 source bytes. Its inline preview contains the header and at most 4 body rows within 4,096 UTF-8 bytes.
- One turn-item page carries at most 65,536 UTF-8 bytes of inline Markdown source. Additional large structures are represented through ordered resources rather than enlarging the page.

These are storage and paging thresholds, not visible truncation limits. Exact source remains available through the owning projection or resource.

Projection format V1 recognizes GFM block structure through a bounded streaming state machine.
It retains source line endings and byte ranges exactly. A logical line ends at LF, treats CRLF as
one ending, and counts a final nonempty unterminated segment as one line. Bare CR remains source
text rather than an implicit line ending.

Each canonical content reference snapshots both logical UTF-8 bytes and ordered render pieces.
That distinction matters because an image marker is a real ordered piece with zero logical byte
width. Projection reaches end-of-input only after both frontiers, so marker-only and trailing-marker
input remains visible and later live appends cannot enter an older projection snapshot.

Paragraph and fallback span splitting chooses the greatest valid UTF-8 boundary at or below 8,192
bytes. Every span retains one shared block-group identity and its exact ordinal and source range,
so splitting never manufactures separate Markdown paragraphs. No split requires grapheme or word
boundary discovery.

Fenced code recognition accepts the GFM backtick and tilde fence forms. The resource payload is
the exact code body, excluding the opening and closing fence lines; language metadata derives from
the bounded opening info string. An unfinished fence remains an open projection group while live
and is closed deterministically at proven terminal input without inventing a closing fence.

Table recognition uses the bounded GFM header-and-delimiter form. Escaped pipes and code spans are
handled while counting columns. If a candidate header, delimiter, row, or other construct exceeds
the parser's bounded decision window before it can be classified safely, it becomes exact
source-preserving fallback spans instead of forcing whole-block materialization.

Projection and resource identities use a domain-separated SHA-256 derivation over the projection
format version, owning item identity, source block start, and output ordinal, truncated to the
stable 128-bit Syndic identity payload. Their revisions likewise derive from immutable record
facts and exclude item-projection generation. Immutable closed-prefix membership is itself
generation-independent. A live generation owns only its provisional end-of-input membership
suffix, set, optional resumable build state, and head selection. Later source revisions resume from
the stable parser checkpoint and reuse exact closed-prefix projection and resource records without
replaying source from byte zero. A stored collision with different facts is corruption or mutation
rejection; it is never resolved by choosing another identity nondeterministically.

# Lazy Markdown Widgets

Large code blocks and large tables should have dedicated Beryl widgets with their own scroll viewport.

Those widgets should request only the visible range they need, and should be able to release data that is no longer visible.

A code block resource should expose metadata such as language, line count, byte count, digest, and optional preview lines.

It should support line-range reads.

A table resource exposes metadata such as row count, column count, header information, byte count,
digest, and optional preview rows.

Its payload read surface uses bounded half-open resource-relative logical UTF-8 ranges with exact
continuation. Syndic storage does not expose semantic row-range or column-range reads.

The parent answer projection should contain lightweight placeholders or refs for these resources rather than embedding the full code block or table body.

# Assistant Message Phases

Narrative commentary is visible assistant output emitted during the agent response.

From the OpenAI API point of view, this can arrive as an assistant message output item with phase metadata such as `commentary`.

Final answers can arrive as assistant message output with phase metadata such as `final_answer`.

Syndic should preserve phase metadata when the provider supplies it.

If phase metadata is missing, Syndic should store the message with an unknown phase rather than inventing a classification.

Codex App Server's behavior is a useful precedent here: it does not need to invent narrative commentary text. It projects assistant message items and preserves their phase when present.

# Syndic References

`syndic://` is the reference language for Syndic-addressable things.

Syndic references can point to turns, threads, messages, code blocks, tables, generated images, user-attached files, and other concrete objects that Syndic tracks.

Syndic references should include the resource kind explicitly.

A generic `item` kind should not be used when the referenced object has concrete rendering or loading semantics.

Canonical examples:

- `syndic:///turn/<id>`
- `syndic:///thread/<id>`
- `syndic:///message/<id>`
- `syndic:///codeblock/<id>`
- `syndic:///table/<id>`
- `syndic:///image/<id>`
- `syndic:///attachment/<id>`
- `syndic:///alias/image/<name>`

The canonical reference for Syndic-owned objects should be ID-based.

Canonical references use the strict `syndic:///kind/<id>` URI shape: scheme `syndic`, empty authority, absolute path, one registered lowercase-ASCII kind segment, and one canonical percent-encoded identifier segment. Parsers reject non-empty authorities, omitted third slashes, dot segments, query strings, fragments, unknown kinds, empty ids, non-canonical percent encoding, and extra path segments.

Object ids are globally unique within one Beryl home and never reassigned. A reference is resolved only against that exact home; it cannot address another home or arbitrary filesystem content.

Human-readable aliases use `syndic:///alias/<kind>/<name>`, are unique within `(Beryl home, kind)` under Unicode normalization and case folding, and never share the canonical object-id grammar.

Aliases are convenience handles that resolve through Syndic.

When a user submits input containing an alias reference, Syndic should resolve it to a concrete ID for that turn. The resolved reference should be captured on the turn so old turns do not silently change if an alias is later retargeted.

Reference resolution enforces the same owning-history and feature authorization as direct typed access. Merely knowing a reference string grants no additional read permission.

Canonical references and captured alias resolutions participate in future reachability analysis, but physical deletion is unavailable. An alias record itself is not proof that otherwise unreachable bytes are safe to retain or delete.

Syndic does not automatically expand a reference into model context. An owning feature or the CAS projection/recovery system must explicitly select bounded metadata, preview text, source ranges, or bytes under its own authority and budgets; otherwise the reference remains ordinary captured text plus typed metadata.

# Heavy Item Storage

Heavy item bytes live outside canonical turn-item records.

Canonical items retain bounded typed metadata and stable content or resource references. The
resource metadata owned by the relevant Syndic projection or feature records media type, byte
length, digest, preview ranges, exact backing identity, and required provenance without embedding
the payload in the turn item.

Transcript and history metadata reads never load generated-image bytes, attachment bytes, large
logs, or other heavy payloads. Textual code and table resources remain indexed ranges over their
canonical logical-text backing. Images, attachments, and other externally owned byte payloads use
Beryl-home sidecars only through their owning feature and storage contracts.

# File Generation

Raw model text is not arbitrary binary file generation.

Images are a first-class OpenAI API output mode, and image result bytes or references should become Syndic-addressable generated image items.

Arbitrary files are better modeled as ordinary filesystem outputs created by tools. For example, an agent can write files under an execution root, create archives through a shell or code tool, or produce files in a sandbox/container.

Absolute filesystem paths are local mutable references for those files, not immutable content
identities or preservation proof.

A file path can change contents over time, can be deleted, and can be affected by branch checkout.

If a turn must preserve the exact file content observed at submit time, Syndic needs a snapshot or digest-backed attachment reference rather than only a filesystem path.

# Lazy History Access

The turn DAG supports revision-bound, bounded cursor walking.

Useful walking directions include:

- Backward from a selected turn toward its root.
- Forward from a turn to its children.
- Forward and backward through whatever visible turn sequence a thread-scoped view later defines.
- Forward and backward through the items of a turn.
- Forward and backward through independently loadable Markdown block ranges.

Cursor reads return lightweight metadata by default.

Generated-image bytes, attachment bytes, large logs, and other heavy payloads load only through
explicit bounded range or feature-owned fetch operations.

# Replay And Context

The canonical graph must be sufficient to supply the exact eligible complete Syndic prefix for a
new agent turn. Replay or recovery injection never substitutes a summary, suffix, omission, or
truncation for that prefix.

That does not mean every UI projection must load the full graph or every heavy item byte.

Syndic maintains only the derived projections declared by the system and package authority for
transcript presentation, activity, and resource access.
The activity projection is a revision-bound paged index over exact lifecycle sources and bounded
derived facts, not a second payload store or parent transcript narrative. Ephemeral or unfinished
derived projections may be rebuilt or invalidated from the canonical turn graph and reference
metadata; a current item projection under a proven-terminal turn is finalized durable history and
cannot be rewritten in place. A named thread's transcript-view index remains rebuildable as its
selected path changes, but it only reorders or selects those frozen historical projections.

Markdown block projections are one such derived projection.

They are rebuildable from each canonical item's selected content source and resource metadata.

# Derived Lineage

Editing a prior user input never changes an existing submitted turn or its parent edge.

For replacement editing, Syndic creates a replacement turn from the edited turn's parent and atomically moves only the selected thread's committed tail and current-draft binding to the replacement path.

The original target and descendants remain immutable durable Syndic turns. They may remain reachable through another thread or become unreachable from every named thread.

Canonical closure work may finish one of those retained descendants after the replacement. The
closure changes only that descendant's canonical item and projection frontier. It does not make
the old branch selected again and does not invalidate the replacement thread's transcript.

Syndic does not physically delete unreachable turns, items, resources, or projections. The database may grow with unreachable records until a later garbage-collection design is approved.

Retry and regenerate are not Beryl product features. Users ask for another attempt by sending another normal input.
