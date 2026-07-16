# Syndic Phase 7 Projection Skeleton

## Scope

Checkpoint 3 Phase 7 bounded transcript and resource projection storage.

## Invalidated Approach

The initial V1 skeleton represented one item projection through only an inline string,
resource count, lifecycle, revision, and owner identities. Resource metadata retained a kind,
media type, length, and digest without any durable byte backing. A transcript head retained an
entry count but no bounded selected-path rebuild cursor.

## Evidence

- `doc/plan.md` Phase 7 requires deterministic crash-resumable projection, explicit range reads,
  exact provenance, bounded page materialization, and no visible truncation.
- `doc/systems/syndic-conversation-history/concepts.md` accepts exact paragraph, code, table, and
  page thresholds while allowing arbitrarily large canonical items.
- `crates/syndic-storage/src/record/projection.rs` had no block/source-range shape, resource byte
  locator, item-projection generation, or persisted build frontier.
- `crates/beryl-home-store/src/sidecar/operations.rs` admits one complete `&[u8]` and therefore
  cannot construct an arbitrarily large textual resource with bounded memory.
- Submitted user input retains `ComposerV1` atoms. Its logical UTF-8 text is not necessarily a
  contiguous encoded range because atom headers and image markers occupy encoded bytes between
  text segments. A physical chunk-byte index alone therefore cannot locate arbitrary user-authored
  Markdown or a large user-authored code/table resource by logical text offset.
- A named selected path is efficiently available tail-to-root, while transcript positions are
  root-to-tail. One atomic rebuild would therefore require an unbounded path materialization or
  repeated full-path rescans.
- The Phase 6 `FinalizeNextTurnItem` shape advanced the finalized-item frontier in the same step
  that froze live canonical content. That leaves no truthful interval in which Phase 7 can build
  and publish an exact projection set from immutable canonical source before declaring the item
  completely finalized.

## Why It Failed

Implementing Phase 7 on that skeleton would require at least one forbidden workaround: buffering
a whole turn or resource, rescanning an unbounded history for each page, hiding parser state in an
unrelated field, exposing partial mixed generations, or truncating visible content.

It also left a summary inconsistency: live-event and item-finalization mutations could force
`complete = false` even when the selected transcript remained current and all selected turns were
complete.

## Course Correction

- Keep canonical turn bytes in bounded content chunks and add exact physical-byte and logical-text
  span indexes. The logical index maps canonical UTF-8 offsets to encoded ranges for both
  `ComposerV1` and `Utf8V1` without duplicating whole text.
- Store explicit item-projection generations and bounded resumable Markdown-build state.
- Store immutable closed-prefix membership independently of generation and keep only a live
  snapshot's provisional end-of-input outputs in its generation-owned suffix. All consumers use
  one logical membership resolver; reading the suffix family directly is invalid because it loses
  the reusable prefix.
- Store explicit transcript-generation path and publication build state.
- Represent large textual resources as canonical logical-text ranges with bounded indexed reads
  instead of duplicating or whole-buffering their bytes.
- Publish only complete coherent generations; retain interrupted and superseded derived state for
  future garbage collection.
- Separate canonical-source freezing from finalized-frontier publication. Live projection work may
  consume one exact current canonical snapshot, but source advance atomically makes its selected
  projection stale and supersedes incomplete work. The finalized-item frontier advances only from
  frozen immutable source after a visible item has one current completed projection set.
  Operational items need no transcript projection.
- Centralize exact history-summary derivation with transcript publication and contributing
  mutations.
- Reopen deterministically replays the same bounded parser engine from recorded checkpoints and
  validates every reachable set, membership record, projection, resource, digest, and transcript
  cursor. Immutable primary projection/resource records left unreachable by an interrupted write
  remain future garbage-collection candidates rather than visible authority.

## Selected-Path Side Effects

