# Scope

Phase 13 bounded replay of marker-separated authored composer text.

# Invalidated Approach

The first `sealed_content_text_segment_range` API accepted caller-supplied segment start and end
offsets. Each page used constant-count neighboring span and piece reads to validate those endpoints,
while physical page traversal rejected a `break_before` span if it encountered an interior marker.

# Evidence And Failure

Endpoint validity does not prove that the complete interval contains no other marker. In particular,
a call with `start == segment_end` returns an empty terminal page without traversing payload. An
interval beginning at the first text span and ending at content EOF could therefore be accepted even
when one or more markers lie inside it.

Sequential replay does eventually reject when a later page reaches an interior `break_before`, but
an earlier page may already have supplied a valid-looking prefix. Used as a replayable outbound
source, that would discover malformed segment authority only after `turn/start` bytes had left
Beryl, unnecessarily converting a local preparation defect into completion-unknown delivery.

The focused tests proved valid endpoints, UTF-8 continuation, and marker rejection during traversed
pages, but did not exercise a terminal-only invalid interval or require whole-interval proof before
page replay.

The same review found that endpoint helpers treated an enum-shaped marker record at an offset as
sufficient evidence. They did not authenticate its encoded bytes and digest, marker identity and
label, or contiguous content-piece and marker ordinals. A corrupted synthetic marker could therefore
fabricate an empty boundary, especially where adjacent markers share one logical text offset.

# Required Course Correction

Separate validation from replay.

- Accept only the optional opaque preceding-marker boundary returned by a prior proof rather than a
  caller-supplied piece ordinal or raw bounds. No cursor selects the unique leading/whole segment; a
  cursor selects the unique segment directly after that authenticated marker. This distinguishes
  adjacent empty segments that share offsets without trusting caller-constructed boundary identity.
- Perform one bounded scan of that derived consecutive ordered piece interval.
- Return an opaque proof bound to the exact sealed `ContentReference` only after the derived bounds
  and next authenticated marker or exact EOF are established.
- Authenticate each encountered marker through its canonical encoded bytes and digest, exact marker
  identity and label, contiguous piece and marker ordinals, and exact boundary ordinals.
- Require that proof for every later segment-range page read; callers cannot construct it from raw
  offsets.
- Continue validating each physical page path and exact manifest authority, but do not rescan the
  whole segment per page.
- Keep total preparation plus sequential replay linear and fixed-memory.

App prepared-input work remains paused until the endpoint-only API and its tests are replaced in
place. No marker-bearing CAS submission was enabled by the invalidated reader.

# Affected Authority And Proof

The correction updates `crates/syndic-storage/doc/design.md`, root `doc/plan.md`, and the Phase 13
rework tracker. Regression tests must cover terminal-only and prefix-then-marker invalid ranges in
addition to valid empty and nonempty segments.
