# Repeated Source Chunk Admission Shortens Visible Code Ranges

A descriptor allowance and response-byte allowance do not guarantee that an indexed display query
can return every requested line. The first native Code-panel mount requested sixteen ordinary short
lines from a two-thousand-line source, but received only three descriptors and an explicit
continuation. The query-local work ledger correctly refused further source acquisition.

Each line's display fragment created a separate source-window reader. Its local chunk memo ended
with that fragment, so the next line reread and charged the same bounded physical content chunk.
The native qualification `native_code_panel_mounts_bounded_lines_and_routes_selected_wheel_to_inner_viewport`
exposed this gap; earlier single-line display qualification did not exercise it.

The correction retains one shared source chunk in the existing query/command ledger. Cache misses
retain the original admission charge; cache hits still authenticate their freshly read spans,
owner, ordinal, digest and exact byte extent. Ledger disposal releases the memo, and a new ledger
reads its own source. Read, retained-byte, descriptor and response limits remain in force.

Consumers must still handle genuine partial responses and pathological lines explicitly. The
[indexed structure contract](../systems/syndic-conversation-history/design.md#indexed-text-resource-structure-queries)
and [Code panel](../gui/widgets/code-panel/spec.md) remain authoritative. Mounted continuation and
source-action acceptance is still part of root plan implementation.

## Opening-Fence Fixture And Genuine Partial Admission

On 2026-10-11 the real ordinary-driver/native fixture grew saved Code from an empty opening
fence to one line and then 2,000 lines. Mount identity survived all three extents, but the helper
assumed that its origin range must be complete. Runs `fb3036cb-5d2c-4061-9c82-a2376c7e1bfe`
(52 passed, one failed) and `2ddb9ba1-e19c-490e-8278-d5d5c01552c9` exposed an authenticated
response for `Code(0..16)` with fifteen items, `next = Code(15)`, fragment offset zero and
`request_complete = false`. Native paint realized all fifteen lines. This is an explicit partial
response, not evidence of an empty-panel or source-growth failure. Server timeout and terminal
conflict followed the fixture's failed wait and do not qualify normal completion.

Opening-fence delivery qualification accepts the fresh bounded origin response independently
of request completeness, then retains the existing complete-range checks at the manual line-900
anchor through further growth and completion. Every early extent waits for an actual Range
response, rather than treating an Anchor reply's extent as realized Code. At that qualification
boundary, full continuation composition and pathological lines remained explicit Phase 762 work;
accepting the fixture's partial origin response did not accept those remaining production behaviors.

## Bounded Whole-Line Continuation Qualification

The mounted consumer now follows whole-line Range continuations through the existing selected-history
provider and indexed residency. Earlier admitted pages remain paintable while the next read is
pending, with an exact cursor/root/source/owner match. Residency charges every retained page;
the mounted window admits at most sixteen pages and realizes at most sixteen logical lines.
A changed requested range, source or retired mount releases that retained state.

Native ordinary-driver opening/growth cases now require complete origin delivery and the expected
realized count. Mounted regressions also qualify the fifteen-plus-one response, wrong-root refusal,
anchor retention, range-change disposal, all sixteen one-line pages and their cumulative byte
accounting. A continuation can admit the first fragment of an oversized later line without dropping
earlier lines. Range fragment composition, direct Display traversal and interaction with later
fragments remain unaccepted; whole-line qualification does not establish complete long-line behavior.
