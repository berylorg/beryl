# Scope

Range-backed word segmentation for the owned `gpui-text-input` dependency.

# Invalidated Approach

Phase 108 planned to resolve exact Unicode word boundaries across arbitrary bounded source pages by
using the public API of the already resolved `unicode-segmentation` dependency while never
concatenating an unbounded spanning segment or complete logical value.

# Evidence

`gpui-text-input/Cargo.lock` resolves `unicode-segmentation` 1.13.2. Its
`src/grapheme.rs` publicly exposes `GraphemeCursor` and describes it as supporting non-contiguous
text. Its `src/word.rs` instead exposes `UWordBounds<'a>` over one borrowed contiguous `&'a str`;
the `UWordBoundsState` streaming state and word-category machinery are private.

An exact word segment may span every supplied page and, in the worst case, the entire source.
Feeding the public contiguous iterator therefore requires retaining or concatenating unbounded
text. Reimplementing the private state outside the dependency would no longer satisfy the
authoritative requirement to use the exact resolved dependency policy.

# Course Correction

The implementation worker stopped and removed its incomplete range-source drafts. The sibling
repository was restored to only its pre-existing Operator-owned documentation changes.

The Operator authorized the long-term correction: a full owned clone at sibling path
`unicode-segmentation-fork`, with all changes on branch `unicode-segmentation-fork` based on exact
upstream commit `d446fa8f0089b10fb1f971a452e7ccd995646f7a` for resolved crate version 1.13.2. The fork
now exposes the accepted streaming `WordCursor`; it and the legacy contiguous iterator share one
UAX #29 transition engine and generated tables. No whole-value buffer, guessed page-edge boundary,
copied out-of-sync private state, or application-level adapter is permitted.

# Affected Work

Phase 108 in `doc/plan.md` established and accepted the fork. Phases 109 through 113 consume that
boundary.
