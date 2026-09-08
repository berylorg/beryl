# Persistent Range Repair Budgets

## Invalidated Local Repair

Readiness for the [sequence split-height repair](syndic-draft-piece-split-height.md) first proposed
joining a collapsed recursive fragment with its nearest intact sibling. It preserves canonical
shape, but a valid binary tree of height 45 with `2^45` one-byte leaves and deletion ranks
`[1, 2^45 - 1)` requires 259 distinct stored node reads across the two repaired splits. Only 175
nodes are emitted, so the existing post-construction 256-emission check does not prevent that read
limit violation. The recurrence is `(3H - 3) + (3H - 8)`; generated-node accesses and cache hits are
excluded. An independent lazy mathematical model checked the recurrence through height 63.

## One-Command Byte Lower Bound

A second investigation considered a single traversal of both boundaries and one normalization of
the surviving fringe. Even that cannot guarantee the aggregate 4,194,304-byte acquisition/emission
ceiling for every supported canonical tree.

Construct a height-46 root with two height-45 boundary spines. Each spine node has 128 children;
the left follows its last child and the right its first. Every other child roots an intact
minimum binary marker-only subtree. The adjacent boundary text leaves contain `ab` and `cd`.
Delete UTF-8 range `[1,3)`, which removes `bc` and no markers. The tree has
`254 * 2^45 - 252 = 8,936,830,510,563,076` pieces; its heights, fanouts and checked `u64` totals are
within the declared envelope.

All 90 spine ancestors contain removed text and cannot be reused in the successor. Each exposes
127 intact, disjoint marker-subtree references that must remain reachable. Those 11,430 references
must be read and encoded in new parents; descending into a retained subtree requires additional
references rather than eliminating one. The current node codec encodes each marker-only child in
186 bytes: id 16, digest 32, text summary 24, piece/marker counts 16, marker digest 32, and two
33-byte marker search keys. Relevant source is `draft_piece/codec.rs::encode_node`,
`enc_text_summary`, and `enc_search_key`.

The lower bound is therefore `2 * 90 * 127 * 186 = 4,251,960` bytes, exceeding the ceiling by
57,656 bytes before keys, record overhead, changed-child links, text leaves, or validation reads.
This is a codec/arithmetic counterexample, not a materialized enormous-tree runtime test. Root
inspection independently checked the codec widths and arithmetic. The owning bounds are in
`design-schema-v7.md` under V7 Bounds And Canonical Encoding; path-copy acquisitions retain the
same command byte limit.

## Required Continuation Boundary

The current `Applying` phase retains one fragment and fixed replacement boundaries. In
`persistent.rs`, it rederives those boundaries and performs both splits and the final join before
entering `Inserting`. Preparation failure creates no command or durable successor, so adding a
read/byte gate alone would retry the same expensive work without advancing.

The proposed correction is resumable partial range surgery, with a canonical intermediate working
root and exact remaining interval or repair cursor bound to each progress receipt and session
custody endpoint. Owning authority must settle cursor meaning, progress, replay and corruption
checks before implementation. Existing fields might suffice, but their current whole-fragment
semantics do not; no unchanged-format claim is established. Raising limits, persisting unary
internal padding, or repeatedly rejecting unchanged work does not meet the existing contract.

Work stopped under the Operator's technical-plan rule. No production source, tests, manifests, or
storage-format authority changed during that readiness investigation. The Operator subsequently
approved the revised resumable repair; owning-design readiness and implementation are tracked in
[the implementation plan](../plan.md).
