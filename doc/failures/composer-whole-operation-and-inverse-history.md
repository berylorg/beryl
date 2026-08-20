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

Staging digest authority was self-referential: the head digest committed the selected progress-
receipt digest while that receipt committed the successor head digest, and the page digest's broad
field commitment included the successor cumulative identity derived from that page digest while
failing to authenticate the page ceilings explicitly. A mutable or chained record digest cannot
commit a downstream value derived from itself; zero-placeholder encoding does not make that cycle
canonical or implementable.

The custody outcome wording further assumed that `Conflict` could clear an admitted receiving or
finished operation from `Staging` to `None`. Admission already proves that custody's predecessor is
the session newest pair, and the single slot prevents another same-session mutation from changing
that pair while staging remains coherent. Treating the observed pair as another session or the
durable current selector conflated staging custody with session isolation or publication conflict.

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

Keep `Conflict` in the closed pre-build evidence and public outcome unions, but permit it before
begin only: ordinal one, `None`-to-`None`, with the stale expected pair, exact observed current
pair (that session's newest pair), and observed revision. Admitted staging may terminalize only as
`Rejected`, `Cancelled`, or `Error` with exact `Staging`-to-`None` evidence; attempted admitted
`Conflict` fails closed without mutation, and replay of an older valid terminal conflict uses its
immutable closure rather than later session or publication state.

Give every chained record an explicit acyclic canonical digest preimage. The staging-head digest
omits only the selected receipt digest while retaining its key and transition ordinal; storage
authenticates the selected receipt separately, and the receipt still commits the before/after head
digests. The staging-page digest commits its complete natural key, separate progress transition
ordinal, cursors, both page ceilings, prior cumulative identity, checked successor totals, and exact
canonical page-item bytes while excluding only its derived successor cumulative identity and its own
digest field, with no placeholder. Derive the successor cumulative identity afterward from the prior
identity and page digest under its separate domain. Decode, local validation, and closure validation
recompute both digests, while complete canonical byte equality remains replay authority.

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
