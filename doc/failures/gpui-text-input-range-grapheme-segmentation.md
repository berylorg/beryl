# Scope

Exact grapheme segmentation across bounded pages in the owned `gpui-text-input` dependency.

# Invalidated Approach

The accepted range segmentation coordinator recreated `unicode-segmentation::GraphemeCursor` for
each supplied page and required every incomplete-grapheme successor page to contain and strictly
grow the complete prior range. The then-active widget-lifecycle plan would have mounted this
coordinator while enforcing one finite page-byte cap.

# Evidence

`gpui-text-input/src/range_segmentation.rs` turns `PreContext`, `NextChunk`, and `PrevChunk` into a
spanning demand whose `required` value is the current complete page range. Resume validation rejects
a successor unless it contains that range and grows in the requested direction. Consequently, the
retained page grows cumulatively until the cursor proves the boundary.

Unicode grapheme clusters have no finite maximum byte length. The focused segmentation test records
only the largest page used by its small corpus and therefore does not prove a configured finite bound
for adversarial clusters. Exact traversal through the current continuation can require an
arbitrarily large `RangePage`.

# Why It Failed

A non-contiguous cursor API is not automatically a fixed-residency streaming boundary. Recreating
the cursor over successively larger containing slices preserves exact results but transfers the
unbounded cluster length into page residency, contradicting the package's hard page limit.

# Course Correction

The widget-lifecycle attempt stopped without failing over-cap, guessing a page-edge boundary, or
assembling an unbounded grapheme. The Operator accepted exact fixed-state streaming without a
semantic grapheme cap as the prerequisite to the widget lifecycle.

The exact resolved 1.13.2 implementation does not allocate or retain grapheme text:
`GraphemeCursor` stores offsets, categories, counters, and boundary flags, while `Graphemes` borrows
the caller's existing contiguous string and returns borrowed slices. The initial memory growth was
therefore in the text-input wrapper, not an inherent heap-allocation vulnerability in the crate.

However, a preserved-cursor prototype exposed an exact partitioning defect in the dependency's
non-contiguous GB11 pre-context protocol. After a bounded context chunk consumes the required ZWJ
without reaching the earlier `Extended_Pictographic`, `handle_emoji` retains only `Emoji` plus the
next offset. It then incorrectly requires the first reverse scalar of every earlier chunk to be
another ZWJ. With one-scalar pages, the family emoji resolved at byte 8 instead of byte 26.

The owned fork must first retain fixed GB11 substate distinguishing “searching for the initial ZWJ”
from “ZWJ consumed; scan `Extend*` for `Extended_Pictographic`,” and audit its other pre-context rules
for partition invariance. This remains fixed-size Unicode state and matches the finite-state model in
[UAX #29](https://unicode.org/reports/tr29/#State_Machines). After that dependency boundary is
accepted, `gpui-text-input` can preserve one cursor across independently bounded pages. Total scan
time may grow with adversarial input, but retained memory may not.

# Affected Work

Phase 115 completed and independently accepted the authorized dependency cursor correction under
the amended owned-fork design. Phase 117 completed and independently accepted the fixed-residency
`gpui-text-input` continuation against both canonical and explicit local dependency graphs. The
range-backed widget lifecycle remains Phase 118; its incomplete draft and narrow ownership-transfer
hooks are not accepted implementation and must remain outside live module and API membership until
that phase resumes.
