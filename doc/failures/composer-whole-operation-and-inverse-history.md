# Composer Whole-Operation And Inverse-History Assumption

## Invalidated Approach

The planned large-composer undo path expected `gpui-text-input` to retain every fragment of one
logical mutation behind a finite cumulative ceiling, then expected the Beryl host to stream copied
inverse text and marker witnesses from historical roots through ordinary multi-edit reconstruction.

## Why It Failed

That combination makes logical edit and undo size depend on a whole-operation resident collection
and an arbitrary fragment count. It also loses the clean identity of the historical immutable root,
cannot place restored inline objects at successor-relative positions in newly inserted text, and
turns one undo into reconstruction work whose partial progress, cancellation, replay, collision,
and exact directed-selection result cannot share one terminal settlement.

Viewport-proportional realization was the same category of mistake: a nominal drawable area can
exceed the configured retained projection even though the logical document and paged scroll extent
remain valid.

The first Syndic correction still encoded the final fragment count and terminal fragment-chain in
`DraftPieceEditHeaderV1` before `prepare_draft_piece_fragment` would admit fragment one, constrained
every fragment ordinal by that declared final count, and required caller fragments again during
reconciliation. The accepted app-neutral cursor protocol does not know terminal totals until authenticated
`finish-input` and releases each page after durable acceptance. The app therefore could not drive
that builder before finish without accumulating or later resupplying the complete operation. A
bounded post-finish builder did not provide durable pre-finish page custody.

The first custody schema wording also made the staging-head digest commit the selected progress-
receipt digest while that receipt committed the successor head digest. Those two outputs could not
be computed without a digest cycle, so the described codec and replay closure were not
implementable as written.

## Course Correction

Use one app-neutral cursor/session protocol with bounded source and proposal pages, canonical
cumulative identity, explicit finish-input, immediate payload release, and one terminal settlement.
Small edits use its one-page fast path; large edits make bounded progress without a cumulative
fragment cap.

Place an append-only Syndic intake boundary before candidate construction. One exact
`(draft, candidate session, operation)` staging head owns independent source and proposal lane
frontiers; immutable bounded pages and fixed progress receipts advance it under the candidate
session's single tagged custody slot. Authenticated finish fixes the two final lane endpoints and
atomically transfers that custody to the existing copy-on-write builder, which derives its final
header and replays durable pages in bounded work. The transfer has five fixed bounded record
effects, while pre-build terminal receipts retain outcome-specific noncommit evidence and
distinguish `None`-to-`None` terminal-before-begin from `Staging`-to-`None` terminalization.
Reconciliation and same-home restart use the
natural staging records rather than caller-retained payload. Pre-build cancellation or failure
terminalizes only staging; no staging page can become a candidate, history transition, current
draft, `ComposerV1` materialization, submission, or transcript authority.

The staging head continues to store the selected receipt key and digest, but its own digest omits
only the selected receipt digest and includes the selected key/transition ordinal. Storage then
point-reads and authenticates that receipt digest independently; the receipt still commits the
before/after head digests and all receipt fields. This establishes a computable one-way digest
order while preserving exact selected-receipt closure and canonical byte-equality replay.

Store compact durable same-draft root-transition journal/frontier records in Syndic/Fjall. Ordinary
candidate adoption appends a transition; undo and redo directly adopt an authenticated retained
historical root under a new candidate generation and restore exact caret and directed selection.
Retention uses a configurable durable byte budget and pins eligible roots until later garbage
collection; no copied inverse text or whole marker registry is authoritative.

Editor realization uses configured retained-memory and per-frame work budgets, priority credits,
bounded filler, and explicit capacity saturation. The shell and renderer retain responsibility for
an unrepresentable drawable surface or framebuffer.

## Affected Authority

The correction is owned by the composer feature and GUI docs, the bounded-resource and Syndic
conversation-history systems, the `beryl-app` and `syndic-storage` package boundaries, and the
external `gpui-text-input` design and widget specification.