The first canonical-finalization implementation treated a turn's immutable `origin_thread_id` as
proof that the turn still belonged to that named thread's selected path. Replacement submission
can move the thread tail to a sibling while the original terminal turn still has first-time
projection closure pending. Unconditional finalization then staled the replacement transcript and
changed its history summary even though the finalized turn was off path.

The correction separates canonical authority from selected-view effects. Retained turn content
and finalization always converge, but transcript invalidation and summary changes require an exact
selected-path membership proof. One deterministic immutable ancestor skip per non-root turn makes
that proof constant-memory and bounded independently of history length.

## Invalidated Ancestry Bound

The first bound claimed that the one-skip ancestry walk needed at most 128 point reads because each
step either cleared a depth bit or crossed one boundary. That omitted the repeated smaller dyadic
regions entered after a skip would cross the requested target. The true full-`u64` triangular
upper bound is 64 + 63 + ... + 1 = 2,080 point reads.

The implementation and tests now use that exact ceiling. This preserves one 128-bit skip per turn
and constant resident memory. A 64-entry binary-lifting table would reduce reads but add roughly a
kilobyte of ancestry metadata to every turn, so it is not accepted for this workload.

## Logical EOF Is Not Piece EOF

The first range loader declared projection EOF as soon as the parser's consumed logical UTF-8 byte
count equaled the content summary's logical byte length. Composer image markers intentionally have
zero width in that coordinate space while remaining ordered render-significant content pieces.
Consequently, a trailing marker and every marker-only input were skipped.

The first correction treated EOF as a conjunction of the logical byte frontier and absence of the
next ordered content-piece record. That fixed markers but was still invalid for live history: a
later append uses the same content id, so replaying an older completed generation could observe the
newly appended next piece and consume beyond its source snapshot.

The accepted correction makes ordered piece count part of `ContentSummary` and therefore every
exact `ContentReference`. The loader stops only at that referenced piece frontier and also requires
the referenced logical byte frontier. A present in-snapshot zero-width marker is projected and
advances the piece ordinal without inventing source bytes; a post-snapshot piece is never queried.
The parser emits its synthetic empty projection only when neither logical text nor any referenced
content piece exists, so marker-only input does not acquire a spurious empty block.

## UTF-8-Safe Chunks Are Not Always Full

The first Phase 7 content validator required every non-final immutable content chunk to occupy the
entire 65,536-byte ceiling. That assumption contradicts canonical construction: when the nominal
boundary falls inside a multibyte UTF-8 scalar, construction moves the boundary backward so no
chunk or logical text span splits the scalar. A valid non-final chunk can therefore be a few bytes
short of the ceiling.

The length rule was removed rather than adding an exception range. Chunk records already reject
empty or oversized payloads, ordered byte spans prove exact contiguity, and each manifest validates
the exact chunk count, encoded-byte total, and domain-separated chunk-chain digest. Those are the
authoritative boundary and corruption proofs; nominal fullness is not.

## Broad Thread Revision Is Not Transcript Revision

The first completed-head validator required a transcript build's captured thread revision to equal
the current broad thread revision. Accepted-input admission and draft rotation legitimately advance
that revision without changing the committed tail or selected-path digest, so a correct unchanged
transcript was rejected after ordinary input admission.

Completed builds are immutable rather than republished for draft-only changes. Their source
revision may lag the current thread revision, but never exceed it, while exact tail and path digest
agreement retains transcript authority. Collecting and publishing builds still require exact
revision equality because their resumable work must not cross any concurrent thread mutation.

## Affected Authority And Proofs

The correction updates `crates/syndic-storage/doc/design.md`,
`doc/systems/syndic-conversation-history/design.md`, its `concepts.md`, root `doc/plan.md`, and the
Checkpoint 3 rework tracker. Phase 7 proofs must cover bounded construction, interruption at every
publication frontier, deterministic rebuild, source-range reads, threshold boundaries, and
finalized-history rewrite rejection.
