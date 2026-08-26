# Composer Whole-Operation And History-Lineage Assumptions

## Invalidated Approach

The composer mutation path repeatedly treated a logically unbounded edit as if one process could
retain its complete effects: first as whole-operation edit/inverse vectors, then as an app-owned Cut
page/target/item collection, and later as a storage-wide marker prepass feeding one long-lived
pending marker effect. That last correction could finish one marker but could not represent a
second marker in the same atomic mutation without retaining a marker queue or publishing sequential
draft commits.

Related invalid assumptions were that caller replay could reconstruct a released proposal prefix,
every move or same-id replacement could be forced into fragment one, global cumulative order could
select history lineage, and recursive digest re-proof could establish integrity beyond the actual
same-database trust boundary.

## Decisive Evidence

- `DraftPieceDurableBuildContinuationV1` stored marker continuation as a single `Option`. The
  `marker_continuation_transition` rejection and
  `later_marker_effect_cannot_overtake_one_pending_effect` test fixed that one pending effect as a
  global barrier rather than a bounded continuation for the current fragment.
- The Phase 180 bounded two-marker `InvalidRoot` probe wrote the clipboard successfully, advanced
  unreachable builder work for the first removal, and then rejected the second marker while the
  draft stayed unchanged. Sequentially committing the removals would violate the required one
  logical Cut and leave partial draft mutation possible.
- Retaining every Cut proposal page, selected target, text item, or marker item makes app memory
  proportional to selection size. Pre-scanning or reordering effects merely moves that unbounded
  collection and breaks natural fragment order.
- Whole-operation edit and inverse collections make edit/undo capacity depend on arbitrary fragment
  count and resident payload. Caller-reconstructed post-finish prefixes cannot be durable restart or
  replay authority after released pages.
- Independent editor sessions can fork one history frontier at overlapping cumulative positions, so
  global order cannot select the chosen lineage. The selected head's authenticated 64-level ancestor
  witness can do so with fixed retained state.
- One widget page can translate to 257 physical storage pages. Publishing those pages in separate
  commands can leave an unreconcilable prefix after the widget payload is released. Separately,
  measured large by-value settlement closures exhausted the ordinary 2 MiB worker stack; the test
  payload itself was not causal.
- The durable fold remapped an already successor-relative marker anchor through the current source
  and successor logical frontiers. A later proposal page targeting anchor 1 inside an earlier
  `0..2 -> AB` fragment underflowed as `1 - 2 + 2` and returned `InvalidRoot`. Removing that remap
  exposed the coupled physical-frontier error: splitting the earlier text for the marker left the
  stored successor boundary before the remaining text, so a following fragment targeted the wrong
  leaf.

## Accepted Correction

The app keeps one validated widget page and its one prepared atomic physical-page batch until exact
target selection, then releases it. Propagated Cut writes the clipboard first and re-reads the exact
captured immutable binding/range through bounded text and object cursors. It carries one bounded
proposal page, fixed cursor/digest/boundary state, and only previous/current/lookahead marker facts,
submitting each page immediately rather than retaining the whole operation.

After finish, storage alone resumes from durable staging custody. Immutable canonical fragment
records are the only effect collection. One fragment-ordered fold carries an `O(1)` marker-effect
scan frontier—next fragment/scanned prefix, completed count, and cumulative effect chain—and at most
one fixed-size active effect for that exact fragment. The active effect completes removal,
range-application, optional insertion, and unreachable path copies before one atomic transition
installs all three working roots, advances the frontier/count/chain, clears the slot, and permits the
next fragment. Later effects remain in cursor-addressed immutable fragments; there is no global
prepass, pending-effect queue, registry, or marker continuation list.

Composite mapping retains fixed logical UTF-8 source and successor frontiers. Storage derives each
marker gap and order from the current removal-applied roots and index, so adjacent, sparse, and same-
anchor markers need no per-marker delta accumulation. Byte-equal target closure is replay authority;
malformed, skipped, repeated, decreasing, mismatched, premature-EOF, or partial state fails closed.
Only authenticated EOF with no active effect and exact staging/effect/root coherence permits final
cross-validation. Candidate, history, session, and settlement publication remains one HomeStore
atomic command; per-effect progress is unreachable builder state, never sequential draft commits.

Marker insertion, move, and same-id replacement anchors remain successor-relative exactly as
staged. When insertion splits a leaf behind the physical successor boundary, the fold shifts that
boundary across the split and inserted marker while preserving its logical coordinate. It does not
reinterpret the marker anchor, buffer or reorder pages, or move the fragment's predecessor-relative
replacement point.

Undo and redo retain immutable historical roots plus the selected-lineage fixed 64-level witness,
not inverse payloads or global seeks. Physical page admission remains one prepared atomic batch, and
large settlement closure state is heap-indirected or split across mutually exclusive execution
branches without changing canonical durable bytes.

## Affected Authority And Remaining Risks

The correction is authoritative in `doc/features/composer/design.md`,
`doc/systems/syndic-conversation-history/design.md`, `crates/syndic-storage/doc/design.md`, and
`crates/beryl-app/doc/design.md`.

Remaining implementation risks are fragment-frontier off-by-one errors, incorrect effect-chain or
three-root binding, starting a later fragment while active state exists, premature EOF validation,
adjacent or same-anchor gap derivation mistakes, stale binding-qualified Cut re-read, partial
physical-page batch classification, malformed replacement-record decoding, selected-lineage witness
construction errors, and future by-value growth of settlement closures.
