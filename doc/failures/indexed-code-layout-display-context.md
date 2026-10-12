# Unicode Context Must Fit The Display Window

The presentation layout index requests bounded backward grapheme context through the ordinary
indexed Code Display route. A context lower bound may lie inside a UTF-8 scalar. Requesting that
byte directly does not guarantee coverage of the context's required end: Display rounds the
start backward to a scalar boundary and then applies its fixed fragment ceiling.

The saved fixture `"🍒".repeat(2048) + "xxx" + "🍒".repeat(2049)` exposes the mismatch.
A context request for bytes 8191..16383 previously asked Display to start at 8191. Its valid
response began at 8188 and ended at 16379. Context admission requires the exact trailing edge,
so repeating the same query could never advance. Independent layout review identified this
non-progressing route before native qualification.

For a context lower bound beyond the logical-line origin, the consumer requests up to three
bytes farther forward, within the requested interval. Display's backward UTF-8 alignment then
starts at or beyond the lower bound and its bounded window covers the required end. Admission
trims the leading edge forward to a scalar boundary and still requires the exact trailing edge.
The exact line origin is already aligned and needs no adjustment. Forward reads retain their
different rule: exact leading coverage with a trailing scalar boundary.

The native regression
`native_code_layout_unicode_context_crosses_unaligned_display_edges_without_stalling` exercises
this saved source through the ordinary provider, distant horizontal layout, terminal extent and
retirement. The [layout contract](../systems/transcript-presentation/renderer-architecture.md#indexed-no-wrap-code-layout)
remains authoritative. Keep context direction and Display alignment explicit in future changes;
an authenticated bounded response alone does not prove that the next grapheme step can consume it.
