# Scope

Checkpoint 3 Phase 13 provider-frame storage, canonical narrative authority, bounded projection,
and reopen validation in `syndic-storage`.

# Invalidated Approach

The first ProviderItemV1 storage cutover treated the latest frame-local text-span set as the
item's current projectable narrative. A sealed frame retained its own encoded range, text spans,
observation, and cumulative lifecycle state, while the canonical item selected only that latest
sealed frame.

# Evidence

CAS agent-message and plan delta notifications carry incremental suffix text. The ProviderItemV1
encoder therefore emits each delta frame's logical spans from local offset zero and records only
that delta's bytes. It does not emit the already accepted prefix again.

For example, a start with an empty assistant message followed by deltas `"Hello"` and `" world"`
produces three frame-local views: empty, `"Hello"`, and `" world"`. Selecting only the latest frame
would render `" world"`, not `"Hello world"`. A later completion frame carries the full normalized
item, but pinned CAS intends its narrative to equal the already accumulated public start-and-delta
text rather than revise it.

# Why It Failed

Frame structural authority and item presentation authority have different update semantics.
Provider frames are immutable append-only evidence, while transcript narrative is one append view
over start and delta spans. Completion is a lifecycle and equality fence over that same narrative,
not another selected view.

Replaying all prior item frames before each delta publication would recover the text in constant
resident memory, but it would make a long streamed item quadratic in CPU and storage reads.
Materializing or copying cumulative text into each source/canonical record would violate the
bounded chunked-storage design. Treating completion as another append would duplicate the complete
text. Treating it as a replacement snapshot would conceal a protocol invariant violation and create
unnecessary projection generations.

The first narrative staging draft also assumed that one discard/counting encoder traversal could
derive both the sealed frame reference and the narrative-chain target before the durable staging
traversal. That is impossible under the accepted hash authority: every narrative-span chain step
includes the final frame encoded digest, while that digest becomes known only after the complete
frame has streamed through the encoder. Retaining every emitted span until then would make resident
memory proportional to the frame and violate the same bounded-work contract.

# Rejected Workarounds

- Do not select only the latest frame-local span set.
- Do not concatenate completion after live deltas.
- Do not select completion as a replacement narrative when it disagrees with live capture.
- Do not replay the whole item stream for every new delta or projection build.
- Do not copy cumulative narrative bytes into canonical metadata or a new whole-text value.
- Do not conceal the distinction behind an ambiguous canonical `content()` accessor.
- Do not retain a frame-sized vector of narrative spans, substitute placeholder frame digests, or
  weaken the narrative-chain hash domain to preserve the old two-traversal implementation shape.

# Required Course Correction

Retain ProviderItemV1 as the sole byte authority, but add a bounded durable narrative-view index.
Start and delta frames append new narrative span references to one selected item-owned generation.
Completion streams its exact normalized narrative through a bounded byte-for-byte comparison with
that generation. Agreement seals the same source and permits the completion frame to reuse proven
prior ranges without copying text. Disagreement retains exact completion evidence, leaves the live
generation selected, and makes history explicitly incomplete. Each selected narrative reference
carries exact bounded frontiers and one chain digest over ordered selecting-frame provenance,
physical source ranges, and logical ranges without copying text.

Projection builds and completed sets must name a closed source reference: either composer content
for user input or one provider narrative generation. Their resumable cursor walks the selected
narrative-span index directly, preserving linear bounded parsing, stable-prefix reuse for deltas,
and exact restart behavior. Agreeing completion seals the same append source and reuses its stable
projection state. No cross-segmentation digest shortcut or persisted hash implementation state is
introduced; equality is checked against exact bytes through bounded reads.

# Status

Implementation resumed after Operator acceptance. The self-identifying narrative references,
canonical span-chain records/codecs, exact build frontiers, in-place V2 family
replacement, and bounded provider-frame staging cutover are complete. Staging advances independent
content and narrative frontiers through bounded commands while published authority remains at the
prior sealed frame. Partial reopen verifies source UTF-8 and digest evidence whenever the referenced
bytes are already within the staged content frontier and defers only genuinely unavailable byte
proof.

The later completion-snapshot interpretation was invalidated before atomic publication. Its
disposable snapshot mode and completion-created generation must be removed in place; no persisted
compatibility path is required because interim V2 homes remain disposable under the rework.

Preparation now uses two constant-resident read-only encoder traversals before resumable durable
staging: the first derives the exact sealed frame/content target, and the second re-encodes against
that known frame digest to derive the exact narrative-chain target. The durable staging traversal
then writes bounded chunks and narrative spans against those fixed targets. This preserves bounded
memory and exact reconciliation at the cost of one additional linear CPU traversal.

Closed projection sources, atomic live publication, bounded reads, finalization, and full reopen
replay remain the next correction slices. Their focused runtime proofs cannot execute until those
older consumers cross the intentional removal-first compile gap.
